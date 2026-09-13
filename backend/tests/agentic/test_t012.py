"""T012 arithmetic slice; this does not close the full review-room acceptance."""

from __future__ import annotations


def test_t012(run_case):
    result = run_case(
        "T012",
        {"scenario": "recorded_arithmetic_review", "author_total": "110.00"},
    )
    assert result["author_conclusion_hidden"] is True
    assert result["arithmetic_checked"] is True
    assert result["calculation_link_preserved"] is True
    assert result["unsupported_prose_incomplete"] is True
    assert result["quantity_not_authorized"] is True
