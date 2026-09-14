"""Read-only coverage, revision, estimate and submission checks through the real tool fence."""

from __future__ import annotations

import hashlib
import json

import pytest
from test_estimates import approval, seed

from quantix.ai_tools import ToolArgumentError
from quantix.estimates import EstimateService
from quantix.office_tools import OfficeContext, source_tools
from quantix.outputs import OutputService
from quantix.repository import Repository


def _pdf(repo, tender_id, pages: list[str], *, path="Specs/Concrete.pdf", warnings=None, ocr=False):
    raw = "\n".join(pages).encode()
    artifact, _ = repo.register_artifact(
        tender_id,
        path,
        hashlib.sha256(raw).hexdigest(),
        len(raw),
        {
            "kind": "pdf",
            "status": "needs_attention" if warnings else "extracted",
            "metadata": {
                "page_count": len(pages),
                "pages_processed": len(pages),
                "segment_count": sum(bool(text) for text in pages),
            },
            "warnings": warnings or [],
            "segments": [
                {
                    "locator": f"page:{index}",
                    "page": index,
                    "text": text,
                    "metadata": {"method": "ocr" if ocr else "embedded_text"},
                }
                for index, text in enumerate(pages, start=1)
                if text
            ],
        },
    )
    return artifact


def _manager(repo, tender_id):
    run = repo.create_run(tender_id, "manager", "Synthetic check")
    return OfficeContext(repo, tender_id, run["id"])


async def _call(context, name, arguments=None):
    definition = next(item for item in source_tools() if item.name == name)
    return json.loads(await definition.invoke(context, arguments or {}))


@pytest.mark.asyncio
async def test_coverage_reports_unreadable_and_ocr_pages_without_claiming_analysis(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic coverage Tender")
    _pdf(repo, tender["id"], ["Concrete grade C30/37.", "Cover 40 mm."])
    _pdf(
        repo,
        tender["id"],
        ["", "Site visit form"],
        path="Forms/Visit.pdf",
        ocr=True,
        warnings=[
            {
                "code": "pdf_no_text",
                "message": "No readable embedded text was found on this page.",
                "locator": "page:1",
            },
        ],
    )
    context = _manager(repo, tender["id"])

    exceptions = await _call(context, "inspect_extraction_coverage")
    assert exceptions["totals"]["documents"] == 2
    assert exceptions["totals"]["pages_without_text"] == 1
    assert exceptions["totals"]["ocr_pages"] == 1
    assert [row["name"] for row in exceptions["documents"]] == ["Visit.pdf"]
    assert exceptions["documents"][0]["pages_without_text"] == ["page:1"]
    assert "not analysed or reviewed coverage" in exceptions["limitation"]

    everything = await _call(context, "inspect_extraction_coverage", {"exceptions_only": False})
    assert len(everything["documents"]) == 2
    with pytest.raises(ToolArgumentError):
        await _call(context, "inspect_extraction_coverage", {"limit": 500})


@pytest.mark.asyncio
async def test_revision_compare_and_impact_name_exact_changes_and_dependent_records(tmp_path):
    from quantix.execution_context import engineer_identity
    from quantix.work_product_models import WorkProductDraft
    from quantix.work_products import WorkProductService

    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic revision Tender")
    first = _pdf(
        repo, tender["id"], ["Concrete grade C30/37.", "Cover 40 mm.", "Formwork by contractor."]
    )
    old_grade = repo.artifact_evidence(tender["id"], first["id"])[0]
    finding = repo.add_finding(
        tender["id"],
        "Concrete grade",
        "C30/37 required.",
        "requirement",
        [old_grade["id"]],
        origin="agent",
    )
    repo.add_finding(tender["id"], "Unrelated", "Not linked.", "observation", [], origin="agent")
    WorkProductService(repo).save_draft(
        engineer_identity(tender["id"]),
        WorkProductDraft(kind="note", title="Grade", content="Use C30/37.",
                         source_refs=[old_grade["id"]], idempotency_key="grade-note"),
    )
    context = _manager(repo, tender["id"])
    with pytest.raises(ToolArgumentError, match="no earlier version"):
        await _call(context, "compare_source_versions", {"artifact_id": first["id"]})

    second = _pdf(
        repo, tender["id"], ["Concrete grade C35/45.", "Cover 40 mm.", "", "Curing for 7 days."]
    )
    compared = await _call(context, "compare_source_versions", {"artifact_id": second["id"]})
    assert (compared["from_version"], compared["to_version"]) == (1, 2)
    assert compared["summary"] == {"changed": 1, "added": 1, "removed": 1, "unchanged": 1}
    changed = next(item for item in compared["changes"] if item["change"] == "changed")
    assert changed["locator"] == "page:1" and "C35/45" in changed["excerpt"]
    assert changed["evidence_id"] in {
        row["id"] for row in repo.artifact_evidence(tender["id"], second["id"])
    }
    assert not context.seen_sources, "A comparison excerpt is not a citable source read."

    impact = await _call(context, "trace_change_impact", {"artifact_id": second["id"]})
    assert impact["replaced_versions"] == [1]
    assert impact["affected_counts"]["findings"] == 1
    assert impact["affected"]["findings"][0]["id"] == finding["id"]
    assert impact["affected"]["findings"][0]["marked_stale"] is True
    assert impact["affected_counts"]["work_products"] == 1
    assert "Nothing was changed" in impact["limitation"]
    assert repo.list_findings(tender["id"])[0]["title"] in {"Concrete grade", "Unrelated"}


@pytest.mark.asyncio
async def test_estimate_coverage_lists_unpriced_and_duplicate_rows_with_totals_state(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic estimate Tender")
    seed(repo, tender["id"])
    estimates = EstimateService(repo)
    estimates.refresh(tender["id"])
    context = _manager(repo, tender["id"])

    result = await _call(context, "check_estimate_coverage")
    assert result["summary"]["boq_rows"] == 1
    assert result["summary"]["estimate_complete"] is False
    row = result["rows"][0]
    assert {"no_rate", "unconfirmed", "unknown_vat"} <= set(row["needs"])
    assert row["unit"] == "m3"
    assert "does not prove" in result["limitation"]


@pytest.mark.asyncio
async def test_submission_rehearsal_reports_blockers_without_exporting(tmp_path):
    from quantix.submissions import SubmissionService

    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic package Tender")
    original = b"Synthetic preserved source".ljust(100, b" ")
    digest = hashlib.sha256(original).hexdigest()
    seed(repo, tender["id"], digest=digest)
    (repo.objects / digest).write_bytes(original)
    estimates = EstimateService(repo)
    estimates.refresh(tender["id"])
    context = _manager(repo, tender["id"])

    empty = await _call(context, "rehearse_submission")
    assert empty["ready"] is False
    assert any(item["code"] == "no_documents" for item in empty["blockers"])
    assert any("No submission requirements are registered" in item for item in empty["warnings"])

    services = SubmissionService(repo)
    source = estimates.view(tender["id"])["items"][0]["source_id"]
    services.requirements.propose(
        tender["id"],
        {
            "title": "Priced BOQ",
            "detail": "Provide the priced bill.",
            "source_ids": [source],
            "deliverable_kind": "boq_xlsx",
        },
    )
    draft = OutputService(repo).generate(tender["id"], approval(kind="boq_xlsx"))
    rehearsed = await _call(context, "rehearse_submission", {"output_ids": [draft["id"]]})
    assert rehearsed["ready"] is False
    assert rehearsed["requirements_checked"] == 1
    assert rehearsed["documents_checked"][0]["output_id"] == draft["id"]
    assert any(item["code"] == "requirement_approval" for item in rehearsed["blockers"])
    assert services.list(tender["id"]) == []
    with pytest.raises(ToolArgumentError, match="not saved"):
        await _call(context, "rehearse_submission", {"output_ids": ["missing"]})


@pytest.mark.asyncio
async def test_estimate_coverage_flags_repeated_rows_with_the_same_unit(tmp_path):
    from test_estimates import cell

    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic duplicate Tender")
    header = [
        cell("A1", "Item"),
        cell("B1", "Description"),
        cell("C1", "Unit"),
        cell("D1", "Quantity"),
    ]
    rows = [
        [
            cell(f"A{row}", f"C0{row}"),
            cell(f"B{row}", "Cast concrete  foundations"),
            cell(f"C{row}", "m3"),
            cell(f"D{row}", 12.5),
        ]
        for row in (2, 3)
    ]
    repo.register_artifact(
        tender["id"],
        "Area/Bill.xlsx",
        "e" * 64,
        100,
        {
            "kind": "spreadsheet",
            "status": "extracted",
            "metadata": {},
            "segments": [
                {
                    "locator": f"sheet:Works/row:{index}",
                    "text": "Row",
                    "sheet": "Works",
                    "kind": "spreadsheet_row",
                    "metadata": {"row": index, "cells": cells},
                }
                for index, cells in ((1, header), (2, rows[0]), (3, rows[1]))
            ],
        },
    )
    EstimateService(repo).refresh(tender["id"])
    result = await _call(_manager(repo, tender["id"]), "check_estimate_coverage")
    assert result["summary"]["boq_rows"] == 2
    assert result["summary"]["possible_duplicates"] == 2
    first, second = result["rows"]
    assert "possible_duplicate" in first["needs"]
    assert first["possible_duplicate_of"] == [second["item_id"]]
