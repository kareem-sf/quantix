"""T073 acceptance."""

from __future__ import annotations


def test_t073(run_case):
    result = run_case("T073", {"scenario": "push_to_talk", "transcript": "Approve the bid", "auto_send": False})
    assert result['text_is_unsent_draft'] is True
    assert result['approvals_created'] == 0
    assert result['always_listening'] is False
