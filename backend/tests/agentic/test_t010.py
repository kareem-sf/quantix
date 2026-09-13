"""T010: status from saved records without provider work."""

from __future__ import annotations


def test_t010(run_case):
    result = run_case("T010", {"scenario": "status_and_constraint_during_work"})
    assert result["status_provider_requests"] == 0
    assert result["mixed_revision_publications"] == 0
    assert result["draft_preserved"] is True
    assert result["status_ok"] is True
