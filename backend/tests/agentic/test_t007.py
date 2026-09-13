"""T007: adaptive work graphs without a workflow catalogue."""

from __future__ import annotations


def test_t007(run_case):
    result = run_case(
        "T007",
        {"scenario": "graph_cycle_and_revision", "cycle": ["A", "B", "A"]},
    )
    assert result["cycle_rejected"] is True
    assert result["partial_mutations"] == 0
    assert result["unaffected_outputs_preserved"] is True
