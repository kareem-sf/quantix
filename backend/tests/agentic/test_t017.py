"""T017 acceptance."""

from __future__ import annotations


def test_t017(run_case):
    result = run_case("T017", {"scenario": "generic_work_product", "rows": 10000, "unsafe_markup": "<script>bad()</script>"})
    assert result['export_rows'] == 10000
    assert result['executed_scripts'] == 0
    assert result['old_version_accessible'] is True
