"""Lifecycle transitions retain immutable profile snapshots and truthful receipts."""

from __future__ import annotations

import pytest
from test_staff_routing import _ready_binding_workspace

from quantix.execution_context import engineer_identity
from quantix.staff_lifecycle import StaffLifecycleService, ensure_staff_can_admit_work
from quantix.staff_lifecycle_models import StaffLifecycleRequest
from quantix.staff_models import OfficeConflict
from quantix.staff_store import StaffStore


def _request(staff_id: str, expected_version: int, target: str, key: str):
    return StaffLifecycleRequest(
        staff_id=staff_id,
        expected_version=expected_version,
        target=target,
        reason=f"Synthetic {target} transition.",
        idempotency_key=key,
    )


def test_transition_writes_new_immutable_profile_snapshot(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, _envelope, _routing, staff, _root, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    service = StaffLifecycleService(repo)

    retired = service.transition(
        engineer_identity(tender["id"]),
        _request(staff.staff.id, staff.staff.version, "retired", "retire-v1"),
    )

    assert retired.staff.version == staff.staff.version + 1
    assert retired.staff.lifecycle == "retired"
    assert StaffStore(repo).get_staff(tender["id"], staff.staff.id, staff.staff.version).lifecycle == "available"
    with repo.db.connect() as conn:
        versions = conn.execute(
            "SELECT version,lifecycle FROM office_staff_versions WHERE tender_id=? AND staff_id=? ORDER BY version",
            (tender["id"], staff.staff.id),
        ).fetchall()
    assert [(row["version"], row["lifecycle"]) for row in versions] == [
        (staff.staff.version, "available"),
        (retired.staff.version, "retired"),
    ]


def test_transition_receipt_replays_its_snapshot_after_a_later_transition(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, _envelope, _routing, staff, _root, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    service = StaffLifecycleService(repo)
    identity = engineer_identity(tender["id"])

    retired = service.transition(
        identity,
        _request(staff.staff.id, staff.staff.version, "retired", "retire-v1"),
    )
    reactivated = service.transition(
        identity,
        _request(staff.staff.id, retired.staff.version, "available", "reactivate-v2"),
    )
    replay = service.transition(
        identity,
        _request(staff.staff.id, staff.staff.version, "retired", "retire-v1"),
    )

    assert reactivated.staff.version == retired.staff.version + 1
    assert replay.replayed is True
    assert replay.staff.version == retired.staff.version
    assert replay.staff.lifecycle == "retired"
    assert replay.staff.version != reactivated.staff.version


def test_retirement_still_blocks_busy_identity(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, _envelope, routing, staff, root, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    binding = routing.bind(root, staff.staff.id, staff.work_order.id, _envelope.route_options[0].id, "lifecycle")
    from quantix.staff_assignments import StaffAssignmentService

    assignment = StaffAssignmentService(repo).queue(root, binding.id, "lifecycle-assignment")
    with pytest.raises(OfficeConflict, match="active work"):
        StaffLifecycleService(repo).transition(
            engineer_identity(tender["id"]),
            _request(staff.staff.id, staff.staff.version, "retired", "retire-busy"),
        )
    assert StaffAssignmentService(repo).get(tender["id"], assignment.id).status == "queued"


def test_new_work_guard_reads_current_state_without_invalidating_pinned_versions(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, _envelope, _routing, staff, _root, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    lifecycle = StaffLifecycleService(repo)
    retired = lifecycle.transition(
        engineer_identity(tender["id"]),
        _request(staff.staff.id, staff.staff.version, "retired", "guard-retire"),
    )

    with repo.atomic() as conn:
        with pytest.raises(OfficeConflict, match="new work"):
            ensure_staff_can_admit_work(conn, tender["id"], staff.staff.id)

    assert StaffStore(repo).get_staff(tender["id"], staff.staff.id, staff.staff.version).lifecycle == "available"
    assert StaffStore(repo).get_staff(tender["id"], staff.staff.id, retired.staff.version).lifecycle == "retired"
