"""T021 acceptance."""

from __future__ import annotations


def test_t021(run_case):
    result = run_case("T021", {"scenario": "company_asset_expiry_and_scope", "certificate_expired": True})
    assert result['eligibility_satisfied'] is False
    assert result['unauthorized_prompt_bytes'] == 0
    assert result['history_retained'] is True
