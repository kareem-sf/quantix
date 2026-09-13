"""Combined review explicitly grants only displayed current Grok extras authority."""

import json

import pytest

from quantix.ai_connections import AIConnectionService
from quantix.ai_models import ConnectionInput
from quantix.ai_policy import AIPolicyService, BudgetMeter
from quantix.ai_readiness import ready_evidence
from quantix.ai_setup_store import SetupStore
from quantix.jobs import JobManager
from quantix.plan_review import PlanReviewService
from quantix.repository import Repository


def extras_review(tmp_path, monkeypatch):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic extras review")
    connections = AIConnectionService(repo)
    account = connections.create({"name": "Grok subscription", "provider_id": "grok_build", "protocol": "grok_build",
        "auth_type": "client_login", "billing": "subscription", "settings": {"allow_provider_managed_extras": True}})
    model = connections.save_model(account["id"], {"model_id": "synthetic-grok", "display_name": "Synthetic Grok",
        "capabilities": {"tools": True, "structured_output": True, "max_output_tokens": 8192}})
    route = {"connection_id": account["id"], "model_id": model["model_id"], "reasoning": None,
             "max_output_tokens": 2048, "web_search": False, "max_search_calls": 1}
    policy = AIPolicyService(repo)
    policy.update(tender["id"], {"allowed_connection_ids": [account["id"]], "manager": route, "specialist": route,
        "provider_managed_extras": {account["id"]: account["revision"]}, "run_budget_usd": 10,
        "tender_budget_usd": 100, "max_requests": 12, "engineer_confirmed": True, "rationale": "Synthetic grant"})
    plan = repo.create_plan(tender["id"], "Review concrete", [
        {"title": "Concrete", "role": "Reviewer", "description": "Review concrete", "source_ids": []}])
    with connections.lease(account["id"], exclusive=True):
        current = connections.account_access_changed(account["id"])
    store = SetupStore(repo)
    proof = {"check": {"status": "passed", "model_id": model["model_id"]},
             "checked_revision": current["revision"], "checked_component_version": "synthetic"}
    store.update(account["id"], selected_model_id=model["model_id"], **proof)
    store.save_model_check(account["id"], model["model_id"], proof)
    monkeypatch.setattr("quantix.ai_components.AIComponentService.status", lambda *_: {"state": "ready", "version": "synthetic"})
    jobs = JobManager(repo, object())
    service = PlanReviewService(repo, save_runs_in_transaction=jobs.queue_approved_plan_runs)
    return repo, tender, plan, connections, account, current, route, policy, service


@pytest.mark.asyncio
async def test_review_displays_current_extras_then_explicit_approval_renews_grant(tmp_path, monkeypatch):
    repo, tender, plan, connections, old, current, route, policy, service = extras_review(tmp_path, monkeypatch)
    review = service.review(tender["id"], plan["id"])
    assert review["can_approve"], review["blockers"]
    assert review["snapshot"]["provider_managed_extras"] == {old["id"]: current["revision"]}
    for displayed in review["routes"]:
        assert displayed["provider_managed_extras"] is True
        assert "cannot guarantee" in displayed["spending_detail"]
    change = next(item for item in review["meaningful_changes"] if item["code"] == "provider_extras_changed")
    assert change["before"]["revision"] == old["revision"]
    assert change["after"]["revision"] == current["revision"]
    assert policy.get(tender["id"])["provider_managed_extras"] == {old["id"]: old["revision"]}
    preflight_run = repo.create_run(tender["id"], "manager", "Synthetic budget gate")
    with pytest.raises(ValueError, match="extra spending"):
        await BudgetMeter(policy, tender["id"], preflight_run["id"], route).before_request(10, 100)
    repo.update_run(preflight_run["id"], status="completed")
    # Running work changes the review fingerprint, so approval uses its freshly displayed review.
    review = service.review(tender["id"], plan["id"])
    receipt = service.approve_and_start(tender["id"], plan["id"], {"fingerprint": review["fingerprint"], "engineer_confirmed": True})
    assert policy.get(tender["id"])["provider_managed_extras"] == {old["id"]: current["revision"]}
    meter = BudgetMeter(policy, tender["id"], receipt["work_intents"][0]["run_id"], route)
    assert await meter.before_request(10, 100)
    assert ready_evidence(repo, current, route["model_id"])["checked_revision"] == current["revision"]


@pytest.mark.parametrize("change", ["flag", "account", "budget", "grant", "usage"])
def test_changed_extras_review_cannot_grant_or_queue(tmp_path, monkeypatch, change):
    repo, tender, plan, connections, old, current, route, policy, service = extras_review(tmp_path, monkeypatch)
    review = service.review(tender["id"], plan["id"])
    assert review["can_approve"], review["blockers"]
    if change == "flag":
        values = {key: value for key, value in current.items() if key in ConnectionInput.model_fields}
        values["settings"] = values["settings"] | {"allow_provider_managed_extras": False}
        connections.update(current["id"], values)
    elif change == "account":
        with connections.lease(current["id"], exclusive=True):
            connections.account_access_changed(current["id"])
    else:
        with repo.db.connect(write=True) as conn:
            row = conn.execute("SELECT data_json FROM tender_ai_policy WHERE tender_id=?", (tender["id"],)).fetchone()
            data = json.loads(row[0])
            if change == "budget":
                data["run_budget_usd"] = 5
            elif change == "grant":
                data["provider_managed_extras"] = {}
            else:
                data["tender_budget_usd"] = 0
            conn.execute("UPDATE tender_ai_policy SET data_json=? WHERE tender_id=?", (json.dumps(data), tender["id"]))
    before = policy.get(tender["id"])["provider_managed_extras"]
    with pytest.raises(ValueError, match="review"):
        service.approve_and_start(tender["id"], plan["id"], {"fingerprint": review["fingerprint"], "engineer_confirmed": True})
    assert policy.get(tender["id"])["provider_managed_extras"] == before
    assert repo.list_runs(tender["id"]) == []
    assert repo.get_plan(tender["id"], plan["id"])["status"] == "proposed"


def test_unready_extras_model_still_blocks_combined_approval(tmp_path, monkeypatch):
    repo, tender, plan, _, old, current, route, policy, service = extras_review(tmp_path, monkeypatch)
    SetupStore(repo).invalidate_model_checks(current["id"], route["model_id"])
    review = service.review(tender["id"], plan["id"])
    assert review["can_approve"] is False
    assert any(item["code"] == "model_check" for item in review["blockers"])
    assert policy.get(tender["id"])["provider_managed_extras"] == {old["id"]: old["revision"]}


def test_combined_review_does_not_renew_undisplayed_extras_grants(tmp_path, monkeypatch):
    repo, tender, plan, connections, old, current, _, policy, service = extras_review(tmp_path, monkeypatch)
    other = connections.create({"name": "Other Grok account", "provider_id": "grok_build", "protocol": "grok_build",
        "auth_type": "client_login", "billing": "subscription", "settings": {"allow_provider_managed_extras": True}})
    with connections.lease(other["id"], exclusive=True):
        connections.account_access_changed(other["id"])
    with repo.db.connect(write=True) as conn:
        data = json.loads(conn.execute("SELECT data_json FROM tender_ai_policy WHERE tender_id=?", (tender["id"],)).fetchone()[0])
        data["allowed_connection_ids"].append(other["id"])
        data["provider_managed_extras"][other["id"]] = other["revision"]
        data["_connection_versions"][other["id"]] = other["revision"]
        conn.execute("UPDATE tender_ai_policy SET data_json=? WHERE tender_id=?", (json.dumps(data), tender["id"]))
    review = service.review(tender["id"], plan["id"])
    assert review["can_approve"]
    assert review["snapshot"]["provider_managed_extras"] == {current["id"]: current["revision"]}
    service.approve_and_start(tender["id"], plan["id"], {"fingerprint": review["fingerprint"], "engineer_confirmed": True})
    assert policy.get(tender["id"])["provider_managed_extras"] == {
        old["id"]: current["revision"], other["id"]: other["revision"]}
