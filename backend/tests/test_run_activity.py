"""Synthetic execution history: exact details, scoped recovery and invoke outcomes."""

import asyncio
import json
import sqlite3
from types import SimpleNamespace

import pytest

from quantix.repository import Repository


def workspace(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic activity Tender")
    run = repo.create_run(tender["id"], "manager", "Inspect the synthetic BOQ")
    return repo, tender, run, SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=run["id"])


def test_complete_payload_is_paged_and_secrets_never_persist(tmp_path):
    from quantix.run_activity import ActivityRecorder
    from quantix.run_activity_reader import RunActivityService

    repo, tender, run, context = workspace(tmp_path)
    recorder = ActivityRecorder(context)
    full = "Concrete quantity 12 m³\n" * 6000
    operation = recorder.start(
        "tool",
        "Reading quantities",
        {"inputs": {"api_key": "private-canary", "source_id": "source-a"}},
        tool="read_source",
    )
    recorder.record(
        operation,
        "tool",
        "completed",
        "Read quantities",
        {"outputs": full, "signature": "signature-canary"},
    )
    service = RunActivityService(repo)
    page = service.page(tender["id"], run["id"])
    assert len(page.items) == 2
    assert page.items[0].operation_id == page.items[1].operation_id
    detail = service.detail(tender["id"], run["id"], page.items[-1].event_id, limit=1000)
    chunks = [detail.text]
    while detail.has_more:
        detail = service.detail(
            tender["id"],
            run["id"],
            page.items[-1].event_id,
            offset=detail.offset + len(detail.text),
            limit=1000,
        )
        chunks.append(detail.text)
    assert json.loads("".join(chunks))["outputs"] == full
    with repo.db.connect() as conn:
        stored = str([tuple(row) for row in conn.execute("SELECT * FROM run_activity_payloads")])
        stored += str([tuple(row) for row in conn.execute("SELECT * FROM run_events")])
        assert "private-canary" not in stored and "signature-canary" not in stored
    assert detail.redacted


def test_model_outcome_pointer_opens_returned_content_not_empty_terminal_event(tmp_path):
    from quantix.run_activity import ActivityRecorder
    from quantix.run_activity_reader import RunActivityService

    repo, tender, run, context = workspace(tmp_path)
    recorder = ActivityRecorder(context)
    operation = recorder.start("model_request", "Request", {"messages": ["Synthetic input"]})
    recorder.record(
        operation,
        "model_output",
        "observed",
        "Provider supplied output",
        {"parts": [{"text": "Synthetic output"}]},
    )
    recorder.record(operation, "model_request", "completed", "Request completed")
    service = RunActivityService(repo)
    rows = service.page(tender["id"], run["id"]).items
    detail = service.detail(tender["id"], run["id"], rows[-1].event_id)
    assert detail.output_event_id == rows[1].event_id
    assert detail.input_event_id == rows[0].event_id


def test_pages_resume_after_anchor_and_reject_cross_tender_cursors(tmp_path):
    from quantix.run_activity import ActivityRecorder
    from quantix.run_activity_reader import RunActivityService

    repo, tender, run, context = workspace(tmp_path)
    recorder, service = ActivityRecorder(context), RunActivityService(repo)
    for index in range(7):
        recorder.start("tool", f"Step {index}", {"index": index})
    page = service.page(tender["id"], run["id"], limit=3)
    assert [item.message for item in page.items] == ["Step 4", "Step 5", "Step 6"]
    older = service.page(tender["id"], run["id"], before=page.before_cursor, limit=3)
    assert [item.message for item in older.items] == ["Step 1", "Step 2", "Step 3"]
    assert service.page(tender["id"], run["id"], after=page.cursor).items == []
    recorder.start("tool", "Next step")
    assert service.page(tender["id"], run["id"], after=page.cursor).items[0].message == "Next step"
    other = repo.create_tender("Other")
    with pytest.raises(KeyError):
        service.page(other["id"], run["id"], after=page.cursor)
    other_run = repo.create_run(other["id"], "manager")
    with pytest.raises(ValueError):
        service.page(other["id"], other_run["id"], after=page.cursor)
    with repo.atomic() as conn:
        conn.execute("DELETE FROM run_events WHERE id=?", (page.items[-1].event_id,))
    assert service.page(tender["id"], run["id"], after=page.cursor).reset_required


def test_legacy_events_have_truthful_incomplete_details(tmp_path):
    from quantix.run_activity_reader import RunActivityService

    repo, tender, run, _ = workspace(tmp_path)
    repo.event(
        run["id"],
        "tool_started",
        "Reading a source",
        {"tool": "read_source", "api_key": "old-secret"},
    )
    service = RunActivityService(repo)
    page = service.page(tender["id"], run["id"])
    assert page.items[0].capture_status == "historical"
    detail = service.detail(tender["id"], run["id"], page.items[0].event_id)
    assert "old-secret" not in detail.text
    assert "inputs" in detail.unavailable_fields


def test_column_headers_are_preserved_while_authentication_headers_are_scrubbed():
    from quantix.activity_privacy import sanitize

    value, redacted, _ = sanitize(
        {
            "headers": ["Quantity", "Unit"],
            "http": {
                "headers": {
                    "Authorization": "Bearer private-token",
                    "Content-Type": "application/json",
                }
            },
        }
    )
    assert value["headers"] == ["Quantity", "Unit"]
    assert value["http"]["headers"]["Content-Type"] == "application/json"
    assert "private-token" not in str(value)
    assert redacted


@pytest.mark.asyncio
async def test_nested_and_concurrent_tools_keep_parent_identity_and_outcomes(tmp_path):
    from quantix.ai_tools import tool
    from quantix.office_tools import OfficeContext
    from quantix.run_activity_reader import RunActivityService
    from quantix.tool_policy import dispatch

    repo, tender, run, _ = workspace(tmp_path)
    context = OfficeContext(repo, tender["id"], run["id"])

    @tool
    async def child(ctx, value: int):
        return {"quantity": value}

    @tool
    async def parent(ctx, value: int):
        return await dispatch("nested", child, ctx.context, {"value": value})

    await asyncio.gather(
        *(dispatch("direct", parent, context, {"value": index}) for index in (1, 2))
    )
    rows = RunActivityService(repo).page(tender["id"], run["id"]).items
    started = [item for item in rows if item.phase == "started"]
    assert len(started) == 4
    parents = {item.operation_id for item in started if item.tool == "parent"}
    assert {item.parent_operation_id for item in started if item.tool == "child"} == parents
    assert len([item for item in rows if item.phase == "completed"]) == 4


@pytest.mark.asyncio
async def test_capture_failure_prevents_tool_dispatch_and_next_admission(tmp_path, monkeypatch):
    from quantix.ai_tools import tool
    from quantix.run_activity import ActivityRecorder, ActivityRecordingError
    from quantix.tool_policy import dispatch

    repo, _tender, _run, context = workspace(tmp_path)
    calls = []

    @tool
    async def read(ctx):
        calls.append(True)
        return "ok"

    def fail(*args, **kwargs):
        raise sqlite3.OperationalError("disk full")

    monkeypatch.setattr(ActivityRecorder, "_append", fail)
    with pytest.raises(ActivityRecordingError):
        await dispatch("direct", read, context, {})
    assert calls == []


@pytest.mark.asyncio
async def test_cancelled_and_blocked_calls_are_not_completed(tmp_path):
    from quantix.ai_tools import tool
    from quantix.run_activity_reader import RunActivityService
    from quantix.tool_policy import ToolFenceError, dispatch

    repo, tender, run, context = workspace(tmp_path)

    @tool
    async def cancelled(ctx):
        raise asyncio.CancelledError()

    @tool
    async def refused(ctx):
        raise ValueError("Source is outside reviewed scope")

    with pytest.raises(asyncio.CancelledError):
        await dispatch("direct", cancelled, context, {})
    with pytest.raises(ToolFenceError):
        await dispatch("direct", refused, context, {})
    phases = [item.phase for item in RunActivityService(repo).page(tender["id"], run["id"]).items]
    assert "interrupted" in phases and "blocked" in phases and "completed" not in phases


def test_capture_failure_survives_outer_transaction_rollback(tmp_path, monkeypatch):
    from quantix.run_activity import ActivityRecorder, ActivityRecordingError

    repo, tender, run, context = workspace(tmp_path)
    original = ActivityRecorder._append

    def fail(*args, **kwargs):
        raise sqlite3.OperationalError("disk full")

    monkeypatch.setattr(ActivityRecorder, "_append", fail)
    with pytest.raises(ActivityRecordingError), repo.atomic():
        ActivityRecorder(context).start("tool", "Preparing work")
    monkeypatch.setattr(ActivityRecorder, "_append", original)
    fresh = SimpleNamespace(repo=Repository(tmp_path), tender_id=tender["id"], run_id=run["id"])
    with pytest.raises(ActivityRecordingError):
        ActivityRecorder(fresh).start("tool", "Must not dispatch")


@pytest.mark.asyncio
async def test_result_returned_after_stop_is_retained_without_publication(tmp_path):
    from quantix.ai_tools import tool
    from quantix.run_activity_reader import RunActivityService
    from quantix.tool_policy import dispatch

    repo, tender, run, context = workspace(tmp_path)

    @tool
    async def returns_after_stop(ctx):
        repo.update_run(run["id"], status="cancelled")
        return {"quantity": 42}

    with pytest.raises(InterruptedError):
        await dispatch("direct", returns_after_stop, context, {})
    service = RunActivityService(repo)
    last = service.page(tender["id"], run["id"]).items[-1]
    assert last.phase == "interrupted"
    assert json.loads(service.detail(tender["id"], run["id"], last.event_id).text)["outputs"] == {
        "quantity": 42
    }


def test_legacy_filters_match_their_visible_category_and_phase(tmp_path):
    from quantix.run_activity_reader import RunActivityService

    repo, tender, run, _ = workspace(tmp_path)
    repo.event(run["id"], "tool_failed", "Tool failed")
    repo.event(run["id"], "assignment_failed", "Assignment failed")
    repo.event(run["id"], "source_inspected", "Source inspected")
    service = RunActivityService(repo)
    assert len(service.page(tender["id"], run["id"], errors_only=True).items) == 2
    assert len(service.page(tender["id"], run["id"], category="source").items) == 1
    assert service.page(tender["id"], run["id"], actor_id="manager").items == []


def test_stop_and_recovery_revoke_authority_even_if_activity_capture_fails(tmp_path, monkeypatch):
    from test_catalog_authority import configured_office

    from quantix.jobs import JobManager
    from quantix.run_activity import ActivityRecorder
    from quantix.settings import SettingsService
    from quantix.team import TeamService
    from quantix.team_models import StaffDraft

    repo, tender, *_, route = configured_office(tmp_path, monkeypatch)
    run = repo.create_run(tender["id"], "manager", "Capture stop")
    team = TeamService(repo)
    member = team.hire(tender["id"], run["id"], StaffDraft(
        name="Samir Haddad", role="Quantity Surveyor", specialisms=["BOQ"],
        background="Pricing.", working_style="Careful."))
    assignment = team.assign(tender["id"], run["id"], member.id, title="Check", brief="Check.",
                             expected_result="Result", source_ids=[], route=route)
    jobs = JobManager(repo, SettingsService(repo))

    def fail(*args, **kwargs):
        raise sqlite3.OperationalError("disk full")

    monkeypatch.setattr(ActivityRecorder, "_append", fail)
    jobs._stop(repo.get_run(run["id"]), None)
    assert repo.get_run(run["id"])["status"] == "cancelled"
    assert team.get(tender["id"], assignment.id).status == "cancelled"
    repo.recover_interrupted_runs()


@pytest.mark.asyncio
async def test_native_provider_tools_are_visible_before_the_model_response_finishes(tmp_path):
    from pydantic import BaseModel
    from pydantic_ai.messages import (
        NativeToolCallPart,
        NativeToolReturnPart,
        PartEndEvent,
        PartStartEvent,
    )

    from quantix.ai_api_engine import DraftStreamEvents
    from quantix.run_activity import ActivityRecorder
    from quantix.run_activity_reader import RunActivityService

    repo, tender, run, context = workspace(tmp_path)
    request = ActivityRecorder(context).start(
        "model_request", "Sending request", provider="openai", model="synthetic"
    )

    class Output(BaseModel):
        summary: str

    call = NativeToolCallPart(
        "web_search", {"query": "synthetic cement"}, "native-1", provider_name="openai"
    )
    returned = NativeToolReturnPart(
        "web_search", {"results": ["synthetic result"]}, "native-1", provider_name="openai"
    )

    async def stream():
        yield PartStartEvent(index=0, part=call)
        yield PartEndEvent(index=0, part=call)
        rows = RunActivityService(repo).page(tender["id"], run["id"]).items
        assert any(row.tool == "web_search" for row in rows)
        yield PartStartEvent(index=1, part=returned)
        yield PartEndEvent(index=1, part=returned)

    await DraftStreamEvents(
        context,
        {"provider_id": "openai", "protocol": "openai_responses"},
        Output,
        request_operation=lambda: request,
    ).handle(None, stream())
    rows = RunActivityService(repo).page(tender["id"], run["id"]).items
    native = [row for row in rows if row.category == "provider_tool"]
    assert len({row.operation_id for row in native}) == 1
    assert native[-1].phase == "completed"


@pytest.mark.parametrize(
    "outcome,phase", [("failed", "failed"), ("denied", "blocked"), ("interrupted", "interrupted")]
)
def test_native_provider_tool_failure_outcomes_remain_truthful(tmp_path, outcome, phase):
    from pydantic_ai.messages import NativeToolReturnPart

    from quantix.provider_tool_activity import observe_provider_tool
    from quantix.run_activity import ActivityRecorder
    from quantix.run_activity_reader import RunActivityService

    repo, tender, run, context = workspace(tmp_path)
    recorder = ActivityRecorder(context)
    request = recorder.start("model_request", "Synthetic provider request")
    returned = NativeToolReturnPart(
        "web_search", {"error": "Synthetic provider failure"}, "call-1", outcome=outcome
    )
    observe_provider_tool(recorder, request, returned, {}, set())
    service = RunActivityService(repo)
    row = service.page(tender["id"], run["id"]).items[-1]
    assert row.phase == phase
    detail = service.detail(tender["id"], run["id"], row.event_id)
    assert json.loads(detail.text)["provider_outcome"] == outcome
