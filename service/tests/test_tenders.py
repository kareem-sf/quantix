import pytest
from conftest import TOKEN
from fastapi.testclient import TestClient

from quantix.api.app import create_app


def test_health_needs_no_token(tmp_path):
    with TestClient(create_app(tmp_path, TOKEN)) as c:
        assert c.get("/health").json() == {"status": "ok"}


@pytest.mark.parametrize("header", [None, "Bearer wrong", TOKEN])
def test_tenders_need_the_launch_token(tmp_path, header):
    headers = {"Authorization": header} if header else {}
    with TestClient(create_app(tmp_path, TOKEN)) as c:
        assert c.get("/tenders", headers=headers).status_code == 401


def test_create_list_and_get(client):
    first = client.post("/tenders", json={"name": "  Al Noor School  ", "due_date": "2026-10-14"})
    assert first.status_code == 201
    assert first.json()["name"] == "Al Noor School"
    assert first.json()["due_date"] == "2026-10-14"
    second = client.post("/tenders", json={"name": "Riyadh Warehouse"}).json()

    listed = client.get("/tenders").json()
    assert {t["name"] for t in listed} == {"Riyadh Warehouse", "Al Noor School"}
    created = [t["created_at"] for t in listed]
    assert created == sorted(created, reverse=True)
    assert client.get(f"/tenders/{second['id']}").json() == second


def test_rejects_a_blank_name(client):
    assert client.post("/tenders", json={"name": "   "}).status_code == 422


def test_unknown_tender(client):
    assert client.get("/tenders/nope").status_code == 404


def test_data_survives_a_restart(tmp_path):
    headers = {"Authorization": f"Bearer {TOKEN}"}
    with TestClient(create_app(tmp_path, TOKEN), headers=headers) as c:
        c.post("/tenders", json={"name": "Kept"})
    with TestClient(create_app(tmp_path, TOKEN), headers=headers) as c:
        assert [t["name"] for t in c.get("/tenders").json()] == ["Kept"]
    assert (tmp_path / "quantix.sqlite").exists()
