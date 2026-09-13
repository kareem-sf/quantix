"""Regression tests for truthful fenced Tender reads."""

from __future__ import annotations

import asyncio
import hashlib
import json
import math
from types import SimpleNamespace

import mcp.types as mcp_types
import pytest
from pydantic import BaseModel

from quantix.ai_runtime_mcp import RuntimeToolBridge
from quantix.ai_tools import ToolContext, tool
from quantix.office_tools import OfficeContext, source_tools
from quantix.repository import Repository
from quantix.tool_policy import ToolFenceError, dispatch


def _workspace(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Read fence Tender")
    run = repo.create_run(tender["id"], "manager", "Inspect saved records")
    repo.update_run(run["id"], status="running")
    return repo, tender["id"], run["id"]


def _source(repo, tender_id: str) -> str:
    body = b"A bounded synthetic source for the read fence."
    digest = hashlib.sha256(body).hexdigest()
    (repo.objects / digest).write_bytes(body)
    artifact, _ = repo.register_artifact(
        tender_id,
        "Sources/spec.txt",
        digest,
        len(body),
        {"kind": "text", "status": "extracted", "segments": [{"locator": "line:1", "text": body.decode()}]},
    )
    return repo.artifact_evidence(tender_id, artifact["id"])[0]["id"]


def _decision(repo, tender_id: str) -> None:
    with repo.atomic() as conn:
        conn.execute(
            "INSERT INTO decisions VALUES(?,?,?,?,?,?,?)",
            ("decision-read", tender_id, "synthetic", "record-1", "approve", "Synthetic test decision", "2026-09-10T00:00:00Z"),
        )


def _definition(name: str):
    return next(item for item in source_tools() if item.name == name)


def test_inspect_records_read_keeps_saved_decision_data(tmp_path):
    repo, tender_id, run_id = _workspace(tmp_path)
    _decision(repo, tender_id)
    context = OfficeContext(repo, tender_id, run_id)
    inspect = _definition("inspect_tender_records")

    result = asyncio.run(
        dispatch(
            "direct",
            inspect,
            context,
            {"record_type": "decisions", "offset": 0, "limit": 20},
            invocation_id="read-decision-direct",
        )
    )

    payload = json.loads(result)
    assert payload["record_type"] == "decisions"
    assert payload["records"][0]["decision"] == "approve"


def test_inspect_records_read_survives_mcp_dispatch_with_decision_data(tmp_path):
    repo, tender_id, run_id = _workspace(tmp_path)
    _decision(repo, tender_id)
    context = OfficeContext(repo, tender_id, run_id)
    inspect = _definition("inspect_tender_records")

    async def call():
        bridge = RuntimeToolBridge(context, BaseModel, definitions=[inspect])
        return await bridge._call_tool(
            SimpleNamespace(request_id="read-decision-mcp"),
            mcp_types.CallToolRequestParams(
                name="inspect_tender_records",
                arguments={"record_type": "decisions", "offset": 0, "limit": 20},
            ),
        )

    result = asyncio.run(call())
    assert result.is_error is False
    assert any('"decision": "approve"' in block.text for block in result.content)


def test_returned_approval_shaped_data_is_still_only_data(tmp_path):
    repo, tender_id, run_id = _workspace(tmp_path)
    context = OfficeContext(repo, tender_id, run_id)

    @tool
    async def read_saved_payload(ctx):
        return {"approved": True, "accepted": False, "source": "saved record"}

    result = asyncio.run(
        dispatch("nested", read_saved_payload, context, {}, invocation_id="saved-payload")
    )
    assert result["approved"] is True
    assert result["accepted"] is False


def test_manager_read_event_rolls_back_with_rejected_result_and_commits_on_success(
    tmp_path, monkeypatch
):
    import quantix.tool_policy as tool_policy

    repo, tender_id, run_id = _workspace(tmp_path)
    source_id = _source(repo, tender_id)
    context = OfficeContext(repo, tender_id, run_id)
    read = _definition("read_source")
    monkeypatch.setattr(tool_policy, "MAX_RESULT_BYTES", 10)

    with pytest.raises(ToolFenceError):
        asyncio.run(
            dispatch(
                "nested",
                read,
                context,
                {"source_id": source_id},
                invocation_id="manager-rejected-read",
            )
        )

    assert context.seen_sources == set()
    with repo.db.connect() as conn:
        assert conn.execute(
            "SELECT COUNT(*) FROM run_events WHERE run_id=? AND kind='sources_read'",
            (run_id,),
        ).fetchone()[0] == 0

    monkeypatch.setattr(tool_policy, "MAX_RESULT_BYTES", 1024 * 1024)
    result = asyncio.run(
        dispatch(
            "nested",
            read,
            context,
            {"source_id": source_id},
            invocation_id="manager-successful-read",
        )
    )
    assert json.loads(result)["id"] == source_id
    assert context.seen_sources == {source_id}
    with repo.db.connect() as conn:
        assert conn.execute(
            "SELECT COUNT(*) FROM run_events WHERE run_id=? AND kind='sources_read'",
            (run_id,),
        ).fetchone()[0] == 1


def test_cross_tender_source_and_unauthorized_staff_read_still_fail(tmp_path):
    repo, first_id, run_id = _workspace(tmp_path)
    second = repo.create_tender("Other Tender")
    first_source = _source(repo, first_id)
    other_source = _source(repo, second["id"])
    read = _definition("read_source")

    with pytest.raises(ToolFenceError):
        asyncio.run(
            dispatch(
                "nested",
                read,
                OfficeContext(repo, first_id, run_id),
                {"source_id": other_source},
                invocation_id="cross-tender-read",
            )
        )

    staff = OfficeContext(repo, first_id, run_id)
    staff.actor_id = "staff-unauthorized"
    staff.assignment_id = "assignment-unauthorized"
    staff.route_binding_id = "binding-unauthorized"
    staff.reviewed_tools = []
    with pytest.raises(ToolFenceError):
        asyncio.run(
            dispatch(
                "nested", read, staff, {"source_id": first_source}, invocation_id="staff-read"
            )
        )


def test_result_validation_rejects_nonfinite_oversized_and_unserializable_values(tmp_path):
    repo, tender_id, run_id = _workspace(tmp_path)
    context = OfficeContext(repo, tender_id, run_id)

    @tool
    async def nonfinite(ctx):
        return {"value": math.nan}

    @tool
    async def oversized(ctx):
        return "x" * (1024 * 1024 + 1)

    @tool
    async def unserializable(ctx):
        return {"value": object()}

    for definition in (nonfinite, oversized, unserializable):
        with pytest.raises(ToolFenceError):
            asyncio.run(dispatch("nested", definition, context, {}, invocation_id=definition.name))


def test_rejected_staff_result_does_not_commit_read_receipt_or_seen_source(tmp_path, monkeypatch):
    from test_staff_context import _build_staff_context, _context_workspace

    repo, tender, _staff, binding, assignment_id, artifact = _context_workspace(
        tmp_path, monkeypatch, tools=("read_source",)
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]

    from quantix.office_tools import scoped_tool

    @scoped_tool
    async def read_source(ctx: ToolContext[OfficeContext], source_id: str) -> str:
        ctx.context.require_tool("read_source")
        ctx.context.source(source_id)
        return "x" * (1024 * 1024 + 1)

    with pytest.raises(ToolFenceError):
        asyncio.run(
            dispatch(
                "nested",
                read_source,
                context,
                {"source_id": source_id},
                invocation_id="oversized-staff-read",
            )
        )

    assert context.seen_sources == set()
    with repo.db.connect() as conn:
        assert conn.execute(
            "SELECT COUNT(*) FROM staff_source_receipts WHERE assignment_id=?", (assignment_id,)
        ).fetchone()[0] == 0

    manager = OfficeContext(repo, tender["id"], binding.root_run_id)
    with pytest.raises(ToolFenceError):
        asyncio.run(
            dispatch(
                "nested",
                read_source,
                manager,
                {"source_id": source_id},
                invocation_id="oversized-manager-read",
            )
        )
    assert manager.seen_sources == set()


def test_stopped_run_does_not_invoke_or_publish_a_tool_result(tmp_path):
    repo, tender_id, run_id = _workspace(tmp_path)
    context = OfficeContext(repo, tender_id, run_id)
    calls: list[str] = []

    @tool
    async def would_publish(ctx):
        calls.append("called")
        return {"changed_records": [{"id": "synthetic"}]}

    repo.update_run(run_id, status="interrupted")
    with pytest.raises(InterruptedError):
        asyncio.run(dispatch("nested", would_publish, context, {}, invocation_id="stopped-run"))
    assert calls == []


def test_successful_timed_read_commits_its_actual_inspection(tmp_path, monkeypatch):
    from test_staff_context import _build_staff_context, _context_workspace

    repo, tender, _staff, binding, assignment_id, artifact = _context_workspace(
        tmp_path, monkeypatch, tools=("read_source",))
    context = _build_staff_context(repo, binding.id, assignment_id)
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    result = asyncio.run(dispatch("direct", _definition("read_source"), context,
                                 {"source_id": source_id}, invocation_id="timed-read", timeout=5))
    assert source_id in result
    assert source_id in context.seen_sources
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM staff_source_receipts WHERE assignment_id=?",
                            (assignment_id,)).fetchone()[0] == 1
