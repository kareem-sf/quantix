"""Persistence contract tests for generated Tender Office staff."""

from __future__ import annotations

import hashlib
from concurrent.futures import ThreadPoolExecutor

import pytest

from quantix import diagnostics
from quantix.manager_profile import ManagerProfileService, OfficeConflict
from quantix.repository import Repository
from quantix.staff_models import (
    ManagerCreationContext,
    Personality,
    StaffProfileDraft,
    StaffWorkOrder,
)
from quantix.staff_store import StaffStore


@pytest.fixture(autouse=True)
def close_temporary_diagnostics():
    """Close only a writer created by a Repository in this test."""

    before = diagnostics._writer
    yield
    after = diagnostics._writer
    if after is not None and after is not before:
        after.close()


def _personality(**overrides):
    values = {
        "description": "A careful construction engineering colleague.",
        "traits": ["careful", "direct"],
        "communication_style": "State the evidence and next action clearly.",
        "problem_solving_style": "Break the work into bounded checks.",
        "collaboration_style": "Keep ownership and handoffs explicit.",
        "uncertainty_handling": "Label assumptions and request missing evidence.",
        "initiative": "Propose the next safe step when supported.",
        "explanation_style": "Lead with the answer and cite the source.",
        "language_preferences": ["English"],
        "working_habits": ["Keep source references with conclusions."],
    }
    values.update(overrides)
    return Personality(**values)


def _profile(**overrides):
    values = {
        "display_name": "Maya Nasser",
        "role": "Unregistered package reconciliation analyst",
        "title": "Tender Reconciliation Lead",
        "specialisms": ["Cross-package quantity reconciliation"],
        "persona": "A practical analyst who keeps proposed adjustments traceable.",
        "personality": _personality(),
        "responsibilities": ["Reconcile quantities across received schedules."],
        "objectives": ["Produce a reviewable variance list."],
        "methods": ["Compare schedules and cite exact source rows."],
        "deliverables": ["A variance list with source references."],
        "success_criteria": ["Every material variance has a source and status."],
        "context_needs": ["Current quantity schedules and Tender revision."],
        "requested_tool_ids": [],
        "creation_reason": "Created to reconcile the quantity schedules for this task.",
    }
    values.update(overrides)
    return StaffProfileDraft(**values)


def _order(**overrides):
    values = {
        "brief": "Review the received package and identify material quantity gaps.",
        "goal": "Give the Tender Manager a source-backed gap list.",
        "source_ids": [],
        "expected_outputs": ["Source-backed gap list"],
        "completion_checks": ["Each gap has a source or explicit missing-source note"],
    }
    values.update(overrides)
    return StaffWorkOrder(**values)


def _workspace(tmp_path):
    repo = Repository(tmp_path / "quantix")
    manager = ManagerProfileService(repo)
    tender = repo.create_tender("Synthetic staff Tender")
    run = repo.create_run(tender["id"], "conversation", "Create one colleague")
    context = ManagerCreationContext(tender["id"], run["id"], manager.get().version, "scope-1")
    return repo, manager, tender["id"], run["id"], context, StaffStore(repo)


def _source(repo, tender_id, name="Sources/spec.pdf", body=b"synthetic source"):
    digest = hashlib.sha256(body).hexdigest()
    artifact, _ = repo.register_artifact(
        tender_id,
        name,
        digest,
        len(body),
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [{"locator": "page:1", "text": body.decode()}],
        },
    )
    return repo.artifact_evidence(tender_id, artifact["id"])[0]["id"]


def test_fresh_tender_has_no_generated_staff(tmp_path):
    _repo, _manager, tender_id, _run_id, _context, store = _workspace(tmp_path)

    assert store.list_staff(tender_id) == []


def test_generated_profile_and_assignment_persist_unfamiliar_fields_and_sources(tmp_path):
    repo, _manager, tender_id, _run_id, context, store = _workspace(tmp_path)
    source_id = _source(repo, tender_id)
    profile = _profile(
        role="Carbon ledger field synthesiser",
        title="Embodied Carbon Reconciliation Specialist",
        requested_tool_ids=["tool-source-index"],
    )

    receipt = store.create_generated(
        context,
        profile,
        _order(source_ids=[source_id]),
        "create-1",
    )

    assert receipt.replayed is False
    assert receipt.staff.id
    assert receipt.staff.version == 1
    assert receipt.staff.lifecycle == "available"
    assert receipt.staff.portrait.style == "notionists-v1"
    assert receipt.staff.portrait.seed == receipt.staff.id
    assert receipt.staff.role == "Carbon ledger field synthesiser"
    assert receipt.staff.requested_tool_ids == ["tool-source-index"]
    assert receipt.work_order.staff_version == 1
    assert receipt.work_order.work_order.source_ids == [source_id]
    assert store.list_staff(tender_id) == [receipt.staff]
    assert store.get_work_order(tender_id, receipt.work_order.id) == receipt.work_order


def test_revision_adds_immutable_version_and_keeps_old_assignment_snapshot(tmp_path):
    repo, manager, tender_id, _run_id, context, store = _workspace(tmp_path)
    source_id = _source(repo, tender_id)
    first = store.create_generated(context, _profile(), _order(source_ids=[source_id]), "create-1")
    second_run = repo.create_run(tender_id, "conversation", "Adapt colleague")
    second_context = ManagerCreationContext(
        tender_id, second_run["id"], manager.get().version, "scope-1"
    )

    revision = store.revise_generated(
        second_context,
        first.staff.id,
        1,
        _profile(display_name="Youssef Haddad", title="Package Evidence Lead"),
        "revise-1",
    )

    assert revision.replayed is False
    assert revision.staff.version == 2
    assert revision.staff.display_name == "Youssef Haddad"
    assert revision.staff.portrait == first.staff.portrait
    assert store.get_staff(tender_id, first.staff.id, 1).display_name == "Maya Nasser"
    assert store.get_staff(tender_id, first.staff.id).version == 2
    old_order = store.get_work_order(tender_id, first.work_order.id)
    assert old_order.staff_version == 1
    assert old_order.work_order == first.work_order.work_order


def test_same_idempotency_key_replays_and_changed_body_conflicts(tmp_path):
    _repo, _manager, tender_id, _run_id, context, store = _workspace(tmp_path)
    first = store.create_generated(context, _profile(), _order(), "same-key")

    replay = store.create_generated(
        context,
        _profile(display_name="Maya Nasser"),
        _order(),
        "same-key",
    )
    assert replay.replayed is True
    assert replay.staff.id == first.staff.id
    assert store.list_staff(tender_id) == [first.staff]

    with pytest.raises(OfficeConflict):
        store.create_generated(context, _profile(display_name="Different colleague"), _order(), "same-key")

    changed_scope = ManagerCreationContext(
        context.tender_id,
        context.run_id,
        context.manager_profile_version,
        "different-scope",
    )
    with pytest.raises(OfficeConflict):
        store.create_generated(changed_scope, _profile(), _order(), "same-key")

    changed_manager_version = ManagerCreationContext(
        context.tender_id,
        context.run_id,
        context.manager_profile_version + 1,
        context.scope_id,
    )
    with pytest.raises(OfficeConflict):
        store.create_generated(changed_manager_version, _profile(), _order(), "same-key")


def test_same_key_is_scoped_to_tender_and_cross_tender_sources_are_rejected(tmp_path):
    repo, manager, tender_id, _run_id, context, store = _workspace(tmp_path)
    other = repo.create_tender("Other synthetic Tender")
    other_run = repo.create_run(other["id"], "conversation", "Other work")
    other_context = ManagerCreationContext(
        other["id"], other_run["id"], manager.get().version, "scope-2"
    )
    other_source = _source(repo, other["id"], name="Sources/other.pdf", body=b"other source")

    first = store.create_generated(context, _profile(), _order(), "same-key")
    second = store.create_generated(
        other_context,
        _profile(display_name="Other Tender colleague"),
        _order(source_ids=[other_source]),
        "same-key",
    )
    assert first.staff.id != second.staff.id

    with pytest.raises(ValueError):
        store.create_generated(context, _profile(), _order(source_ids=[other_source]), "cross-source")
    with pytest.raises(KeyError):
        store.get_staff(other["id"], first.staff.id)
    with pytest.raises(KeyError):
        store.get_work_order(other["id"], first.work_order.id)


def test_terminal_run_allows_stable_replay_but_rejects_new_mutation(tmp_path):
    repo, _manager, tender_id, run_id, context, store = _workspace(tmp_path)
    first = store.create_generated(context, _profile(), _order(), "terminal-key")
    repo.update_run(run_id, status="completed")

    replay = store.create_generated(context, _profile(), _order(), "terminal-key")
    assert replay.replayed is True
    assert replay.staff.id == first.staff.id
    with pytest.raises(OfficeConflict):
        store.create_generated(context, _profile(display_name="New colleague"), _order(), "new-key")


def test_outer_transaction_rollback_removes_staff_order_and_receipt(tmp_path):
    repo, _manager, tender_id, _run_id, context, store = _workspace(tmp_path)

    with pytest.raises(RuntimeError):
        with repo.atomic():
            store.create_generated(context, _profile(), _order(), "rollback-key")
            raise RuntimeError("synthetic event append failed")

    assert store.list_staff(tender_id) == []
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM office_staff").fetchone()[0] == 0
        assert conn.execute("SELECT COUNT(*) FROM office_work_orders").fetchone()[0] == 0
        assert conn.execute("SELECT COUNT(*) FROM office_generation_receipts").fetchone()[0] == 0


def test_concurrent_duplicate_creation_has_one_identity_and_replays(tmp_path):
    repo, _manager, tender_id, _run_id, context, store = _workspace(tmp_path)

    def create():
        return store.create_generated(context, _profile(), _order(), "concurrent-key")

    with ThreadPoolExecutor(max_workers=6) as workers:
        receipts = list(workers.map(lambda _item: create(), range(12)))

    assert {receipt.staff.id for receipt in receipts} == {receipts[0].staff.id}
    assert sum(not receipt.replayed for receipt in receipts) == 1
    assert len(store.list_staff(tender_id)) == 1


def test_creation_does_not_change_unrelated_authority_rows(tmp_path):
    repo, _manager, tender_id, _run_id, context, store = _workspace(tmp_path)
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "CREATE TABLE synthetic_ai_authority (tender_id TEXT PRIMARY KEY, revision INTEGER NOT NULL, data_json TEXT NOT NULL)"
        )
        conn.execute(
            "INSERT INTO synthetic_ai_authority VALUES(?,?,?)",
            (tender_id, 4, '{"allowed": ["synthetic-model"]}'),
        )
        before = conn.execute("SELECT * FROM synthetic_ai_authority").fetchone()

    store.create_generated(context, _profile(), _order(), "authority-key")

    with repo.db.connect() as conn:
        assert conn.execute("SELECT * FROM synthetic_ai_authority").fetchone() == before
