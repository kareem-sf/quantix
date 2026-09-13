"""Reviewed parallel work must be usable and cannot outrun its saved grant."""

import pytest
from test_plan_review import _approval_context
from test_staff_routing import _ready_binding_workspace

from quantix.office_concurrency import drain_queued_assignments
from quantix.staff_assignments import StaffAssignmentService


def test_new_work_review_proposes_bounded_parallel_staff(tmp_path, monkeypatch):
    _repo, tender, plan, _connection, _model, _policy, service = _approval_context(tmp_path, monkeypatch)
    review = service.review(tender["id"], plan["id"])
    assert review["delegation"] is not None
    envelope = review["delegation"]
    assert (envelope["max_staff"], envelope["max_assignments"]) == (4, 12)
    assert (envelope["max_depth"], envelope["max_concurrency"]) == (2, 2)


def test_engineer_can_save_exact_depth_and_concurrency(tmp_path, monkeypatch):
    _repo, tender, plan, _connection, _model, _policy, service = _approval_context(tmp_path, monkeypatch)
    review = service.review(tender["id"], plan["id"])
    envelope = review["delegation"]
    updated = service.update_delegation(tender["id"], plan["id"], {
        "expected_version": 0,
        "source_scope": "reviewed_tender",
        "artifact_ids": [item["artifact_id"] for item in envelope["artifacts"]],
        "tool_ids": [tool["id"] for tool in envelope["tools"]],
        "allowed_draft_outputs": envelope["allowed_draft_outputs"],
        "route_option_ids": [route["id"] for route in envelope["route_options"]],
        "max_staff": 4, "max_assignments": 12,
        "max_depth": 3, "max_concurrency": 4,
        "max_requests": envelope["max_requests"],
        "max_search_calls": envelope["max_search_calls"],
    })
    assert updated["max_depth"] == 3
    assert updated["max_concurrency"] == 4
    refreshed = service.review(tender["id"], plan["id"])
    assert refreshed["delegation"]["max_depth"] == 3
    assert refreshed["delegation"]["max_concurrency"] == 4
    assert refreshed["fingerprint"] != review["fingerprint"]


def test_reviewed_parallel_grant_can_bind_a_real_assignment(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, ctx, _artifact = _ready_binding_workspace(
        tmp_path, monkeypatch, max_concurrency=2, max_depth=2,
    )
    binding = routing.bind(ctx, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "parallel-bind")
    assignment = StaffAssignmentService(repo).queue(ctx, binding.id, "parallel-queue")
    assert assignment.status == "queued"
    assert routing.validate_root(tender["id"], ctx.run_id, ctx.scope_id).envelope.max_concurrency == 2


@pytest.mark.asyncio
async def test_scheduler_cannot_increase_reviewed_concurrency(tmp_path, monkeypatch):
    async def forbidden_dispatch(*args, **kwargs):
        pytest.fail("An assignment exceeded its reviewed concurrency before dispatch.")

    monkeypatch.setattr("quantix.office_concurrency._run_branch", forbidden_dispatch)
    repo, tender, _connection, _model, _route, envelope, routing, staff, ctx, _artifact = _ready_binding_workspace(
        tmp_path, monkeypatch,
    )
    binding = routing.bind(ctx, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "serial-bind")
    assignment = StaffAssignmentService(repo).queue(ctx, binding.id, "serial-queue")
    with pytest.raises(ValueError, match="reviewed concurrency"):
        await drain_queued_assignments(repo, tender["id"], ctx.run_id, max_concurrency=2, max_requests=1)
    assert StaffAssignmentService(repo).get(tender["id"], assignment.id).status == "queued"
