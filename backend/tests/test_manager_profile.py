"""Persistence and contract tests for the customizable Tender Manager."""

import re
from concurrent.futures import ThreadPoolExecutor
from dataclasses import FrozenInstanceError

import pytest
from pydantic import ValidationError

from quantix import diagnostics
from quantix.manager_profile import ManagerProfileService, OfficeConflict
from quantix.repository import Repository
from quantix.staff_models import (
    ManagerCreationContext,
    ManagerProfileEdit,
    Personality,
    StaffProfileDraft,
    StaffWorkOrder,
)


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
        "description": "A calm, evidence-led construction engineering coordinator.",
        "traits": ["careful", "direct"],
        "communication_style": "Clear and concise, with the next decision stated first.",
        "problem_solving_style": "Break the issue into bounded checks and trace each conclusion.",
        "collaboration_style": "Invite focused input and assign ownership explicitly.",
        "uncertainty_handling": "Label assumptions and request the missing source or decision.",
        "initiative": "Propose the next safe step when the available evidence is sufficient.",
        "explanation_style": "Use plain language with short supporting detail.",
        "language_preferences": ["English"],
        "working_habits": ["Keep source references with every material conclusion."],
    }
    values.update(overrides)
    return Personality(**values)


def _draft(**overrides):
    values = {
        "display_name": "Amina El-Sayed",
        "role": "Package reconciliation analyst",
        "title": "Tender Reconciliation Lead",
        "specialisms": ["Cross-package quantity reconciliation"],
        "persona": "A practical analyst who keeps every proposed adjustment traceable.",
        "personality": _personality(),
        "responsibilities": ["Reconcile quantities across the received schedules."],
        "objectives": ["Produce a reviewable variance list."],
        "methods": ["Compare the schedules and cite the exact source rows."],
        "deliverables": ["A variance list with source references."],
        "success_criteria": ["Every material variance has a source and status."],
        "context_needs": ["Current quantity schedules and the Tender revision."],
        "requested_tool_ids": [],
        "creation_reason": "Created to reconcile the quantity schedules for this Tender task.",
    }
    values.update(overrides)
    return StaffProfileDraft(**values)


def _edit(version, **overrides):
    values = {
        "expected_version": version,
        "display_name": "Tender Manager",
        "title": "Tender Manager",
        "persona": "A neutral, evidence-led coordinator for construction tenders.",
        "personality": _personality(),
        "working_preferences": ["State the next action clearly."],
    }
    values.update(overrides)
    return ManagerProfileEdit(**values)


def test_fresh_home_has_one_manager_and_reopening_retains_id(tmp_path):
    repo = Repository(tmp_path)
    service = ManagerProfileService(repo)

    first = service.get()
    assert first.display_name == "Tender Manager"
    assert first.version == 1
    assert re.fullmatch(r"[a-f0-9]{32}", first.id)

    reopened = ManagerProfileService(Repository(tmp_path)).get()
    assert reopened.id == first.id
    assert reopened.version == first.version


def test_custom_profile_is_arbitrary_and_old_version_remains_immutable(tmp_path):
    service = ManagerProfileService(Repository(tmp_path))
    original = service.get()

    updated = service.update(
        _edit(
            original.version,
            display_name="Rania Haddad",
            title="Commercial Tender Coordinator",
            persona="Ask precise questions, explain tradeoffs, and keep the engineer in control.",
            personality=_personality(
                communication_style="Use measured Arabic and English terminology with a short action list.",
                initiative="Suggest one useful follow-up whenever evidence supports it.",
                language_preferences=["Arabic", "English"],
            ),
            working_preferences=["Show assumptions", "Keep references beside findings"],
        )
    )

    assert updated.id == original.id
    assert updated.version == original.version + 1
    assert updated.display_name == "Rania Haddad"
    assert updated.personality.language_preferences == ["Arabic", "English"]
    assert service.profile_version(original.version).display_name == "Tender Manager"
    assert service.profile_version(original.version).version == original.version
    assert ManagerProfileService(Repository(tmp_path)).get().display_name == "Rania Haddad"


def test_stale_expected_version_raises_office_conflict(tmp_path):
    service = ManagerProfileService(Repository(tmp_path))
    original = service.get()
    service.update(_edit(original.version, display_name="First update"))

    with pytest.raises(OfficeConflict):
        service.update(_edit(original.version, display_name="Stale update"))


def test_concurrent_initialization_creates_one_manager(tmp_path):
    def initialize():
        return ManagerProfileService(Repository(tmp_path)).get()

    with ThreadPoolExecutor(max_workers=6) as workers:
        managers = list(workers.map(lambda _: initialize(), range(12)))

    assert {manager.id for manager in managers} == {managers[0].id}
    assert {manager.version for manager in managers} == {1}


def test_server_owned_and_unknown_fields_are_rejected():
    with pytest.raises(ValidationError):
        _edit(1, account_id="secret", grants=["send_supplier_email"])

    with pytest.raises(ValidationError):
        _draft(id="forged", creator_id="engineer", created_at="now")


def test_generic_dtos_allow_arbitrary_roles_and_require_professional_fields():
    draft = _draft(role="Unregistered field synthesis coordinator", title="Novel Title")
    assert draft.role == "Unregistered field synthesis coordinator"

    with pytest.raises(ValidationError):
        _draft(responsibilities=[])
    with pytest.raises(ValidationError):
        _draft(personality=None)
    with pytest.raises(ValidationError):
        _draft(success_criteria=[])

    work_order = StaffWorkOrder(
        brief="Review the received package and identify material gaps.",
        goal="Give the Tender Manager a source-backed gap list.",
        source_ids=[],
        expected_outputs=["Source-backed gap list"],
        completion_checks=["Each gap has a source or an explicit missing-source note"],
    )
    assert work_order.source_ids == []


def test_manager_creation_context_is_frozen_server_only_dataclass():
    context = ManagerCreationContext("tender", "run", 1, "scope")
    with pytest.raises(FrozenInstanceError):
        context.run_id = "other"


def test_profile_edit_leaves_existing_ai_policy_and_account_snapshot_unchanged(tmp_path):
    repo = Repository(tmp_path)
    tender_id = repo.create_tender("Synthetic Manager boundary")["id"]
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "CREATE TABLE ai_connections (id TEXT PRIMARY KEY, data_json TEXT NOT NULL, credential_ref TEXT, credential_mode TEXT NOT NULL)"
        )
        conn.execute(
            "CREATE TABLE ai_setup_accounts (connection_id TEXT PRIMARY KEY REFERENCES ai_connections(id), data_json TEXT NOT NULL)"
        )
        conn.execute(
            "CREATE TABLE tender_ai_policy (tender_id TEXT PRIMARY KEY REFERENCES tenders(id), revision INTEGER NOT NULL, data_json TEXT NOT NULL, updated_at TEXT NOT NULL)"
        )
        conn.execute(
            "INSERT INTO ai_connections VALUES(?,?,?,?)",
            ("synthetic-connection", '{"provider":"test"}', "synthetic-ref", "direct"),
        )
        conn.execute(
            "INSERT INTO ai_setup_accounts VALUES(?,?)",
            ("synthetic-connection", '{"account":"synthetic"}'),
        )
        conn.execute(
            "INSERT INTO tender_ai_policy VALUES(?,?,?,?)",
            (tender_id, 3, '{"allowed_connection_ids":["synthetic-connection"]}', "before"),
        )
        before = {
            "connection": conn.execute("SELECT * FROM ai_connections").fetchone(),
            "account": conn.execute("SELECT * FROM ai_setup_accounts").fetchone(),
            "policy": conn.execute("SELECT * FROM tender_ai_policy").fetchone(),
        }

    service = ManagerProfileService(repo)
    service.update(_edit(service.get().version, display_name="Boundary Manager"))

    with repo.db.connect() as conn:
        after = {
            "connection": conn.execute("SELECT * FROM ai_connections").fetchone(),
            "account": conn.execute("SELECT * FROM ai_setup_accounts").fetchone(),
            "policy": conn.execute("SELECT * FROM tender_ai_policy").fetchone(),
        }
    assert after == before
