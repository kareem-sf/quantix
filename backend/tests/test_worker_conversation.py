"""The original-client conversation boundary exposes only structured submission."""

import sys
from contextlib import asynccontextmanager
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock

import pytest

sys.path.insert(0, str(Path(__file__).parents[1] / "ai_worker"))

from quantix_ai_worker.server import Worker

from quantix.ai_runtime_mcp import SUBMIT_TOOL, RuntimeToolBridge
from quantix.conversation import ConversationOutput


@pytest.mark.asyncio
async def test_worker_declares_conversation_with_the_execution_schema():
    declared = await Worker.list_tools(Worker.__new__(Worker), None, None)
    tools = {item.name: item for item in declared.tools}
    assert tools["conversation"].input_schema == tools["execute"].input_schema


@pytest.mark.asyncio
async def test_no_tools_submit_description_does_not_request_source_reading():
    bridge = RuntimeToolBridge(None, ConversationOutput, definitions=[])
    listed = await bridge._list_tools(None, None)
    assert [tool.name for tool in listed.tools] == [SUBMIT_TOOL]
    assert "after reading" not in listed.tools[0].description
    assert "structured" in listed.tools[0].description


@pytest.mark.asyncio
@pytest.mark.parametrize("operation", ["conversation", "execute"])
async def test_codex_turn_scopes_instruction_to_the_requested_operation(tmp_path, monkeypatch, operation):
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
            {"protocol": "codex", "billing": "subscription", "auth_type": "client_login", "_operation": operation},
            {}, context, "Supplied instruction", {},
        )
    assert SUBMIT_TOOL in observed["base"]
    assert "Do not run commands" in observed["base"]
    if operation == "conversation":
        assert "Read Tender evidence only" not in observed["base"]
        assert "engineering instruction" not in observed["turn"]
        # The turn must point the model at the engineer, not at the machinery,
        # or a greeting comes back as a report about the turn itself.
        assert "Answer the engineer's most recent message" in observed["turn"]
        assert "do not describe this process" in observed["turn"]
    else:
        assert "Read Tender evidence only" in observed["base"]
        assert "engineering instruction" in observed["turn"]



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


def test_a_spoken_conversation_reply_is_accepted_when_nothing_was_submitted():
    bridge = RuntimeToolBridge(None, ConversationOutput, definitions=[], operation="conversation")
    output = bridge.result("Hello. Ask me for a scope summary whenever you are ready.")
    assert output.kind == "conversation"
    assert output.reply.startswith("Hello.")


def test_spoken_text_never_completes_a_run_that_can_publish_records():
    for operation in ("execute", "check"):
        bridge = RuntimeToolBridge(None, ConversationOutput, definitions=[], operation=operation)
        with pytest.raises(ValueError, match="without submitting"):
            bridge.result("Here is my answer in prose.")


def test_a_conversation_with_no_answer_at_all_still_reports_the_missing_proposal():
    bridge = RuntimeToolBridge(None, ConversationOutput, definitions=[], operation="conversation")
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
