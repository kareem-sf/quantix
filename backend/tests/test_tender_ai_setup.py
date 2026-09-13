"""Choosing and switching the Tender Manager's AI from the prompt box."""

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient

from quantix.ai_connections import AIConnectionService
from quantix.ai_policy import AIPolicyService
from quantix.ai_setup_routes import create_router
from quantix.repository import Repository


def _account(repo, name, billing="metered"):
    connections = AIConnectionService(repo)
    connection = connections.create(
        {
            "name": name,
            "provider_id": "openai",
            "protocol": "openai_chat",
            "base_url": "https://api.example.test/v1",
            "auth_type": "api_key",
            "billing": billing,
            "credentials": {"api_key": "synthetic-key"},
            "session_only": True,
        }
    )
    model = connections.save_model(
        connection["id"],
        {
            "model_id": "synthetic-model",
            "display_name": "Synthetic model",
            "capabilities": {"tools": True, "max_output_tokens": 20000},
            "pricing": {
                "input_per_million": 1,
                "output_per_million": 2,
                "source": "synthetic",
                "as_of": "2026-09-09",
            },
        },
    )
    return {
        "id": connection["id"],
        "stage": "ready",
        "check": {"status": "passed", "model_id": model["model_id"]},
        "models": [model],
        "connection": connection,
    }


class _Setup:
    def __init__(self, *accounts):
        self.accounts = {account["id"]: account for account in accounts}

    def get(self, account_id):
        return self.accounts[account_id]


def _client(repo, setup):
    app = FastAPI()
    app.include_router(create_router(repo, setup))
    return TestClient(app)


def _choose(client, tender_id, account, budget=None):
    return client.post(
        f"/api/tenders/{tender_id}/ai-setup",
        json={
            "account_id": account["id"],
            "model_id": "synthetic-model",
            "budget_usd": budget,
            "engineer_confirmed": True,
        },
    )


def test_first_paid_choice_requires_one_allowance(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic tender")
    first = _account(repo, "First")
    client = _client(repo, _Setup(first))

    with pytest.raises(ValueError, match="spending allowance"):
        _choose(client, tender["id"], first)

    assert _choose(client, tender["id"], first, budget=50).status_code == 200
    policy = AIPolicyService(repo).get(tender["id"])
    assert policy["manager"]["connection_id"] == first["id"]
    assert policy["tender_budget_usd"] == 50
    assert policy["run_budget_usd"] == 50


def test_switching_accounts_keeps_the_approved_allowance(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic tender")
    first = _account(repo, "First")
    second = _account(repo, "Second")
    client = _client(repo, _Setup(first, second))
    assert _choose(client, tender["id"], first, budget=75).status_code == 200

    response = _choose(client, tender["id"], second)

    assert response.status_code == 200
    policy = AIPolicyService(repo).get(tender["id"])
    assert policy["manager"]["connection_id"] == second["id"]
    assert policy["specialist"]["connection_id"] == second["id"]
    assert policy["tender_budget_usd"] == 75
    assert policy["run_budget_usd"] == 75
    assert set(policy["allowed_connection_ids"]) == {first["id"], second["id"]}

    # Switching back needs no new allowance either.
    assert _choose(client, tender["id"], first).status_code == 200
    assert AIPolicyService(repo).get(tender["id"])["manager"]["connection_id"] == first["id"]


def test_switching_requires_the_model_access_check(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic tender")
    unchecked = _account(repo, "Unchecked")
    unchecked["check"] = {"status": "not_checked", "model_id": None}
    client = _client(repo, _Setup(unchecked))

    with pytest.raises(ValueError, match="Check this AI model"):
        _choose(client, tender["id"], unchecked, budget=10)
