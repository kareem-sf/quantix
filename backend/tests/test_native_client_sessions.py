"""Original-client session methods and scope remain separate from API credentials."""

import sys
from contextlib import asynccontextmanager
from pathlib import Path
from types import SimpleNamespace

import pytest

sys.path.insert(0, str(Path(__file__).parents[1] / "ai_worker"))

from quantix_ai_worker.codex import execute_codex, record_token_usage
from quantix_ai_worker.grok import headless_command


@pytest.mark.asyncio
async def test_codex_uses_exact_native_resume_with_fresh_scoped_bridge(tmp_path, monkeypatch):
    import openai_codex
    from openai_codex import ApprovalMode, Sandbox
    from openai_codex.types import TurnCompletedNotification

    events, methods = [], []

    class Control:
        async def event(self, kind, message, data):
            events.append((kind, data))

    class Bridge:
        names = ["quantix_submit_result"]
        url = "http://127.0.0.1:12345/mcp"
        token = "source-only-fixture"
        closed = False

        @asynccontextmanager
        async def serve(self):
            yield self

        async def result(self, text=None):
            return {"summary": "Ready for review."}

    class Codex:
        id = "native-exact-session"

        def __init__(self, config):
            assert config.env["CODEX_HOME"] == str(tmp_path / "codex")

        async def __aenter__(self):
            return self

        async def close(self):
            pass

        async def account(self):
            return SimpleNamespace(model_dump=lambda **kwargs: {"account": {"type": "chatgpt"}})

        async def thread_start(self, **kwargs):
            raise AssertionError("A compatible native session must use the native resume method.")

        async def thread_resume(self, identifier, **kwargs):
            methods.append(identifier)
            assert identifier == self.id and kwargs["model"] == "exact-model"
            assert (
                kwargs["approval_mode"] == ApprovalMode.deny_all
                and kwargs["sandbox"] == Sandbox.read_only
            )
            assert "Earlier session context is historical" in kwargs["base_instructions"]
            assert Path(kwargs["cwd"]).is_relative_to(tmp_path)
            return self

        async def turn(self, instruction, **kwargs):
            assert kwargs["model"] == "exact-model"
            return self

        async def stream(self):
            yield SimpleNamespace(
                method="item/agentMessage/delta",
                payload=SimpleNamespace(delta="Ready for review.", item_id="message"),
            )
            yield SimpleNamespace(
                method="turn/completed",
                payload=TurnCompletedNotification.model_validate(
                    {
                        "threadId": self.id,
                        "turn": {"id": "turn", "status": "completed", "items": [], "error": None},
                    }
                ),
            )

        async def interrupt(self):
            pass

    monkeypatch.setattr(openai_codex, "AsyncCodex", Codex)
    connection = {
        "id": "account",
        "revision": 1,
        "provider_id": "codex",
        "protocol": "codex",
        "auth_type": "client_login",
        "billing": "subscription",
    }
    route = {"model_id": "exact-model", "max_output_tokens": 1024}
    binding = {
        "id": "a" * 32,
        "run_id": "run",
        "connection_id": "account",
        "connection_revision": 1,
        "protocol": "codex",
        "model_id": "exact-model",
        "scope_fingerprint": "b" * 64,
        "settings_fingerprint": "c" * 64,
        "provider_session_id": Codex.id,
    }
    context = SimpleNamespace(
        account_home=tmp_path,
        operation_id="run",
        bridge=Bridge(),
        control=Control(),
        session_binding=binding,
    )
    result = await execute_codex(
        route,
        connection,
        {},
        context,
        "Inspect the approved source.",
        object,
        before_request=lambda *args, **kwargs: "reservation",
        on_response=lambda *args: None,
    )
    assert methods == [Codex.id] and result["usage"]["session_id"] == Codex.id
    assert any(
        kind == "assistant_text_delta" and data["text"] == "Ready for review."
        for kind, data in events
    )


def test_resumed_codex_usage_does_not_charge_prior_thread_tokens_again():
    last = SimpleNamespace(
        input_tokens=5,
        output_tokens=2,
        cached_input_tokens=3,
        reasoning_output_tokens=1,
        total_tokens=7,
    )
    total = SimpleNamespace(
        input_tokens=1000,
        output_tokens=500,
        cached_input_tokens=500,
        reasoning_output_tokens=10,
        total_tokens=1500,
    )
    usage = {}
    record_token_usage(usage, SimpleNamespace(last=last, total=total), resuming=True)
    record_token_usage(usage, SimpleNamespace(last=last, total=total), resuming=True)
    assert usage["input_tokens"] == 10 and usage["output_tokens"] == 4 and usage["requests"] == 2


def test_grok_resume_never_matches_another_sessions_title(tmp_path):
    route = {"model_id": "exact-model"}
    with pytest.raises(ValueError, match="UUID"):
        headless_command(
            ["grok"],
            route,
            2,
            tmp_path / "agent.md",
            tmp_path / "prompt",
            resume_session_id="Another Tender",
        )
    identifier = "12345678-1234-1234-1234-123456789abc"
    command = headless_command(
        ["grok"], route, 2, tmp_path / "agent.md", tmp_path / "prompt", resume_session_id=identifier
    )
    assert command[-2:] == ["--resume", identifier]
    assert command[command.index("--disallowed-tools") + 1] == "Agent"
