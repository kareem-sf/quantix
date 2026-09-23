from decimal import Decimal

from test_documents import read_all, upload
from test_estimate import bill

from quantix import company
from quantix.boq import records as boq
from quantix.estimate import records as estimate
from quantix.office import agents
from quantix.office import records as office


def priced_tender(client, name, rate, status):
    tender_id = client.post("/tenders", json={"name": name}).json()["id"]
    upload(client, tender_id, {"Bill.xlsx": bill()})
    docs = read_all(client, tender_id)
    with client.app.state.sessions() as session:
        omar = office.hire(session, tender_id, "Omar Haddad", "Estimator", {})
        line = boq.ItemIn(
            item="3.1",
            description="Excavation",
            unit="m3",
            quantity=Decimal("1240"),
            document_id=docs["Bill.xlsx"]["id"],
            page=1,
            quote="A1=3.1 | B1=Excavation | C1=m3 | D1=1240",
        )
        boq.propose_items(session, tender_id, omar, [line], False)
        boq.approve_all_items(session, tender_id)
        note = "Plant 4.5 m3/hr at 83.25 per hour."
        estimate.propose_rate(
            session, tender_id, omar.id, "3.1", "estimate", note, unit_rate=Decimal(rate), status=status
        )
        session.commit()
        return tender_id, omar.id


def test_every_team_reads_the_firms_rules(client):
    tender_id = client.post("/tenders", json={"name": "Synthetic school"}).json()["id"]
    rule = client.post("/rules", json={"topic": "Markups", "text": "Overheads 5%, profit 7% unless told otherwise."})
    client.post("/rules", json={"topic": "Exclusions", "text": "Always exclude dewatering below 2 m."})
    assert [r["topic"] for r in client.get("/rules").json()] == ["Exclusions", "Markups"]

    with client.app.state.sessions() as session:
        member = office.hire(session, tender_id, "Omar Haddad", "Estimator", {})
        brief = agents.situation(session, member, [])
    assert (
        "The firm's rules, which the whole office follows:\n"
        "- Exclusions: Always exclude dewatering below 2 m.\n"
        "- Markups: Overheads 5%, profit 7% unless told otherwise."
    ) in brief

    assert client.delete(f"/rules/{rule.json()['id']}").status_code == 204
    assert [r["topic"] for r in client.get("/rules").json()] == ["Exclusions"]


def test_the_engineer_records_how_a_tender_went(client):
    tender_id = client.post("/tenders", json={"name": "Synthetic school"}).json()["id"]
    assert client.get(f"/tenders/{tender_id}").json()["outcome"] == "open"
    assert client.patch(f"/tenders/{tender_id}", json={"outcome": "won"}).json()["outcome"] == "won"
    assert client.patch(f"/tenders/{tender_id}", json={"outcome": "maybe"}).status_code == 422


def test_past_tenders_offer_their_approved_rates_as_benchmarks(client):
    old, _ = priced_tender(client, "Riyadh school 2025", "18.50", "approved")
    client.patch(f"/tenders/{old}", json={"outcome": "won"})
    priced_tender(client, "Jeddah clinic", "22.00", "proposed")  # never approved, so not a benchmark
    current, _ = priced_tender(client, "Synthetic school", "19.00", "approved")

    with client.app.state.sessions() as session:
        found = company.past_rates(session, current, "excavation")
        assert [(p.tender, p.outcome, p.item, p.rate, p.unit, p.basis) for p in found] == [
            ("Riyadh school 2025", "won", "3.1", Decimal("18.50"), "m3", "estimate")
        ]
        assert company.past_rates(session, current, "blockwork") == []
