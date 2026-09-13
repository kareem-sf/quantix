"""Synthetic provider-boundary checks for one generated staff turn."""

from __future__ import annotations

import asyncio
import json

import pytest
from test_staff_routing import _ready_binding_workspace

from quantix.office_messages import OfficeMessageService
from quantix.office_types import OfficeOutput
from quantix.staff_assignment_models import StaffResult
from quantix.staff_assignments import StaffAssignmentService
from quantix.staff_runtime import run_staff_assignment
from quantix.staff_runtime_models import (
    StaffCompletedDraft,
    StaffProviderOutput,
    StaffQuestion,
)


def _workspace(tmp_path, monkeypatch):
    (
        repo,
        tender,
        connection,
        model,
        route,
        envelope,
        routing,
        staff,
        root_context,
        artifact,
    ) = _ready_binding_workspace(
        tmp_path,
        monkeypatch,
        requested_tool_ids=["read_source"],
        envelope_tools=("read_source",),
    )
    binding = routing.bind(
        root_context,
        staff.staff.id,
        staff.work_order.id,
        envelope.route_options[0].id,
        "runtime-binding",
    )
    assignment = StaffAssignmentService(repo).queue(root_context, binding.id, "runtime-assignment")
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    return repo, tender, connection, model, route, staff, binding, assignment, source_id


@pytest.mark.asyncio
async def test_runtime_uses_bound_role_context_route_and_root_meter(tmp_path, monkeypatch):
    repo, tender, connection, model, route, staff, binding, assignment, source_id = _workspace(
        tmp_path, monkeypatch
    )
    calls = {}

    async def fake_execute(
        selected_route,
        selected_connection,
        credentials,
        context,
        instruction,
        output_type,
        *,
        before_request,
        on_response,
        **kwargs,
    ):
        calls.update(
            route=selected_route,
            connection=selected_connection,
            credentials=credentials,
            context=context,
            instruction=instruction,
            output_type=output_type,
            definitions=kwargs["definitions"],
        )
        assert context.staff_profile.role == staff.staff.role
        assert context.staff_work_order.id == staff.work_order.id
        assert "prior Manager context" not in instruction
        assert "Manager's whole Tender" not in instruction
        assert {item.name for item in calls["definitions"]} == {"read_source"}

        read_source = calls["definitions"][0]
        await read_source.invoke(context, {"source_id": source_id, "offset": 0, "limit": 20})
        reservation = await before_request(200, selected_route["max_output_tokens"])
        await on_response(
            {
                "requests": 1,
                "input_tokens": 20,
                "output_tokens": 12,
                "usage_complete": True,
            },
            reservation,
        )
        return {
            "output": StaffProviderOutput(
                result=StaffCompletedDraft(
                    output=OfficeOutput(
                        summary="The reviewed source was checked.",
                        source_ids=[source_id],
                    ),
                    authored_notes=["Ready for Manager review."],
                )
            ),
            "usage": {"requests": 1, "input_tokens": 20, "output_tokens": 12},
            "web_sources": [],
        }

    monkeypatch.setattr("quantix.staff_runtime.execute_api", fake_execute)
    outcome = await run_staff_assignment(repo, tender["id"], assignment.id)

    assert outcome.assignment.status == "completed"
    assert outcome.result_id
    assert outcome.question_message_id is None
    assert outcome.usage["requests"] == 1
    assert calls["route"] == binding.route.model_dump(mode="json")
    assert calls["connection"]["id"] == connection["id"]
    assert calls["connection"]["_model"]["model_id"] == model["model_id"]
    assert calls["credentials"] == {"api_key": "synthetic-key"}
    assert calls["output_type"] is StaffProviderOutput
    saved = StaffAssignmentService(repo).get_result(tender["id"], outcome.result_id)
    assert isinstance(saved, StaffResult)
    assert saved.source_ids_read == (source_id,)
    assert saved.source_bases[0].source_id == source_id
    assert saved.office_output.summary == "The reviewed source was checked."

    messages = OfficeMessageService(repo).page(tender["id"], assignment_id=assignment.id).items
    assert messages[-1].kind == "finding"
    assert messages[-1].sender.id == staff.staff.id
    assert messages[-1].artifact_refs[0].id == outcome.result_id

    with repo.db.connect() as conn:
        usage_rows = conn.execute(
            "SELECT data_json FROM ai_usage WHERE tender_id=? AND run_id=?",
            (tender["id"], binding.root_run_id),
        ).fetchall()
    assert len(usage_rows) == 1
    assert json.loads(usage_rows[0][0])["actor_id"] == staff.staff.id


@pytest.mark.asyncio
async def test_runtime_persists_actual_question_and_waits_without_fake_result(
    tmp_path, monkeypatch
):
    repo, tender, _connection, _model, _route, _staff, _binding, assignment, _source_id = (
        _workspace(tmp_path, monkeypatch)
    )

    async def fake_execute(*args, before_request, on_response, **kwargs):
        reservation = await before_request(100, 128)
        await on_response(
            {
                "requests": 1,
                "input_tokens": 8,
                "output_tokens": 9,
                "usage_complete": True,
            },
            reservation,
        )
        return {
            "output": StaffProviderOutput(
                result=StaffQuestion(question="Which drawing revision should I use?")
            ),
            "usage": {"requests": 1, "input_tokens": 8, "output_tokens": 9},
            "web_sources": [],
        }

    monkeypatch.setattr("quantix.staff_runtime.execute_api", fake_execute)
    outcome = await run_staff_assignment(repo, tender["id"], assignment.id)

    assert outcome.assignment.status == "waiting"
    assert outcome.result_id is None
    assert outcome.question_message_id
    message = OfficeMessageService(repo).get(tender["id"], outcome.question_message_id)
    assert message.kind == "question"
    assert message.text == "Which drawing revision should I use?"
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM office_staff_results").fetchone()[0] == 0


@pytest.mark.asyncio
async def test_runtime_rejects_unread_source_and_marks_assignment_failed(tmp_path, monkeypatch):
    repo, tender, _connection, _model, _route, _staff, _binding, assignment, _source_id = (
        _workspace(tmp_path, monkeypatch)
    )
    missing_source_id = "unread-source-id"

    async def fake_execute(*args, before_request, on_response, **kwargs):
        reservation = await before_request(100, 128)
        await on_response(
            {"requests": 1, "input_tokens": 8, "output_tokens": 9, "usage_complete": True},
            reservation,
        )
        return {
            "output": StaffProviderOutput(
                result=StaffCompletedDraft(
                    output=OfficeOutput(
                        summary="Unsupported citation.", source_ids=[missing_source_id]
                    )
                )
            ),
            "usage": {"requests": 1},
            "web_sources": [],
        }

    monkeypatch.setattr("quantix.staff_runtime.execute_api", fake_execute)
    with pytest.raises(ValueError, match="read|source"):
        await run_staff_assignment(repo, tender["id"], assignment.id)
    assert StaffAssignmentService(repo).get(tender["id"], assignment.id).status == "failed"
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM office_staff_results").fetchone()[0] == 0


@pytest.mark.asyncio
async def test_runtime_cancellation_marks_assignment_interrupted_and_keeps_charge_uncertain(
    tmp_path, monkeypatch
):
    repo, tender, _connection, _model, _route, _staff, binding, assignment, _source_id = _workspace(
        tmp_path, monkeypatch
    )
    admitted = asyncio.Event()
    release = asyncio.Event()

    async def fake_execute(*args, before_request, on_response, **kwargs):
        reservation = await before_request(100, 128)
        admitted.set()
        await release.wait()
        await on_response(
            {"requests": 1, "input_tokens": 8, "output_tokens": 9, "usage_complete": True},
            reservation,
        )
        return {
            "output": StaffProviderOutput(
                result=StaffCompletedDraft(output=OfficeOutput(summary="Late draft."))
            ),
            "usage": {"requests": 1},
            "web_sources": [],
        }

    monkeypatch.setattr("quantix.staff_runtime.execute_api", fake_execute)
    task = asyncio.create_task(run_staff_assignment(repo, tender["id"], assignment.id))
    await admitted.wait()
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task
    assert StaffAssignmentService(repo).get(tender["id"], assignment.id).status == "interrupted"
    with repo.db.connect() as conn:
        row = conn.execute(
            "SELECT data_json FROM ai_usage WHERE tender_id=? AND run_id=?",
            (tender["id"], binding.root_run_id),
        ).fetchone()
    assert json.loads(row[0])["status"] == "uncertain"
