"""Real Manager delegation tools queue work without entering a provider lease."""

import json

import pytest
from test_staff_routing import _ready_binding_workspace

from quantix.manager_runtime import ManagerRunProfiles
from quantix.office_manager_tools import manager_office_tools
from quantix.office_messages import OfficeMessageService
from quantix.office_tools import OfficeContext
from quantix.staff_assignments import StaffAssignmentService
from quantix.staff_context import build_staff_context


def manager_workspace(tmp_path, monkeypatch):
    repo, tender, _, _, _, envelope, _, staff, root, _ = _ready_binding_workspace(tmp_path, monkeypatch)
    pinned = ManagerRunProfiles(repo).get(tender["id"], root.run_id)
    context = OfficeContext(repo, tender["id"], root.run_id, actor_id=pinned.id,
                            approved_scope=repo.approved_scope(tender["id"], root.scope_id))
    definitions = {item.name: item for item in manager_office_tools(repo, tender["id"], root.run_id, root.scope_id)}
    args = {"staff_id": staff.staff.id, "work_order_id": staff.work_order.id, "route_option_id": envelope.route_options[0].id}
    return repo, tender, context, definitions, args


@pytest.mark.asyncio
async def test_execute_staff_queues_once_without_calling_provider(tmp_path, monkeypatch):
    repo, tender, context, tools, args = manager_workspace(tmp_path, monkeypatch)
    original_runs = repo.list_runs(tender["id"])
    monkeypatch.setattr("quantix.ai_execution.execute_api", lambda *_a, **_k: pytest.fail("A Manager tool must only queue staff"))
    first = json.loads(await tools["execute_staff"].invoke(context, args, invocation_id="queue-once"))
    second = json.loads(await tools["execute_staff"].invoke(context, args, invocation_id="queue-once"))
    assert first["assignment"]["id"] == second["assignment"]["id"]
    assert first["assignment"]["status"] == "queued"
    assert len(StaffAssignmentService(repo).list(tender["id"])) == 1
    assert len(OfficeMessageService(repo).page(tender["id"]).items) == 1
    # Display activity may advance; work identity, result and spending cannot.
    assert [{k: v for k, v in run.items() if k not in {"detail", "updated_at"}}
            for run in repo.list_runs(tender["id"])] == [
        {k: v for k, v in run.items() if k not in {"detail", "updated_at"}} for run in original_runs]


@pytest.mark.asyncio
async def test_question_reply_requeues_same_assignment_only_once_and_keeps_private_context(tmp_path, monkeypatch):
    repo, tender, context, tools, args = manager_workspace(tmp_path, monkeypatch)
    original_runs = repo.list_runs(tender["id"])
    queued = json.loads(await tools["execute_staff"].invoke(context, args, invocation_id="question-work"))["assignment"]
    assignments = StaffAssignmentService(repo)
    running = assignments.start(tender["id"], queued["id"], queued["revision"])
    staff_context = build_staff_context(repo, running.route_binding_id, running.id)
    messages = OfficeMessageService(repo)
    question = messages.post_staff(staff_context, "question", "Which wall zone should I compare?", idempotency_key="actual-question")
    waiting = assignments.wait_for_reply(tender["id"], running.id, running.revision, question.text)
    reply_args = {"assignment_id": running.id, "question_message_id": question.id, "text": "Compare the north wall only."}
    response = json.loads(await tools["answer_staff_question"].invoke(context, reply_args, invocation_id="answer-once"))
    replay = json.loads(await tools["answer_staff_question"].invoke(context, reply_args, invocation_id="answer-once"))
    assert response == replay
    assert response["assignment"]["id"] == waiting.id
    assert response["assignment"]["status"] == "queued"
    assert response["assignment"]["revision"] == waiting.revision + 1
    resumed_context = build_staff_context(repo, running.route_binding_id, running.id)
    assert resumed_context.history[-1]["text"] == reply_args["text"]
    assert resumed_context.seen_sources == set()
    assert [{k: v for k, v in run.items() if k not in {"detail", "updated_at"}}
            for run in repo.list_runs(tender["id"])] == [
        {k: v for k, v in run.items() if k not in {"detail", "updated_at"}} for run in original_runs]


@pytest.mark.asyncio
async def test_reply_to_non_question_cannot_requeue_or_save_a_message(tmp_path, monkeypatch):
    repo, tender, context, tools, args = manager_workspace(tmp_path, monkeypatch)
    queued = json.loads(await tools["execute_staff"].invoke(context, args, invocation_id="work"))["assignment"]
    assignments = StaffAssignmentService(repo)
    running = assignments.start(tender["id"], queued["id"], queued["revision"])
    staff_context = build_staff_context(repo, running.route_binding_id, running.id)
    messages = OfficeMessageService(repo)
    note = messages.post_staff(staff_context, "note", "An authored note", idempotency_key="actual-note")
    assignments.wait_for_reply(tender["id"], running.id, running.revision, "Waiting for clarification.")
    before = messages.page(tender["id"])
    with pytest.raises(ValueError, match="question"):
        await tools["answer_staff_question"].invoke(context, {"assignment_id": running.id, "question_message_id": note.id, "text": "Continue"}, invocation_id="invalid-answer")
    assert messages.page(tender["id"]) == before
    assert assignments.get(tender["id"], running.id).status == "waiting"
