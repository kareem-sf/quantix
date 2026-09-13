"""T020 acceptance."""

from __future__ import annotations


def test_t020(run_case):
    result = run_case("T020", {"scenario": "tender_profile_calendar", "local_deadline": "2026-09-30T12:00:00+03:00"})
    assert result['deadline_utc'] == '2026-09-30T09:00:00Z'
    assert result['unknown_measurement_rule_preserved'] is True
    assert result['accepted_money_changed'] is False
