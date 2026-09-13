"""T005: exact handoff consumption and revalidated prior-work reuse."""

from __future__ import annotations


def test_t005(run_case):
    result = run_case(
        "T005",
        {"scenario": "full_handoff", "rows": 750, "selected_row": 701, "prior_root": True},
    )
    assert result["selected_row"] == 701
    assert result["values_exact"] is True
    assert result["copied_source_receipts"] == 0


def test_t005_conflict_and_source_revision(run_case):
    result = run_case(
        "T005",
        {"scenario": "full_handoff", "rows": 750, "selected_row": 701, "prior_root": True},
    )
    assert result["guess_status"] == 404
    assert result["replay_conflict"] is True
    assert result["applicability_after_revision"] == "needs_review"
    assert result["total_rows"] == 750
