import json

import pytest

from quantix.ai_connections import AIConnectionService
from quantix.ai_policy import AIPolicyService
from quantix.manager_runtime import ManagerRunProfiles
from quantix.plan_review import PlanReviewService
from quantix.repository import Repository


def _connection(repo, name="OpenAI"):
    connections = AIConnectionService(repo)
    connection = connections.create(
        {
            "name": name,
            "provider_id": "openai",
            "protocol": "openai_responses",
            "base_url": "https://api.example.test/v1",
            "auth_type": "api_key",
            "billing": "metered",
            "credentials": {"api_key": "synthetic-key"},
            "session_only": True,
        }
    )
    model = connections.save_model(
        connection["id"],
        {
            "model_id": "synthetic-model",
            "display_name": "Synthetic model",
            "capabilities": {
                "tools": True,
                "structured_output": True,
                "web_search": True,
                "reasoning": ["medium", "high"],
                "max_output_tokens": 20000,
            },
            "pricing": {
                "input_per_million": 1,
                "output_per_million": 2,
                "source": "synthetic",
                "as_of": "2026-09-09",
            },
        },
    )
    return connection, model


def _plan(repo):
    tender = repo.create_tender("Synthetic foundations")
    plan = repo.create_plan(
        tender["id"],
        "Synthetic review",
        [
            {
                "title": "Review concrete",
                "description": "Check the concrete requirements.",
                "role": "concrete",
                "source_ids": [],
            },
            {
                "title": "Review steel",
                "description": "Check the reinforcement requirements.",
                "role": "steel",
                "source_ids": [],
            },
        ],
    )
    return tender, plan


def _route(connection_id, *, reasoning="medium", output=4096, search=False, calls=7):
    return {
        "connection_id": connection_id,
        "model_id": "synthetic-model",
        "reasoning": reasoning,
        "max_output_tokens": output,
        "web_search": search,
        "max_search_calls": calls,
    }


def _mark_direct_model_ready(repo, connection, model):
    """Persist the same synthetic setup proof used by the real readiness gate."""
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "CREATE TABLE IF NOT EXISTS ai_setup_accounts (connection_id TEXT PRIMARY KEY, data_json TEXT NOT NULL)"
        )
        conn.execute(
            "CREATE TABLE IF NOT EXISTS ai_setup_model_checks (connection_id TEXT NOT NULL, model_id TEXT NOT NULL, data_json TEXT NOT NULL, PRIMARY KEY(connection_id,model_id))"
        )
        proof = {
            "checked_revision": connection["revision"],
            "checked_component_version": "synthetic",
            "check": {"status": "passed", "model_id": model["model_id"]},
        }
        conn.execute(
            "INSERT OR REPLACE INTO ai_setup_accounts VALUES(?,?)",
            (connection["id"], json.dumps({"stage": "ready", "selected_model_id": model["model_id"], **proof})),
        )
        conn.execute(
            "INSERT OR REPLACE INTO ai_setup_model_checks VALUES(?,?,?)",
            (connection["id"], model["model_id"], json.dumps(proof)),
        )


def test_review_get_is_read_only_and_returns_typed_blockers(tmp_path):
    repo = Repository(tmp_path)
    tender, plan = _plan(repo)
    service = PlanReviewService(repo)

    review = service.review(tender["id"], plan["id"])

    assert review["plan_id"] == plan["id"]
    assert review["fingerprint"]
    assert review["tasks"][0]["title"] == "Review concrete"
    assert review["ai_summary"]
    assert review["can_approve"] is False
    assert {item["code"] for item in review["blockers"]} >= {"configuration"}
    with repo.db.connect() as conn:
        assert conn.execute("SELECT 1 FROM plan_ai_team").fetchone() is None
        assert conn.execute("SELECT 1 FROM tender_ai_policy").fetchone() is None


def test_refresh_keeps_task_specific_route_settings(tmp_path):
    repo = Repository(tmp_path)
    tender, plan = _plan(repo)
    connection, _ = _connection(repo)
    policy = AIPolicyService(repo)
    manager = _route(connection["id"], reasoning="medium", output=2048, search=False, calls=3)
    specialist = _route(connection["id"], reasoning="medium", output=4096, search=False, calls=4)
    role_route = _route(connection["id"], reasoning="high", output=12288, search=True, calls=9)
    policy.update(
        tender["id"],
        {
            "allowed_connection_ids": [connection["id"]],
            "manager": manager,
            "specialist": specialist,
            "role_routes": {},
            "fallback_routes": [],
            "run_budget_usd": 10,
            "tender_budget_usd": 100,
            "max_requests": 12,
            "engineer_confirmed": True,
            "rationale": "Synthetic policy",
        },
    )
    first = policy.propose_team(
        tender["id"], plan["id"],
        recommendations={plan["tasks"][0]["id"]: role_route},
    )
    refreshed = policy.propose_team(tender["id"], plan["id"], refresh=True)

    first_route = next(item["route"] for item in first["specialists"] if item["task_id"] == plan["tasks"][0]["id"])
    refreshed_route = next(item["route"] for item in refreshed["specialists"] if item["task_id"] == plan["tasks"][0]["id"])
    assert refreshed_route == first_route


def test_catalog_null_to_checked_capability_does_not_revoke_revision(tmp_path):
    repo = Repository(tmp_path)
    connection, _ = _connection(repo)
    connections = AIConnectionService(repo)
    current = connections.get(connection["id"])
    # A provider catalog can initially omit a capability. A passed synthetic
    # check establishes it without changing the connection authority revision.
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "CREATE TABLE IF NOT EXISTS ai_setup_model_checks (connection_id TEXT NOT NULL, model_id TEXT NOT NULL, data_json TEXT NOT NULL, PRIMARY KEY(connection_id,model_id))"
        )
        conn.execute(
            "INSERT INTO ai_setup_model_checks VALUES(?,?,?)",
            (connection["id"], "synthetic-model", json.dumps({"checked_revision": current["revision"], "check": {"status": "passed", "model_id": "synthetic-model"}})),
        )
        conn.execute(
            "UPDATE ai_connection_models SET data_json=? WHERE connection_id=? AND model_id=?",
            (
                json.dumps(
                    {
                        "model_id": "synthetic-model",
                        "display_name": "Synthetic model",
                        "capabilities": {"tools": None, "structured_output": None},
                        "pricing": None,
                        "connection_id": connection["id"],
                        "source": "provider",
                        "updated_at": "2026-09-09T00:00:00+00:00",
                    },
                    separators=(",", ":"),
                ),
                connection["id"],
                "synthetic-model",
            ),
        )
    revision = connections.get(connection["id"])["revision"]
    connections.store_discovered_models(
        connection["id"],
        [
            {
                "model_id": "synthetic-model",
                "display_name": "Synthetic model",
                "capabilities": {"tools": True, "structured_output": True},
                "pricing": None,
            }
        ],
        revision,
    )
    assert connections.get(connection["id"])["revision"] == revision


def test_discovery_check_discovery_check_preserves_observed_capabilities(tmp_path):
    repo = Repository(tmp_path)
    connections = AIConnectionService(repo)
    connection = connections.create(
        {
            "name": "OpenAI",
            "provider_id": "openai",
            "protocol": "openai_chat",
            "base_url": "https://api.example.test/v1",
            "auth_type": "api_key",
            "billing": "metered",
            "credentials": {"api_key": "synthetic-key"},
            "session_only": True,
        }
    )
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "CREATE TABLE ai_setup_model_checks (connection_id TEXT NOT NULL, model_id TEXT NOT NULL, data_json TEXT NOT NULL, PRIMARY KEY(connection_id,model_id))"
        )
    discovered = {
        "model_id": "provider-model",
        "display_name": "Provider model",
        "capabilities": {"tools": None, "structured_output": None},
        "pricing": None,
    }
    connections.store_discovered_models(connection["id"], [discovered], connection["revision"])
    current = connections.get(connection["id"])
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "INSERT INTO ai_setup_model_checks VALUES(?,?,?)",
            (connection["id"], "provider-model", json.dumps({"checked_revision": current["revision"], "check": {"status": "passed", "model_id": "provider-model"}})),
        )
    connections.observe_model_capabilities(
        connection["id"], "provider-model", {"tools": True, "structured_output": True}, current["revision"]
    )
    checked = connections.get(connection["id"])
    assert checked["revision"] == current["revision"]

    connections.store_discovered_models(
        connection["id"],
        [{**discovered, "capabilities": {"tools": None, "structured_output": None}}],
        checked["revision"],
    )
    refreshed = connections.get(connection["id"])
    assert refreshed["revision"] == checked["revision"]
    assert connections.models(connection["id"])[0]["capabilities"]["tools"] is True

    # An explicit capability denial invalidates the affected model proof,
    # while account authority remains unchanged.
    connections.store_discovered_models(
        connection["id"],
        [{**discovered, "capabilities": {"tools": False, "structured_output": True}}],
        refreshed["revision"],
    )
    assert connections.get(connection["id"])["revision"] == refreshed["revision"]
    with repo.db.connect() as conn:
        assert conn.execute("SELECT 1 FROM ai_setup_model_checks WHERE connection_id=? AND model_id=?", (connection["id"], "provider-model")).fetchone() is None


def test_setup_discovery_check_discovery_check_keeps_readiness_on_same_model(tmp_path, monkeypatch):
    class SyntheticDirect:
        async def catalog(self, _connection, _credentials):
            return [{
                "model_id": "provider-model",
                "display_name": "Provider model",
                "capabilities": {"tools": None, "structured_output": None},
                "pricing": {"input_per_million": 1, "output_per_million": 1, "source": "synthetic", "as_of": "2026"},
            }]

        async def check(self, _route, _connection, _credentials, *, before_request, on_response):
            reservation = await before_request(100, 1024)
            await on_response({"usage_complete": True, "input_tokens": 10, "output_tokens": 20, "requests": 1}, reservation)
            return {"tools_supported": True, "output_supported": True}

        def close(self):
            return None

    monkeypatch.setattr("quantix.ai_direct.direct_runtime_status", lambda _connection: {
        "component_id": "direct-api", "state": "ready", "detail": "ready", "progress": 100, "version": "synthetic",
    })
    from quantix.ai_setup import AISetupService

    repo = Repository(tmp_path)
    connections = AIConnectionService(repo)
    connection = connections.create({
        "name": "OpenAI", "provider_id": "openai", "protocol": "openai_chat",
        "base_url": "https://api.example.test/v1", "auth_type": "api_key",
        "billing": "metered", "credentials": {"api_key": "synthetic-key"}, "session_only": True,
    })
    setup = AISetupService(repo, direct=SyntheticDirect())
    import asyncio

    asyncio.run(setup._discover(connection["id"]))
    account = setup.get(connection["id"])
    assert account["stage"] == "ready_to_check"
    preview = setup.check_preview(connection["id"])
    revision_before_check = connections.get(connection["id"])["revision"]
    asyncio.run(setup._check(connection["id"], preview))
    account_after_check = setup.get(connection["id"])
    assert account_after_check["stage"] == "ready"
    assert connections.get(connection["id"])["revision"] == revision_before_check

    asyncio.run(setup._discover(connection["id"]))
    after_refresh = setup.get(connection["id"])
    assert after_refresh["stage"] == "ready"
    assert connections.get(connection["id"])["revision"] == revision_before_check
    asyncio.run(setup._check(connection["id"], setup.check_preview(connection["id"])))
    assert setup.get(connection["id"])["stage"] == "ready"


def test_approval_requires_exact_review_and_creates_intents_once(tmp_path, monkeypatch):
    repo = Repository(tmp_path)
    tender, plan = _plan(repo)
    connection, model = _connection(repo)
    connection = AIConnectionService(repo).get(connection["id"])
    policy = AIPolicyService(repo)
    route = _route(connection["id"])
    policy.update(
        tender["id"],
        {
            "allowed_connection_ids": [connection["id"]],
            "manager": route,
            "specialist": route,
            "role_routes": {},
            "fallback_routes": [],
            "run_budget_usd": 10,
            "tender_budget_usd": 100,
            "max_requests": 12,
            "engineer_confirmed": True,
            "rationale": "Synthetic policy",
        },
    )
    policy.propose_team(tender["id"], plan["id"])
    # Model a legacy plan policy/account proof. The account remains ready on
    # its current revision and needs explicit review to renew this route.
    with repo.db.connect(write=True) as conn:
        row = conn.execute("SELECT data_json FROM tender_ai_policy WHERE tender_id=?", (tender["id"],)).fetchone()
        legacy = json.loads(row[0])
        legacy["_connection_versions"][connection["id"]] = 6
        conn.execute("UPDATE tender_ai_policy SET revision=?,data_json=? WHERE tender_id=?", (4, json.dumps(legacy), tender["id"]))
    # Explicitly seed a current model check; readiness remains separate from
    # the connection catalog and is consumed by approval validation.
    with repo.db.connect(write=True) as conn:
        conn.execute(
            "CREATE TABLE IF NOT EXISTS ai_setup_accounts (connection_id TEXT PRIMARY KEY, data_json TEXT NOT NULL)"
        )
        conn.execute(
            "CREATE TABLE IF NOT EXISTS ai_setup_model_checks (connection_id TEXT NOT NULL, model_id TEXT NOT NULL, data_json TEXT NOT NULL, PRIMARY KEY(connection_id,model_id))"
        )
        conn.execute(
            "INSERT INTO ai_setup_accounts VALUES(?,?)",
            (connection["id"], json.dumps({"stage": "ready", "selected_model_id": model["model_id"], "check": {"status": "passed", "model_id": model["model_id"]}, "checked_revision": connection["revision"], "checked_component_version": "synthetic"})),
        )
        conn.execute(
            "INSERT INTO ai_setup_model_checks VALUES(?,?,?)",
            (connection["id"], model["model_id"], json.dumps({"checked_revision": connection["revision"], "checked_component_version": "synthetic", "check": {"status": "passed", "model_id": model["model_id"]} })),
        )
    monkeypatch.setattr(
        "quantix.plan_review.ready_evidence",
        lambda _repo, _connection, model_id: {
            "checked_revision": connection["revision"],
            "checked_component_version": "synthetic",
            "check": {"status": "passed", "model_id": model_id},
        },
    )
    monkeypatch.setattr(
        "quantix.ai_direct.direct_runtime_status",
        lambda _connection: {"state": "ready", "version": "synthetic"},
    )

    def queue_runs(tender_id, _plan_id, current_plan, _review):
        root = repo.create_run(
            tender_id,
            "manager",
            f"Carry out the approved work plan: {current_plan['title']}",
        )
        ManagerRunProfiles(repo).capture(tender_id, root["id"])
        repo.event(root["id"], "approved_office", "The Tender Manager is queued for the approved work.", {"plan_id": _plan_id})
        return [{"run": root, "task_id": None}]

    scheduled = []
    service = PlanReviewService(
        repo,
        save_runs_in_transaction=queue_runs,
        schedule_after_commit=lambda tender_id, plan_id, intents: scheduled.append(
            (tender_id, plan_id, intents, repo.get_plan(tender_id, plan_id)["status"])
        ),
    )
    review = service.review(tender["id"], plan["id"])
    assert review["can_approve"] is True
    with repo.db.connect() as conn:
        decision_count_before = conn.execute("SELECT COUNT(*) FROM decisions WHERE tender_id=?", (tender["id"],)).fetchone()[0]
    with repo.db.connect(write=True) as conn:
        row = conn.execute(
            "SELECT data_json FROM ai_connection_models WHERE connection_id=? AND model_id=?",
            (connection["id"], model["model_id"]),
        ).fetchone()
        updated = json.loads(row[0])
        updated["updated_at"] = "2026-09-09T23:59:59+00:00"
        conn.execute(
            "UPDATE ai_connection_models SET data_json=? WHERE connection_id=? AND model_id=?",
            (json.dumps(updated), connection["id"], model["model_id"]),
        )
    assert service.review(tender["id"], plan["id"])["fingerprint"] == review["fingerprint"]

    result = service.approve_and_start(tender["id"], plan["id"], {"fingerprint": review["fingerprint"], "engineer_confirmed": True, "rationale": "Synthetic approval"})
    again = service.approve_and_start(tender["id"], plan["id"], {"fingerprint": review["fingerprint"], "engineer_confirmed": True, "rationale": "Synthetic retry"})

    assert result["plan"]["status"] == "approved"
    assert len(result["work_intents"]) == 1
    assert again["work_intents"] == result["work_intents"]
    assert len(scheduled) == 1
    assert scheduled[0][3] == "approved"
    assert {item["run_id"] for item in scheduled[0][2]} == {item["run_id"] for item in result["work_intents"]}
    assert result["work_intents"][0]["kind"] == "manager"
    assert result["work_intents"][0]["task_id"] is None
    assert policy.routes_for(tender["id"], plan_id=plan["id"])
    for task in plan["tasks"]:
        assert policy.routes_for(tender["id"], plan_id=plan["id"], task_id=task["id"])
    with repo.db.connect() as conn:
        policy_data = json.loads(conn.execute("SELECT data_json FROM tender_ai_policy WHERE tender_id=?", (tender["id"],)).fetchone()[0])
        assert policy_data["_connection_versions"][connection["id"]] == connection["revision"]
        assert conn.execute("SELECT COUNT(*) FROM decisions WHERE tender_id=?", (tender["id"],)).fetchone()[0] == decision_count_before + 1

    with pytest.raises(ValueError, match="pattern"):
        service.approve_and_start(tender["id"], plan["id"], {"fingerprint": "wrong", "engineer_confirmed": True, "rationale": "No"})


def test_approval_queue_failure_rolls_back_plan_team_policy_and_runs(tmp_path, monkeypatch):
    repo = Repository(tmp_path)
    tender, plan = _plan(repo)
    connection, model = _connection(repo)
    connection = AIConnectionService(repo).get(connection["id"])
    policy = AIPolicyService(repo)
    route = _route(connection["id"])
    policy.update(
        tender["id"],
        {
            "allowed_connection_ids": [connection["id"]],
            "manager": route,
            "specialist": route,
            "role_routes": {},
            "fallback_routes": [],
            "run_budget_usd": 10,
            "tender_budget_usd": 100,
            "max_requests": 12,
            "engineer_confirmed": True,
            "rationale": "Synthetic policy",
        },
    )
    _mark_direct_model_ready(repo, connection, model)
    monkeypatch.setattr("quantix.plan_review.ready_evidence", lambda *_args: {"check": {"status": "passed"}})
    monkeypatch.setattr(
        "quantix.ai_direct.direct_runtime_status",
        lambda _connection: {"state": "ready", "version": "synthetic"},
    )

    def queue_fails_second(tender_id, _plan_id, current_plan, _review):
        root = repo.create_run(
            tender_id,
            "manager",
            f"Carry out the approved work plan: {current_plan['title']}",
        )
        ManagerRunProfiles(repo).capture(tender_id, root["id"])
        raise RuntimeError("synthetic queue failure")

    service = PlanReviewService(repo, save_runs_in_transaction=queue_fails_second)
    review = service.review(tender["id"], plan["id"])
    with pytest.raises(RuntimeError, match="queue failure"):
        service.approve_and_start(
            tender["id"], plan["id"],
            {"fingerprint": review["fingerprint"], "engineer_confirmed": True, "rationale": ""},
        )
    assert repo.get_plan(tender["id"], plan["id"])["status"] == "proposed"
    assert repo.list_runs(tender["id"]) == []
    with repo.db.connect() as conn:
        assert conn.execute("SELECT 1 FROM plan_ai_team WHERE plan_id=?", (plan["id"],)).fetchone() is None
        saved = json.loads(conn.execute("SELECT data_json FROM tender_ai_policy WHERE tender_id=?", (tender["id"],)).fetchone()[0])
        assert saved["_connection_versions"][connection["id"]] == connection["revision"]


def _approval_context(tmp_path, monkeypatch):
    repo = Repository(tmp_path)
    tender, plan = _plan(repo)
    connection, model = _connection(repo)
    connection = AIConnectionService(repo).get(connection["id"])
    policy = AIPolicyService(repo)
    route = _route(connection["id"])
    policy.update(
        tender["id"],
        {
            "allowed_connection_ids": [connection["id"]],
            "manager": route,
            "specialist": route,
            "role_routes": {},
            "fallback_routes": [],
            "run_budget_usd": 10,
            "tender_budget_usd": 100,
            "max_requests": 12,
            "engineer_confirmed": True,
            "rationale": "Synthetic policy",
        },
    )
    monkeypatch.setattr("quantix.plan_review.ready_evidence", lambda *_args: {"check": {"status": "passed"}})
    service = PlanReviewService(repo, save_runs_in_transaction=lambda *_args: [])
    return repo, tender, plan, connection, model, policy, service


@pytest.mark.parametrize("change", ["source", "usage", "budget", "account", "idle"])
def test_review_fingerprint_rejects_concurrent_source_usage_budget_account_or_idle_change(tmp_path, monkeypatch, change):
    repo, tender, plan, connection, _model, policy, service = _approval_context(tmp_path, monkeypatch)
    review = service.review(tender["id"], plan["id"])
    if change == "source":
        repo.register_artifact(tender["id"], "new.pdf", "a" * 64, 1, {"kind": "pdf", "status": "unsupported", "segments": []})
    elif change == "usage":
        usage_run = repo.create_run(tender["id"], "task", "Usage holder")
        with repo.db.connect(write=True) as conn:
            conn.execute(
                "INSERT INTO ai_usage VALUES(?,?,?,?,?,?,?)",
                ("usage", usage_run["id"], tender["id"], connection["id"], "synthetic-model", json.dumps({"status": "reserved", "reserved_usd": 1, "requests": 1}), "2026-09-09T00:00:00+00:00"),
            )
    elif change == "budget":
        current = policy.get(tender["id"])
        policy.update(
            tender["id"],
            {
                "allowed_connection_ids": current["allowed_connection_ids"],
                "manager": current["manager"],
                "specialist": current["specialist"],
                "role_routes": current["role_routes"],
                "fallback_routes": current["fallback_routes"],
                "run_budget_usd": 9,
                "tender_budget_usd": current["tender_budget_usd"],
                "max_requests": current["max_requests"],
                "engineer_confirmed": True,
                "rationale": "Synthetic budget change",
            },
        )
    elif change == "account":
        AIConnectionService(repo).rename(connection["id"], "Renamed account")
    else:
        repo.create_run(tender["id"], "manager", "Synthetic busy work")

    with pytest.raises(ValueError, match="review"):
        service.approve_and_start(
            tender["id"], plan["id"],
            {"fingerprint": review["fingerprint"], "engineer_confirmed": True, "rationale": "Synthetic approval"},
        )
