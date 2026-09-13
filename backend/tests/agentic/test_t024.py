"""T024 acceptance."""

from __future__ import annotations


def test_t024(run_case):
    result = run_case("T024", {"scenario": "ocr_partial_reprocess", "pages": 5, "successful_pages": 3})
    assert result['original_hash_unchanged'] is True
    assert result['extracted_pages'] == 3
    assert result['exception_pages'] == 2
