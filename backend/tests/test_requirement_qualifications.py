"""A citation cannot turn a conditional source clause into an unconditional duty."""

import pytest
from test_estimates import approval
from test_source_boq import document

from quantix.repository import Repository
from quantix.tender_requirements import RequirementService


def setup_clause(tmp_path, text):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Synthetic qualifications")["id"]
    _, source = document(repo, tid, text)
    run = repo.create_run(tid, "manager", "Prepare requirements")
    service = RequirementService(repo)
    proposal = {
        "title": "Separate offers",
        "detail": "Provide separate offers.",
        "source_ids": [source["id"]],
        "deliverable_kind": "registers_xlsx",
    }
    return repo, tid, run, service, proposal


@pytest.mark.parametrize(
    "clause",
    [
        "If the invitation requires separate offers, provide two PDF files.",
        "إذا اشترطت المناقصة فصل العرضين، يجب تقديم ملفين منفصلين.",
    ],
)
def test_dropped_condition_is_rejected_but_exact_condition_is_preserved(tmp_path, clause):
    _, tid, run, service, proposal = setup_clause(tmp_path, clause)
    with pytest.raises(ValueError, match="condition"):
        service.propose(
            tid,
            proposal | {"source_quote": clause, "applicability": "unconditional"},
            origin="manager",
            run_id=run["id"],
        )
    saved = service.propose(
        tid,
        proposal | {"source_quote": clause, "applicability": "conditional", "condition": clause},
        origin="manager",
        run_id=run["id"],
    )
    assert saved["condition"] == clause
    assert saved["applicability"] == "conditional"
    with pytest.raises(ValueError, match="applicability"):
        service.decide(tid, saved["id"], approval(decision="approve"))
    approved = service.decide(
        tid, saved["id"], approval(decision="approve", applicability_reviewed=True)
    )
    assert approved["status"] == "approved"
    assert approved["condition"] == clause


def test_cherry_picked_quote_does_not_drop_prefix_condition(tmp_path):
    _, tid, run, service, proposal = setup_clause(tmp_path, "If requested, provide two PDF files.")
    with pytest.raises(ValueError, match="condition"):
        service.propose(
            tid,
            proposal | {"source_quote": "provide two PDF files.", "applicability": "unconditional"},
            origin="manager",
            run_id=run["id"],
        )
    with pytest.raises(ValueError, match="condition|clause"):
        service.propose(
            tid,
            proposal
            | {
                "source_quote": "provide two PDF files.",
                "applicability": "conditional",
                "condition": "provide two PDF files.",
            },
            origin="manager",
            run_id=run["id"],
        )


def test_nonempty_duty_text_cannot_substitute_for_an_exception(tmp_path):
    clause = "Provide two PDF files, except exempt bidders may submit one."
    _, tid, run, service, proposal = setup_clause(tmp_path, clause)
    with pytest.raises(ValueError, match="exception"):
        service.propose(
            tid,
            proposal
            | {
                "source_quote": "Provide two PDF files",
                "applicability": "unconditional",
                "exceptions": ["Provide two PDF files"],
            },
            origin="manager",
            run_id=run["id"],
        )


def test_exception_and_quote_attribution_are_required_for_manager(tmp_path):
    clause = "Provide a guarantee, except small enterprises may submit an exemption certificate."
    _, tid, run, service, proposal = setup_clause(tmp_path, clause)
    with pytest.raises(ValueError, match="clause|quote"):
        service.propose(tid, proposal, origin="manager", run_id=run["id"])
    with pytest.raises(ValueError, match="exception"):
        service.propose(
            tid,
            proposal | {"source_quote": clause, "applicability": "unconditional"},
            origin="manager",
            run_id=run["id"],
        )
    saved = service.propose(
        tid,
        proposal
        | {
            "source_quote": clause,
            "applicability": "unconditional",
            "exceptions": ["small enterprises may submit an exemption certificate"],
        },
        origin="manager",
        run_id=run["id"],
    )
    assert saved["exceptions"] == ["small enterprises may submit an exemption certificate"]


def test_unqualified_historical_proposals_remain_visible_and_require_review(tmp_path):
    repo, tid, _, service, proposal = setup_clause(tmp_path, "Provide two PDF files.")
    saved = service.propose(tid, proposal)
    with repo.db.connect() as conn:
        before = conn.execute(
            "SELECT payload_json FROM submission_requirements WHERE id=?", (saved["id"],)
        ).fetchone()[0]
    assert saved["applicability"] == "unknown"
    assert any("applicability" in warning.lower() for warning in saved["warnings"])
    with pytest.raises(ValueError, match="applicability"):
        service.decide(tid, saved["id"], approval(decision="approve"))
    with repo.db.connect() as conn:
        assert (
            conn.execute(
                "SELECT payload_json FROM submission_requirements WHERE id=?", (saved["id"],)
            ).fetchone()[0]
            == before
        )
