"""Real Pydantic AI streaming over synthetic providers; no inference/network."""

import asyncio
from contextlib import asynccontextmanager
from types import SimpleNamespace

import pytest
from pydantic import BaseModel
from pydantic_ai.messages import ModelRequest, UserPromptPart
from pydantic_ai.models import ModelRequestParameters
from pydantic_ai.models.function import DeltaToolCall, FunctionModel

from quantix.ai_api_engine import MeteredModel, run_model
from quantix.ai_api_provider import APIModelBinding
from quantix.repository import Repository

ROUTE = {"model_id": "synthetic-stream", "max_output_tokens": 1024, "web_search": False}
CONNECTION = {
    "provider_id": "openai",
    "protocol": "openai_responses",
    "billing": "metered",
    "_model": {"capabilities": {}},
}
MESSAGES = [ModelRequest(parts=[UserPromptPart("Synthetic instruction")])]


def metered(function, *, expected="synthetic-stream", before=None):
    admitted, reported = [], []
    model = MeteredModel(
        FunctionModel(stream_function=function, model_name="synthetic-stream"),
        route=ROUTE,
        connection=CONNECTION,
        context=None,
        expected_model=expected,
        before_request=before
        or (lambda count, limit: admitted.append((count, limit)) or "reservation"),
        on_response=lambda usage, reservation: reported.append((usage, reservation)),
    )
    return model, admitted, reported


@pytest.mark.asyncio
async def test_stream_reserves_once_and_finalizes_complete_usage_once():
    async def chunks(messages, info):
        yield "First "
        yield "second."

    model, admitted, reported = metered(chunks)
    async with model.request_stream(MESSAGES, None, ModelRequestParameters()) as stream:
        events = [event async for event in stream]
    assert events
    assert len(admitted) == len(reported) == 1
    assert reported[0][0]["usage_complete"] is True
    assert reported[0][1] == "reservation"


@pytest.mark.asyncio
async def test_stream_early_exit_keeps_incomplete_usage_reservation():
    async def chunks(messages, info):
        yield "First "
        yield "second."

    model, admitted, reported = metered(chunks)
    async with model.request_stream(MESSAGES, None, ModelRequestParameters()) as stream:
        async for event in stream:
            break
    assert len(admitted) == len(reported) == 1
    assert reported[0][0]["usage_complete"] is False
    assert stream.cancelled is True


@pytest.mark.asyncio
async def test_stream_cancellation_records_partial_usage_without_converting_cancellation():
    waiting = asyncio.Event()

    async def chunks(messages, info):
        yield "First "
        waiting.set()
        await asyncio.Event().wait()

    model, admitted, reported = metered(chunks)

    async def consume():
        async with model.request_stream(MESSAGES, None, ModelRequestParameters()) as stream:
            async for event in stream:
                pass

    task = asyncio.create_task(consume())
    await waiting.wait()
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task
    assert len(admitted) == len(reported) == 1
    assert reported[0][0]["usage_complete"] is False


@pytest.mark.asyncio
async def test_stream_refused_budget_never_opens_provider():
    called = []

    async def chunks(messages, info):
        called.append(True)
        yield "Forbidden"

    def refuse(*args):
        raise ValueError("No allowance remains.")

    model, _, reported = metered(chunks, before=refuse)
    with pytest.raises(ValueError, match="allowance"):
        async with model.request_stream(MESSAGES, None, ModelRequestParameters()) as stream:
            async for event in stream:
                pass
    assert called == [] and reported == []


@pytest.mark.asyncio
async def test_stream_model_mismatch_accounts_but_withholds_result():
    async def chunks(messages, info):
        yield "Text"

    model, admitted, reported = metered(chunks, expected="different-approved-model")
    with pytest.raises(ValueError, match="different model"):
        async with model.request_stream(MESSAGES, None, ModelRequestParameters()) as stream:
            async for event in stream:
                pass
    assert len(admitted) == len(reported) == 1


@pytest.mark.asyncio
@pytest.mark.parametrize("mode", ["auto", "tool", "native", "prompted"])
async def test_real_agent_streams_readable_summary_before_completion_and_validates_final(
    tmp_path, monkeypatch, mode
):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic stream")
    run = repo.create_run(tender["id"], "manager")
    context = SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=run["id"])
    observed = []

    class Output(BaseModel):
        summary: str
        private_table: list[str]

    summary = "Synthetic measured draft. " * 15

    async def chunks(messages, info):
        first = '{"summary":"' + summary
        last = 'Done.","private_table":["do-not-stream-this-table"]}'
        yield (
            {0: DeltaToolCall(name=info.output_tools[0].name, json_args=first)}
            if info.output_tools
            else first
        )
        observed.extend(repo.run_events(run["id"]))
        yield {0: DeltaToolCall(json_args=last)} if info.output_tools else last

    @asynccontextmanager
    async def binding(*args):
        yield APIModelBinding(
            FunctionModel(stream_function=chunks, model_name="synthetic-stream"),
            {"max_tokens": 1024},
            None,
        )

    monkeypatch.setattr("quantix.ai_api_engine.model_for_route", binding)
    admitted, reported = [], []
    connection = {
        **CONNECTION,
        "_model": {"source": "provider", "capabilities": {"structured_output": True}},
    }
    result = await run_model(
        {**ROUTE, "output_mode": mode},
        connection,
        {},
        context,
        "Synthetic instruction",
        Output,
        definitions=[],
        before_request=lambda *args: admitted.append(args) or "reservation",
        on_response=lambda *args: reported.append(args),
    )
    events = repo.run_events(run["id"])
    deltas = [event["data"]["text"] for event in events if event["kind"] == "assistant_text_delta"]
    assert any(event["kind"] == "assistant_text_delta" for event in observed)
    assert "".join(deltas) == summary + "Done."
    assert "do-not-stream-this-table" not in str(events)
    assert result["output"].private_table == ["do-not-stream-this-table"]
    assert len(admitted) == len(reported) == 1


@pytest.mark.asyncio
async def test_actual_agent_cancellation_retains_partial_usage_and_unpublished_draft(
    tmp_path, monkeypatch
):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic cancelled stream")
    run = repo.create_run(tender["id"], "manager")
    context = SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=run["id"])
    waiting = asyncio.Event()

    class Output(BaseModel):
        summary: str

    async def chunks(messages, info):
        yield {
            0: DeltaToolCall(
                name=info.output_tools[0].name, json_args='{"summary":"' + "Tentative draft. " * 30
            )
        }
        waiting.set()
        await asyncio.Event().wait()

    @asynccontextmanager
    async def binding(*args):
        yield APIModelBinding(
            FunctionModel(stream_function=chunks, model_name="synthetic-stream"),
            {"max_tokens": 1024},
            None,
        )

    monkeypatch.setattr("quantix.ai_api_engine.model_for_route", binding)
    reported = []
    task = asyncio.create_task(
        run_model(
            ROUTE,
            CONNECTION,
            {},
            context,
            "Synthetic instruction",
            Output,
            definitions=[],
            before_request=lambda *args: "reservation",
            on_response=lambda *args: reported.append(args),
        )
    )
    await waiting.wait()
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task
    assert len(reported) == 1 and reported[0][0]["usage_complete"] is False
    assert any(event["kind"] == "assistant_text_delta" for event in repo.run_events(run["id"]))
    assert repo.get_run(run["id"])["result"] == {}


@pytest.mark.asyncio
async def test_partial_usage_is_reported_even_when_transport_close_fails(monkeypatch):
    from pydantic_ai.models.function import FunctionStreamedResponse

    async def chunks(messages, info):
        yield "Partial draft"
        yield " remainder"

    async def failed_close(self):
        raise RuntimeError("private transport diagnostics")

    monkeypatch.setattr(FunctionStreamedResponse, "close_stream", failed_close)
    model, admitted, reported = metered(chunks)
    with pytest.raises(ConnectionError):
        async with model.request_stream(MESSAGES, None, ModelRequestParameters()) as stream:
            async for event in stream:
                break
    assert len(admitted) == len(reported) == 1
    assert reported[0][0]["usage_complete"] is False


@pytest.mark.asyncio
async def test_output_correction_reserves_each_request_and_resets_abandoned_draft(
    tmp_path, monkeypatch
):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic correction")
    run = repo.create_run(tender["id"], "manager")
    context = SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=run["id"])

    class Output(BaseModel):
        summary: str

    replies = iter(["Rejected draft. " * 30, "Accepted correction. " * 30])

    async def chunks(messages, info):
        reply = next(replies)
        yield {0: DeltaToolCall(name=info.output_tools[0].name, json_args='{"summary":"' + reply)}
        yield {0: DeltaToolCall(json_args='"}')}

    @asynccontextmanager
    async def binding(*args):
        yield APIModelBinding(
            FunctionModel(stream_function=chunks, model_name="synthetic-stream"),
            {"max_tokens": 1024},
            None,
        )

    monkeypatch.setattr("quantix.ai_api_engine.model_for_route", binding)

    def validate(output, sources):
        if not output.summary.startswith("Accepted"):
            raise ValueError("Planted evidence correction needed.")

    admitted, reported = [], []
    result = await run_model(
        ROUTE,
        CONNECTION,
        {},
        context,
        "Synthetic instruction",
        Output,
        definitions=[],
        before_request=lambda *args: admitted.append(args) or f"reservation-{len(admitted)}",
        on_response=lambda *args: reported.append(args),
        validate_output=validate,
    )
    deltas = [
        event["data"]
        for event in repo.run_events(run["id"])
        if event["kind"] == "assistant_text_delta"
    ]
    assert result["output"].summary.startswith("Accepted")
    assert len(admitted) == len(reported) == 2
    assert deltas[-1]["reset"] is True and deltas[-1]["text"].startswith("Accepted")


@pytest.mark.asyncio
async def test_summary_projection_never_emits_signatures_raw_reasoning_or_tool_arguments(tmp_path):
    from pydantic_ai.messages import (
        PartDeltaEvent,
        PartEndEvent,
        PartStartEvent,
        ThinkingPart,
        ThinkingPartDelta,
        ToolCallPart,
    )

    from quantix.ai_api_engine import DraftStreamEvents

    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic supplied summary")
    run = repo.create_run(tender["id"], "manager")
    context = SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=run["id"])

    class Output(BaseModel):
        summary: str

    thinking = ThinkingPart(
        "Checking quantities",
        provider_name="openai",
        signature="signature-canary",
        provider_details={"raw_content": ["raw-reasoning-canary"]},
    )

    async def events():
        yield PartStartEvent(index=0, part=thinking)
        yield PartDeltaEvent(index=0, delta=ThinkingPartDelta(content_delta=" against the source."))
        yield PartEndEvent(
            index=0,
            part=ThinkingPart("Checking quantities against the source.", provider_name="openai"),
        )
        yield PartStartEvent(
            index=1, part=ToolCallPart("read_private_source", {"summary": "private-tool-canary"})
        )

    await DraftStreamEvents(context, CONNECTION, Output).handle(None, events())
    saved = repo.run_events(run["id"])
    assert [event["data"]["text"] for event in saved] == ["Checking quantities against the source."]
    assert "canary" not in str(saved)


@pytest.mark.asyncio
async def test_stream_transport_failure_retains_reservation_and_has_no_sdk_error_leak():
    async def chunks(messages, info):
        raise RuntimeError("secret-token-provider-body")
        yield "unreachable"

    model, admitted, reported = metered(chunks)
    with pytest.raises(ConnectionError) as caught:
        async with model.request_stream(MESSAGES, None, ModelRequestParameters()) as stream:
            async for event in stream:
                pass
    assert len(admitted) == 1 and reported == []
    assert "secret-token" not in str(caught.value)


@pytest.mark.asyncio
async def test_stop_blocks_late_stream_event_append(tmp_path):
    from pydantic_ai.messages import FinalResultEvent, PartStartEvent, TextPart

    from quantix.ai_api_engine import DraftStreamEvents

    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic stopped stream")
    run = repo.create_run(tender["id"], "manager")
    context = SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=run["id"])

    class Output(BaseModel):
        summary: str

    async def events():
        yield PartStartEvent(index=0, part=TextPart('{"summary":"Unaccepted draft"}'))
        repo.update_run(run["id"], status="cancelled")
        yield FinalResultEvent(tool_name=None, tool_call_id=None)

    with pytest.raises(InterruptedError):
        await DraftStreamEvents(context, CONNECTION, Output).handle(None, events())
    assert repo.run_events(run["id"]) == []


@pytest.mark.asyncio
async def test_partial_stream_keeps_real_durable_money_reservation(tmp_path):
    from quantix.ai_connections import AIConnectionService
    from quantix.ai_policy import AIPolicyService, BudgetMeter

    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic stream spending")
    connections = AIConnectionService(repo)
    connection = connections.create(
        {
            "name": "Synthetic paid",
            "provider_id": "openai",
            "protocol": "openai_responses",
            "auth_type": "api_key",
            "credentials": {"api_key": "synthetic"},
            "session_only": True,
        }
    )
    connections.save_model(
        connection["id"],
        {
            "model_id": ROUTE["model_id"],
            "display_name": "Synthetic model",
            "capabilities": {"tools": True},
            "pricing": {
                "input_per_million": 1,
                "output_per_million": 2,
                "source": "synthetic-test-only",
                "as_of": "2026-09-13",
            },
        },
    )
    route = {**ROUTE, "connection_id": connection["id"], "reasoning": None, "max_search_calls": 3}
    policy = AIPolicyService(repo)
    policy.update(
        tender["id"],
        {
            "allowed_connection_ids": [connection["id"]],
            "manager": route,
            "run_budget_usd": 1,
            "tender_budget_usd": 1,
            "engineer_confirmed": True,
            "rationale": "Synthetic test",
        },
    )
    run = repo.create_run(tender["id"], "manager")
    meter = BudgetMeter(policy, tender["id"], run["id"], route)

    async def chunks(messages, info):
        yield "Partial paid response"
        yield " not consumed"

    model = MeteredModel(
        FunctionModel(stream_function=chunks, model_name=ROUTE["model_id"]),
        route=route,
        connection={**meter.connection, "_model": meter.model},
        context=None,
        before_request=meter.before_request,
        on_response=meter.on_response,
    )
    async with model.request_stream(MESSAGES, None, ModelRequestParameters()) as stream:
        async for event in stream:
            break
    usage = policy.usage(tender["id"])
    assert len(usage) == 1 and usage[0]["status"] == "uncertain"
    assert usage[0]["reserved_usd"] > 0 and policy.get(tender["id"])["reserved_usd"] > 0


@pytest.mark.asyncio
async def test_staff_stream_keeps_trusted_assignment_identity(tmp_path):
    from pydantic_ai.messages import FinalResultEvent, PartStartEvent, TextPart

    from quantix.ai_api_engine import DraftStreamEvents
    from quantix.staff_runtime_models import StaffProviderOutput

    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic staff stream")
    run = repo.create_run(tender["id"], "manager")
    context = SimpleNamespace(
        repo=repo,
        tender_id=tender["id"],
        run_id=run["id"],
        assignment_id="a" * 32,
        actor_id="b" * 32,
    )

    async def events():
        yield PartStartEvent(
            index=0,
            part=TextPart('{"result":{"kind":"question","question":"Which revision applies?"}}'),
        )
        yield FinalResultEvent(tool_name=None, tool_call_id=None)

    await DraftStreamEvents(context, CONNECTION, StaffProviderOutput).handle(None, events())
    saved = repo.run_events(run["id"])
    assert saved[0]["data"] == {
        "text": "Which revision applies?",
        "assignment_id": "a" * 32,
        "actor_id": "b" * 32,
    }
