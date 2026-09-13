"""T004: scoped staff notebooks and bounded reconstruction."""

from __future__ import annotations


def test_t004(run_case):
    result = run_case("T004", {"scenario": "notebook_reconstruction", "notes": 1000, "restart": True})
    assert result["relevant_note_restored"] is True
    assert result["unrelated_notes_in_prompt"] == 0
    assert result["received_implies_source_read"] is False


def test_t004_pagination_and_supersede(run_case):
    result = run_case("T004", {"scenario": "notebook_reconstruction", "notes": 1000, "restart": True})
    assert result["page_count"] == result["unique_ids"] == result["total"] == 1000
    assert result["stale_kept"] is True
    assert result["reconstructed_count"] <= 20
    assert result["other_staff_blocked"] is True
