from __future__ import annotations

import json
import threading
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
from copy import deepcopy
from dataclasses import replace

import pytest
from test_staff_routing import _ready_binding_workspace

from quantix.estimates import EstimateService
from quantix.office import prepare_result
from quantix.office_events import OfficeEventService
from quantix.office_research import ResearchRecord
from quantix.office_types import OfficeOutput, PlanProposal, PreparedOfficeResult, TaskProposal
from quantix.staff_assignment_models import PreparedStaffDraft, StaffAssignment, StaffResult
from quantix.staff_assignments import StaffAssignmentService
from quantix.staff_context import build_staff_context
from quantix.staff_models import OfficeConflict


def _prepared(context, staff, assignment_id, binding_id, output, **overrides):
    values = {
        "tender_id": context.tender_id,
        "run_id": context.run_id,
        "output": output,
        "usage": {},
        "source_ids_read": (),
        "web_sources": (),
        "approved_plan_id": context.scope_id,
        "actor_id": staff.staff.id,
        "staff_version": staff.staff.version,
        "assignment_id": assignment_id,
        "route_binding_id": binding_id,
    }
    values.update(overrides)
    return PreparedOfficeResult(**values)


def test_assignment_public_contract_is_available():
    assert StaffAssignment.model_fields
    assert StaffResult.model_fields
    assert PreparedStaffDraft.__dataclass_params__.frozen is True


def test_queue_requires_an_actual_assignment_store():
    with pytest.raises((TypeError, ValueError)):
        StaffAssignmentService(None)


def test_queue_start_wait_and_terminal_events_are_transactional(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, context, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    binding = routing.bind(
        context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "assign"
    )
    service = StaffAssignmentService(repo)

    queued = service.queue(context, binding.id, "queue")
    assert queued.status == "queued"
    assert queued.revision == 1
    assert service.queue(context, binding.id, "queue").id == queued.id
    running = service.start(tender["id"], queued.id, 1)
    assert (running.status, running.revision) == ("running", 2)
    with pytest.raises(OfficeConflict):
        service.start(tender["id"], queued.id, 2)
    waiting = service.wait_for_reply(
        tender["id"], queued.id, 2, "The drawing scale needs your confirmation."
    )
    assert (waiting.status, waiting.revision) == ("waiting", 3)
    cancelled = service.cancel(tender["id"], queued.id)
    assert (cancelled.status, cancelled.revision) == ("cancelled", 4)
    assert service.cancel(tender["id"], queued.id).revision == 4

    office_events = service.events.page(tender["id"], limit=20).items
    assert [event.event_type for event in office_events] == [
        "assignment_queued",
        "assignment_started",
        "assignment_waiting",
        "assignment_cancelled",
    ]


def test_queue_changed_replay_and_queue_event_failure_leave_no_partial_rows(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, context, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    binding = routing.bind(
        context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "queue-rollback-binding"
    )
    service = StaffAssignmentService(repo)
    with pytest.raises(RuntimeError, match="queue event failure"):
        monkeypatch.setattr(
            service.events,
            "append",
            lambda *_args, **_kwargs: (_ for _ in ()).throw(RuntimeError("queue event failure")),
        )
        service.queue(context, binding.id, "queue-rollback")
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM office_assignments").fetchone()[0] == 0
        assert conn.execute("SELECT COUNT(*) FROM office_assignment_receipts").fetchone()[0] == 0

    monkeypatch.setattr(service.events, "append", OfficeEventService(repo).append)
    queued = service.queue(context, binding.id, "queue-replay")
    with pytest.raises(OfficeConflict):
        service.queue(context, "another-binding", "queue-replay")
    assert service.get(tender["id"], queued.id).status == "queued"


def test_real_staff_context_preparation_retains_inner_actor_attribution(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, context, artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch, requested_tool_ids=["read_source"])
    )
    evidence = repo.artifact_evidence(tender["id"], artifact["id"])[0]
    binding = routing.bind(
        context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "prepare-binding"
    )
    service = StaffAssignmentService(repo)
    assignment = service.queue(context, binding.id, "prepare-queue")
    running = service.start(tender["id"], assignment.id, assignment.revision)
    staff_context = build_staff_context(repo, binding.id, running.id)
    staff_context.source(evidence["id"], offset=0, limit=10)
    prepared = prepare_result(
        OfficeOutput(summary="Prepared from the staff context.", source_ids=[evidence["id"]]),
        staff_context,
        {},
        ResearchRecord(staff_context),
    )
    assert prepared.actor_id == staff.staff.id
    assert prepared.assignment_id == running.id
    result = service.save_result(
        tender["id"],
        running.id,
        PreparedStaffDraft(running.id, staff.staff.id, staff.staff.version, binding.id, prepared),
    )
    assert result.source_bases[0].source_id == evidence["id"]


def test_save_result_detaches_draft_and_replays_exactly(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, context, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    binding = routing.bind(
        context,
        staff.staff.id,
        staff.work_order.id,
        envelope.route_options[0].id,
        "result-binding",
    )
    service = StaffAssignmentService(repo)
    assignment = service.queue(context, binding.id, "result-queue")
    running = service.start(tender["id"], assignment.id, assignment.revision)
    prepared = _prepared(
        context,
        staff,
        running.id,
        binding.id,
        OfficeOutput(summary="The reviewed package has no recorded issue."),
        usage={"requests": 1},
    )
    draft = PreparedStaffDraft(
        assignment_id=running.id,
        staff_id=staff.staff.id,
        staff_version=staff.staff.version,
        route_binding_id=binding.id,
        prepared=prepared,
        authored_notes=["Draft ready for Manager review."],
    )

    result = service.save_result(tender["id"], running.id, draft)
    assert result.office_output.summary == prepared.output.summary
    assert result.currentness == "current"
    assert service.get(tender["id"], running.id).status == "completed"
    replay = service.save_result(tender["id"], running.id, draft)
    assert replay.id == result.id
    assert service.get_result(tender["id"], result.id).id == result.id
    with pytest.raises(OfficeConflict):
        service.save_result(
            tender["id"],
            running.id,
            PreparedStaffDraft(
                assignment_id=running.id,
                staff_id=staff.staff.id,
                staff_version=staff.staff.version,
                route_binding_id=binding.id,
                prepared=_prepared(
                    context,
                    staff,
                    running.id,
                    binding.id,
                    OfficeOutput(summary="Changed draft"),
                    usage={"requests": 1},
                ),
            ),
        )


def test_assignment_and_result_views_are_tender_scoped_and_late_results_are_rejected(
    tmp_path, monkeypatch
):
    repo, tender, _connection, _model, _route, envelope, routing, staff, context, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    other_tender = repo.create_tender("Other synthetic Tender")
    binding = routing.bind(
        context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "scope-binding"
    )
    service = StaffAssignmentService(repo)
    assignment = service.queue(context, binding.id, "scope-queue")
    running = service.start(tender["id"], assignment.id, assignment.revision)
    assert service.list(other_tender["id"]) == []
    with pytest.raises(KeyError):
        service.get(other_tender["id"], assignment.id)
    with pytest.raises(KeyError):
        service.get_result(other_tender["id"], "missing-result")

    cancelled = service.cancel(tender["id"], running.id, "Stopped for a scope review.")
    prepared = _prepared(context, staff, cancelled.id, binding.id, OfficeOutput(summary="Late candidate."))
    with pytest.raises(OfficeConflict):
        service.save_result(
            tender["id"],
            cancelled.id,
            PreparedStaffDraft(cancelled.id, staff.staff.id, staff.staff.version, binding.id, prepared),
        )


def test_prepared_identity_mismatch_and_source_change_are_rejected_before_completion(
    tmp_path, monkeypatch
):
    repo, tender, _connection, _model, _route, envelope, routing, staff, context, artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    evidence = repo.artifact_evidence(tender["id"], artifact["id"])[0]
    binding = routing.bind(
        context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "identity-binding"
    )
    service = StaffAssignmentService(repo)
    assignment = service.queue(context, binding.id, "identity-queue")
    running = service.start(tender["id"], assignment.id, assignment.revision)
    prepared = _prepared(
        context,
        staff,
        running.id,
        binding.id,
        OfficeOutput(summary="Identity candidate.", source_ids=[evidence["id"]]),
        source_ids_read=(evidence["id"],),
    )
    forged = PreparedStaffDraft(
        running.id, "forged-staff", staff.staff.version, binding.id, prepared
    )
    with pytest.raises(OfficeConflict):
        service.save_result(tender["id"], running.id, forged)
    assert service.get(tender["id"], running.id).status == "running"

    repo.register_artifact(
        tender["id"],
        "Sources/spec.pdf",
        "c" * 64,
        1,
        {"kind": "pdf", "status": "extracted", "segments": [{"locator": "page:1", "text": "changed"}]},
    )
    with pytest.raises(ValueError, match="changed"):
        service.save_result(
            tender["id"],
            running.id,
            PreparedStaffDraft(running.id, staff.staff.id, staff.staff.version, binding.id, prepared),
        )


def test_result_revalidates_route_model_after_preparation(tmp_path, monkeypatch):
    repo, tender, connection, model, _route, envelope, routing, staff, context, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    binding = routing.bind(
        context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "route-change-binding"
    )
    service = StaffAssignmentService(repo)
    assignment = service.queue(context, binding.id, "route-change-queue")
    running = service.start(tender["id"], assignment.id, assignment.revision)
    prepared = _prepared(context, staff, running.id, binding.id, OfficeOutput(summary="Route candidate."))
    changed_model = deepcopy(model)
    changed_model["capabilities"] = {
        **changed_model["capabilities"],
        "tools": False,
    }
    model_input = {
        key: changed_model[key]
        for key in ("model_id", "display_name", "capabilities", "pricing")
    }
    routing.policy.connections.save_model(connection["id"], model_input)
    with pytest.raises(ValueError, match="tool support|capability|model"):
        service.save_result(
            tender["id"],
            running.id,
            PreparedStaffDraft(running.id, staff.staff.id, staff.staff.version, binding.id, prepared),
        )
    assert service.get(tender["id"], running.id).status == "running"


def test_result_revalidates_captured_item_basis_before_completion(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, context, artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    evidence = repo.artifact_evidence(tender["id"], artifact["id"])[0]
    estimates = EstimateService(repo)
    item_id = "synthetic-item"
    item_data = {
        "document": artifact["name"],
        "sheet": "",
        "locator": evidence["locator"],
        "description": "Synthetic concrete item",
        "unit": "m3",
        "quantity_cell": "",
        "quantity_candidates": {},
        "supplied_quantity": "10",
        "confirmed": False,
        "issues": [],
        "unit_rate": None,
        "components": [],
        "currency": None,
        "tax_basis": "unknown",
        "vat_percent": None,
        "provenance": None,
    }
    with repo.atomic() as conn:
        conn.execute(
            "INSERT INTO boq_items(id,tender_id,source_id,artifact_id,active,data_json) VALUES(?,?,?,?,1,?)",
            (item_id, tender["id"], evidence["id"], artifact["id"], json.dumps(item_data)),
        )
    expected_basis = estimates.rate_basis(tender["id"], item_id)["fingerprint"]
    binding = routing.bind(
        context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "item-basis-binding"
    )
    service = StaffAssignmentService(repo)
    assignment = service.queue(context, binding.id, "item-basis-queue")
    running = service.start(tender["id"], assignment.id, assignment.revision)
    prepared = _prepared(
        context,
        staff,
        running.id,
        binding.id,
        OfficeOutput(summary="Item basis candidate."),
        item_bases=((item_id, expected_basis),),
    )
    with repo.atomic() as conn:
        conn.execute(
            "UPDATE boq_items SET data_json=? WHERE id=?",
            (json.dumps({**item_data, "description": "Changed concrete item"}), item_id),
        )
    with pytest.raises(ValueError, match="BOQ item changed"):
        service.save_result(
            tender["id"],
            running.id,
            PreparedStaffDraft(running.id, staff.staff.id, staff.staff.version, binding.id, prepared),
        )
    assert service.get(tender["id"], running.id).status == "running"


def test_prepared_draft_checks_every_server_identity(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, context, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    binding = routing.bind(
        context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "identity-check-binding"
    )
    service = StaffAssignmentService(repo)
    assignment = service.queue(context, binding.id, "identity-check-queue")
    running = service.start(tender["id"], assignment.id, assignment.revision)

    version_prepared = _prepared(context, staff, running.id, binding.id, OfficeOutput(summary="Version"))
    route_prepared = _prepared(context, staff, running.id, binding.id, OfficeOutput(summary="Route"))
    plan_prepared = replace(version_prepared, approved_plan_id="foreign-plan")
    relabelled_prepared = replace(version_prepared, actor_id="forged-manager")
    variants = (
        PreparedStaffDraft(running.id, staff.staff.id, staff.staff.version + 1, binding.id, version_prepared),
        PreparedStaffDraft(running.id, staff.staff.id, staff.staff.version, "forged-binding", route_prepared),
        PreparedStaffDraft(running.id, staff.staff.id, staff.staff.version, binding.id, plan_prepared),
        PreparedStaffDraft(running.id, staff.staff.id, staff.staff.version, binding.id, relabelled_prepared),
    )
    for draft in variants:
        with pytest.raises(OfficeConflict):
            service.save_result(tender["id"], running.id, draft)
    assert service.get(tender["id"], running.id).status == "running"


def test_result_save_rolls_back_when_completion_event_fails(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, context, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    binding = routing.bind(
        context,
        staff.staff.id,
        staff.work_order.id,
        envelope.route_options[0].id,
        "rollback-binding",
    )
    service = StaffAssignmentService(repo)
    assignment = service.queue(context, binding.id, "rollback-queue")
    running = service.start(tender["id"], assignment.id, assignment.revision)
    prepared = _prepared(context, staff, running.id, binding.id, OfficeOutput(summary="Rollback candidate"))
    draft = PreparedStaffDraft(running.id, staff.staff.id, staff.staff.version, binding.id, prepared)

    def fail_event(*_args, **_kwargs):
        raise RuntimeError("synthetic event failure")

    monkeypatch.setattr(service.events, "append", fail_event)
    with pytest.raises(RuntimeError, match="synthetic event failure"):
        service.save_result(tender["id"], running.id, draft)
    assert service.get(tender["id"], running.id).status == "running"
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM office_staff_results").fetchone()[0] == 0


def test_source_bases_are_saved_and_historical_result_becomes_needs_review(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, context, artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    evidence = repo.artifact_evidence(tender["id"], artifact["id"])[0]
    binding = routing.bind(
        context,
        staff.staff.id,
        staff.work_order.id,
        envelope.route_options[0].id,
        "source-binding",
    )
    service = StaffAssignmentService(repo)
    assignment = service.queue(context, binding.id, "source-queue")
    running = service.start(tender["id"], assignment.id, assignment.revision)
    prepared = _prepared(
        context,
        staff,
        running.id,
        binding.id,
        OfficeOutput(summary="Source-backed draft.", source_ids=[evidence["id"]]),
        source_ids_read=(evidence["id"],),
    )
    draft = PreparedStaffDraft(running.id, staff.staff.id, staff.staff.version, binding.id, prepared)

    result = service.save_result(tender["id"], running.id, draft)
    assert result.source_ids_read == (evidence["id"],)
    assert result.source_bases[0].source_id == evidence["id"]
    assert result.source_bases[0].artifact_id == artifact["id"]

    repo.register_artifact(
        tender["id"],
        "Sources/spec.pdf",
        "b" * 64,
        1,
        {"kind": "pdf", "status": "extracted", "segments": [{"locator": "page:1", "text": "changed"}]},
    )
    assert service.get_result(tender["id"], result.id).currentness == "needs_review"


def test_result_rejects_output_kind_outside_exact_binding_authority(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, context, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    binding = routing.bind(
        context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "permission-binding"
    )
    service = StaffAssignmentService(repo)
    assignment = service.queue(context, binding.id, "permission-queue")
    running = service.start(tender["id"], assignment.id, assignment.revision)
    prepared = _prepared(
        context,
        staff,
        running.id,
        binding.id,
        OfficeOutput(
            summary="The draft includes an unapproved plan.",
            plan=PlanProposal(
                title="Unapproved plan",
                tasks=[TaskProposal(title="Task", description="Task", role="Role")],
            ),
        ),
    )
    with pytest.raises(ValueError, match="outside the reviewed delegation"):
        service.save_result(
            tender["id"],
            running.id,
            PreparedStaffDraft(running.id, staff.staff.id, staff.staff.version, binding.id, prepared),
        )
    assert service.get(tender["id"], running.id).status == "running"


def test_exact_result_replay_survives_terminal_root_and_recovery_is_one_way(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, context, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    binding = routing.bind(
        context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "terminal-binding"
    )
    service = StaffAssignmentService(repo)
    assignment = service.queue(context, binding.id, "terminal-queue")
    running = service.start(tender["id"], assignment.id, assignment.revision)
    prepared = _prepared(context, staff, running.id, binding.id, OfficeOutput(summary="Retained draft."))
    draft = PreparedStaffDraft(running.id, staff.staff.id, staff.staff.version, binding.id, prepared)
    result = service.save_result(tender["id"], running.id, draft)
    repo.update_run(context.run_id, status="completed")
    assert service.save_result(tender["id"], running.id, draft).id == result.id
    assert service.get_result(tender["id"], result.id).currentness == "current"


def test_startup_recovery_interrupts_only_active_assignments(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, context, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    binding = routing.bind(
        context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "recovery-binding"
    )
    service = StaffAssignmentService(repo)
    assignment = service.queue(context, binding.id, "recovery-queue")
    repo.update_run(context.run_id, status="completed")

    assert service.recover_interrupted() == 1
    interrupted = service.get(tender["id"], assignment.id)
    assert interrupted.status == "interrupted"
    assert service.recover_interrupted() == 0
    assert len(service.events.page(tender["id"], limit=20).items) == 2


def test_concurrent_admission_takes_authority_before_sqlite_write(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, context, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    binding = routing.bind(
        context, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "lock-binding"
    )
    service = StaffAssignmentService(repo)
    connections = service.routing.policy.connections
    original_guard = connections.authority_guard
    original_atomic = repo.atomic
    active = {}
    events = []
    event_lock = threading.Lock()

    @contextmanager
    def tracked_guard():
        thread_id = threading.get_ident()
        with original_guard():
            with event_lock:
                active[thread_id] = active.get(thread_id, 0) + 1
                events.append((thread_id, "authority_enter"))
            try:
                yield
            finally:
                with event_lock:
                    events.append((thread_id, "authority_exit"))
                    active[thread_id] -= 1
                    if active[thread_id] == 0:
                        del active[thread_id]

    @contextmanager
    def tracked_atomic():
        thread_id = threading.get_ident()
        with original_atomic() as conn:
            with event_lock:
                events.append((thread_id, "sqlite_enter", thread_id in active))
            yield conn

    monkeypatch.setattr(connections, "authority_guard", tracked_guard)
    monkeypatch.setattr(repo, "atomic", tracked_atomic)
    with ThreadPoolExecutor(max_workers=2) as pool:
        results = list(
            pool.map(
                lambda _unused: service.queue(context, binding.id, "same-concurrent-key"),
                range(2),
            )
        )

    assert results[0].id == results[1].id
    assert all(event[2] for event in events if event[1] == "sqlite_enter")
    for thread_id in {event[0] for event in events}:
        thread_events = [event[1] for event in events if event[0] == thread_id]
        assert thread_events.index("authority_enter") < thread_events.index("sqlite_enter")
