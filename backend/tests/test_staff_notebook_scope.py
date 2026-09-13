"""Notebook provenance and prompt-scope regressions."""

from __future__ import annotations

import hashlib
import json
from dataclasses import replace

import pytest
from test_staff_context import _context_workspace, _context_workspace_with_unreviewed_source

from quantix.estimates import EstimateService
from quantix.execution_context import (
    engineer_identity,
    identity_from_office_context,
)
from quantix.office_types import OfficeOutput, PreparedOfficeResult
from quantix.staff_assignment_models import PreparedStaffDraft
from quantix.staff_assignments import StaffAssignmentService
from quantix.staff_context import build_staff_context, staff_prompt_context
from quantix.staff_notebook_models import NotebookEntryDraft, NotebookQuery
from quantix.staff_notebooks import StaffNotebookService


def test_append_rejects_assignment_and_reference_forgery(tmp_path, monkeypatch):
    repo, tender, staff, binding, assignment_id, artifact = _context_workspace(tmp_path, monkeypatch)
    context = build_staff_context(repo, binding.id, assignment_id)
    notebooks = StaffNotebookService(repo)
    identity = identity_from_office_context(context)

    with pytest.raises(ValueError, match="assignment"):
        notebooks.append(
            identity,
            staff.staff.id,
            NotebookEntryDraft(
                kind="finding",
                text="Forged assignment note.",
                assignment_id="forged-assignment",
            ),
        )
    with pytest.raises(ValueError, match="source reference"):
        notebooks.append(
            identity,
            staff.staff.id,
            NotebookEntryDraft(
                kind="finding",
                text="Forged source note.",
                assignment_id=assignment_id,
                refs=["source:not-reviewed"],
            ),
        )


def test_unsupported_reference_types_fail_with_an_exact_blocker(tmp_path, monkeypatch):
    repo, _tender, staff, binding, assignment_id, _artifact = _context_workspace(tmp_path, monkeypatch)
    context = build_staff_context(repo, binding.id, assignment_id)

    with pytest.raises(ValueError, match="reference type 'mystery' is unsupported"):
        StaffNotebookService(repo).append(
            identity_from_office_context(context),
            staff.staff.id,
            NotebookEntryDraft(
                kind="finding",
                text="Unsupported reference note.",
                assignment_id=assignment_id,
                refs=["mystery:private-record"],
            ),
        )


def test_prompt_reconstruction_requires_exact_assignment_lineage(tmp_path, monkeypatch):
    repo, _tender, staff, binding, assignment_id, _artifact = _context_workspace(tmp_path, monkeypatch)
    context = build_staff_context(repo, binding.id, assignment_id)
    notebooks = StaffNotebookService(repo)
    engineer = engineer_identity(context.tender_id)

    notebooks.append(
        engineer,
        staff.staff.id,
        NotebookEntryDraft(kind="finding", text="Private earlier source observation.", refs=[]),
    )
    notebooks.append(
        identity_from_office_context(context),
        staff.staff.id,
        NotebookEntryDraft(
            kind="finding",
            text="Review the current Tender source package. Current assignment observation.",
            assignment_id=assignment_id,
            refs=[],
        ),
    )

    reconstructed = notebooks.reconstruct(
        identity_from_office_context(context),
        staff.staff.id,
        query="observation",
        limit=20,
    )
    assert [entry.text for entry in reconstructed] == [
        "Review the current Tender source package. Current assignment observation."
    ]
    prompt = staff_prompt_context(context)
    assert "Private earlier source observation." not in str(prompt)
    assert "Current assignment observation." in str(prompt)


def test_append_rejects_source_outside_current_grant(tmp_path, monkeypatch):
    repo, tender, staff, binding, assignment_id, _allowed_artifact, _excluded_artifact, excluded = (
        _context_workspace_with_unreviewed_source(tmp_path, monkeypatch, tools=("read_source",))
    )
    context = build_staff_context(repo, binding.id, assignment_id)

    with pytest.raises(ValueError, match="outside.*scope"):
        StaffNotebookService(repo).append(
            identity_from_office_context(context),
            staff.staff.id,
            NotebookEntryDraft(
                kind="finding",
                text="Unreviewed source claim.",
                assignment_id=assignment_id,
                refs=[f"source:{excluded['id']}"],
            ),
        )


def test_revised_source_does_not_relabel_historical_note_as_superseded(tmp_path, monkeypatch):
    repo, tender, staff, _binding, _assignment_id, artifact = _context_workspace(tmp_path, monkeypatch)
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    service = StaffNotebookService(repo)
    note = service.append(
        engineer_identity(tender["id"]),
        staff.staff.id,
        NotebookEntryDraft(
            kind="finding",
            text="The earlier source observation remains historical.",
            refs=[source_id],
        ),
    )

    body = b"A revised synthetic source."
    digest = hashlib.sha256(body).hexdigest()
    (repo.objects / digest).write_bytes(body)
    repo.register_artifact(
        tender["id"],
        artifact["relative_path"],
        digest,
        len(body),
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [{"locator": "page:1", "text": body.decode()}],
        },
    )

    page = service.retrieve(
        engineer_identity(tender["id"]),
        NotebookQuery(staff_id=staff.staff.id, query="historical", limit=20),
    )
    assert page.items[0].id == note.id
    assert page.items[0].current is True
    assert page.items[0].stale is False
    assert service.reconstruct(
        engineer_identity(tender["id"]), staff.staff.id, query="historical", limit=20
    ) == []


def test_staff_notebook_rejects_terminal_assignment_even_when_root_is_active(tmp_path, monkeypatch):
    repo, tender, staff, binding, assignment_id, _artifact = _context_workspace(tmp_path, monkeypatch)
    context = build_staff_context(repo, binding.id, assignment_id)
    identity = identity_from_office_context(context)
    StaffAssignmentService(repo).cancel(tender["id"], assignment_id)

    with pytest.raises(ValueError, match="assignment.*active"):
        StaffNotebookService(repo).append(
            identity,
            staff.staff.id,
            NotebookEntryDraft(kind="finding", text="Terminal assignment note."),
        )
    with pytest.raises(ValueError, match="assignment.*active"):
        StaffNotebookService(repo).reconstruct(
            identity,
            staff.staff.id,
            query="Terminal assignment",
            limit=20,
        )


def test_staff_notebook_rejects_forged_root_identity(tmp_path, monkeypatch):
    repo, _tender, staff, binding, assignment_id, _artifact = _context_workspace(tmp_path, monkeypatch)
    context = build_staff_context(repo, binding.id, assignment_id)
    forged = replace(identity_from_office_context(context), root_run_id="forged-root")

    with pytest.raises(ValueError, match="assignment identity"):
        StaffNotebookService(repo).append(
            forged,
            staff.staff.id,
            NotebookEntryDraft(kind="finding", text="Forged root note."),
        )
    with pytest.raises(ValueError, match="assignment identity"):
        StaffNotebookService(repo).reconstruct(
            forged,
            staff.staff.id,
            query="Forged root",
            limit=20,
        )


def test_staff_result_reference_rechecks_changed_boq_basis(tmp_path, monkeypatch):
    from test_staff_routing import _ready_binding_workspace

    repo, tender, _connection, _model, _route, envelope, routing, staff, root, artifact = (
        _ready_binding_workspace(tmp_path, monkeypatch)
    )
    evidence = repo.artifact_evidence(tender["id"], artifact["id"])[0]
    estimates = EstimateService(repo)
    item_id = "notebook-boq-item"
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
    binding = routing.bind(root, staff.staff.id, staff.work_order.id, envelope.route_options[0].id, "notebook-result-binding")
    assignments = StaffAssignmentService(repo)
    assignment = assignments.queue(root, binding.id, "notebook-result-queue")
    running = assignments.start(tender["id"], assignment.id, assignment.revision)
    result = assignments.save_result(
        tender["id"],
        running.id,
        PreparedStaffDraft(
            running.id,
            staff.staff.id,
            staff.staff.version,
            binding.id,
            PreparedOfficeResult(
                tender_id=tender["id"],
                run_id=root.run_id,
                output=OfficeOutput(summary="A saved staff result."),
                usage={},
                source_ids_read=(),
                web_sources=(),
                item_bases=((item_id, expected_basis),),
                approved_plan_id=binding.plan_id,
                actor_id=staff.staff.id,
                staff_version=staff.staff.version,
                assignment_id=running.id,
                route_binding_id=binding.id,
            ),
        ),
    )
    notebook = StaffNotebookService(repo)
    notebook.append(
        engineer_identity(tender["id"]),
        staff.staff.id,
        NotebookEntryDraft(
            kind="finding",
            text="Historical result before the BOQ changed.",
            refs=[f"staff_result:{result.id}"],
        ),
    )
    with repo.atomic() as conn:
        conn.execute(
            "UPDATE boq_items SET data_json=? WHERE id=?",
            (json.dumps({**item_data, "description": "Changed concrete item"}), item_id),
        )

    assert notebook.reconstruct(
        engineer_identity(tender["id"]), staff.staff.id, query="BOQ changed", limit=20
    ) == []
