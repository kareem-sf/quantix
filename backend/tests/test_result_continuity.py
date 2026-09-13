"""Completed work stays reachable and cannot leave old progress looking current."""

import pytest
from test_requirement_qualifications import setup_clause
from test_source_boq import row
from test_work_brief import _draft

from quantix.ai_tools import ToolArgumentError
from quantix.estimates import EstimateService
from quantix.office import _validate_output
from quantix.office_tools import OfficeContext
from quantix.office_types import OfficeOutput
from quantix.work_brief import WorkBriefService


def test_requirement_and_boq_results_link_to_the_exact_persisted_records(tmp_path):
    repo, tid, run, requirements, proposal = setup_clause(
        tmp_path, "A1 Concrete foundations 12.5 m3"
    )
    requirement = requirements.propose(
        tid,
        proposal
        | {
            "source_quote": "A1 Concrete foundations 12.5 m3",
            "applicability": "unconditional",
        },
        origin="manager",
        run_id=run["id"],
    )
    source = repo.get_evidence(tid, proposal["source_ids"][0])
    item = EstimateService(repo).propose_source_row(tid, row(source), run_id=run["id"])
    repo.add_message(tid, "manager", "Saved the required work.", run_id=run["id"])
    repo.update_run(
        run["id"],
        status="completed",
        result={"summary": "Saved", "submission_requirements": [requirement]},
    )
    links = repo.message_page(tid)["items"][-1]["result_links"]
    by_kind = {link["kind"]: link for link in links}
    assert by_kind["requirement"]["id"] == requirement["id"]
    assert (
        by_kind["requirement"]["target"]
        == f"/tenders/{tid}/submission?view=requirements&record={requirement['id']}"
    )
    assert by_kind["boq_item"]["id"] == item["id"]
    other = repo.create_tender("Other")["id"]
    assert repo.message_page(other)["items"] == []


def test_later_engineering_records_make_old_progress_explicitly_outdated(tmp_path):
    repo, tid, first, _, _ = setup_clause(tmp_path, "Source")
    briefs = WorkBriefService(repo)
    brief = briefs.save(
        tid,
        first["id"],
        "manager",
        _draft(),
        expected_version=0,
        inspected_source_ids=set(),
        idempotency_key="brief-first",
    )
    repo.update_run(first["id"], status="completed", result={"summary": "First step"})
    greeting = repo.create_run(tid, "conversation", "Hi")
    repo.update_run(
        greeting["id"], status="completed", result={"kind": "conversation", "reply": "Hello"}
    )
    assert briefs.current(tid).progress_current is True
    later = repo.create_run(tid, "manager", "Continue")
    repo.update_run(
        later["id"],
        status="completed",
        result={"summary": "Next step", "submission_requirements": [{"id": "saved"}]},
    )
    current = briefs.current(tid)
    assert current.progress_current is False
    assert current.latest_work_run_id == later["id"]
    assert current.version == brief.version
    assert current.steps == brief.steps


def test_material_continuation_requires_updated_working_brief(tmp_path):
    repo, tid, first, _, proposal = setup_clause(tmp_path, "Provide two offers.")
    briefs = WorkBriefService(repo)
    briefs.save(
        tid,
        first["id"],
        "manager",
        _draft(),
        expected_version=0,
        inspected_source_ids=set(),
        idempotency_key="first",
    )
    repo.update_run(first["id"], status="completed", result={"summary": "First step"})
    next_run = repo.create_run(tid, "manager", "Continue")
    context = OfficeContext(repo, tid, next_run["id"])
    context.seen_sources.update(proposal["source_ids"])
    output = OfficeOutput(
        summary="Saved requirement",
        submission_requirements=[
            proposal | {"source_quote": "Provide two offers.", "applicability": "unconditional"}
        ],
    )
    with pytest.raises(ToolArgumentError, match="brief"):
        _validate_output(output, context)
    briefs.save(
        tid,
        next_run["id"],
        "manager",
        _draft(next_step="Review saved requirements."),
        expected_version=1,
        inspected_source_ids=set(),
        idempotency_key="second",
    )
    _validate_output(output, context)
