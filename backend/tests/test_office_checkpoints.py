"""Checkpoint lineage, effect reconciliation, and resume surfacing."""

from __future__ import annotations

import pytest

from quantix.db import now
from quantix.execution_context import OfficeExecutionIdentity
from quantix.office_checkpoints import (
    CheckpointDraft,
    OfficeCheckpointService,
    ResumeCheckpointRequest,
)
from quantix.repository import Repository


def _ctx(tender_id: str, root_run_id: str | None = None) -> OfficeExecutionIdentity:
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


def _lineage(repo, tender_id: str):
    plan = repo.create_plan(
        tender_id,
        "Resume plan",
        [{"title": "Review", "role": "Reviewer", "description": "Review", "source_ids": []}],
    )
    origin = repo.create_run(tender_id, "manager", "Original work")
    repo.update_run(origin["id"], status="failed")
    resumed = repo.create_run(tender_id, "manager", "Original work")
    stamp = now()
    from quantix.office_checkpoints import OfficeCheckpointService as _Checkpoints
    from quantix.office_resume import OfficeResumeService as _ResumeOrigins
    from quantix.staff_routing import StaffRoutingService as _Routing

    _Routing(repo)
    _Checkpoints(repo)
    _ResumeOrigins(repo)
    with repo.atomic() as conn:
        conn.execute(
            "INSERT INTO office_delegation_grants(id,tender_id,plan_id,review_fingerprint,policy_revision,envelope_json,created_at)"
            " VALUES(?,?,?,?,?,?,?)",
            ("grant-1", tender_id, plan["id"], "f" * 64, 0, "{}", stamp),
        )
        conn.execute(
            "INSERT INTO decisions VALUES(?,?,?,?,?,?,?)",
            ("decision-1", tender_id, "office_run", resumed["id"], "resume", "Resume.", stamp),
        )
        conn.execute(
            "INSERT INTO office_resume_origins VALUES(?,?,?,?,?,?,?,?)",
            (
                resumed["id"],
                tender_id,
                plan["id"],
                "grant-1",
                origin["id"],
                origin["id"],
                "decision-1",
                stamp,
            ),
        )
    return origin["id"], resumed["id"]


def test_checkpoint_reuse_requires_resume_lineage(tmp_path):
    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Checkpoint Tender")
    origin_id, resumed_id = _lineage(repo, tender["id"])
    other = repo.create_run(tender["id"], "manager", "Unrelated work")
    service = OfficeCheckpointService(repo)
    saved = service.checkpoint(
        _ctx(tender["id"], origin_id),
        CheckpointDraft(
            step_id="extraction",
            input_fingerprint="extract-v1",
            outputs={"pages": 4},
            remaining_dependencies=["calculation"],
        ),
    )
    reused = service.resume_checkpoint(
        _ctx(tender["id"], resumed_id),
        ResumeCheckpointRequest(
            checkpoint_id=saved.id,
            expected_basis_fingerprint="extract-v1",
            idempotency_key="lineage-reuse",
        ),
    )
    assert reused.outputs == {"pages": 4}
    again = service.resume_checkpoint(
        _ctx(tender["id"], resumed_id),
        ResumeCheckpointRequest(
            checkpoint_id=saved.id,
            expected_basis_fingerprint="extract-v1",
            idempotency_key="lineage-reuse-again",
        ),
    )
    assert again.id == saved.id
    with pytest.raises(ValueError, match="unrelated work"):
        service.resume_checkpoint(
            _ctx(tender["id"], other["id"]),
            ResumeCheckpointRequest(
                checkpoint_id=saved.id,
                expected_basis_fingerprint="extract-v1",
                idempotency_key="lineage-stranger",
            ),
        )
    with pytest.raises(ValueError, match="basis changed"):
        service.resume_checkpoint(
            _ctx(tender["id"], resumed_id),
            ResumeCheckpointRequest(
                checkpoint_id=saved.id,
                expected_basis_fingerprint="extract-v2",
                idempotency_key="lineage-stale",
            ),
        )
    with repo.db.connect() as conn:
        reuses = conn.execute(
            "SELECT COUNT(*) FROM office_checkpoint_reuses WHERE checkpoint_id=?", (saved.id,)
        ).fetchone()[0]
    assert reuses == 1

    listed = service.reusable_for_root(tender["id"], resumed_id)
    assert [(item.checkpoint_id, item.step_id) for item in listed] == [(saved.id, "extraction")]
    assert service.reusable_for_root(tender["id"], other["id"]) == []


def test_effect_state_machine_rejects_unsafe_jumps(tmp_path):
    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Effect Tender")
    service = OfficeCheckpointService(repo)
    ctx = _ctx(tender["id"], None)

    assert service.effect_state(tender["id"], "smtp-send") is None
    assert service.record_effect(ctx, "smtp-send", "uncertain").state == "uncertain"
    assert service.effect_state(tender["id"], "smtp-send") == "uncertain"
    with pytest.raises(ValueError, match="cannot move"):
        service.record_effect(ctx, "smtp-send", "prepared")
    assert service.record_effect(ctx, "smtp-send", "attempted").state == "attempted"
    assert service.record_effect(ctx, "smtp-send", "confirmed").state == "confirmed"
    assert service.effect_state(tender["id"], "smtp-send") == "confirmed"
    with pytest.raises(ValueError, match="cannot move"):
        service.record_effect(ctx, "smtp-send", "attempted")
    with pytest.raises(ValueError, match="operation key"):
        service.record_effect(ctx, "  ", "prepared")
