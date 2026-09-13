"""T035 acceptance."""

from __future__ import annotations


def test_t035(run_case):
    result = run_case("T035", {"scenario": "cad_units_and_xref", "width": "4", "height": "5", "unit": "m", "missing_xref": True})
    assert result['area'] == '20.00'
    assert result['complete_coverage'] is False
    assert result['original_preserved'] is True
