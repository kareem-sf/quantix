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


def test_the_due_date_changes_on_its_own(client):
    tender = client.post("/tenders", json={"name": "Al Noor School"}).json()
    changed = client.patch(f"/tenders/{tender['id']}", json={"due_date": "2026-09-30"}).json()
    assert (changed["due_date"], changed["outcome"]) == ("2026-09-30", "open")
    kept = client.patch(f"/tenders/{tender['id']}", json={"outcome": "submitted"}).json()
    assert (kept["due_date"], kept["outcome"]) == ("2026-09-30", "submitted")  # a change to the outcome keeps the date
    assert client.patch(f"/tenders/{tender['id']}", json={"due_date": None}).json()["due_date"] is None


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


def test_deleting_a_tender_removes_everything_quantix_keeps_for_it(client, tmp_path):
    from test_documents import PDF, read_all, upload

    tender_id = client.post("/tenders", json={"name": "Old test"}).json()["id"]
    upload(client, tender_id, {"Conditions.pdf": PDF})
    read_all(client, tender_id)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": "team", "text": "Hello."})
    assert (tmp_path / "tenders" / tender_id).exists()

    assert client.delete(f"/tenders/{tender_id}").status_code == 204
    assert client.get(f"/tenders/{tender_id}").status_code == 404
    assert not (tmp_path / "tenders" / tender_id).exists()
    with client.app.state.sessions() as session:
        from sqlalchemy import text

        for table in ("documents", "pages", "messages"):
            assert session.execute(text(f"select count(*) from {table}")).scalar() == 0
    assert client.delete(f"/tenders/{tender_id}").status_code == 404


def test_the_newest_migration_keeps_every_tender_and_its_records(tmp_path):
    """The engineer's database already holds tenders when Quantix upgrades it, with foreign keys enforced."""
    from alembic import command
    from alembic.config import Config
    from alembic.script import ScriptDirectory
    from sqlalchemy import create_engine, event, text

    from quantix.core import db

    engine = create_engine(f"sqlite:///{tmp_path / 'quantix.sqlite'}")
    event.listen(engine, "connect", db._sqlite_pragmas)
    config = Config()
    config.set_main_option("script_location", str(db.MIGRATIONS))
    with engine.begin() as connection:
        config.attributes["connection"] = connection
        command.upgrade(config, ScriptDirectory.from_config(config).get_revision("head").down_revision)
    with engine.begin() as connection:
        connection.execute(
            text(
                "INSERT INTO tenders (id, owner_id, name, outcome, created_at) "
                "VALUES ('t1', 'local', 'Substation', 'open', '2026-09-27 10:00:00')"
            )
        )
        connection.execute(
            text(
                "INSERT INTO messages (tender_id, sender, channel, kind, text, created_at) "
                "VALUES ('t1', 'engineer', 'team', 'message', 'Go.', '2026-09-27 10:00:00')"
            )
        )
    db._upgrade(engine)
    with engine.connect() as connection:
        assert connection.execute(text("SELECT name FROM tenders")).all() == [("Substation",)]
        assert connection.execute(text("SELECT text FROM messages")).all() == [("Go.",)]
