"""T002: Manager profile pinning and dynamic generation acceptance."""

from __future__ import annotations


def test_t002(run_case):
    result = run_case(
        "T002",
        {
            "scenario": "manager_profile_pin",
            "edit_during_run": True,
            "role": "Facade procurement timing analyst",
        },
    )
    assert result["run_profile_version"] == 1
    assert result["current_profile_version"] == 2
    assert result["cross_tender_leaks"] == 0


def test_t002_role_is_not_a_route_and_unsupported_tools_block(run_case):
    result = run_case(
        "T002",
        {
            "scenario": "manager_profile_pin",
            "edit_during_run": True,
            "role": "Facade procurement timing analyst",
        },
    )
    assert result["staff_role"] == "Facade procurement timing analyst"
    assert result["title_used_as_route"] is False
    assert any(item["id"] == "tool-not-installed" for item in result["unavailable_tools"])


def test_t002_stale_save_and_local_portrait(run_case):
    result = run_case(
        "T002",
        {
            "scenario": "manager_profile_pin",
            "edit_during_run": True,
            "role": "واجهة توقيت التوريد",
        },
    )
    assert result["stale_save_status"] == 409
    assert result["stale_save_preserved_version"] == 2
    assert result["portrait_style"] == "notionists-v1"
    assert result["portrait_is_local"] is True
    assert result["provider_requests"] >= 1
