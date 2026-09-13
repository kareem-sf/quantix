"""T032 acceptance."""

from __future__ import annotations


def test_t032(run_case):
    result = run_case("T032", {"scenario": "unit_calculation", "quantity": "2.5", "factor": "0.24", "invalid_add_units": ["m2", "m3"]})
    assert result['product'] == '0.60'
    assert result['dimension_error'] is True
    assert result['reproducible'] is True
