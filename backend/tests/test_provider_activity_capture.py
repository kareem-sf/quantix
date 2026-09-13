"""Synthetic public stream coverage; no inference or real Tender mutations."""
import json
import sys
from contextlib import asynccontextmanager
from pathlib import Path
from types import SimpleNamespace

import pytest
from pydantic import BaseModel
from pydantic_ai.messages import (
    FinalResultEvent,
    PartEndEvent,
    PartStartEvent,
    TextPart,
    ThinkingPart,
)

from quantix.ai_api_engine import DraftStreamEvents
from quantix.ai_api_provider import build_model_settings
from quantix.repository import Repository

sys.path.insert(0, str(Path(__file__).parents[1] / "ai_worker"))
from quantix_ai_worker.client_session import ClientEventBuffer


class Output(BaseModel):
    summary: str


@pytest.mark.asyncio
async def test_direct_draft_and_summary_keep_full_sections(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic lossless output")
    run = repo.create_run(tender["id"], "manager")
    context = SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=run["id"])
    content = "full draft " * 10000
    summary = "public summary " * 1000
    async def events():
        yield PartStartEvent(index=0, part=ThinkingPart(summary, provider_name="openai"))
        yield PartEndEvent(index=0, part=ThinkingPart(summary, provider_name="openai"))
        yield PartStartEvent(index=1, part=TextPart('{"summary":"' + content + '"}'))
        yield FinalResultEvent(tool_name=None, tool_call_id=None)
    await DraftStreamEvents(context, {"provider_id": "openai", "protocol": "openai_responses"}, Output).handle(None, events())
    saved = repo.run_events(run["id"])
    assert "".join(e["data"]["text"] for e in saved if e["kind"] == "assistant_text_delta") == content
    assert [e["data"]["text"] for e in saved if e["kind"] == "assistant_reasoning_summary"][-1] == summary


@pytest.mark.asyncio
async def test_client_buffer_retains_long_drafts_and_separate_summary_sections():
    saved = []
    class Control:
        async def event(self, kind, message, data):
            saved.append((kind, data))
    buffer = ClientEventBuffer(SimpleNamespace(control=Control()))
    content = "draft " * 12000
    await buffer.text(content, item_id="first")
    await buffer.summary("summary one " * 1000, item_id="reasoning", summary_index=0)
    await buffer.summary("summary two", item_id="reasoning", summary_index=1)
    buffer.summary_snapshot("summary two", item_id="reasoning", summary_index=1)
    await buffer.finish()
    assert "".join(data["text"] for kind, data in saved if kind == "assistant_text_delta") == content
    summaries = [data for kind, data in saved if kind == "assistant_reasoning_summary" and data.get("complete")]
    assert [data["text"] for data in summaries] == ["summary one " * 1000, "summary two"]


def test_summary_settings_do_not_change_reasoning_effort():
    route = {"model_id": "gpt-5", "max_output_tokens": 8192, "reasoning": "high"}
    settings = build_model_settings(route, {"provider_id": "openai", "protocol": "openai_responses"})
    assert settings["openai_reasoning_effort"] == "high"
    assert settings["openai_reasoning_summary"] == "auto"
    google = build_model_settings({**route, "model_id": "gemini-3-pro"}, {"provider_id": "google", "protocol": "google"})
    assert google["google_thinking_config"] == {"thinking_level": "high", "include_thoughts": True}
    ordinary = build_model_settings({"model_id": "gpt-4.1-mini", "max_output_tokens": 8192}, {"provider_id": "openai", "protocol": "openai_responses"})
    assert "openai_reasoning_summary" not in ordinary
    ordinary_google = build_model_settings({"model_id": "gemini-2.0-flash", "max_output_tokens": 8192}, {"provider_id": "google", "protocol": "google"})
    assert "google_thinking_config" not in ordinary_google


def activity_payloads(repo, run_id):
    with repo.db.connect() as conn:
        return [(json.loads(row[0]), json.loads(row[1])) for row in conn.execute(
            "SELECT e.data_json,p.content FROM run_events e JOIN run_activity_payloads p ON p.event_id=e.id WHERE e.run_id=? ORDER BY e.id", (run_id,))]


@pytest.mark.asyncio
async def test_real_agent_records_separate_request_sections_and_canonical_input(tmp_path, monkeypatch):
    from pydantic_ai.models.function import DeltaToolCall, FunctionModel

    from quantix.ai_api_engine import run_model
    from quantix.ai_api_provider import APIModelBinding
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic request records")
    run = repo.create_run(tender["id"], "manager")
    context = SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=run["id"])
    texts = iter(["Rejected first draft " * 5000, "Accepted second draft " * 5000])
    async def stream(messages, info):
        text = next(texts)
        yield {0: DeltaToolCall(name=info.output_tools[0].name, json_args=json.dumps({"summary": text}))}
    @asynccontextmanager
    async def binding(*args):
        yield APIModelBinding(FunctionModel(stream_function=stream, model_name="fixture"), {"max_tokens": 8192}, None)
    monkeypatch.setattr("quantix.ai_api_engine.model_for_route", binding)
    def validate(output, sources):
        if output.summary.startswith("Rejected"):
            raise ValueError("Correct the synthetic evidence.")
    await run_model({"model_id": "fixture", "max_output_tokens": 8192},
        {"provider_id": "openai", "protocol": "openai_responses", "billing": "subscription"},
        {"api_key": "synthetic-secret-key"}, context, "Complete supplied instruction synthetic-secret-key", Output,
        definitions=[], before_request=lambda *a: "reservation", on_response=lambda *a: None, validate_output=validate)
    captured = activity_payloads(repo, run["id"])
    prepared = [(meta, value) for meta, value in captured if meta["category"] == "model_request" and meta["phase"] == "prepared"]
    assert len(prepared) == 2
    assert "Complete supplied instruction" in str(prepared[0][1])
    assert "synthetic-secret-key" not in str(captured)
    drafts = [(meta, value) for meta, value in captured if meta["category"] == "draft" and meta["phase"] == "completed"]
    assert len(drafts) == 2
    assert len({meta["parent_operation_id"] for meta, _ in drafts}) == 2
    assert [value["text"] for _, value in drafts] == ["Rejected first draft " * 5000, "Accepted second draft " * 5000]
    assert len([meta for meta, _ in captured if meta["category"] == "model_output"]) == 2


@pytest.mark.parametrize("provider,protocol,model,expected", [
    ("openai", "openai_responses", "gpt-5", "public"),
    ("anthropic", "anthropic", "claude-sonnet-4-6", "public"),
    ("google", "google", "gemini-3-pro", "public"),
    ("xai", "openai_responses", "grok-4.6", "public"),
    ("xai", "openai_responses", "grok-4", None),
    ("custom", "openai_responses", "gpt-5", None),
])
def test_only_documented_public_summaries_are_projected(provider, protocol, model, expected):
    from quantix.ai_api_engine import public_summary
    part = ThinkingPart("public", signature="secret", provider_details={"raw_content": ["private"]})
    assert public_summary(part, {"provider_id": provider, "protocol": protocol}, model) == expected


@pytest.mark.asyncio
async def test_legacy_and_durable_public_text_both_redact_known_credentials(tmp_path):
    from quantix.native_execution import project_client_event
    from quantix.run_activity import ActivityRecorder, register_activity_secrets
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic redaction")
    run = repo.create_run(tender["id"], "manager")
    context = SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=run["id"])
    register_activity_secrets(context, ["custom-credential-canary"])
    operation = ActivityRecorder(context).start("model_request", "Synthetic request")
    async def events():
        yield PartStartEvent(index=0, part=TextPart('{"summary":"custom-credential-canary"}'))
        yield FinalResultEvent(tool_name=None, tool_call_id=None)
    await DraftStreamEvents(context, {}, Output, request_operation=lambda: operation).handle(None, events())
    project_client_event(context, "assistant_reasoning_summary", {"text": "custom-credential-canary", "complete": True}, request_operation=operation)
    assert "custom-credential-canary" not in str(repo.run_events(run["id"]))
    assert "custom-credential-canary" not in str(activity_payloads(repo, run["id"]))


@pytest.mark.asyncio
async def test_native_control_records_canonical_catalogue_and_public_sections(tmp_path):
    import mcp.types as types

    from quantix.ai_runtime_mcp import RuntimeToolBridge, WorkerControlBridge
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic native request")
    run = repo.create_run(tender["id"], "manager")
    context = SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=run["id"])
    source = RuntimeToolBridge(context, Output, definitions=[])
    control = WorkerControlBridge(source, {"provider_id": "codex", "protocol": "codex"}, {"model_id": "fixture"})
    async def emit(kind, data):
        result = await control._call_tool(None, types.CallToolRequestParams(name="event", arguments={"kind": kind, "message": "Fixture", "data": data}))
        assert not result.is_error, result.content
    await emit("runtime_request_activity", {"request_id": "request-1", "phase": "prepared", "payload": {"instructions": "Exact supplied content"}})
    await emit("assistant_reasoning_summary", {"request_id": "request-1", "item_id": "reasoning", "summary_index": 0, "text": "complete " * 2000, "complete": True})
    await emit("runtime_request_activity", {"request_id": "request-1", "phase": "completed", "payload": {"usage": {"requests": 2}}})
    captured = activity_payloads(repo, run["id"])
    prepared = captured[0][1]
    assert prepared["instructions"] == "Exact supplied content"
    assert prepared["quantix_tool_catalogue"][0]["inputSchema"]["properties"]["summary"]["type"] == "string"
    assert "supplied conversation" in prepared["quantix_tool_instructions"]
    summary = next((meta, payload) for meta, payload in captured if meta["category"] == "reasoning_summary")
    assert summary[0]["parent_operation_id"] == context._activity_parent_operation
    assert summary[1]["text"] == "complete " * 2000


def test_stopped_native_request_can_retain_already_observed_partial_section(tmp_path):
    from quantix.native_execution import project_client_event
    from quantix.run_activity import ActivityRecorder
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic stopped native request")
    run = repo.create_run(tender["id"], "manager")
    context = SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=run["id"])
    operation = ActivityRecorder(context).start("model_request", "Supplied input")
    repo.update_run(run["id"], status="cancelled")
    project_client_event(context, "runtime_public_section", {"text": "Already observed draft", "delta": False, "complete": False}, request_operation=operation)
    captured = activity_payloads(repo, run["id"])
    assert captured[-1][0]["capture_status"] == "partial"
    assert captured[-1][1]["text"] == "Already observed draft"
    assert repo.get_run(run["id"])["status"] == "cancelled"
