"""Retirement is enforced by actual binding and assignment admission."""
import pytest
from test_staff_routing import _ready_binding_workspace

from quantix.execution_context import engineer_identity
from quantix.staff_assignments import StaffAssignmentService
from quantix.staff_lifecycle import StaffLifecycleService
from quantix.staff_lifecycle_models import StaffLifecycleRequest
from quantix.staff_models import OfficeConflict


def test_retirement_blocks_new_binding_and_previously_bound_unqueued_work(tmp_path, monkeypatch):
    repo, tender, *_middle, envelope, routing, staff, context, _artifact = _ready_binding_workspace(
        tmp_path, monkeypatch, max_assignments=3)
    binding = routing.bind(context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "before-retirement")
    StaffLifecycleService(repo).transition(engineer_identity(tender["id"]), StaffLifecycleRequest(
        staff_id=staff.staff.id, expected_version=staff.staff.version, target="retired",
        reason="Synthetic retirement before assignment", idempotency_key="retire-admission"))
    with pytest.raises(OfficeConflict, match="retired|archived|available"):
        routing.bind(context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "after-retirement")
    with pytest.raises(OfficeConflict, match="retired|archived|available"):
        StaffAssignmentService(repo).queue(context, binding.id, "queue-after-retirement")
