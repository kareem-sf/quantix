"""T011: checkpoints, invalidated reuse and uncertain external effects."""

from __future__ import annotations


def test_t011(run_case):
    result = run_case(
        "T011",
        {"scenario": "checkpoint_crash", "crash_after": "extraction", "smtp_outcome": "uncertain"},
    )
    assert result["reused_extraction"] is True
    assert result["source_revision_invalidated"] is True
    assert result["uncertain_delivery"] is True
