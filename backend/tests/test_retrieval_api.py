"""HTTP search returns the shared retrieval envelope."""

import hashlib
import importlib

import pytest
from fastapi.testclient import TestClient


@pytest.fixture
def client(tmp_path, monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    monkeypatch.setattr("keyring.get_password", lambda *_: None)
    app = importlib.import_module("quantix.api").create_app(tmp_path / "home", "test-session-token")
    with TestClient(app) as test_client:
        test_client.headers["Authorization"] = "Bearer test-session-token"
        yield test_client


def _add(repo, tender_id, path, text, *, kind="pdf"):
    body = text.encode()
    artifact, _ = repo.register_artifact(
        tender_id,
        path,
        hashlib.sha256(body).hexdigest(),
        len(body),
        {
            "kind": kind,
            "status": "extracted",
            "segments": [{"locator": "page:1", "text": text, "page": 1}],
        },
    )
    return artifact


def test_default_auto_uses_words_when_meaning_is_not_ready(client):
    repo = client.app.state.repo
    tender = repo.create_tender("Auto")
    _add(repo, tender["id"], "Spec/a.pdf", "Reinforced concrete slab 28 m3.")
    payload = client.get(f"/api/tenders/{tender['id']}/search", params={"q": "reinforced"}).json()
    assert payload["requested_mode"] == "auto"
    assert payload["actual_mode"] == "words"
    assert payload["hits"][0]["text"].startswith("Reinforced")
    assert payload["coverage"]["meaning_status"] != "ready"


def test_strict_meaning_and_combined_still_refuse_unavailable_indexes(client):
    repo = client.app.state.repo
    tender = repo.create_tender("Strict")
    _add(repo, tender["id"], "Spec/a.pdf", "Reinforced concrete slab.")
    meaning = client.get(
        f"/api/tenders/{tender['id']}/search", params={"q": "reinforced", "mode": "meaning"}
    )
    combined = client.get(
        f"/api/tenders/{tender['id']}/search", params={"q": "reinforced", "mode": "combined"}
    )
    assert meaning.status_code == 409
    assert combined.status_code == 409


def test_document_kind_query_reaches_the_server(client):
    repo = client.app.state.repo
    tender = repo.create_tender("Kind")
    _add(repo, tender["id"], "Drawings/a.pdf", "Concrete grade C30/37", kind="pdf")
    sheet = _add(repo, tender["id"], "BOQ/bill.xlsx", "Concrete grade C30/37 quantity", kind="xlsx")
    payload = client.get(
        f"/api/tenders/{tender['id']}/search",
        params={"q": "C30/37", "mode": "words", "document_kind": "xlsx", "limit": 3},
    ).json()
    assert [hit["artifact_id"] for hit in payload["hits"]] == [sheet["id"]]
    assert payload["ranking_version"]
