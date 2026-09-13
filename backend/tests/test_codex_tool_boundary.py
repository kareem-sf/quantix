"""Codex's own MCP resource listing is tolerated; any other outside tool still stops the run."""

import sys
from contextlib import asynccontextmanager
from pathlib import Path
from types import SimpleNamespace

import pytest

sys.path.insert(0, str(Path(__file__).parents[1] / "ai_worker"))

from quantix_ai_worker.codex import execute_codex  # noqa: E402


def _fake_codex(tmp_path, items):
    from openai_codex.types import TurnCompletedNotification

    class Codex:
        id = "native-boundary-session"

        def __init__(self, config):
            pass

        async def __aenter__(self):
            return self

        async def close(self):
            pass

        async def account(self):
            return SimpleNamespace(model_dump=lambda **kwargs: {"account": {"type": "chatgpt"}})

        async def thread_start(self, **kwargs):
            return self

        async def turn(self, instruction, **kwargs):
            return self

        async def stream(self):
            for item in items:
                yield SimpleNamespace(method="item/started", payload=SimpleNamespace(item=item))
            yield SimpleNamespace(
                method="turn/completed",
                payload=TurnCompletedNotification.model_validate(
                    {"threadId": self.id, "turn": {"id": "turn", "status": "completed", "items": [], "error": None}}
                ),
            )

        async def interrupt(self):
            pass

    return Codex


async def _run(tmp_path, monkeypatch, items):
    import openai_codex

    events = []

    class Control:
        async def event(self, kind, message, data):
            events.append((kind, data))

    class Bridge:
        names = ["search_sources", "quantix_submit_result"]
        url = "http://127.0.0.1:12345/mcp"
        token = "source-only-fixture"
        closed = False

        @asynccontextmanager
        async def serve(self):
            yield self

        async def result(self, text=None):
            return {"summary": "Ready for review."}

    monkeypatch.setattr(openai_codex, "AsyncCodex", _fake_codex(tmp_path, items))
    connection = {"id": "account", "revision": 1, "provider_id": "codex", "protocol": "codex",
                  "auth_type": "client_login", "billing": "subscription"}
    route = {"model_id": "exact-model", "max_output_tokens": 1024}
    context = SimpleNamespace(account_home=tmp_path, operation_id="run", bridge=Bridge(),
                              control=Control(), session_binding=None)
    result = await execute_codex(route, connection, {}, context, "Inspect the approved source.", object,
                                 before_request=lambda *args, **kwargs: "reservation",
                                 on_response=lambda *args: None)
    return result, events


def _mcp(server, tool):
    return SimpleNamespace(type="mcpToolCall", server=server, tool=tool)


@pytest.mark.asyncio
async def test_resource_listing_helper_does_not_end_the_run(tmp_path, monkeypatch):
    result, events = await _run(tmp_path, monkeypatch, [
        _mcp("", "list_mcp_resources"),
        _mcp("quantix", "search_sources"),
    ])
    assert result["output"] == {"summary": "Ready for review."}
    tools = [data["tool"] for kind, data in events if kind == "runtime_tool_activity"]
    assert tools == ["search_sources"]


@pytest.mark.asyncio
@pytest.mark.parametrize("item", [_mcp("codex_apps", "search_sources"), _mcp("quantix", "read_mcp_resource"),
                                  _mcp("quantix", "unapproved_tool")])
async def test_other_outside_tools_still_stop_and_are_named(tmp_path, monkeypatch, item):
    from quantix_ai_worker.common import RuntimeUnavailable

    with pytest.raises(RuntimeUnavailable, match=f"'{item.tool}' on server '{item.server}'"):
        await _run(tmp_path, monkeypatch, [item])
