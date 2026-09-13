"""T019 acceptance."""

from __future__ import annotations


def test_t019(run_case):
    result = run_case("T019", {"scenario": "watch_dedup_and_sleep", "same_observations": 3, "missed_checks": 2})
    assert result['notifications'] == 1
    assert result['missed_checks'] == 2
    assert result['commercial_sends'] == 0
