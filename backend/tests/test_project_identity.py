import importlib
import json
import sqlite3
import time

import pytest
from fastapi.testclient import TestClient
from openpyxl import Workbook

from quantix.project_identity import (
    PreparedIdentityResult,
    ProjectIdentity,
    _score,
    apply_identity,
    package_name,
)
from quantix.repository import Repository


@pytest.fixture
def client(tmp_path, monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    monkeypatch.setattr("keyring.get_password", lambda *_: None)
    app = importlib.import_module("quantix.api").create_app(tmp_path / "home", "test-session-token")
    with TestClient(app) as test_client:
        test_client.headers["Authorization"] = "Bearer test-session-token"
        yield test_client


def _package(tmp_path, folder="Harbour_Road Upgrade"):
    package = tmp_path / folder
    package.mkdir()
    workbook = Workbook()
    workbook.active.append(["Item", "Description", "Unit", "Quantity"])
    workbook.active.append(["C1", "Reinforced concrete kerb", "m", 120])
    workbook.save(package / "Bill of Quantities.xlsx")
    workbook.close()
    return package


def _wait(client, run):
    deadline = time.monotonic() + 15
    while run["status"] in {"queued", "running"} and time.monotonic() < deadline:
        time.sleep(0.03)
        run = client.get(f"/api/runs/{run['id']}").json()
    return run


def test_unnamed_tender_takes_its_package_name_and_queues_identification(client, tmp_path, monkeypatch):
    identified = []
    from quantix.jobs import JobManager

    monkeypatch.setattr(JobManager, "start_analysis", lambda self, tender_id: identified.append(tender_id))
    created = client.post("/api/tenders", json={})
    assert created.status_code == 200, created.text
    tender = created.json()
    assert (tender["name"], tender["name_source"]) == ("New tender", "pending")
    run = client.post(f"/api/tenders/{tender['id']}/imports", json={"source_path": str(_package(tmp_path))}).json()
    # The folder name is never shown as the project name while analysis runs.
    assert client.get(f"/api/tenders/{tender['id']}").json()["tender"]["name_source"] == "pending"
    assert _wait(client, run)["status"] == "completed"
    assert identified == [tender["id"]]


def test_identification_run_publishes_the_project_name(client, tmp_path, monkeypatch):
    from quantix import project_identity
    from quantix.jobs import JobManager

    monkeypatch.setattr(JobManager, "maybe_analyze", lambda *_: None)
    tender = client.post("/api/tenders", json={}).json()
    run = client.post(f"/api/tenders/{tender['id']}/imports", json={"source_path": str(_package(tmp_path))}).json()
    assert _wait(client, run)["status"] == "completed"

    async def identify(repo, tender_id, run_id):
        return PreparedIdentityResult(tender_id, run_id, ProjectIdentity(name="Harbour Road Upgrade Phase 2"), {"requests": 1})

    monkeypatch.setattr(project_identity, "run_identification", identify)
    monkeypatch.setattr(JobManager, "_preflight_route", lambda *_args, **_kwargs: None)
    identify_run = client.portal.call(_start, client.app.state.jobs, tender["id"])
    assert _wait(client, identify_run)["status"] == "completed"
    saved = client.get(f"/api/tenders/{tender['id']}").json()["tender"]
    assert (saved["name"], saved["name_source"]) == ("Harbour Road Upgrade Phase 2", "ai")


async def _start(manager, tender_id):
    return manager.start_identify(tender_id)


def test_named_tender_is_never_renamed_or_identified_by_import(client, tmp_path, monkeypatch):
    from quantix.jobs import JobManager

    monkeypatch.setattr(JobManager, "_ai_ready", lambda *_: "No AI is chosen for this Tender.")
    tender = client.post("/api/tenders", json={"name": "Foundations"}).json()
    assert tender["name_source"] == "engineer"
    run = client.post(f"/api/tenders/{tender['id']}/imports", json={"source_path": str(_package(tmp_path))}).json()
    assert _wait(client, run)["status"] == "completed"
    analysis = next(r for r in client.app.state.repo.list_runs(tender["id"]) if r["kind"] == "analysis")
    assert _wait(client, analysis)["status"] == "completed"
    assert client.get(f"/api/tenders/{tender['id']}").json()["tender"]["name"] == "Foundations"


def test_without_ai_access_the_package_is_analysed_locally_and_named_provisionally(client, tmp_path):
    tender = client.post("/api/tenders", json={}).json()
    run = client.post(f"/api/tenders/{tender['id']}/imports", json={"source_path": str(_package(tmp_path))}).json()
    assert _wait(client, run)["status"] == "completed"
    analysis = next(r for r in client.app.state.repo.list_runs(tender["id"]) if r["kind"] == "analysis")
    assert _wait(client, analysis)["status"] == "completed"
    saved = client.get(f"/api/tenders/{tender['id']}").json()["tender"]
    assert (saved["name"], saved["name_source"]) == ("Harbour Road Upgrade", "package")
    messages = client.get(f"/api/tenders/{tender['id']}/messages").json()
    assert any("Choose the AI for this Tender" in message["content"] for message in messages)


def test_failed_identification_is_visible_and_names_the_tender_provisionally(client, tmp_path, monkeypatch):
    from quantix import project_identity
    from quantix.jobs import JobManager

    tender = client.post("/api/tenders", json={}).json()
    monkeypatch.setattr(JobManager, "maybe_analyze", lambda *_: None)
    run = client.post(f"/api/tenders/{tender['id']}/imports", json={"source_path": str(_package(tmp_path))}).json()
    assert _wait(client, run)["status"] == "completed"

    async def broken(*_args):
        raise ValueError("The AI account needs to sign in again.")

    monkeypatch.setattr(project_identity, "run_identification", broken)
    monkeypatch.setattr(JobManager, "_preflight_route", lambda *_args, **_kwargs: None)
    failed = _wait(client, client.portal.call(_start, client.app.state.jobs, tender["id"]))
    assert failed["status"] == "failed"
    saved = client.get(f"/api/tenders/{tender['id']}").json()["tender"]
    assert (saved["name"], saved["name_source"]) == ("Harbour Road Upgrade", "package")
    messages = client.get(f"/api/tenders/{tender['id']}/messages").json()
    assert any("could not finish analyzing the tender package" in message["content"] for message in messages)


def test_engineer_rename_is_protected_from_identification(client):
    tender = client.post("/api/tenders", json={}).json()
    renamed = client.patch(f"/api/tenders/{tender['id']}", json={"name": "  Corniche Works  "})
    assert renamed.status_code == 200
    assert (renamed.json()["name"], renamed.json()["name_source"]) == ("Corniche Works", "engineer")
    assert client.patch(f"/api/tenders/{tender['id']}", json={"name": " "}).status_code == 422
    repo = client.app.state.repo
    run = repo.create_run(tender["id"], "identify")
    result = apply_identity(repo, PreparedIdentityResult(tender["id"], run["id"], ProjectIdentity(name="Other"), {}))
    assert result["renamed"] is False
    assert repo.get_tender(tender["id"])["name"] == "Corniche Works"


def test_identification_keeps_proposed_facts_separate_from_reviewed_profile(tmp_path):
    from quantix.execution_context import engineer_identity
    from quantix.tender_profile import TenderProfileService
    from quantix.tender_profile_models import ProfilePatch

    repo = Repository(tmp_path / "home")
    tender = repo.create_tender()
    profiles = TenderProfileService(repo)
    profiles.update(engineer_identity(tender["id"]), ProfilePatch(expected_revision=1, idempotency_key="seed", country="Egypt"))
    run = repo.create_run(tender["id"], "identify")
    identity = ProjectIdentity(
        name="New Administrative Capital Water Network", client="NUCA", location="Cairo", country="Saudi Arabia",
        contract_type="FIDIC Red Book", currencies=["egp", "usd"], submission_deadline="12 October 2026, 12:00",
        summary="Supply and installation of a potable water network.", sources=["01 ITT/Invitation.pdf"],
    )
    with repo.atomic():
        result = apply_identity(repo, PreparedIdentityResult(tender["id"], run["id"], identity, {"requests": 1}))
    saved = repo.get_tender(tender["id"])
    assert (saved["name"], saved["name_source"]) == ("New Administrative Capital Water Network", "ai")
    profile = profiles.get(tender["id"])
    assert profile.country == "Egypt"  # the engineer's value is kept
    assert not profile.geography and not profile.contract_type and not profile.currencies
    assert "01 ITT/Invitation.pdf" not in profile.source_refs
    assert result["profile_fields"] == []
    assert result["identity"]["contract_type"] == "FIDIC Red Book"
    assert "need source review" in repo.messages(tender["id"])[-1]["content"]
    message = repo.messages(tender["id"])[-1]
    assert message["role"] == "manager"
    assert "**New Administrative Capital Water Network**" in message["content"]
    assert "Submission deadline: 12 October 2026, 12:00" in message["content"]


def test_package_names_and_document_priority():
    assert package_name("D:\\Tenders\\Harbour_Road  Upgrade\\") == "Harbour Road Upgrade"
    assert package_name("/tmp/Juhayna Dairy.ZIP") == "Juhayna Dairy"
    telling = {"relative_path": "01 Tender/Invitation to Tender.pdf"}
    buried = {"relative_path": "drawings/structure/level 3/S-301.dwg"}
    assert _score(telling) > _score(buried)


@pytest.mark.parametrize("previous_approval", [False, True])
def test_new_tender_creation_does_not_grant_ai_authority(client, monkeypatch, previous_approval):
    from quantix.ai_policy import AIPolicyService
    from quantix.ai_setup import AISetupService

    repo = client.app.state.repo
    policy = AIPolicyService(repo)
    if previous_approval:
        earlier = repo.create_tender("Earlier project")
        with repo.db.connect(write=True) as conn:
            conn.execute("INSERT INTO tender_ai_policy VALUES(?,?,?,?)", (earlier["id"], 1, json.dumps({
                "allowed_connection_ids": ["acct"], "manager": {"connection_id": "acct", "model_id": "model"},
                "run_budget_usd": 1.5, "tender_budget_usd": 2.0, "provider_managed_extras": {"acct": 1},
            }), "2026-09-13T00:00:00Z"))
    grants = []
    monkeypatch.setattr(AIPolicyService, "update", lambda self, tender_id, values: grants.append(values))
    monkeypatch.setattr(AISetupService, "list", lambda self: [
        {"stage": "ready", "check": {"model_id": "model"},
         "connection": {"id": "acct", "revision": 1, "billing": "subscription"}},
    ])
    # Capture either inheritance branch without requiring a real account.
    monkeypatch.setattr("quantix.ai_setup_routes.select_tender_ai", lambda *args, **kwargs: grants.append("selection"))
    created = client.post("/api/tenders", json={})
    assert created.status_code == 200
    assert grants == []
    current = policy.get(created.json()["id"])
    assert current["manager"] is None
    assert current["allowed_connection_ids"] == []


def test_existing_workspaces_gain_the_name_source_column(tmp_path):
    from quantix import db

    home = tmp_path / "home"
    Repository(home)
    path = home / "quantix.sqlite"
    with sqlite3.connect(path) as conn:
        conn.execute("ALTER TABLE tenders DROP COLUMN name_source")
        conn.execute("INSERT INTO tenders(id,name,created_at,updated_at) VALUES('t','Old','x','x')")
        conn.execute("PRAGMA user_version=2")
    repo = Repository(home)
    assert repo.get_tender("t")["name_source"] == "engineer"
    with sqlite3.connect(path) as conn:
        assert conn.execute("PRAGMA user_version").fetchone()[0] == db.CURRENT_SCHEMA_VERSION == 4


def test_identification_runs_pin_the_manager_profile(tmp_path):
    from quantix.jobs import JobManager
    from quantix.manager_runtime import ManagerRunProfiles

    repo = Repository(tmp_path / "home")
    tender = repo.create_tender()
    with repo.atomic():
        run = JobManager(repo, object())._queue(tender["id"], "identify", "Identify the project from its package.")
    # Budget guards read this snapshot; a missing one failed the live run.
    ManagerRunProfiles(repo).get(tender["id"], run["id"])


async def test_subscription_route_reserves_without_a_usd_allowance(tmp_path, monkeypatch):
    from test_catalog_authority import configured_office

    from quantix.ai_policy import BudgetMeter

    repo, tender, _connections, _connection, _model, policy, route = configured_office(tmp_path, monkeypatch)
    current = policy.get(tender["id"])
    policy.update(tender["id"], {
        "allowed_connection_ids": current["allowed_connection_ids"], "manager": route, "specialist": route,
        "run_budget_usd": None, "tender_budget_usd": None, "max_requests": 32,
        "engineer_confirmed": True, "rationale": "Subscription route without a USD allowance",
    })
    run = repo.create_run(tender["id"], "manager")
    meter = BudgetMeter(policy, tender["id"], run["id"], route)
    meter.connection = meter.connection | {"billing": "subscription"}
    assert await meter.before_request(1000, 1000)
