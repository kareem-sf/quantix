import importlib
import time

import pytest
from fastapi.testclient import TestClient
from openpyxl import Workbook


@pytest.fixture
def client(tmp_path, monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    monkeypatch.setattr("keyring.get_password", lambda *_: None)
    try:
        factory = importlib.import_module("quantix.api").create_app
    except ModuleNotFoundError:
        pytest.fail("The local API has not been implemented")
    app = factory(tmp_path / "home", "test-session-token")
    with TestClient(app) as test_client:
        test_client.headers["Authorization"] = "Bearer test-session-token"
        yield test_client


def test_local_api_requires_session_auth(client):
    assert client.get("/api/health", headers={"Authorization": ""}).status_code == 401
    assert client.get("/api/health").status_code == 200


def test_run_activity_routes_are_read_only_authenticated_and_tender_scoped(client):
    from types import SimpleNamespace

    from quantix.run_activity import ActivityRecorder
    repo = client.app.state.repo
    tender = repo.create_tender("Synthetic activity API")
    other = repo.create_tender("Other synthetic activity API")
    run = repo.create_run(tender["id"], "manager")
    recorder = ActivityRecorder(SimpleNamespace(repo=repo, tender_id=tender["id"], run_id=run["id"]))
    recorder.start("tool", "Reading synthetic quantities", {"inputs": {"source_id": "synthetic"}})
    path = f"/api/tenders/{tender['id']}/runs/{run['id']}/activity"
    before = repo.run_events(run["id"])
    assert client.get(path, headers={"Authorization": ""}).status_code == 401
    page = client.get(path)
    assert page.status_code == 200
    identifier = page.json()["items"][0]["event_id"]
    assert client.get(f"{path}/{identifier}").json()["total_chars"] > 0
    assert client.get(path.replace(tender["id"], other["id"])).status_code == 404
    assert client.get(f"{path}/{identifier}?limit=64001").status_code == 422
    assert client.get(path, params={"after": page.json()["cursor"]}).json()["items"] == []
    assert repo.run_events(run["id"]) == before
    assert repo.get_run(run["id"])["status"] == "queued"


def test_latest_restore_notice_has_a_typed_read_route(client):
    response = client.get("/api/backups/latest")
    assert response.status_code == 200
    assert response.json() is None


@pytest.mark.parametrize("origin", ["https://untrusted.invalid", "null", ""])
def test_valid_bearer_cannot_mutate_from_an_explicit_unapproved_origin(client, origin):
    response = client.post(
        "/api/tenders", json={"name": "Must not be created"}, headers={"Origin": origin}
    )
    assert response.status_code == 403
    assert client.get("/api/tenders").json() == []


def test_approved_origin_and_no_origin_clients_can_mutate(client):
    assert (
        client.post(
            "/api/tenders", json={"name": "Desktop"}, headers={"Origin": "http://tauri.localhost"}
        ).status_code
        == 200
    )
    assert client.post("/api/tenders", json={"name": "Local CLI"}).status_code == 200
    assert len(client.get("/api/tenders").json()) == 2


def test_unapproved_origin_preflight_is_rejected_before_routing(client):
    response = client.options(
        "/api/tenders",
        headers={"Origin": "https://untrusted.invalid", "Access-Control-Request-Method": "POST"},
    )
    assert response.status_code == 403


def test_knowledge_routes_are_registered_behind_local_auth(client):
    assert "knowledge" in client.get("/api/health").json()["capabilities"]
    assert client.get("/api/knowledge").json() == []
    assert client.get("/api/knowledge", headers={"Authorization": ""}).status_code == 401


@pytest.mark.parametrize("capability", ["measurements", "submissions"])
def test_reviewed_work_routes_are_registered_behind_local_auth(client, capability):
    assert capability in client.get("/api/health").json()["capabilities"]
    tender = client.post("/api/tenders", json={"name": "Reviewed work"}).json()
    url = f"/api/tenders/{tender['id']}/{capability}"
    response = client.get(url)
    assert response.status_code == 200
    assert response.json() == []
    assert client.get(url, headers={"Authorization": ""}).status_code == 401


def test_unconfigured_manager_retains_an_honest_empty_conversation(client):
    tender = client.post("/api/tenders", json={"name": "Foundations"}).json()
    result = client.post(
        f"/api/tenders/{tender['id']}/messages", json={"content": "Analyse the package"}
    )
    assert result.status_code == 409
    assert "Settings" in result.json()["detail"]
    assert client.get(f"/api/tenders/{tender['id']}/messages").json() == []


def test_real_import_and_search_through_http(client, tmp_path):
    package = tmp_path / "package"
    package.mkdir()
    workbook = Workbook()
    workbook.active.append(["Item", "Description", "Unit", "Quantity"])
    workbook.active.append(["C1", "Reinforced concrete slab", "m3", 28])
    workbook.save(package / "Civil.xlsx")
    workbook.close()
    tender = client.post("/api/tenders", json={"name": "Foundations"}).json()
    response = client.post(
        f"/api/tenders/{tender['id']}/imports", json={"source_path": str(package)}
    )
    assert response.status_code == 200
    run = response.json()
    deadline = time.monotonic() + 15
    while run["status"] in {"queued", "running"} and time.monotonic() < deadline:
        time.sleep(0.03)
        run = client.get(f"/api/runs/{run['id']}").json()
    assert run["status"] == "completed", run
    overview = client.get(f"/api/tenders/{tender['id']}").json()
    assert overview["artifact_count"] == 1
    hits = client.get(f"/api/tenders/{tender['id']}/search", params={"q": "reinforced"}).json()
    assert hits["actual_mode"] in {"words", "combined"}
    assert hits["hits"][0]["sheet"] == "Sheet"
    source = client.get(f"/api/tenders/{tender['id']}/evidence/{hits['hits'][0]['id']}").json()
    assert "28" in source["text"]
    messages = client.get(f"/api/tenders/{tender['id']}/messages").json()
    assert messages[0]["role"] == "system"
    assert "not a completed engineering review" in messages[0]["content"]


def test_settings_never_return_provider_secret(client, monkeypatch):
    secret = "sk-test-private-value"
    monkeypatch.setattr("keyring.set_password", lambda *_: pytest.fail("Use synthetic session credentials"))
    account = client.post("/api/ai/connections", json={
        "name": "Synthetic OpenAI", "provider_id": "openai", "protocol": "openai_responses",
        "credentials": {"api_key": secret}, "session_only": True,
    })
    assert account.status_code == 200
    assert account.json()["credential_state"] == "session"
    assert secret not in account.text
    for endpoint in ("/api/ai/connections", "/api/settings", "/api/health", "/api/reset/preview"):
        response = client.get(endpoint)
        assert response.status_code == 200
        assert secret not in response.text
    # Office preferences no longer accept credential fields; even rejected
    # write-only input must stay out of the public validation response.
    rejected = client.patch(
        "/api/settings", json={"api_key": "sk-test-private-value", "default_currency": "EGP"}
    )
    assert rejected.status_code == 422
    assert secret not in rejected.text
    response = client.patch("/api/settings", json={"default_currency": "EGP"})
    assert response.status_code == 200
    assert response.json()["default_currency"] == "EGP"
    assert secret not in response.text


def test_original_download_preserves_bytes_and_scopes_historic_sources(client):
    import hashlib

    repo = client.app.state.repo
    tid = repo.create_tender("Preserved CAD")["id"]
    original = b"Synthetic CAD bytes"
    digest = hashlib.sha256(original).hexdigest()
    (repo.objects / digest).write_bytes(original)
    artifact, _ = repo.register_artifact(
        tid,
        "Drawings/source.dwg",
        digest,
        len(original),
        {
            "kind": "cad",
            "status": "unsupported",
            "segments": [],
        },
    )
    endpoint = f"/api/tenders/{tid}/artifacts/{artifact['id']}"
    assert client.get(endpoint).json()["content_hash"] == digest
    response = client.get(endpoint + "/original")
    assert response.status_code == 200
    assert response.content == original
    assert "attachment" in response.headers["content-disposition"]
    assert client.get(endpoint + "/original", headers={"Authorization": ""}).status_code == 401
    foreign = repo.create_tender("Other")["id"]
    assert client.get(endpoint.replace(tid, foreign) + "/original").status_code == 404
    repo.register_artifact(
        tid,
        "Drawings/source.dwg",
        "f" * 64,
        9,
        {
            "kind": "cad",
            "status": "unsupported",
            "segments": [],
        },
    )
    assert client.get(endpoint).json()["is_current"] is False
    assert client.get(endpoint + "/original").content == original
    (repo.objects / digest).write_bytes(b"Changed bytes")
    assert client.get(endpoint + "/original").status_code == 409
