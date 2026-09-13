"""Atomic handoffs and repeated clarification within the same assignment."""

import json

import pytest
from test_staff_runtime import _workspace

from quantix.manager_runtime import ManagerRunProfiles
from quantix.office_manager_tools import manager_office_tools
from quantix.office_messages import OfficeMessageService
from quantix.office_tools import OfficeContext
from quantix.office_types import OfficeOutput
from quantix.staff_assignments import StaffAssignmentService
from quantix.staff_runtime import run_staff_assignment
from quantix.staff_runtime_models import StaffCompletedDraft, StaffProviderOutput, StaffQuestion


def provider_result(kind):
    async def provider(*_args, before_request, on_response, **_kwargs):
        reservation = await before_request(100, 100)
        usage = {"requests": 1, "input_tokens": 100, "output_tokens": 20, "web_search_calls": 0, "usage_complete": True}
        await on_response(usage, reservation)
        branch = StaffQuestion(question="Please clarify this comparison.") if kind == "question" else StaffCompletedDraft(output=OfficeOutput(summary="Actual staff draft."))
        return {"output": StaffProviderOutput(result=branch), "usage": usage, "web_sources": []}
    return provider


@pytest.mark.asyncio
@pytest.mark.parametrize("kind", ["question", "completed"])
async def test_message_and_assignment_outcome_commit_together(tmp_path, monkeypatch, kind):
    repo, tender, _, _, _, _, _, assignment, _ = _workspace(tmp_path, monkeypatch)
    monkeypatch.setattr("quantix.staff_runtime.execute_api", provider_result(kind))

    def fail(*_args, **_kwargs):
        raise ValueError("Synthetic handoff transaction failure")

    if kind == "question":
        monkeypatch.setattr(StaffAssignmentService, "wait_for_reply", fail)
    else:
        monkeypatch.setattr(OfficeMessageService, "post_staff_result", fail)
    with pytest.raises(ValueError, match="Synthetic handoff"):
        await run_staff_assignment(repo, tender["id"], assignment.id)
    assert StaffAssignmentService(repo).get(tender["id"], assignment.id).status == "failed"
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM office_staff_results").fetchone()[0] == 0
        assert conn.execute("SELECT COUNT(*) FROM office_messages").fetchone()[0] == 0


@pytest.mark.asyncio
async def test_second_question_has_its_own_receipt_after_real_manager_reply(tmp_path, monkeypatch):
    repo, tender, _, _, _, _, binding, assignment, _ = _workspace(tmp_path, monkeypatch)
    monkeypatch.setattr("quantix.staff_runtime.execute_api", provider_result("question"))
    first = await run_staff_assignment(repo, tender["id"], assignment.id)
    manager = ManagerRunProfiles(repo).get(tender["id"], binding.root_run_id)
    context = OfficeContext(repo, tender["id"], binding.root_run_id, actor_id=manager.id)
    answer = next(item for item in manager_office_tools(repo, tender["id"], binding.root_run_id, binding.plan_id) if item.name == "answer_staff_question")
    response = json.loads(await answer.invoke(context, {
        "assignment_id": assignment.id, "question_message_id": first.question_message_id,
        "text": "Use the north zone.",
    }, invocation_id="actual-reply"))
    assert response["assignment"]["status"] == "queued"
    second = await run_staff_assignment(repo, tender["id"], assignment.id)
    assert second.assignment.status == "waiting"
    assert second.question_message_id != first.question_message_id
    assert len([message for message in OfficeMessageService(repo).page(tender["id"]).items if message.kind == "question"]) == 2
