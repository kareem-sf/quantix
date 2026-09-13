"""Steering admission, turn-boundary application, and saved status reads."""

from __future__ import annotations

import pytest
from test_catalog_authority import configured_office

from quantix.execution_context import OfficeExecutionIdentity
from quantix.instruction_models import InstructionRevisionRequest
from quantix.office_instructions import OfficeInstructionService
from quantix.office_types import OfficeOutput
from quantix.staff_models import OfficeConflict


def _engineer(tender_id: str, root_run_id: str | None = None) -> OfficeExecutionIdentity:
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


def test_admit_replay_and_inactive_root_fencing(tmp_path, monkeypatch):
    repo, tender, *_ = configured_office(tmp_path, monkeypatch)
    run = repo.create_run(tender["id"], "manager", "Steered work")
    service = OfficeInstructionService(repo)

    first = service.admit(
        _engineer(tender["id"], run["id"]),
        InstructionRevisionRequest(
            kind="constraint",
            content="Keep the programme out of Ramadan.",
            idempotency_key="steering-once",
        ),
    )
    assert first.state == "admitted" and first.replayed is False
    replay = service.admit(
        _engineer(tender["id"], run["id"]),
        InstructionRevisionRequest(
            kind="constraint",
            content="Keep the programme out of Ramadan.",
            idempotency_key="steering-once",
        ),
    )
    assert replay.replayed is True and replay.id == first.id
    with pytest.raises(OfficeConflict):
        service.admit(
            _engineer(tender["id"], run["id"]),
            InstructionRevisionRequest(
                kind="urgent",
                content="Different content.",
                idempotency_key="steering-once",
            ),
        )
    with pytest.raises(ValueError):
        service.admit(
            _engineer(tender["id"], run["id"]),
            InstructionRevisionRequest(kind="constraint", content="   ", idempotency_key="blank"),
        )
    repo.update_run(run["id"], status="completed")
    with pytest.raises(OfficeConflict):
        service.admit(
            _engineer(tender["id"], run["id"]),
            InstructionRevisionRequest(
                kind="constraint",
                content="Too late.",
                idempotency_key="steering-late",
            ),
        )
    listed = service.list(tender["id"], run["id"])
    assert [item.kind for item in listed] == ["constraint"]


def test_status_reports_saved_owners_without_provider_work(tmp_path, monkeypatch):
    from test_staff_context import _build_staff_context, _context_workspace

    repo, tender, staff, binding, assignment_id, _artifact = _context_workspace(
        tmp_path, monkeypatch, tools=("read_source",)
    )
    context = _build_staff_context(repo, binding.id, assignment_id)
    snapshot = OfficeInstructionService(repo).status(_engineer(tender["id"], context.run_id))
    assert snapshot.provider_requests == 0
    assert staff.staff.id in snapshot.owners
    assert context.run_id in snapshot.owners


@pytest.mark.asyncio
async def test_steering_applies_at_the_next_turn_without_rewriting_results(tmp_path, monkeypatch):
    from quantix.office import _run as run_office

    repo, tender, *_ = configured_office(tmp_path, monkeypatch)
    run = repo.create_run(tender["id"], "manager", "Steered turn")
    service = OfficeInstructionService(repo)
    service.admit(
        _engineer(tender["id"], run["id"]),
        InstructionRevisionRequest(
            kind="constraint",
            content="Keep the programme out of Ramadan.",
            idempotency_key="steering-turn",
        ),
    )
    prompts: list[str] = []

    async def provider(route, connection, credentials, context, prompt, output_type, **options):
        prompts.append(prompt if isinstance(prompt, str) else str(prompt))
        reservation = await options["before_request"](200, 200)
        usage = {
            "requests": 1,
            "input_tokens": 200,
            "output_tokens": 100,
            "web_search_calls": 0,
            "usage_complete": True,
        }
        await options["on_response"](usage, reservation)
        return {
            "output": OfficeOutput(summary="Steady Manager turn."),
            "usage": usage,
            "web_sources": [],
        }

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    await run_office(repo, tender["id"], run["id"], "Review the package.")
    assert len(prompts) == 1
    assert "Engineer steering received during the previous step" in prompts[0]
    assert "Keep the programme out of Ramadan" in prompts[0]
    assert [item.state for item in service.list(tender["id"], run["id"])] == ["applied"]

    # Applied steering is not repeated on the following turn.
    prompts.clear()
    await run_office(repo, tender["id"], run["id"], "Review the package.")
    assert prompts and "Engineer steering" not in prompts[0]


@pytest.mark.asyncio
async def test_cancel_steering_interrupts_work_without_new_results(tmp_path, monkeypatch):
    import asyncio

    from quantix.office import _run as run_office

    repo, tender, *_ = configured_office(tmp_path, monkeypatch)
    run = repo.create_run(tender["id"], "manager", "Cancelled turn")
    service = OfficeInstructionService(repo)
    service.admit(
        _engineer(tender["id"], run["id"]),
        InstructionRevisionRequest(
            kind="cancel",
            content="Stop here.",
            idempotency_key="steering-cancel",
        ),
    )

    async def provider(route, connection, credentials, context, prompt, output_type, **options):
        raise AssertionError("Cancelled work must not reach the provider.")

    monkeypatch.setattr("quantix.ai_execution.execute_api", provider)
    with pytest.raises(asyncio.CancelledError):
        await run_office(repo, tender["id"], run["id"], "Review the package.")
    assert [item.state for item in service.list(tender["id"], run["id"])] == ["applied"]
