from __future__ import annotations

from types import SimpleNamespace

import mcp.types as types
import pytest
from pydantic import BaseModel, ConfigDict, ValidationError

from quantix.ai_api_engine import LocalToolBridge
from quantix.ai_api_errors import DirectAPIError
from quantix.ai_runtime_mcp import RuntimeToolBridge
from quantix.ai_tools import ToolContext, tool


class Draft(BaseModel):
    model_config = ConfigDict(extra="forbid")

    label: str
    options: dict[str, int]


class Result(BaseModel):
    value: str = "ok"


class RecordingRepo:
    def __init__(self):
        self.events = []

    def get_run(self, _run_id):
        return {"status": "running"}

    def event(self, run_id, kind, message, data=None):
        self.events.append({"run_id": run_id, "kind": kind, "message": message, "data": data})

    def update_active_run_detail(self, _run_id, _detail):
        return True


def _read_tool(calls: list[ToolContext]):
    @tool
    async def read_value(ctx: ToolContext[object], value: str) -> str:
        calls.append(ctx)
        return value

    return read_value


def _mutation_tool(calls: list[ToolContext], side_effects: list[dict]):
    @tool(read_only=False, idempotent=True, requires_invocation_id=True)
    async def save_draft(ctx: ToolContext[object], draft: Draft) -> str:
        calls.append(ctx)
        side_effects.append(draft)
        return draft["label"]

    return save_draft


@pytest.mark.asyncio
async def test_tool_definition_metadata_schema_and_context_identity_are_server_only():
    calls: list[ToolContext] = []
    side_effects: list[dict] = []
    definition = _mutation_tool(calls, side_effects)

    assert definition.read_only is False
    assert definition.idempotent is True
    assert definition.requires_invocation_id is True
    assert "invocation_id" not in definition.parameters["properties"]
    assert definition.parameters["additionalProperties"] is False
    assert definition.parameters["properties"]["draft"]["$ref"].startswith("#/$defs/")
    with pytest.raises(ValidationError):
        definition.argument_model.model_validate(
            {"draft": {"label": "x", "options": {}}, "invocation_id": "model-value"}
        )

    await definition.invoke(None, {"draft": {"label": "x", "options": {}}}, invocation_id="trusted")
    assert side_effects == [{"label": "x", "options": {}}]
    assert calls[0].context is None
    assert calls[0].invocation_id == "trusted"


@pytest.mark.asyncio
async def test_mutation_definition_rejects_missing_identity_before_side_effect():
    calls: list[ToolContext] = []
    side_effects: list[dict] = []
    definition = _mutation_tool(calls, side_effects)

    with pytest.raises(ValueError, match="trusted invocation identity"):
        await definition.invoke(None, {"draft": {"label": "x", "options": {}}})

    assert calls == []
    assert side_effects == []


@pytest.mark.asyncio
async def test_read_only_definition_still_works_without_identity():
    calls: list[ToolContext] = []
    definition = _read_tool(calls)

    assert definition.read_only is True
    assert definition.idempotent is False
    assert definition.requires_invocation_id is False
    assert await definition.invoke(None, {"value": "read"}) == "read"
    assert calls[0].invocation_id is None


@pytest.mark.asyncio
async def test_local_bridge_uses_pydantic_ai_tool_call_id_in_opaque_session_identity():
    calls: list[ToolContext] = []
    side_effects: list[dict] = []
    definition = _mutation_tool(calls, side_effects)
    bridge = LocalToolBridge(None, Result, [definition])
    invoke = bridge.adapt(definition)
    arguments = {"draft": {"label": "x", "options": {"count": 1}}}

    await invoke(SimpleNamespace(tool_call_id="provider-call-1"), **arguments)
    first = calls[-1].invocation_id
    await invoke(SimpleNamespace(tool_call_id="provider-call-1"), **arguments)
    second = calls[-1].invocation_id

    assert first == second
    assert first is not None
    assert "provider-call-1" not in first
    assert len(first) <= 160
    await invoke(SimpleNamespace(tool_call_id="provider-call-2"), **arguments)
    assert calls[-1].invocation_id != first

    other_bridge = LocalToolBridge(None, Result, [definition])
    await other_bridge.adapt(definition)(
        SimpleNamespace(tool_call_id="provider-call-1"), **arguments
    )
    assert calls[-1].invocation_id != first


@pytest.mark.asyncio
async def test_local_bridge_rejects_missing_or_malformed_sdk_identity_without_side_effect():
    calls: list[ToolContext] = []
    side_effects: list[dict] = []
    definition = _mutation_tool(calls, side_effects)
    bridge = LocalToolBridge(None, Result, [definition], max_calls=2)
    invoke = bridge.adapt(definition)
    arguments = {"draft": {"label": "x", "options": {}}}

    for index, ctx in enumerate(
        (
        SimpleNamespace(tool_call_id=None),
        SimpleNamespace(tool_call_id=""),
        SimpleNamespace(tool_call_id=42),
        )
    ):
        if index < 2:
            with pytest.raises(ValueError, match="trusted invocation identity"):
                await invoke(ctx, **arguments)
        else:
            with pytest.raises(DirectAPIError, match="tool-call limit"):
                await invoke(ctx, **arguments)

    assert calls == []
    assert side_effects == []
    assert bridge.calls == 3


@pytest.mark.asyncio
async def test_local_bridge_closed_session_blocks_read_and_mutation_calls():
    calls: list[ToolContext] = []
    side_effects: list[dict] = []
    read = _read_tool(calls)
    mutation = _mutation_tool(calls, side_effects)
    bridge = LocalToolBridge(None, Result, [read, mutation])
    bridge.closed = True

    for definition, kwargs in (
        (read, {"value": "read"}),
        (mutation, {"draft": {"label": "x", "options": {}}}),
    ):
        with pytest.raises(InterruptedError, match="session has ended"):
            await bridge.adapt(definition)(SimpleNamespace(tool_call_id="call-1"), **kwargs)

    assert side_effects == []


@pytest.mark.asyncio
async def test_local_mutation_activity_is_recorded_and_missing_identity_writes_no_event(tmp_path):
    calls: list[ToolContext] = []
    side_effects: list[dict] = []
    definition = _mutation_tool(calls, side_effects)
    from quantix.repository import Repository
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic tool activity")
    run = repo.create_run(tender["id"], "manager")
    context = SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=run["id"])
    bridge = LocalToolBridge(context, Result, [definition])

    await bridge.adapt(definition)(
        SimpleNamespace(tool_call_id="provider-call-1"),
        draft={"label": "x", "options": {}},
    )
    before = repo.run_events(run["id"])
    assert [row["data"]["phase"] for row in before] == ["prepared", "started", "completed"]
    assert all(row["data"]["tool"] == "save_draft" for row in before)

    with pytest.raises(ValueError, match="trusted invocation identity"):
        await bridge.adapt(definition)(
            SimpleNamespace(tool_call_id=None),
            draft={"label": "x", "options": {}},
        )
    assert repo.run_events(run["id"]) == before


@pytest.mark.asyncio
async def test_mcp_annotations_and_request_id_identity_are_truthful():
    read_calls: list[ToolContext] = []
    mutation_calls: list[ToolContext] = []
    side_effects: list[dict] = []
    read = _read_tool(read_calls)
    mutation = _mutation_tool(mutation_calls, side_effects)
    bridge = RuntimeToolBridge(None, Result, definitions=[read, mutation])

    listed = await bridge._list_tools(SimpleNamespace(request_id="list-1"), None)
    metadata = {item.name: item.annotations for item in listed.tools}
    assert metadata[read.name].read_only_hint is True
    assert metadata[read.name].idempotent_hint is False
    assert metadata[read.name].destructive_hint is False
    assert metadata[read.name].open_world_hint is False
    assert metadata[mutation.name].read_only_hint is False
    assert metadata[mutation.name].idempotent_hint is True
    assert metadata[mutation.name].destructive_hint is False
    assert metadata[mutation.name].open_world_hint is False

    params = types.CallToolRequestParams(
        name=mutation.name, arguments={"draft": {"label": "x", "options": {}}}
    )
    first_result = await bridge._call_tool(SimpleNamespace(request_id="mcp-call-1"), params)
    assert first_result.is_error is False
    first = mutation_calls[-1].invocation_id
    second_result = await bridge._call_tool(SimpleNamespace(request_id="mcp-call-1"), params)
    assert second_result.is_error is False
    assert mutation_calls[-1].invocation_id == first
    assert first is not None
    assert "mcp-call-1" not in first

    await bridge._call_tool(SimpleNamespace(request_id="mcp-call-2"), params)
    assert mutation_calls[-1].invocation_id != first
    assert side_effects == [
        {"label": "x", "options": {}},
        {"label": "x", "options": {}},
        {"label": "x", "options": {}},
    ]

    read_result = await bridge._call_tool(
        SimpleNamespace(request_id=None),
        types.CallToolRequestParams(name=read.name, arguments={"value": "read"}),
    )
    assert read_result.is_error is False
    assert read_calls[-1].invocation_id is None


@pytest.mark.asyncio
async def test_mcp_missing_or_malformed_request_id_blocks_mutation_without_side_effect():
    calls: list[ToolContext] = []
    side_effects: list[dict] = []
    definition = _mutation_tool(calls, side_effects)
    bridge = RuntimeToolBridge(None, Result, definitions=[definition], max_calls=2)
    params = types.CallToolRequestParams(
        name=definition.name, arguments={"draft": {"label": "x", "options": {}}}
    )

    for request_id in (None, "", True, object()):
        result = await bridge._call_tool(SimpleNamespace(request_id=request_id), params)
        assert result.is_error is True

    assert calls == []
    assert side_effects == []
    assert bridge.calls == 3
    assert bridge.failure is not None


@pytest.mark.asyncio
async def test_mcp_mutation_activity_is_recorded_and_missing_identity_writes_no_event(tmp_path):
    calls: list[ToolContext] = []
    side_effects: list[dict] = []
    definition = _mutation_tool(calls, side_effects)
    from quantix.repository import Repository
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic MCP activity")
    run = repo.create_run(tender["id"], "manager")
    context = SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=run["id"])
    bridge = RuntimeToolBridge(context, Result, definitions=[definition])
    params = types.CallToolRequestParams(
        name=definition.name, arguments={"draft": {"label": "x", "options": {}}}
    )

    result = await bridge._call_tool(SimpleNamespace(request_id="mcp-call-1"), params)
    assert result.is_error is False
    before = repo.run_events(run["id"])
    assert [row["data"]["phase"] for row in before] == ["prepared", "started", "completed"]
    assert all(row["data"]["tool"] == "save_draft" for row in before)

    result = await bridge._call_tool(SimpleNamespace(request_id=None), params)
    assert result.is_error is True
    assert repo.run_events(run["id"]) == before


def test_opaque_identity_keeps_jsonrpc_string_and_integer_ids_distinct():
    from quantix.ai_tools import trusted_invocation_id

    assert trusted_invocation_id("namespace", "tool", "1") != trusted_invocation_id(
        "namespace", "tool", 1
    )


@pytest.mark.asyncio
async def test_mcp_closed_session_blocks_invocation():
    calls: list[ToolContext] = []
    definition = _read_tool(calls)
    bridge = RuntimeToolBridge(None, Result, definitions=[definition])
    bridge.closed = True

    result = await bridge._call_tool(
        SimpleNamespace(request_id="mcp-call-1"),
        types.CallToolRequestParams(name=definition.name, arguments={"value": "read"}),
    )
    assert result.is_error is True
    assert calls == []
