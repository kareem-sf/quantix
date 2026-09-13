"""Concurrent ready branches, per-account locks, reservations, and stops."""

from __future__ import annotations

import asyncio
import threading

import pytest
from test_staff_routing import _ready_binding_workspace, _work_order

from quantix.ai_connections import AIConnectionService
from quantix.execution_context import OfficeExecutionIdentity
from quantix.manager_profile import ManagerProfileService
from quantix.office_concurrency import _locked_connection_id, drain_queued_assignments
from quantix.office_types import OfficeOutput
from quantix.resource_leases import ResourceLeaseService
from quantix.staff_assignments import StaffAssignmentService
from quantix.staff_models import ManagerCreationContext
from quantix.staff_runtime_models import StaffCompletedDraft, StaffProviderOutput
from quantix.staff_store import StaffStore


def _scope(tender_id: str, root_run_id: str) -> OfficeExecutionIdentity:
    return OfficeExecutionIdentity(
        tender_id=tender_id,
        actor_kind="engineer",
        actor_id="engineer",
        root_run_id=root_run_id,
        budget_scope_id=root_run_id,
        assignment_id=None,
        profile_version=None,
        route_binding_id=None,
        instruction_revision_id=None,
        grant_fingerprint=None,
        ownership_epoch=None,
        trusted_invocation_id=None,
    )


def _workspace(tmp_path, monkeypatch, **overrides):
    params = {
        "requested_tool_ids": ["read_source"],
        "envelope_tools": ("read_source",),
        "max_staff": 2,
        "max_assignments": 10,
        "max_depth": 1,
        "max_concurrency": 2,
    }
    params.update(overrides)
    repo, tender, _conn, _model, _route, envelope, routing, staff, root_context, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch, **params)
    )
    return repo, tender, envelope, routing, staff, root_context


def _profile(second: bool = False) -> dict:
    return {
        "display_name": "Second Colleague" if second else "Unfamiliar Colleague",
        "role": "Concrete package synthesiser",
        "title": "Evidence Specialist",
        "specialisms": ["Package review"],
        "persona": "A bounded professional profile.",
        "personality": {
            "description": "Evidence-led.",
            "traits": ["careful"],
            "communication_style": "Plain language.",
            "problem_solving_style": "Use bounded checks.",
            "collaboration_style": "Share exact sources.",
            "uncertainty_handling": "Label gaps.",
            "initiative": "Suggest the next safe step.",
            "explanation_style": "Lead with the answer.",
            "language_preferences": ["English"],
            "working_habits": ["Keep citations"],
        },
        "responsibilities": ["Compare source evidence"],
        "objectives": ["List material gaps"],
        "methods": ["Read current sources"],
        "deliverables": ["A source-backed gap list"],
        "success_criteria": ["Every gap has a source"],
        "context_needs": ["Current Tender sources"],
        "requested_tool_ids": ["read_source"],
        "creation_reason": "Created for this Tender task.",
    }


def _queue_pair(repo, tender_id, routing, envelope, root_context, staff):
    assignments = StaffAssignmentService(repo)
    first_binding = routing.bind(
        root_context,
        staff.staff.id,
        staff.work_order.id,
        envelope.route_options[0].id,
        "bind-first",
    )
    first = assignments.queue(root_context, first_binding.id, "queue-first")
    manager = ManagerProfileService(repo)
    planning = repo.create_run(tender_id, "conversation", "Create second colleague")
    second_staff = StaffStore(repo).create_generated(
        ManagerCreationContext(
            tender_id, planning["id"], manager.get().version, root_context.scope_id
        ),
        _profile(second=True),
        _work_order(),
        "create-second",
    )
    second_binding = routing.bind(
        root_context,
        second_staff.staff.id,
        second_staff.work_order.id,
        envelope.route_options[0].id,
        "bind-second",
    )
    second = assignments.queue(root_context, second_binding.id, "queue-second")
    return first, second


def _provider(summary: str, tracker: dict, fail_on: str | None = None):
    async def execute(route, connection, credentials, context, prompt, output_type, **options):
        with tracker["guard"]:
            tracker["in_flight"] += 1
            tracker["peak"] = max(tracker["peak"], tracker["in_flight"])
        try:
            reservation = await options["before_request"](200, 200)
            barrier: asyncio.Barrier | None = tracker.get("barrier")
            if barrier is not None:
                await asyncio.wait_for(barrier.wait(), timeout=60)
            usage = {
                "requests": 1,
                "input_tokens": 200,
                "output_tokens": 100,
                "web_search_calls": 0,
                "usage_complete": True,
            }
            await options["on_response"](usage, reservation)
            if fail_on is not None and context.assignment_id == fail_on:
                raise RuntimeError("Synthetic sibling failure.")
            return {
                "output": StaffProviderOutput(
                    result=StaffCompletedDraft(output=OfficeOutput(summary=summary))
                ),
                "usage": usage,
                "web_sources": [],
            }
        finally:
            with tracker["guard"]:
                tracker["in_flight"] -= 1

    return execute


@pytest.mark.asyncio
async def test_branches_overlap_up_to_the_reviewed_cap(tmp_path, monkeypatch):
    repo, tender, envelope, routing, staff, root_context = _workspace(tmp_path, monkeypatch)
    first, _second = _queue_pair(repo, tender["id"], routing, envelope, root_context, staff)
    tracker = {
        "guard": threading.Lock(),
        "in_flight": 0,
        "peak": 0,
        "barrier": asyncio.Barrier(2),
    }
    provider = _provider("Branch observation.", tracker)
    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    monkeypatch.setattr("quantix.staff_runtime.execute_api", provider)

    sink: list = []
    await drain_queued_assignments(
        repo,
        tender["id"],
        first.root_run_id,
        max_concurrency=2,
        max_requests=12,
        sink=sink,
    )
    assert tracker["peak"] == 2
    assert sorted(item.assignment.status for item in sink) == ["completed", "completed"]
    with repo.db.connect() as conn:
        usage_roots = {
            row[0]
            for row in conn.execute(
                "SELECT DISTINCT run_id FROM ai_usage WHERE tender_id=?", (tender["id"],)
            )
        }
    assert usage_roots == {first.root_run_id}


@pytest.mark.asyncio
async def test_cap_one_stays_serial_in_creation_order(tmp_path, monkeypatch):
    repo, tender, envelope, routing, staff, root_context = _workspace(tmp_path, monkeypatch)
    first, second = _queue_pair(repo, tender["id"], routing, envelope, root_context, staff)
    tracker = {"guard": threading.Lock(), "in_flight": 0, "peak": 0}
    provider = _provider("Serial observation.", tracker)
    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    monkeypatch.setattr("quantix.staff_runtime.execute_api", provider)

    sink: list = []
    await drain_queued_assignments(
        repo,
        tender["id"],
        first.root_run_id,
        max_concurrency=1,
        max_requests=12,
        sink=sink,
    )
    assert tracker["peak"] == 1
    assert [item.assignment.id for item in sink] == [first.id, second.id]


@pytest.mark.asyncio
async def test_client_login_assignments_share_one_lock_while_api_accounts_overlap(
    tmp_path, monkeypatch
):
    repo, tender, envelope, routing, staff, root_context = _workspace(tmp_path, monkeypatch)
    first, _second = _queue_pair(repo, tender["id"], routing, envelope, root_context, staff)
    connections = AIConnectionService(repo)
    subscription = connections.create(
        {
            "name": "Synthetic subscription",
            "provider_id": "codex",
            "protocol": "codex",
            "auth_type": "client_login",
            "billing": "subscription",
            "credentials": {},
            "session_only": True,
        }
    )
    assert connections.get(subscription["id"])["auth_type"] == "client_login"
    assert _locked_connection_id(repo, tender["id"], first) is None

    # A shared account lock serializes whole assignment turns even at cap 2.
    import quantix.office_concurrency as concurrency

    real_resolver = concurrency._locked_connection_id
    monkeypatch.setattr(concurrency, "_locked_connection_id", lambda r, t, a: "shared-test-account")
    tracker = {"guard": threading.Lock(), "in_flight": 0, "peak": 0}
    provider = _provider("Serialized observation.", tracker)
    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    monkeypatch.setattr("quantix.staff_runtime.execute_api", provider)
    sink: list = []
    await drain_queued_assignments(
        repo,
        tender["id"],
        first.root_run_id,
        max_concurrency=2,
        max_requests=12,
        sink=sink,
    )
    monkeypatch.setattr(concurrency, "_locked_connection_id", real_resolver)
    assert tracker["peak"] == 1
    assert sorted(item.assignment.status for item in sink) == ["completed", "completed"]


def test_stop_prevents_new_reservations_and_records_late_usage(tmp_path, monkeypatch):
    repo, tender, _envelope, _routing, _staff, root_context = _workspace(tmp_path, monkeypatch)
    leases = ResourceLeaseService(repo)
    scope = _scope(tender["id"], root_context.run_id)
    leases.ensure_scope(scope, max_requests=1)
    assert leases.reserve(scope) is True
    assert leases.reserve(scope) is False
    leases.stop(scope)
    assert leases.reserve(scope) is False
    assert leases.counts(scope) == {"admitted": 1, "late": 1}


@pytest.mark.asyncio
async def test_failed_branch_does_not_cancel_its_sibling(tmp_path, monkeypatch):
    repo, tender, envelope, routing, staff, root_context = _workspace(tmp_path, monkeypatch)
    first, _second = _queue_pair(repo, tender["id"], routing, envelope, root_context, staff)
    tracker = {"guard": threading.Lock(), "in_flight": 0, "peak": 0}
    provider = _provider("Surviving observation.", tracker, fail_on=first.id)
    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    monkeypatch.setattr("quantix.staff_runtime.execute_api", provider)

    sink: list = []
    with pytest.raises(ValueError, match="Synthetic sibling failure"):
        await drain_queued_assignments(
            repo,
            tender["id"],
            first.root_run_id,
            max_concurrency=2,
            max_requests=12,
            sink=sink,
        )
    assert [item.assignment.status for item in sink] == ["completed"]
    assert StaffAssignmentService(repo).get(tender["id"], first.id).status == "failed"


@pytest.mark.asyncio
async def test_stopped_root_admits_no_new_branches(tmp_path, monkeypatch):
    repo, tender, envelope, routing, staff, root_context = _workspace(tmp_path, monkeypatch)
    first, _second = _queue_pair(repo, tender["id"], routing, envelope, root_context, staff)
    tracker = {"guard": threading.Lock(), "in_flight": 0, "peak": 0}
    provider = _provider("Unstarted observation.", tracker)
    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    monkeypatch.setattr("quantix.staff_runtime.execute_api", provider)
    repo.update_run(first.root_run_id, status="cancelled")

    sink: list = []
    await drain_queued_assignments(
        repo,
        tender["id"],
        first.root_run_id,
        max_concurrency=2,
        max_requests=12,
        sink=sink,
    )
    assert sink == []
    assert tracker["peak"] == 0
