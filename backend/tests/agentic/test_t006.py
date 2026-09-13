"""T006: office delivery, acknowledgements and question grouping."""

from __future__ import annotations


def test_t006(run_case):
    result = run_case("T006", {"scenario": "delivery_semantics", "duplicate_deliveries": 3})
    assert result["delivery_count"] == 1
    assert result["acknowledgement_count"] == 1
    assert result["engineer_decisions_created"] == 0


def test_t006_grouping_and_forged_status(run_case):
    result = run_case("T006", {"scenario": "delivery_semantics", "duplicate_deliveries": 3})
    assert result["forged_checked_status"] == 409
    assert result["distinct_source_version_message"] is True
    assert result["identical_question_grouped"] is True
    assert result["ack_not_checked"] is True
