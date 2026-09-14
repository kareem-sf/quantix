"""The original-client boundary accepts only structured results, or a summary-only Manager reply."""

import sys
from contextlib import asynccontextmanager
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock

import pytest

sys.path.insert(0, str(Path(__file__).parents[1] / "ai_worker"))


from quantix.ai_runtime_mcp import SUBMIT_TOOL, RuntimeToolBridge
from quantix.office_types import ManagerAnswer
from quantix.team_models import StaffOutput


@pytest.mark.asyncio
async def test_codex_turn_points_the_model_at_the_request(tmp_path, monkeypatch):
    import openai_codex
    from quantix_ai_worker import codex

    observed = {}

    class TurnObserved(Exception):
        pass

    class Client:
        id = "synthetic-thread"

        async def __aenter__(self):
            return self

        async def close(self):
            return None

        async def account(self):
            return SimpleNamespace(model_dump=lambda **_: {"account": {"type": "chatgpt"}})

        async def thread_start(self, **kwargs):
            observed["base"] = kwargs["base_instructions"]
            assert kwargs["ephemeral"] is True
            return self

        async def turn(self, prompt, **kwargs):
            observed["turn"] = prompt
            raise TurnObserved

    @asynccontextmanager
    async def serve():
        yield

    monkeypatch.setattr(openai_codex, "AsyncCodex", lambda *_: Client())
    monkeypatch.setattr(codex, "codex_config", lambda *_args, **_kwargs: {})
    context = SimpleNamespace(account_home=tmp_path, operation_id="synthetic",
                              bridge=SimpleNamespace(serve=serve, names=[SUBMIT_TOOL]),
                              control=SimpleNamespace(event=AsyncMock()))
    with pytest.raises(TurnObserved):
        await codex.execute_codex(
            {"model_id": "synthetic-model", "web_search": False, "max_output_tokens": 1024},
            {"protocol": "codex", "billing": "subscription", "auth_type": "client_login", "_operation": "execute"},
            {}, context, "Supplied instruction", {},
        )
    assert SUBMIT_TOOL in observed["base"]
    assert "Do not run commands" in observed["base"]
    assert "Read Tender evidence only" in observed["base"]
    # The turn must point the model at the request, not at the machinery,
    # or a greeting comes back as a report about the turn itself.
    assert "Carry out the request" in observed["turn"]
    assert "do not describe this process" in observed["turn"]



@pytest.mark.asyncio
async def test_codex_names_the_quantix_server_so_deferred_tools_can_be_loaded(tmp_path, monkeypatch):
    """This account's Codex hides MCP tools behind its own tool-search step."""

    import openai_codex
    from quantix_ai_worker import codex

    observed = {}

    class TurnObserved(Exception):
        pass

    class Client:
        id = "synthetic-thread"

        async def __aenter__(self):
            return self

        async def close(self):
            return None

        async def account(self):
            return SimpleNamespace(model_dump=lambda **_: {"account": {"type": "chatgpt"}})

        async def thread_start(self, **kwargs):
            observed["base"] = kwargs["base_instructions"]
            return self

        async def turn(self, prompt, **kwargs):
            raise TurnObserved

    @asynccontextmanager
    async def serve():
        yield

    monkeypatch.setattr(openai_codex, "AsyncCodex", lambda *_: Client())
    monkeypatch.setattr(codex, "codex_config", lambda *_args, **_kwargs: {})
    context = SimpleNamespace(account_home=tmp_path, operation_id="synthetic",
                              bridge=SimpleNamespace(serve=serve, names=[SUBMIT_TOOL]),
                              control=SimpleNamespace(event=AsyncMock()))
    with pytest.raises(TurnObserved):
        await codex.execute_codex(
            {"model_id": "synthetic-model", "web_search": False, "max_output_tokens": 1024},
            {"protocol": "codex", "billing": "subscription", "auth_type": "client_login",
             "_operation": "conversation"},
            {}, context, "Supplied instruction", {},
        )
    assert "quantix MCP server" in observed["base"]
    assert "tool search by name" in observed["base"]
    # Resource listing is not how the tools load; a model that tried it once lost the run.
    assert "resource listings do not contain these tools" in observed["base"]


def test_codex_reads_the_closing_message_from_the_completed_turn():
    from quantix_ai_worker import codex

    def item(kind, text=""):
        return SimpleNamespace(root=SimpleNamespace(type=kind, text=text))

    turn = SimpleNamespace(items=[
        item("agentMessage", "First answer"),
        item("reasoning"),
        item("agentMessage", "  Hello, how can I help with this Tender?  "),
    ])
    assert codex.final_message(turn) == "Hello, how can I help with this Tender?"
    assert codex.final_message(SimpleNamespace(items=[])) == ""
    assert codex.final_message(SimpleNamespace(items=None)) == ""


def test_a_spoken_manager_reply_is_accepted_as_a_summary_only_answer():
    bridge = RuntimeToolBridge(None, ManagerAnswer, definitions=[])
    output = bridge.result("Hello. Ask me for a scope summary whenever you are ready.")
    assert output.summary.startswith("Hello.")
    assert output.source_ids == [] and output.findings == []


def test_spoken_text_never_completes_a_staff_result():
    bridge = RuntimeToolBridge(None, StaffOutput, definitions=[])
    with pytest.raises(ValueError, match="without submitting"):
        bridge.result("Here is my answer in prose.")


def test_a_manager_turn_with_no_answer_at_all_still_reports_the_missing_result():
    bridge = RuntimeToolBridge(None, ManagerAnswer, definitions=[])
    for spoken in (None, "", "   "):
        with pytest.raises(ValueError, match="without submitting"):
            bridge.result(spoken)


def _failed_turn(kind, message=""):
    from types import SimpleNamespace as NS

    info = NS(model_dump=lambda **_: kind) if kind is not None else None
    return NS(error=NS(codex_error_info=info, message=message))


def test_a_usage_limit_is_reported_as_a_usage_limit_with_its_reset_time():
    from quantix_ai_worker import codex

    reported = "You've hit your usage limit for GPT-5.3-Codex-Spark. Try again at 9:49 PM."
    explained = codex.turn_failure(
        _failed_turn("usageLimitExceeded", reported), "gpt-5.3-codex-spark"
    )
    assert "usage limit" in explained
    assert "gpt-5.3-codex-spark" in explained
    # The account's own sentence carries the reset time, which is the only part
    # the engineer can act on.
    assert reported in explained
    assert "did not complete" not in explained


def test_each_named_codex_failure_explains_what_to_do():
    from quantix_ai_worker import codex

    for kind in ("contextWindowExceeded", "sessionBudgetExceeded", "unauthorized",
                 "cyberPolicy", "badRequest", "threadRollbackFailed", "sandboxError"):
        explained = codex.turn_failure(_failed_turn(kind), "gpt-5.3-codex-spark")
        assert explained and explained[0].isupper()
        assert "codexErrorInfo" not in explained and kind not in explained


def test_an_unnamed_failure_still_passes_on_whatever_codex_said():
    from quantix_ai_worker import codex

    explained = codex.turn_failure(_failed_turn(None, "Something specific broke."), "m")
    assert "Something specific broke." in explained
    silent = codex.turn_failure(_failed_turn(None, ""), "m")
    assert "without saying why" in silent


def test_a_structured_transport_failure_does_not_leak_its_payload():
    from quantix_ai_worker import codex

    explained = codex.turn_failure(
        _failed_turn({"httpConnectionFailed": {"httpStatusCode": 418}}, "teapot"), "m"
    )
    assert "httpConnectionFailed" not in explained
    assert "teapot" in explained
