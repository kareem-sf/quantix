from __future__ import annotations

import json

import pytest
from test_staff_routing import _ready_binding_workspace

from quantix.manager_profile import ManagerProfileService
from quantix.office_message_models import (
    OfficeMessage,
    OfficeParticipant,
    OfficeRecipientTarget,
)
from quantix.office_messages import OfficeMessageService
from quantix.office_tools import OfficeContext
from quantix.office_types import OfficeOutput, PreparedOfficeResult
from quantix.staff_assignment_models import PreparedStaffDraft
from quantix.staff_assignments import StaffAssignmentService
from quantix.staff_context import build_staff_context
from quantix.staff_models import ManagerProfileEdit, OfficeConflict


def _message_assignment(tmp_path, monkeypatch, *, start=True):
    repo, tender, _, _, _, envelope, routing, staff, root, _ = _ready_binding_workspace(tmp_path, monkeypatch)
    binding = routing.bind(root, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "message-bound")
    assignments = StaffAssignmentService(repo)
    assignment = assignments.queue(root, binding.id, "message-queued")
    if start:
        assignment = assignments.start(tender["id"], assignment.id, assignment.revision)
    return repo, tender, assignment, build_staff_context(repo, binding.id, assignment.id)


def test_assignment_inbox_keeps_newest_messages_in_chronological_order(tmp_path, monkeypatch):
    repo, tender, assignment, context = _message_assignment(tmp_path, monkeypatch)
    service = OfficeMessageService(repo)
    saved = [service.post_staff(context, "note", f"Actual note {index}", idempotency_key=f"newest-{index}") for index in range(3)]
    assert [message.id for message in service.for_assignment(tender["id"], assignment.id, limit=2)] == [message.id for message in saved[-2:]]


@pytest.mark.parametrize("changed", ["staff_version", "route_binding_id"])
def test_staff_replay_rejects_a_changed_version_or_binding(tmp_path, monkeypatch, changed):
    repo, _, _, context = _message_assignment(tmp_path, monkeypatch)
    service = OfficeMessageService(repo)
    service.post_staff(context, "note", "Actual note", idempotency_key="bound-replay")
    setattr(context, changed, context.staff_version + 1 if changed == "staff_version" else "different-binding")
    with pytest.raises((OfficeConflict, ValueError), match="different|match|changed"):
        service.post_staff(context, "note", "Actual note", idempotency_key="bound-replay")


def test_queued_staff_cannot_author_a_message_before_execution(tmp_path, monkeypatch):
    repo, _, _, context = _message_assignment(tmp_path, monkeypatch, start=False)
    with pytest.raises(OfficeConflict, match="running|active"):
        OfficeMessageService(repo).post_staff(context, "note", "Not yet authored", idempotency_key="not-started")


def test_message_contract_requires_server_supplied_participants():
    sender = OfficeParticipant(
        id="manager-1",
        version=1,
        display_name="Tender Manager",
        title="Tender Manager",
        kind="manager",
    )
    message = OfficeMessage(
        id="message-1",
        tender_id="tender-1",
        root_run_id="root-1",
        sender=sender,
        recipients=[
            OfficeParticipant(
                id="staff-1",
                version=1,
                display_name="Evidence Specialist",
                title="Evidence Specialist",
                kind="staff",
            )
        ],
        kind="instruction",
        text="Review the current drawing package.",
        created_at="2026-09-10T00:00:00+00:00",
    )

    assert message.sender.id == "manager-1"
    assert message.recipients[0].kind == "staff"
    assert OfficeRecipientTarget(staff_id="staff-1").assignment_id is None


def test_manager_post_resolves_planned_staff_identity_and_event(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, _envelope, _routing, staff, root, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    manager = ManagerProfileService(repo).get()
    context = OfficeContext(
        repo,
        tender["id"],
        root.run_id,
        actor_id=manager.id,
    )

    saved = OfficeMessageService(repo).post_manager(
        context,
        [OfficeRecipientTarget(staff_id=staff.staff.id)],
        "instruction",
        "Review the current drawing package.",
        idempotency_key="manager-message",
    )

    assert saved.sender.id == manager.id
    assert saved.sender.version == manager.version
    assert saved.recipients[0].id == staff.staff.id
    assert saved.recipients[0].version == staff.staff.version
    assert saved.recipients[0].assignment_id is None
    assert [
        event.event_type
        for event in OfficeMessageService(repo).events.page(tender["id"], limit=10).items
    ] == ["message_posted"]


def test_staff_post_is_manager_mediated_and_does_not_create_read_receipts(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, root, artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch, requested_tool_ids=["read_source"])
    )
    binding = routing.bind(
        root, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "message-binding"
    )
    assignment = StaffAssignmentService(repo).queue(root, binding.id, "message-queue")
    assignment = StaffAssignmentService(repo).start(
        tender["id"], assignment.id, assignment.revision
    )
    staff_context = build_staff_context(repo, binding.id, assignment.id)
    source = repo.artifact_evidence(tender["id"], artifact["id"])[0]

    saved = OfficeMessageService(repo).post_staff(
        staff_context,
        "question",
        "Please confirm the drawing scale.",
        reference_requests=({"kind": "source", "source_id": source["id"]},),
        idempotency_key="staff-message",
    )

    assert saved.sender.id == staff.staff.id
    assert saved.sender.assignment_id == assignment.id
    assert saved.recipients[0].kind == "manager"
    assert saved.artifact_refs[0].artifact_id == artifact["id"]
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM staff_source_receipts").fetchone()[0] == 0


def test_manager_assigned_handoff_resolves_exact_source_and_preserves_assignment_target(
    tmp_path, monkeypatch
):
    repo, tender, _connection, _model, _route, envelope, routing, staff, root, artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    binding = routing.bind(
        root,
        staff.staff.id,
        staff.work_order.id,
        envelope.route_options[0].id,
        "handoff-binding",
    )
    assignment = StaffAssignmentService(repo).queue(root, binding.id, "handoff-queue")
    source = repo.artifact_evidence(tender["id"], artifact["id"])[0]
    manager = ManagerProfileService(repo).get()
    context = OfficeContext(repo, tender["id"], root.run_id, actor_id=manager.id)

    saved = OfficeMessageService(repo).post_manager(
        context,
        [OfficeRecipientTarget(staff_id=staff.staff.id, assignment_id=assignment.id)],
        "handoff",
        "Use this source when checking the package.",
        reference_requests=[{"source_id": source["id"]}],
        idempotency_key="assigned-handoff",
    )

    assert saved.assignment_id == assignment.id
    assert saved.recipients[0].assignment_id == assignment.id
    assert saved.artifact_refs[0].content_hash == artifact["content_hash"]
    events = OfficeMessageService(repo).events.page(tender["id"], limit=10).items
    assert [event.event_type for event in events] == [
        "assignment_queued",
        "message_posted",
        "artifact_shared",
    ]


def test_staff_message_replay_keeps_historic_name_and_terminal_new_post_is_denied(
    tmp_path, monkeypatch
):
    repo, tender, _connection, _model, _route, envelope, routing, staff, root, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    binding = routing.bind(
        root,
        staff.staff.id,
        staff.work_order.id,
        envelope.route_options[0].id,
        "replay-binding",
    )
    assignments = StaffAssignmentService(repo)
    assignment = assignments.queue(root, binding.id, "replay-queue")
    assignment = assignments.start(tender["id"], assignment.id, assignment.revision)
    staff_context = build_staff_context(repo, binding.id, assignment.id)
    service = OfficeMessageService(repo)
    saved = service.post_staff(
        staff_context, "note", "The source review is underway.", idempotency_key="replay"
    )

    assignments.cancel(tender["id"], assignment.id)
    replay = service.post_staff(
        staff_context, "note", "The source review is underway.", idempotency_key="replay"
    )
    assert replay.id == saved.id
    with pytest.raises(OfficeConflict):
        service.post_staff(staff_context, "note", "Changed text.", idempotency_key="replay")
    with pytest.raises(OfficeConflict):
        service.post_staff(
            staff_context,
            "note",
            "A new terminal message.",
            idempotency_key="new-terminal",
        )


def test_manager_replay_preserves_renamed_historic_speaker_after_root_finishes(
    tmp_path, monkeypatch
):
    repo, tender, _connection, _model, _route, _envelope, _routing, staff, root, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    manager_service = ManagerProfileService(repo)
    manager = manager_service.get()
    context = OfficeContext(repo, tender["id"], root.run_id, actor_id=manager.id)
    service = OfficeMessageService(repo)
    saved = service.post_manager(
        context,
        [OfficeRecipientTarget(staff_id=staff.staff.id)],
        "note",
        "The original Manager wording is retained.",
        idempotency_key="historic-manager",
    )
    manager_service.update(
        ManagerProfileEdit(
            expected_version=manager.version,
            display_name="Renamed Tender Manager",
            title=manager.title,
            persona=manager.persona,
            personality=manager.personality,
            working_preferences=manager.working_preferences,
        )
    )
    repo.update_run(root.run_id, status="completed")

    replay = service.post_manager(
        context,
        [OfficeRecipientTarget(staff_id=staff.staff.id)],
        "note",
        "The original Manager wording is retained.",
        idempotency_key="historic-manager",
    )
    assert replay.id == saved.id
    assert replay.sender.display_name == manager.display_name


def test_manager_can_mediate_a_same_root_reply(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, _envelope, _routing, staff, root, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    manager = ManagerProfileService(repo).get()
    service = OfficeMessageService(repo)
    context = OfficeContext(repo, tender["id"], root.run_id, actor_id=manager.id)
    original = service.post_manager(
        context,
        [OfficeRecipientTarget(staff_id=staff.staff.id)],
        "question",
        "Please check the drawing note.",
        idempotency_key="same-root-question",
    )
    reply = service.post_manager(
        context,
        [OfficeRecipientTarget(staff_id=staff.staff.id)],
        "reply",
        "I have linked the question to the assigned review.",
        reply_to=original.id,
        idempotency_key="same-root-reply",
    )
    assert reply.reply_to == original.id


def test_manager_relay_can_reply_to_an_actual_staff_question_in_same_root(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, root, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    binding = routing.bind(
        root, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "relay-binding"
    )
    assignments = StaffAssignmentService(repo)
    assignment = assignments.queue(root, binding.id, "relay-queue")
    assignment = assignments.start(tender["id"], assignment.id, assignment.revision)
    staff_context = build_staff_context(repo, binding.id, assignment.id)
    service = OfficeMessageService(repo)
    question = service.post_staff(
        staff_context,
        "question",
        "Which current drawing should I use?",
        idempotency_key="relay-question",
    )
    manager = ManagerProfileService(repo).get()
    reply = service.post_manager(
        OfficeContext(repo, tender["id"], root.run_id, actor_id=manager.id),
        [OfficeRecipientTarget(staff_id=staff.staff.id, assignment_id=assignment.id)],
        "reply",
        "Use the drawing in the reviewed source scope.",
        reply_to=question.id,
        idempotency_key="relay-reply",
    )
    assert reply.reply_to == question.id


def test_pagination_cursor_is_tender_scoped_and_stable_when_new_message_is_appended(
    tmp_path, monkeypatch
):
    repo, tender, _connection, _model, _route, _envelope, _routing, staff, root, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    manager = ManagerProfileService(repo).get()
    context = OfficeContext(repo, tender["id"], root.run_id, actor_id=manager.id)
    service = OfficeMessageService(repo)
    for index in range(3):
        service.post_manager(
            context,
            [OfficeRecipientTarget(staff_id=staff.staff.id)],
            "note",
            f"Message {index}",
            idempotency_key=f"page-{index}",
        )
    first = service.page(tender["id"], limit=2)
    assert [item.text for item in first.items] == ["Message 1", "Message 2"]
    cursor = first.next_cursor
    assert cursor
    service.post_manager(
        context,
        [OfficeRecipientTarget(staff_id=staff.staff.id)],
        "note",
        "Message 3",
        idempotency_key="page-3",
    )
    older = service.page(tender["id"], cursor=cursor, limit=2)
    assert [item.text for item in older.items] == ["Message 0"]
    other = repo.create_tender("Other Tender")
    with pytest.raises(ValueError):
        service.page(other["id"], cursor=cursor, limit=2)


def test_cross_root_reply_is_rejected_without_a_new_message(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, _envelope, _routing, staff, root, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    manager = ManagerProfileService(repo).get()
    service = OfficeMessageService(repo)
    original = service.post_manager(
        OfficeContext(repo, tender["id"], root.run_id, actor_id=manager.id),
        [OfficeRecipientTarget(staff_id=staff.staff.id)],
        "question",
        "Can you confirm the package basis?",
        idempotency_key="reply-original",
    )
    next_root = repo.create_run(tender["id"], "conversation", "Continue explicitly")
    from quantix.manager_runtime import ManagerRunProfiles

    ManagerRunProfiles(repo).capture(tender["id"], next_root["id"])
    with pytest.raises(ValueError, match="Cross-root"):
        service.post_manager(
            OfficeContext(repo, tender["id"], next_root["id"], actor_id=manager.id),
            [OfficeRecipientTarget(staff_id=staff.staff.id)],
            "reply",
            "Continuing under a fresh root.",
            reply_to=original.id,
            idempotency_key="reply-cross-root",
        )


def test_for_assignment_excludes_planning_messages_and_keeps_exact_targeted_history(
    tmp_path, monkeypatch
):
    repo, tender, _connection, _model, _route, envelope, routing, staff, root, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    manager = ManagerProfileService(repo).get()
    service = OfficeMessageService(repo)
    context = OfficeContext(repo, tender["id"], root.run_id, actor_id=manager.id)
    planning = service.post_manager(
        context,
        [OfficeRecipientTarget(staff_id=staff.staff.id)],
        "note",
        "Planning note only.",
        idempotency_key="planning-only",
    )
    binding = routing.bind(
        root, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "prompt-binding"
    )
    assignment = StaffAssignmentService(repo).queue(root, binding.id, "prompt-queue")
    targeted = service.post_manager(
        context,
        [OfficeRecipientTarget(staff_id=staff.staff.id, assignment_id=assignment.id)],
        "instruction",
        "Use the exact assigned source basis.",
        idempotency_key="targeted-only",
    )

    material = service.for_assignment(tender["id"], assignment.id)
    assert [item.id for item in material] == [targeted.id]
    assert planning.id not in {item.id for item in material}
    staff_context = build_staff_context(repo, binding.id, assignment.id)
    assert [item["id"] for item in staff_context.history] == [targeted.id]
    assert staff_context.seen_sources == set()


def test_message_and_events_roll_back_together_when_event_append_fails(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, _envelope, _routing, staff, root, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    manager = ManagerProfileService(repo).get()
    service = OfficeMessageService(repo)

    def fail_event(*_args, **_kwargs):
        raise RuntimeError("synthetic event failure")

    monkeypatch.setattr(service.events, "append", fail_event)
    with pytest.raises(RuntimeError, match="synthetic event failure"):
        service.post_manager(
            OfficeContext(repo, tender["id"], root.run_id, actor_id=manager.id),
            [OfficeRecipientTarget(staff_id=staff.staff.id)],
            "note",
            "This must not be partially saved.",
            idempotency_key="rollback-message",
        )
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM office_messages").fetchone()[0] == 0
        assert conn.execute("SELECT COUNT(*) FROM office_message_recipients").fetchone()[0] == 0
        assert conn.execute("SELECT COUNT(*) FROM office_message_receipts").fetchone()[0] == 0


@pytest.mark.parametrize("summary", ["Actual authored staff finding.", "An actual staff finding. " * 600])
def test_staff_result_post_uses_only_saved_summary_and_result_identity(tmp_path, monkeypatch, summary):
    repo, tender, _connection, _model, _route, envelope, routing, staff, root, _artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    binding = routing.bind(
        root,
        staff.staff.id,
        staff.work_order.id,
        envelope.route_options[0].id,
        "result-message-binding",
    )
    assignments = StaffAssignmentService(repo)
    assignment = assignments.queue(root, binding.id, "result-message-queue")
    running = assignments.start(tender["id"], assignment.id, assignment.revision)
    prepared = PreparedOfficeResult(
        tender_id=tender["id"],
        run_id=root.run_id,
        output=OfficeOutput(summary=summary),
        usage={},
        source_ids_read=(),
        web_sources=(),
        approved_plan_id=root.scope_id,
        actor_id=staff.staff.id,
        staff_version=staff.staff.version,
        assignment_id=running.id,
        route_binding_id=binding.id,
    )
    result = assignments.save_result(
        tender["id"],
        running.id,
        PreparedStaffDraft(running.id, staff.staff.id, staff.staff.version, binding.id, prepared),
    )

    message_service = OfficeMessageService(repo)
    saved = message_service.post_staff_result(tender["id"], result.id, "result-message")
    assert saved.kind == "finding"
    assert saved.text == summary.strip()
    assert saved.sender.id == staff.staff.id
    assert saved.sender.assignment_id == running.id
    assert saved.artifact_refs[0].kind == "staff_result"
    assert saved.artifact_refs[0].staff_version == staff.staff.version
    assert (
        message_service.post_staff_result(tender["id"], result.id, "result-message").id == saved.id
    )


def test_output_reference_uses_saved_file_and_source_manifest_basis(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, envelope, routing, staff, root, artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    binding = routing.bind(
        root, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "output-binding"
    )
    assignment = StaffAssignmentService(repo).queue(root, binding.id, "output-queue")
    evidence = repo.artifact_evidence(tender["id"], artifact["id"])[0]
    output = {
        "id": "output-1",
        "tender_id": tender["id"],
        "filename": "tender-analysis-output.docx",
        "sha256": "a" * 64,
        "metadata": {
            "source_manifest": [
                {
                    "id": artifact["id"],
                    "version": artifact["version"],
                    "content_hash": artifact["content_hash"],
                    "is_current": True,
                    "source_id": evidence["id"],
                }
            ]
        },
    }
    with repo.atomic() as conn:
        conn.execute(
            "CREATE TABLE generated_outputs(id TEXT PRIMARY KEY,tender_id TEXT NOT NULL,record_json TEXT NOT NULL)"
        )
        conn.execute(
            "INSERT INTO generated_outputs(id,tender_id,record_json) VALUES(?,?,?)",
            (output["id"], tender["id"], json.dumps(output)),
        )
    manager = ManagerProfileService(repo).get()
    saved = OfficeMessageService(repo).post_manager(
        OfficeContext(repo, tender["id"], root.run_id, actor_id=manager.id),
        [OfficeRecipientTarget(staff_id=staff.staff.id, assignment_id=assignment.id)],
        "handoff",
        "Open the saved output for the assigned review.",
        reference_requests=[{"output_id": output["id"]}],
        idempotency_key="output-reference",
    )
    assert saved.artifact_refs[0].sha256 == output["sha256"]
    assert saved.artifact_refs[0].filename == output["filename"]
    # Even a planned target without a binding must not receive a new stale handoff.
    with repo.atomic() as conn:
        conn.execute("UPDATE artifacts SET is_current=0 WHERE id=?", (artifact["id"],))
    with pytest.raises(ValueError, match="current"):
        OfficeMessageService(repo).post_manager(
            OfficeContext(repo, tender["id"], root.run_id, actor_id=manager.id),
            [OfficeRecipientTarget(staff_id=staff.staff.id)],
            "handoff", "Use this old output", reference_requests=[{"output_id": output["id"]}],
            idempotency_key="stale-planning-output",
        )
