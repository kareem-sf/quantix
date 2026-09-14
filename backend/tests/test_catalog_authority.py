"""Catalog observations must not silently change approved account authority."""

import pytest

from quantix.ai_connections import AIConnectionService
from quantix.ai_models import ConnectionInput, ModelInput
from quantix.ai_policy import AIPolicyService
from quantix.ai_readiness import ready_evidence, require_ready
from quantix.ai_setup_store import SetupStore
from quantix.repository import Repository


def configured_office(tmp_path, monkeypatch, *, max_requests=12, model_id="synthetic-model", reasoning="high", search=False, repo=None, tender=None, provider_id="openai"):
    repo = repo or Repository(tmp_path)
    tender = tender or repo.create_tender("Synthetic tender")
    connections = AIConnectionService(repo)
    connection = connections.create({
        "name": "Synthetic API", "provider_id": provider_id,
        "protocol": "openai_responses" if search and provider_id == "openai" else "openai_chat",
        "base_url": "https://api.example.test/v1", "auth_type": "api_key", "billing": "metered",
        "credentials": {"api_key": "synthetic-key"}, "session_only": True,
    })
    model = connections.save_model(connection["id"], {
        "model_id": model_id, "display_name": "Synthetic model",
        "capabilities": {"tools": True, "structured_output": True, "web_search": True,
                         "reasoning": [reasoning], "max_output_tokens": 20000, "context_window": 200000},
        "pricing": {"input_per_million": 1, "output_per_million": 2, "web_search_per_call": 0.001,
                    "source": "synthetic", "as_of": "2026-09-09"},
    }, source="provider")
    connection = connections.get(connection["id"])
    monkeypatch.setattr("quantix.ai_direct.direct_runtime_status", lambda _: {
        "state": "ready", "version": "synthetic", "component_id": "direct-api", "detail": "Ready", "progress": 100,
    })
    store = SetupStore(repo)
    evidence = {"check": {"status": "passed", "model_id": model_id},
                "checked_revision": connection["revision"], "checked_component_version": "synthetic"}
    store.update(connection["id"], selected_model_id=model_id, **evidence)
    store.save_model_check(connection["id"], model_id, evidence)
    policy = AIPolicyService(repo)
    route = {"connection_id": connection["id"], "model_id": model_id, "reasoning": reasoning,
             "max_output_tokens": 2048, "web_search": search, "max_search_calls": 3}
    policy.update(tender["id"], {"allowed_connection_ids": [connection["id"]], "manager": route,
        "specialist": route, "run_budget_usd": 10, "tender_budget_usd": 100, "max_requests": max_requests,
        "engineer_confirmed": True, "rationale": "Synthetic approval"})
    return repo, tender, connections, connection, model, policy, route


@pytest.mark.parametrize("source", ["provider", "manual"])
def test_unrelated_catalog_model_keeps_manager_authority(tmp_path, monkeypatch, source):
    repo, tender, connections, account, model, policy, route = configured_office(tmp_path, monkeypatch)
    incoming = {key: model[key] for key in ModelInput.model_fields}
    extra = incoming | {"model_id": "unrelated-new-model"}
    if source == "provider":
        connections.store_discovered_models(account["id"], [incoming, extra], account["revision"])
    else:
        connections.save_model(account["id"], extra)
    assert connections.get(account["id"])["revision"] == account["revision"]
    assert policy.routes_for(tender["id"])[0] == route
    assert policy.routes_for(tender["id"], role="specialist")[0] == route
    assert require_ready(repo, connections.get(account["id"]), model["model_id"]) == "synthetic"


@pytest.mark.parametrize("change", ["removed", "denied", "manual_denied"])
def test_changed_model_invalidates_only_its_readiness_and_cannot_reinherit_proof(tmp_path, monkeypatch, change):
    repo, tender, connections, account, model, policy, route = configured_office(tmp_path, monkeypatch)
    incoming = {key: model[key] for key in ModelInput.model_fields}
    second = incoming | {"model_id": "second-model"}
    connections.save_model(account["id"], second, source="provider")
    store = SetupStore(repo)
    store.save_model_check(account["id"], "second-model", {
        "check": {"status": "passed", "model_id": "second-model"},
        "checked_revision": account["revision"], "checked_component_version": "synthetic"})
    denied = incoming | {"capabilities": incoming["capabilities"] | {"tools": False}}
    if change == "manual_denied":
        connections.save_model(account["id"], denied)
    else:
        connections.store_discovered_models(account["id"], [second] if change == "removed" else [denied, second], account["revision"])
    current = connections.get(account["id"])
    assert current["revision"] == account["revision"]
    assert ready_evidence(repo, current, "second-model") is not None
    assert ready_evidence(repo, current, model["model_id"]) is None
    with pytest.raises(ValueError, match="model|tool"):
        policy.routes_for(tender["id"])
    connections.save_model(account["id"], incoming, source="provider")
    # Recreating SetupStore must not backfill the invalidated selected proof.
    SetupStore(repo)
    with pytest.raises(ValueError, match="check"):
        require_ready(repo, current, model["model_id"])


@pytest.mark.parametrize("change", ["credentials", "endpoint", "billing"])
def test_account_authority_edits_still_revoke_routes(tmp_path, monkeypatch, change):
    repo, tender, connections, account, model, policy, route = configured_office(tmp_path, monkeypatch, provider_id="custom" if change == "billing" else "openai")
    values = {key: value for key, value in account.items() if key in ConnectionInput.model_fields}
    if change == "credentials":
        values.update(credentials={"api_key": "replacement-synthetic-key"}, session_only=True)
    elif change == "endpoint":
        values["base_url"] = "https://other.example.test/v1"
    else:
        values.update(billing="unknown")
    if change != "billing":
        values.update(credentials={"api_key": "replacement-synthetic-key"}, session_only=True)
    connections.update(account["id"], values)
    assert connections.get(account["id"])["revision"] > account["revision"]
    with pytest.raises(ValueError, match="changed|review|approv"):
        policy.routes_for(tender["id"])


@pytest.mark.asyncio
@pytest.mark.parametrize("path", ["setup", "advanced"])
async def test_discovery_respects_explicit_provider_capability_denial(tmp_path, monkeypatch, path):
    from quantix.ai_setup import AISetupService

    repo, _, connections, account, model, *_ = configured_office(tmp_path, monkeypatch)

    class SyntheticDirect:
        async def catalog(self, *_):
            incoming = {key: model[key] for key in ModelInput.model_fields}
            return [incoming | {"capabilities": incoming["capabilities"] | {"tools": False}}]

    monkeypatch.setattr("quantix.ai_setup.documented_capabilities", lambda *_: {"tools": True})
    monkeypatch.setattr("quantix.ai_connections.documented_capabilities", lambda *_: {"tools": True})
    if path == "setup":
        setup = AISetupService(repo, direct=SyntheticDirect())
        await setup._discover(account["id"])
    else:
        monkeypatch.setattr("quantix.ai_direct.DirectAPIService", lambda *_: SyntheticDirect())
        await connections.discover(account["id"])
    assert connections.models(account["id"])[0]["capabilities"]["tools"] is False
    assert ready_evidence(repo, connections.get(account["id"]), model["model_id"]) is None
