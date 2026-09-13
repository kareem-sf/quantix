"""T009: concurrent reservations against one budget scope."""

from __future__ import annotations


def test_t009(run_case):
    result = run_case(
        "T009",
        {"scenario": "parallel_budget_race", "remaining_requests": 2, "simultaneous_requests": 3},
    )
    assert result["admitted"] == 2
    assert result["overspend"] is False
    assert result["late_usage_recorded"] is True
