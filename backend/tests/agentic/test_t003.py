"""T003: staff identity lifecycle and subsequent-work preferences."""

from __future__ import annotations


def test_t003(run_case):
    result = run_case("T003", {"scenario": "retire_active_then_reactivate"})
    assert result["busy_retirement_blocked"] is True
    assert result["reactivation_provider_requests"] == 0
    assert result["identity_preserved"] is True


def test_t003_preferences_do_not_rewrite_inflight_briefs(run_case):
    result = run_case("T003", {"scenario": "retire_active_then_reactivate"})
    assert result["preference_habits"] == ["Be shorter", "Keep source references"]
    assert result["work_order_version_unchanged"] is True
    assert result["preference_version"] == 4
    assert result["replayed_retire"] is True
    assert result["available_after_reactivate"] == 1
    assert result["history_after_reactivate"] == 0
