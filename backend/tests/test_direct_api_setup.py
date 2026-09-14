"""Focused direct API setup regressions; no provider network calls."""

import asyncio
from datetime import UTC, datetime, timedelta

import pytest

from quantix.ai_catalog import direct_provider_presets
from quantix.ai_connections import AIConnectionService, is_subscription_profile
from quantix.ai_setup import AISetupService
from quantix.ai_setup_store import SetupCheckMeter, SetupStore
from quantix.repository import Repository


def _connection(provider, *, protocol=None, base_url=None):
    protocols = {
        "openai": "openai_responses",
        "anthropic": "anthropic",
        "google": "google",
        "xai": "openai_chat",
        "custom": "openai_chat",
    }
    return {
        "name": provider,
        "provider_id": provider,
        "protocol": protocol or protocols[provider],
        "base_url": base_url or (None if provider == "google" else "https://api.example.test/v1"),
        "auth_type": "api_key",
        "billing": "metered",
        "settings": {},
        "credentials": {"api_key": "test-key"},
        "session_only": True,
    }


def test_setup_catalog_exposes_only_the_five_direct_provider_groups():
    assert {preset["id"] for preset in direct_provider_presets()} == {
        "openai",
        "anthropic",
        "google",
        "xai",
        "custom",
    }
    assert direct_provider_presets()[2]["protocols"] == ["google"]


def test_direct_profiles_derive_billing_and_never_keep_local_escape(tmp_path):
    repo = Repository(tmp_path)
    service = AIConnectionService(repo)

    saved = [
        service.create(_connection(provider))
        for provider in ("openai", "anthropic", "google", "xai", "custom")
    ]

    assert {record["billing"] for record in saved[:4]} == {"metered"}
    assert saved[4]["billing"] == "unknown"


def test_unknown_price_meter_requires_explicit_consent_and_keeps_cost_null(tmp_path):
    repo = Repository(tmp_path)
    connection = {"id": "connection", "billing": "metered"}
    model = {"model_id": "unknown-model", "pricing": None}
    SetupStore(repo)
    denied = SetupCheckMeter(repo, connection, model, None, unknown_cost_accepted=False)
    with pytest.raises(ValueError, match="cost"):
        asyncio.run(denied.before_request(100, 1024))

    accepted = SetupCheckMeter(repo, connection, model, None, unknown_cost_accepted=True)
    reservation = asyncio.run(accepted.before_request(100, 1024))
    asyncio.run(
        accepted.on_response(
            {
                "usage_complete": True,
                "input_tokens": 100,
                "output_tokens": 20,
                "requests": 1,
            },
            reservation,
        )
    )
    assert accepted.summary("passed", "checked")["estimated_cost_usd"] is None


def test_saved_account_for_a_removed_provider_is_skipped_and_cannot_be_created(tmp_path):
    import json

    repo = Repository(tmp_path)
    service = AIConnectionService(repo)
    kept = service.create(_connection("openai"))
    legacy = {
        "name": "Old Copilot",
        "provider_id": "copilot",
        "protocol": "copilot",
        "auth_type": "client_login",
        "billing": "subscription",
        "settings": {},
    }
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "INSERT INTO ai_connections VALUES(?,?,?,?)",
            ("legacy-copilot", json.dumps(legacy), None, "none"),
        )

    assert [row["id"] for row in service.list()] == [kept["id"]]
    with pytest.raises(ValueError):
        service.create(legacy)


def test_failed_direct_check_keeps_a_bounded_retry_available(tmp_path, monkeypatch):
    repo = Repository(tmp_path)
    connection = AIConnectionService(repo).create(_connection("openai"))
    service = AIConnectionService(repo)
    service.save_model(
        connection["id"],
        {
            "model_id": "gpt-test",
            "display_name": "Test model",
            "capabilities": {"tools": True, "structured_output": True},
        },
    )
    store = SetupStore(repo)
    store.update(
        connection["id"],
        stage="attention",
        selected_model_id="gpt-test",
        check={"status": "failed", "model_id": "gpt-test", "detail": "Retry the check."},
    )
    import quantix.ai_direct as direct

    monkeypatch.setattr(
        direct,
        "direct_runtime_status",
        lambda _connection: {
            "component_id": "direct-api",
            "state": "ready",
            "detail": "Bundled API adapter is ready.",
            "progress": None,
            "version": "direct-test",
        },
    )
    account = AISetupService(repo, direct=object()).get(connection["id"])
    preview = AISetupService(repo, direct=object()).check_preview(connection["id"])
    assert account["supported"] is True
    assert preview["allowed"] is True
    assert preview["max_input_tokens"] == 16384


class _SubscriptionComponents:
    def status(self, _connection):
        return {
            "component_id": "client-grok",
            "state": "ready",
            "detail": "ready",
            "progress": 100,
            "version": "client-test",
        }


class _SubscriptionWorker:
    async def close(self):
        return None


def test_subscription_preview_uses_nullable_caps_and_ignores_fetched_timestamp(tmp_path):
    repo = Repository(tmp_path)
    connections = AIConnectionService(repo)
    connection = connections.create(
        {
            "name": "Grok subscription",
            "provider_id": "grok_build",
            "protocol": "grok_build",
            "auth_type": "client_login",
            "billing": "subscription",
            "settings": {},
        }
    )
    connections.save_model(
        connection["id"],
        {
            "model_id": "grok-4.5",
            "display_name": "Grok 4.5",
            "capabilities": {"tools": True, "structured_output": True},
        },
    )
    from quantix.ai_setup import AISetupService

    setup = AISetupService(
        repo, components=_SubscriptionComponents(), worker=_SubscriptionWorker(), direct=object()
    )
    current = datetime.now(UTC)
    usage = {
        "fetched_at": current.isoformat(),
        "subscription_tier": "SuperGrok",
        "used_percent": 10,
        "period_type": "weekly",
        "period_start": (current - timedelta(minutes=1)).isoformat(),
        "period_end": (current + timedelta(hours=1)).isoformat(),
        "prepaid_balance_usd": "0",
        "on_demand_cap_usd": "0",
        "on_demand_used_usd": "0",
        "auto_topup_enabled": False,
        "included_only_allowed": True,
        "detail": "Included allowance confirmed.",
    }
    setup.store.update(
        connection["id"],
        stage="ready_to_check",
        selected_model_id="grok-4.5",
        subscription_usage=usage,
    )
    first = setup.check_preview(connection["id"])
    setup.store.update(
        connection["id"],
        subscription_usage={**usage, "fetched_at": (current + timedelta(seconds=2)).isoformat()},
    )
    second = setup.check_preview(connection["id"])
    assert is_subscription_profile(connections.get(connection["id"]))
    assert first["allowed"] is True
    assert first["subscription_check"] is True
    assert first["max_requests"] == 5
    assert first["max_input_tokens"] is None
    assert first["max_output_tokens"] is None
    assert first["fingerprint"] == second["fingerprint"]


def test_grok_extras_preference_is_a_revisioned_subscription_only_action(tmp_path):
    repo = Repository(tmp_path)
    connections = AIConnectionService(repo)
    connection = connections.create(
        {
            "name": "Grok subscription",
            "provider_id": "grok_build",
            "protocol": "grok_build",
            "auth_type": "client_login",
            "billing": "subscription",
            "settings": {},
        }
    )
    from quantix.ai_setup import AISetupService

    setup = AISetupService(
        repo, components=_SubscriptionComponents(), worker=_SubscriptionWorker(), direct=object()
    )
    old_revision = connection["revision"]
    result = asyncio.run(
        setup.action(
            connection["id"],
            {"action": "set_subscription_extras", "allow_provider_managed_extras": True},
        )
    )
    assert result["connection"]["settings"]["allow_provider_managed_extras"] is True
    assert result["connection"]["revision"] == old_revision + 1
