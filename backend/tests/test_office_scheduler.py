"""Bounded descendants, fenced ownership, and serial parent-child execution."""

from __future__ import annotations

import asyncio
import json
from contextlib import contextmanager

import pytest
from test_staff_context import _build_staff_context
from test_staff_routing import _ready_binding_workspace, _work_order

from quantix.ai_connections import AIConnectionService
from quantix.execution_context import OfficeExecutionIdentity
from quantix.manager_profile import ManagerProfileService
from quantix.office_ownership import OfficeOwnershipService, TransferOwnershipRequest
from quantix.office_scheduler import ChildAssignmentRequest, OfficeScheduler
from quantix.office_tools import source_tools
from quantix.office_types import OfficeOutput
from quantix.staff_assignments import StaffAssignmentService
from quantix.staff_models import ManagerCreationContext, OfficeConflict
from quantix.staff_runtime import run_staff_assignment
from quantix.staff_runtime_models import StaffCompletedDraft, StaffProviderOutput
from quantix.staff_store import StaffStore
from quantix.tool_policy import ToolFenceError, dispatch


def _definition(name: str):
    return next(item for item in source_tools() if item.name == name)


def _workspace(tmp_path, monkeypatch, **overrides):
    params = {
        "requested_tool_ids": ["read_source", "request_child_assignment"],
        "envelope_tools": ("read_source", "request_child_assignment"),
        "max_staff": 1,
        "max_assignments": 10,
        "max_depth": 2,
    }
    params.update(overrides)
    repo, tender, _conn, _model, _route, envelope, routing, staff, root_context, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch, **params)
    )
    binding = routing.bind(
        root_context,
        staff.staff.id,
        staff.work_order.id,
        envelope.route_options[0].id,
        "parent-bind",
    )
    assignment = StaffAssignmentService(repo).queue(root_context, binding.id, "parent-queue")
    return repo, tender, staff, binding, assignment, root_context, envelope


def _child_order(index: int) -> dict:
    return {
        **_work_order(),
        "brief": f"Descendant subtask {index}.",
        "goal": "Give the Manager a source-backed subtask draft.",
    }


def _extra_orders(repo, tender_id, staff, root_context, count: int):
    manager = ManagerProfileService(repo)
    planning = repo.create_run(tender_id, "conversation", "Plan descendant work")
    context = ManagerCreationContext(
        tender_id, planning["id"], manager.get().version, root_context.scope_id
    )
    store = StaffStore(repo)
    return [
        store.create_work_order(
            context,
            staff.staff.id,
            staff.staff.version,
            _child_order(index),
            f"child-order-{index}",
        ).work_order
        for index in range(count)
    ]


def _staff_context(repo, tender_id, assignment_id):
    binding_id = StaffAssignmentService(repo).get(tender_id, assignment_id).route_binding_id
    return _build_staff_context(repo, binding_id, assignment_id)


def test_depth_cap_comes_from_the_reviewed_envelope(tmp_path, monkeypatch):
    repo, tender, staff, _binding, parent, _root_context, _envelope = _workspace(
        tmp_path, monkeypatch
    )
    orders = _extra_orders(repo, tender["id"], staff, _root_context, 3)
    tool = _definition("request_child_assignment")
    assignments = StaffAssignmentService(repo)

    first = json.loads(
        asyncio.run(
            dispatch(
                "direct",
                tool,
                _staff_context(repo, tender["id"], parent.id),
                {"work_order_id": orders[0].id},
                invocation_id="scheduler-child",
            )
        )
    )
    child = assignments.get(tender["id"], first["assignment"]["id"])
    assert child.parent_assignment_id == parent.id
    assert child.depth == 1
    assert child.root_run_id == parent.root_run_id
    assert child.status == "queued"

    second = json.loads(
        asyncio.run(
            dispatch(
                "direct",
                tool,
                _staff_context(repo, tender["id"], child.id),
                {"work_order_id": orders[1].id},
                invocation_id="scheduler-grandchild",
            )
        )
    )
    grandchild = assignments.get(tender["id"], second["assignment"]["id"])
    assert grandchild.depth == 2

    with pytest.raises(ToolFenceError):
        asyncio.run(
            dispatch(
                "direct",
                tool,
                _staff_context(repo, tender["id"], grandchild.id),
                {"work_order_id": orders[2].id},
                invocation_id="scheduler-great-grandchild",
            )
        )

    # A generous model-supplied cap cannot widen the reviewed envelope cap.
    scheduler = OfficeScheduler(repo)
    ident = OfficeExecutionIdentity(
        tender_id=tender["id"],
        actor_kind="staff",
        actor_id=staff.staff.id,
        root_run_id=parent.root_run_id,
        budget_scope_id=parent.root_run_id,
        assignment_id=grandchild.id,
        profile_version=None,
        route_binding_id=None,
        instruction_revision_id=None,
        grant_fingerprint=None,
        ownership_epoch=None,
        trusted_invocation_id=None,
    )
    with pytest.raises(ValueError, match="reviewed cap"):
        scheduler.spawn_child(
            ident,
            ChildAssignmentRequest(
                work_order_id=orders[2].id,
                parent_assignment_id=grandchild.id,
                max_depth=8,
                idempotency_key="scheduler-wide-cap",
            ),
        )
    assert scheduler.budget_owners(tender["id"], parent.root_run_id) == 1


def test_capability_outside_parent_scope_cannot_spawn(tmp_path, monkeypatch):
    repo, tender, staff, _binding, parent, _root_context, _envelope = _workspace(
        tmp_path, monkeypatch
    )
    orders = _extra_orders(repo, tender["id"], staff, _root_context, 1)
    tool = _definition("request_child_assignment")
    with pytest.raises(ToolFenceError, match="outside the parent assignment scope"):
        asyncio.run(
            dispatch(
                "direct",
                tool,
                _staff_context(repo, tender["id"], parent.id),
                {"work_order_id": orders[0].id, "capability_ids": ["send_email"]},
                invocation_id="scheduler-rogue-capability",
            )
        )


def test_claim_transfer_release_and_expiry_fencing(tmp_path, monkeypatch):
    repo, tender, staff, _binding, _parent, root_context, _envelope = _workspace(
        tmp_path, monkeypatch
    )

    def ident(actor: str, kind: str = "staff"):
        return OfficeExecutionIdentity(
            tender_id=tender["id"],
            actor_kind=kind,
            actor_id=actor,
            root_run_id=root_context.run_id,
            budget_scope_id=root_context.run_id,
            assignment_id=None,
            profile_version=None,
            route_binding_id=None,
            instruction_revision_id=None,
            grant_fingerprint=None,
            ownership_epoch=None,
            trusted_invocation_id=None,
        )

    ownership = OfficeOwnershipService(repo)
    first = ownership.claim(ident(staff.staff.id), "assignment-1", 1, idempotency_key="claim-a")
    assert first.active and first.epoch == 1
    with pytest.raises(OfficeConflict):
        ownership.claim(ident("other-staff"), "assignment-1", 1, idempotency_key="claim-b")

    moved = ownership.transfer(
        ident(staff.staff.id),
        TransferOwnershipRequest(
            assignment_key="assignment-1",
            expected_epoch=1,
            new_owner_id="other-staff",
            reason="Hand over the stalled check.",
        ),
    )
    assert (moved.owner_id, moved.epoch, moved.active) == ("other-staff", 2, True)
    with pytest.raises(OfficeConflict):
        ownership.transfer(
            ident(staff.staff.id),
            TransferOwnershipRequest(
                assignment_key="assignment-1",
                expected_epoch=1,
                new_owner_id="third-staff",
                reason="Stale epoch replay.",
            ),
        )
    assert ownership.release(ident("other-staff"), "assignment-1") is True
    assert ownership.release(ident("other-staff"), "assignment-1") is False

    expiring = ownership.claim(
        ident(staff.staff.id),
        "assignment-2",
        1,
        idempotency_key="claim-expiring",
        expires_at="2000-01-01T00:00:00Z",
    )
    assert expiring.active is True
    reclaimed = ownership.claim(
        ident("other-staff"), "assignment-2", 1, idempotency_key="claim-after-expiry"
    )
    assert (reclaimed.owner_id, reclaimed.epoch, reclaimed.active) == ("other-staff", 2, True)


@pytest.mark.asyncio
async def test_child_executes_serially_after_the_parent_lease_closes(tmp_path, monkeypatch):
    repo, tender, staff, _binding, parent, _root_context, _envelope = _workspace(
        tmp_path, monkeypatch
    )
    orders = _extra_orders(repo, tender["id"], staff, _root_context, 1)
    assignments = StaffAssignmentService(repo)
    leases = 0
    actual_lease = AIConnectionService.lease
    phases: list[str] = []

    @contextmanager
    def observed_lease(service, connection_id):
        nonlocal leases
        assert leases == 0, "A child must wait until its parent account lease closes"
        with actual_lease(service, connection_id) as account:
            leases += 1
            try:
                yield account
            finally:
                leases -= 1

    monkeypatch.setattr(AIConnectionService, "lease", observed_lease)

    async def provider(route, connection, credentials, context, prompt, output_type, **options):
        assert leases == 1
        definitions = {item.name: item for item in options["definitions"]}
        reservation = await options["before_request"](200, 200)
        usage = {
            "requests": 1,
            "input_tokens": 200,
            "output_tokens": 100,
            "web_search_calls": 0,
            "usage_complete": True,
        }
        if context.assignment_id == parent.id:
            phases.append("parent")
            assert "request_child_assignment" in definitions
            child = json.loads(
                await definitions["request_child_assignment"].invoke(
                    context, {"work_order_id": orders[0].id}, invocation_id="serial-child"
                )
            )
            assert (
                StaffAssignmentService(repo).get(tender["id"], child["assignment"]["id"]).status
                == "queued"
            )
            output = StaffProviderOutput(
                result=StaffCompletedDraft(
                    output=OfficeOutput(summary="Parent observation complete.")
                )
            )
        else:
            phases.append("child")
            output = StaffProviderOutput(
                result=StaffCompletedDraft(
                    output=OfficeOutput(summary="Child observation complete.")
                )
            )
        await options["on_response"](usage, reservation)
        return {"output": output, "usage": usage, "web_sources": []}

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    monkeypatch.setattr("quantix.staff_runtime.execute_api", provider)

    parent_outcome = await run_staff_assignment(repo, tender["id"], parent.id)
    assert parent_outcome.assignment.status == "completed"
    children = [
        item
        for item in assignments.list(
            tender["id"], root_run_id=parent_outcome.assignment.root_run_id
        )
        if item.parent_assignment_id == parent.id
    ]
    assert len(children) == 1
    child_outcome = await run_staff_assignment(repo, tender["id"], children[0].id)
    assert child_outcome.assignment.status == "completed"
    assert child_outcome.assignment.depth == 1
    assert phases == ["parent", "child"]
    with repo.db.connect() as conn:
        roots = {
            row[0]
            for row in conn.execute(
                "SELECT DISTINCT root_run_id FROM office_assignments WHERE tender_id=?",
                (tender["id"],),
            )
        }
        usage_roots = {
            row[0]
            for row in conn.execute(
                "SELECT DISTINCT run_id FROM ai_usage WHERE tender_id=?", (tender["id"],)
            )
        }
    assert roots == usage_roots == {parent_outcome.assignment.root_run_id}
