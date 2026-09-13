"""T008: bounded descendants and exclusive ownership claims."""

from __future__ import annotations


def test_t008(run_case):
    result = run_case(
        "T008",
        {"scenario": "bounded_descendants", "max_depth": 2, "attempt_depth": 3, "claimants": 2},
    )
    assert result["depth_three_blocked"] is True
    assert result["owners"] == 1
    assert result["budget_owners"] == 1
