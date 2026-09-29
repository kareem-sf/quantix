"""The Desk and the tender register: every tender at a glance, archived ones kept, outcomes dated."""

from quantix.office import records as office


def test_the_desk_shows_every_tender_with_what_waits_for_the_engineer(client):
    school = client.post("/tenders", json={"name": "Al Noor School", "due_date": "2026-10-02"}).json()
    depot = client.post("/tenders", json={"name": "Riyadh Depot"}).json()
    with client.app.state.sessions() as session:
        salem = office.hire(session, school["id"], "Salem Al Suwaidi", "Tender Manager", {}, is_manager=True)
        office.ask(session, school["id"], salem, "Site support", "72 working days or 4 months?", ["72", "4 months"])
        session.commit()

    glances = {g["name"]: g for g in client.get("/desk").json()}
    assert glances.keys() == {"Al Noor School", "Riyadh Depot"}
    first = glances["Al Noor School"]
    assert (first["due_date"], first["outcome"], first["archived"]) == ("2026-10-02", "open", False)
    assert first["team"] == "idle"
    assert first["waiting"] == 1  # the question put to the engineer
    assert (first["documents"], first["items"], first["priced"], first["total"]) == (0, 0, 0, None)
    assert glances["Riyadh Depot"]["waiting"] == 0
    assert glances["Riyadh Depot"]["id"] == depot["id"]


def test_archiving_keeps_the_tender_and_an_outcome_is_dated(client):
    tender = client.post("/tenders", json={"name": "Jubail Housing"}).json()
    assert (tender["archived"], tender["outcome_at"]) == (False, None)

    lost = client.patch(f"/tenders/{tender['id']}", json={"outcome": "lost"}).json()
    assert lost["outcome"] == "lost" and lost["outcome_at"] is not None
    again = client.patch(f"/tenders/{tender['id']}", json={"outcome": "lost"}).json()
    assert again["outcome_at"] == lost["outcome_at"]  # the same outcome again keeps its date

    archived = client.patch(f"/tenders/{tender['id']}", json={"archived": True}).json()
    assert archived["archived"] is True and archived["outcome"] == "lost"
    assert [t["archived"] for t in client.get("/tenders").json()] == [True]  # still in the register
    assert client.get("/desk").json()[0]["archived"] is True

    restored = client.patch(f"/tenders/{tender['id']}", json={"archived": False}).json()
    assert restored["archived"] is False
