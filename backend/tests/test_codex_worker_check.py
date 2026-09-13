"""Codex generic checks complete the real local MCP nonce/submission contract."""

import json
import sys
from contextlib import AsyncExitStack, asynccontextmanager
from pathlib import Path
from types import SimpleNamespace

import mcp.types as types
import pytest

sys.path.insert(0, str(Path(__file__).parents[1] / "ai_worker"))

from quantix_ai_worker.remote import connect
from quantix_ai_worker.server import Worker, _nested_known_failure

from quantix.ai_worker_client import AIWorkerClient, connection_check_instruction


@pytest.mark.asyncio
@pytest.mark.parametrize("submission", ["correct", "wrong", "missing", "repeat"])
async def test_codex_check_uses_original_client_and_real_local_mcp_roundtrip(tmp_path, monkeypatch, submission):
    import openai_codex
    from openai_codex.types import TurnCompletedNotification

    calls = []
    captured = {}

    class ScriptedCodex:
        id = "synthetic-thread"

        def __init__(self, config):
            settings = {key: json.loads(value) for key, value in (option.split("=", 1) for option in config.config_overrides)}
            assert set(settings["mcp_servers.quantix.enabled_tools"]) == {"quantix_connection_check", "quantix_submit_result"}
            assert settings["mcp_servers.quantix.default_tools_approval_mode"] == "auto"
            self.endpoint = {"url": settings["mcp_servers.quantix.url"], "token": config.env["QUANTIX_MCP_TOKEN"]}

        async def __aenter__(self):
            return self

        async def close(self):
            pass

        async def account(self):
            return SimpleNamespace(model_dump=lambda **_: {"account": {"type": "chatgpt"}})

        async def thread_start(self, **kwargs):
            captured["base"] = kwargs["base_instructions"]
            assert kwargs["model"] == "gpt-5.3-codex-spark"
            assert kwargs["ephemeral"] is True
            assert "quantix_submit_result" in captured["base"]
            assert "Do not use any other account capabilities, tools" not in captured["base"]
            assert "Read Tender evidence only" not in captured["base"]
            return self

        async def turn(self, prompt, **kwargs):
            captured["turn"] = prompt
            assert "connection check" in prompt.lower()
            assert "engineering instruction" not in prompt
            assert kwargs["model"] == "gpt-5.3-codex-spark"
            return self

        async def stream(self):
            async with AsyncExitStack() as stack:
                source = await connect(stack, self.endpoint)
                catalog = await source.list_tools()
                assert {tool.name for tool in catalog.tools} == {"quantix_connection_check", "quantix_submit_result"}
                submit = next(tool for tool in catalog.tools if tool.name == "quantix_submit_result")
                assert "after reading" not in submit.description
                proof = await source.call_tool("quantix_connection_check", {})
                assert proof.is_error is False
                calls.append("check")
                value = json.loads(proof.content[0].text)["value"]
                if submission == "repeat":
                    # Tool search makes a client re-read its own catalogue, so
                    # an exploratory second call must not fail the account.
                    again = await source.call_tool("quantix_connection_check", {})
                    assert again.is_error is False
                    calls.append("check")
                    assert json.loads(again.content[0].text)["value"] == value
                if submission != "missing":
                    submitted = value if submission in {"correct", "repeat"} else "incorrect"
                    result = await source.call_tool("quantix_submit_result", {"value": submitted})
                    assert result.is_error is False
                    calls.append("submit")
            yield SimpleNamespace(method="turn/completed", payload=TurnCompletedNotification.model_validate({
                "threadId": self.id, "turn": {"id": "synthetic-turn", "status": "completed", "items": [], "error": None}}))

        async def interrupt(self):
            pass

    monkeypatch.setattr(openai_codex, "AsyncCodex", ScriptedCodex)
    account = {"id": "synthetic-codex", "provider_id": "codex", "protocol": "codex",
               "auth_type": "client_login", "billing": "subscription", "settings": {}}
    host = AIWorkerClient(SimpleNamespace(home=tmp_path))
    worker = Worker.__new__(Worker)
    worker.connection = account
    worker.account_home = tmp_path / "ai-runtimes" / account["id"]
    worker.accounts = object()

    class InProcessWorkerSession:
        async def call_tool(self, operation, arguments):
            result = await worker.operation(operation, arguments)
            return types.CallToolResult(content=[], structuredContent={"ok": True, "result": result})

    @asynccontextmanager
    async def session(_):
        yield InProcessWorkerSession()

    monkeypatch.setattr(host, "_session", session)
    monkeypatch.setattr(host, "_execute", host._execute_in_slot)
    route = {"connection_id": account["id"], "model_id": "gpt-5.3-codex-spark",
             "max_output_tokens": 1024, "web_search": False, "reasoning": None}
    if submission == "missing":
        with pytest.raises(ExceptionGroup) as caught:
            await host.check(route, account, {})
        failure = _nested_known_failure(caught.value)
        assert failure is not None and "local client ended without submitting" in str(failure)
        assert calls == ["check"]
    else:
        result = await host.check(route, account, {})
        assert result["tools_supported"] is True
        assert result["checked_count"] == (2 if submission == "repeat" else 1)
        assert result["output_supported"] is (submission in {"correct", "repeat"})
        assert calls == (["check", "check", "submit"] if submission == "repeat"
                         else ["check", "submit"])
    assert "quantix_submit_result" in connection_check_instruction("codex")


def test_direct_api_generic_instruction_retains_sdk_structured_return_contract():
    instruction = connection_check_instruction("openai_chat")
    assert "quantix_submit_result" not in instruction
    assert "return its value unchanged" in instruction


def test_the_check_token_is_accepted_plain_or_inside_the_tool_s_own_object():
    from quantix.ai_worker_client import returned_check_value

    token = "qkihr-Sn5yFCMlaBE5L-ElaV"
    assert returned_check_value(token, token) is True
    # A client that forwards the whole tool result still round-tripped the token.
    assert returned_check_value(json.dumps({"value": token}), token) is True
    assert returned_check_value('{"value": "' + token + '"}', token) is True


def test_a_check_token_that_did_not_survive_is_still_refused():
    from quantix.ai_worker_client import returned_check_value

    token = "qkihr-Sn5yFCMlaBE5L-ElaV"
    assert returned_check_value("something else", token) is False
    assert returned_check_value(json.dumps({"value": "wrong"}), token) is False
    assert returned_check_value(json.dumps({"other": token}), token) is False
    assert returned_check_value(json.dumps([token]), token) is False
    assert returned_check_value(None, token) is False


def test_the_codex_check_contract_separates_the_token_from_its_object():
    from quantix.ai_worker_client import connection_check_instruction

    instruction = connection_check_instruction("codex")
    assert "token string alone" in instruction
    assert "not the object around it" in instruction
