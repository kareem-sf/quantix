"""Provider rejections release budget holds; other failures keep them uncertain."""

import asyncio
import json

from fastapi.testclient import TestClient

from quantix.ai_api_errors import REJECTION_DETAILS, provider_failure, rejected_before_processing
from quantix.ai_connections import AIConnectionService
from quantix.ai_policy import AIPolicyService, BudgetMeter
from quantix.ai_reservations import release_rejected_reservations
from quantix.api import create_app
from quantix.repository import Repository


class _StatusError(Exception):
    def __init__(self, status):
        super().__init__("synthetic provider body that must not be shown")
        self.status_code = status


def _tender_with_paid_route(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic tender")
    connections = AIConnectionService(repo)
    connection = connections.create(
        {
            "name": "Paid",
            "provider_id": "openai",
            "protocol": "openai_chat",
            "base_url": "https://api.example.test/v1",
            "auth_type": "api_key",
            "billing": "metered",
            "credentials": {"api_key": "synthetic-key"},
            "session_only": True,
        }
    )
    connections.save_model(
        connection["id"],
        {
            "model_id": "synthetic-model",
            "display_name": "Synthetic model",
            "capabilities": {"tools": True, "max_output_tokens": 20000},
            "pricing": {"input_per_million": 1, "output_per_million": 2, "source": "synthetic", "as_of": "2026-09-09"},
        },
    )
    route = {
        "connection_id": connection["id"],
        "model_id": "synthetic-model",
        "reasoning": None,
        "max_output_tokens": 4096,
        "web_search": False,
        "max_search_calls": 3,
    }
    policy = AIPolicyService(repo)
    policy.update(
        tender["id"],
        {
            "allowed_connection_ids": [connection["id"]],
            "manager": route,
            "specialist": None,
            "role_routes": {},
            "fallback_routes": [],
            "run_budget_usd": 1,
            "tender_budget_usd": 1,
            "max_requests": 12,
            "engineer_confirmed": True,
            "rationale": "Synthetic approval",
        },
    )
    return repo, tender, policy, route


def _reserve(repo, tender, policy, route):
    run = repo.create_run(tender["id"], "manager", "Synthetic instruction")
    meter = BudgetMeter(policy, tender["id"], run["id"], route)
    asyncio.run(meter.before_request(20000, 4096))
    assert policy.get(tender["id"])["reserved_usd"] > 0
    return run, meter


def test_only_pre_processing_rejections_are_marked():
    assert rejected_before_processing(provider_failure(_StatusError(400)))
    assert rejected_before_processing(provider_failure(_StatusError(429)))
    assert not rejected_before_processing(provider_failure(_StatusError(503)))
    assert not rejected_before_processing(provider_failure(TimeoutError()))
    assert not rejected_before_processing(ValueError("not a provider failure"))
    assert "synthetic provider body" not in str(provider_failure(_StatusError(400)))


def test_rejected_request_releases_its_reservation(tmp_path):
    repo, tender, policy, route = _tender_with_paid_route(tmp_path)
    _, meter = _reserve(repo, tender, policy, route)

    meter.interrupted(provider_failure(_StatusError(400)))

    current = policy.get(tender["id"])
    assert current["reserved_usd"] == 0
    assert current["spent_usd"] == 0
    assert [row["status"] for row in policy.usage(tender["id"])] == ["rejected"]


def test_server_failure_keeps_the_reservation_uncertain(tmp_path):
    repo, tender, policy, route = _tender_with_paid_route(tmp_path)
    _, meter = _reserve(repo, tender, policy, route)

    meter.interrupted(provider_failure(_StatusError(503)))

    assert policy.get(tender["id"])["reserved_usd"] > 0
    assert [row["status"] for row in policy.usage(tender["id"])] == ["uncertain"]


def test_usage_list_serves_metered_rows(tmp_path):
    home = tmp_path / "home"
    home.mkdir()
    repo, tender, policy, route = _tender_with_paid_route(home)
    _, rejected = _reserve(repo, tender, policy, route)
    rejected.interrupted(provider_failure(_StatusError(400)))
    _, failed = _reserve(repo, tender, policy, route)
    failed.interrupted(provider_failure(_StatusError(503)))

    with TestClient(create_app(home, "test-session-token")) as client:
        client.headers["Authorization"] = "Bearer test-session-token"
        response = client.get(f"/api/tenders/{tender['id']}/ai-usage")

    assert response.status_code == 200
    assert sorted(row["status"] for row in response.json()) == ["rejected", "uncertain"]
    assert all("root_run_id" not in row for row in response.json())


def test_startup_repair_releases_holds_left_by_rejected_runs(tmp_path):
    repo, tender, policy, route = _tender_with_paid_route(tmp_path)
    rejected_run, rejected_meter = _reserve(repo, tender, policy, route)
    rejected_meter.interrupted()
    repo.update_run(rejected_run["id"], status="failed", error=REJECTION_DETAILS[0])
    other_run, other_meter = _reserve(repo, tender, policy, route)
    other_meter.interrupted()
    repo.update_run(other_run["id"], status="failed", error="Quantix closed before this work finished.")

    assert release_rejected_reservations(repo) == 1
    assert release_rejected_reservations(repo) == 0

    rows = {row["run_id"]: row for row in policy.usage(tender["id"])}
    assert rows[rejected_run["id"]]["status"] == "rejected"
    assert rows[rejected_run["id"]]["reserved_usd"] == 0
    assert rows[other_run["id"]]["status"] == "uncertain"
    with repo.db.connect() as conn:
        decisions = conn.execute(
            "SELECT decision FROM decisions WHERE target_type='ai_usage'"
        ).fetchall()
    assert [row[0] for row in decisions] == ["release_rejected"]
    assert json.dumps(rows[rejected_run["id"]])
