"""Focused checks for the Grok subscription metadata boundary."""

import sys
from datetime import UTC, datetime, timedelta
from pathlib import Path

# The worker is packaged separately from the core `quantix` distribution.
sys.path.insert(0, str(Path(__file__).parents[1] / "ai_worker"))

from quantix_ai_worker.grok_auth import normalize_billing  # noqa: E402


def _billing():
    now = datetime.now(UTC)
    return {
        "subscription_tier": "SuperGrok",
        "config": {
            "creditUsagePercent": 25.0,
            "currentPeriod": {
                "type": "weekly",
                "start": (now - timedelta(minutes=1)).isoformat(),
                "end": (now + timedelta(hours=1)).isoformat(),
            },
            # The official Cent JSON type uses {} for zero.
            "prepaidBalance": {},
            "onDemandCap": {},
            "onDemandUsed": {},
        },
    }


def test_successful_missing_or_null_rule_means_no_automatic_topup():
    for topup in ({}, {"rule": None}):
        snapshot = normalize_billing(_billing(), topup)

        assert snapshot["auto_topup_enabled"] is False
        assert snapshot["included_only_allowed"] is True


def test_proto_disabled_rule_and_enabled_rule_are_preserved():
    disabled = normalize_billing(_billing(), {"rule": {}})
    disabled_with_explicit_fields = normalize_billing(
        _billing(),
        {
            "rule": {
                "enabled": False,
                "minBeforeHittingSl": {},
                "topupAmount": {},
                "maxAmountPerMonth": {},
            }
        },
    )
    enabled = normalize_billing(_billing(), {"rule": {"enabled": True}})

    assert disabled["auto_topup_enabled"] is False
    assert disabled["included_only_allowed"] is True
    assert disabled_with_explicit_fields["auto_topup_enabled"] is False
    assert disabled_with_explicit_fields["included_only_allowed"] is True
    assert enabled["auto_topup_enabled"] is True
    assert enabled["included_only_allowed"] is False


def test_malformed_rule_remains_unknown_and_blocks_included_only_access():
    snapshots = [
        normalize_billing(_billing(), {"rule": []}),
        normalize_billing(_billing(), {"rule": {"error": "unavailable"}}),
        normalize_billing(_billing(), {"error": "unavailable"}),
        normalize_billing(_billing(), {"rule": {"enabled": "false"}}),
    ]

    assert all(snapshot["auto_topup_enabled"] is None for snapshot in snapshots)
    assert all(snapshot["included_only_allowed"] is False for snapshot in snapshots)
