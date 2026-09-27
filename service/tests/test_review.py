"""The Tender Manager reviews everything his staff propose before it reaches the engineer."""

from decimal import Decimal

import pytest
from pydantic_ai.messages import ModelResponse, TextPart, UserPromptPart
from test_documents import PDF, make_xlsx, read_all, upload
from test_office import scripted, wait_for

from quantix import settings
from quantix.boq import records as boq
from quantix.office import records as office
from quantix.office import tools
from quantix.office.models import Staff
from quantix.review import records as reviews

QUOTES = (
    "A2=3.1 | B2=Excavation to reduce levels | C2=m3 | D2=1240",
    "A3=4.2 | B3=خرسانة مسلحة للبلاطة | C3=م3 | D3=312.4",
)


@pytest.fixture
def office_with_work(client):
    """A tender where Omar entered two BOQ lines and a fact, waiting for Rania, the Tender Manager."""
    tender_id = client.post("/tenders", json={"name": "Synthetic school"}).json()["id"]
    upload(client, tender_id, {"Bill.xlsx": make_xlsx(), "Conditions.pdf": PDF})
    documents = read_all(client, tender_id)
    bill, conditions = documents["Bill.xlsx"]["id"], documents["Conditions.pdf"]["id"]
    with client.app.state.sessions() as session:
        rania = office.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)
        omar = office.hire(session, tender_id, "Omar Haddad", "Quantity Surveyor", {})
        lines = [
            boq.ItemIn(item="3.1", description="Excavation", unit="m3", quantity=Decimal("1240"), document_id=bill,
                       page=1, quote=QUOTES[0]),
            boq.ItemIn(item="4.2", description="Slab", unit="م3", quantity=Decimal("312.4"), document_id=bill,
                       page=1, quote=QUOTES[1]),
        ]  # fmt: skip
        boq.propose_items(session, tender_id, omar, lines)
        boq.propose_fact(session, tender_id, omar, "method_of_measurement", "Re-measured", conditions, 1, "Tender")
        session.commit()
        return tender_id, rania.id, omar.id


def refs(client, tender_id) -> dict[str, str]:
    """The Manager's queue, as {line: ref}."""
    with client.app.state.sessions() as session:
        return {p.record.id: p.ref for p in reviews.pending(session, tender_id)}


def test_the_manager_leads_and_reviews_but_never_produces_records():
    names = {t.__name__ for t in tools.MANAGER}
    assert {"review_queue", "review_details", "review", "hire", "assign_task", "ask_engineer"} <= names
    produce = {t.__name__ for t in tools.PRODUCE}
    assert not names & produce  # every record has a producer and a different reviewer
    assert "propose_rate" in produce and "review" not in {t.__name__ for t in tools.STAFF}


def test_the_manager_accepts_or_sends_back_each_record(client, office_with_work):
    tender_id, rania_id, omar_id = office_with_work
    boq_page = client.get(f"/tenders/{tender_id}/boq").json()
    excavation, slab = boq_page["items"]
    [fact] = boq_page["facts"]
    queue = refs(client, tender_id)
    gates = client.get(f"/tenders/{tender_id}/gates").json()
    assert (gates["manager"], gates["boq"], gates["facts"]) == (3, 0, 0)  # nothing reaches the engineer unreviewed

    with client.app.state.sessions() as session:
        rania = session.get(Staff, rania_id)
        verdict = reviews.Verdict
        report = reviews.review(
            session,
            tender_id,
            rania,
            [
                verdict(record=queue[excavation["id"]], accept=True, note="Quantity matches Bill.xlsx row 2."),
                verdict(record=queue[slab["id"]], accept=False, note="Use the unit as printed, m3, not م3."),
                verdict(record=queue[fact["id"]], accept=True, note="ok"),  # a note that says nothing
                verdict(record="rate 12345678", accept=True, note="Checked against the quote."),
            ],
            autonomous=False,
        )
        session.commit()
    assert report.startswith("Accepted 1 (waiting for the engineer). Sent back 1.\nNot done: ")
    assert f"{queue[fact['id']]}: say in the note what you checked, or what to correct" in report
    assert "rate 12345678: nothing with that reference is waiting for your review" in report

    items = {i["item"]: i for i in client.get(f"/tenders/{tender_id}/boq").json()["items"]}
    assert (items["3.1"]["status"], items["3.1"]["reviewed_by"]) == ("reviewed", rania_id)
    assert "4.2" not in items  # sent back: Omar enters it again
    team = client.get(f"/tenders/{tender_id}/messages", params={"channel": "team"}).json()
    assert (team[-1]["sender"], team[-1]["text"]) == (
        rania_id,
        "Omar, I sent back BOQ item 4.2: Use the unit as printed, m3, not م3.",
    )
    gates = client.get(f"/tenders/{tender_id}/gates").json()
    assert (gates["manager"], gates["boq"]) == (1, 1)  # the fact still waits for him; the line waits for the engineer


def test_the_queue_says_who_did_what_and_shows_the_detail(client, office_with_work):
    tender_id, _, _ = office_with_work
    queue = client.get(f"/tenders/{tender_id}/review").json()
    assert [(w["kind"], w["line"].split(" · ", 2)[1]) for w in queue] == [
        ("boq", "by Omar"),
        ("boq", "by Omar"),
        ("fact", "by Omar"),
    ]
    assert queue[0]["line"].endswith("3.1: Excavation · 1,240 m3")
    with client.app.state.sessions() as session:
        assert reviews.counts(session, tender_id) == "Omar: 2 BOQ lines, 1 fact"
        found = reviews.find(session, tender_id, queue[0]["ref"])
        assert reviews.details(session, found) == f"From Bill.xlsx, page 1: “{QUOTES[0]}”"


def test_in_an_autonomous_office_his_acceptance_approves_and_the_engineer_can_reopen(client, office_with_work):
    tender_id, rania_id, _ = office_with_work
    [fact] = client.get(f"/tenders/{tender_id}/boq").json()["facts"]
    with client.app.state.sessions() as session:
        verdicts = [reviews.Verdict(record=refs(client, tender_id)[fact["id"]], accept=True, note="Read it on page 1.")]
        reviews.review(session, tender_id, session.get(Staff, rania_id), verdicts, autonomous=True)
        session.commit()
    [fact] = client.get(f"/tenders/{tender_id}/boq").json()["facts"]
    assert (fact["status"], fact["review_note"]) == ("office_approved", "Read it on page 1.")

    excavation = client.get(f"/tenders/{tender_id}/boq").json()["items"][0]
    refused = client.post(f"/records/boq/{excavation['id']}/reopen", json={"reason": "No."})
    assert refused.json()["detail"] == "Only approved work can be reopened; send back what is still waiting instead."
    assert client.post(f"/records/fact/{fact['id']}/reopen", json={"reason": "Check page 2."}).status_code == 204
    assert client.get(f"/tenders/{tender_id}/boq").json()["facts"] == []
    team = client.get(f"/tenders/{tender_id}/messages", params={"channel": "team"}).json()
    assert (team[-1]["sender"], team[-1]["text"]) == (
        "engineer",
        "Omar, I sent back the method of measurement: Check page 2.",
    )


def test_the_manager_wakes_for_new_work_in_his_queue(client, office_with_work, tmp_path):
    tender_id, _, _ = office_with_work
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})
    manager_prompts: list[str] = []

    def brain(messages, info):
        if "You are Rania Farouk" in info.instructions:
            prompt = next(str(p.content) for m in messages for p in m.parts if isinstance(p, UserPromptPart))
            if not manager_prompts or manager_prompts[-1] != prompt:
                manager_prompts.append(prompt)
        return ModelResponse(parts=[TextPart("Done.")])

    client.app.state.office.model = lambda: scripted(brain)
    client.app.state.office.engineer_spoke(tender_id)  # nothing was said to him: his queue alone wakes him
    wait_for(lambda o: manager_prompts, client, tender_id)
    assert (
        "Waiting for your review: Omar: 2 BOQ lines, 1 fact. Go through them with review_queue" in (manager_prompts[0])
    )
    client.app.state.office.engineer_spoke(tender_id)
    wait_for(lambda o: True, client, tender_id)
    assert len(manager_prompts) == 1  # the same queue doesn't wake him again
