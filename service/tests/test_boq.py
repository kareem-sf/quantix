from decimal import Decimal

import pytest
from pydantic_ai.messages import ModelResponse, TextPart, ToolCallPart, ToolReturnPart
from pydantic_ai.models.function import FunctionModel
from test_documents import PDF, make_xlsx, read_all, upload
from test_office import wait_for

from quantix import settings
from quantix.boq import records
from quantix.boq.records import ItemIn
from quantix.office import records as office


@pytest.fixture
def package(client):
    tender_id = client.post("/tenders", json={"name": "Synthetic school"}).json()["id"]
    upload(client, tender_id, {"Bill.xlsx": make_xlsx(), "Conditions.pdf": PDF})
    documents = read_all(client, tender_id)
    return tender_id, documents["Bill.xlsx"]["id"], documents["Conditions.pdf"]["id"]


@pytest.fixture
def qs(client, package):
    with client.app.state.sessions() as session:
        member = office.hire(session, package[0], "Omar Haddad", "Quantity Surveyor", {})
        session.commit()
        return member.id


def propose(client, tender_id, staff_id, lines, autonomous=False):
    with client.app.state.sessions() as session:
        from quantix.office.models import Staff

        report = records.propose_items(session, tender_id, session.get(Staff, staff_id), lines, autonomous)
        session.commit()
        return report


def line(bill, item="3.1", quantity="1240", quote="A2=3.1 | B2=Excavation to reduce levels | C2=m3 | D2=1240", **kw):
    fields = {"item": item, "description": "Excavation", "unit": "m3", "document_id": bill, "page": 1, "quote": quote}
    return ItemIn(**{**fields, "quantity": Decimal(quantity) if quantity else None, **kw})


def test_items_are_saved_only_when_the_page_backs_them(client, package, qs):
    tender_id, bill, _ = package
    arabic_quote = "A3=4.2 | B3=خرسانة مسلحة للبلاطة | C3=م3 | D3=312.4"  # quoted without the vowel marks
    report = propose(
        client,
        tender_id,
        qs,
        [
            line(bill),
            line(bill, item="4.2", quantity="312.4", quote=arabic_quote, unit="م3", description="خرسانة"),
            line(bill, item="9.9", quote="A9=9.9 | B9=Imaginary item"),
            line(bill, item="3.1", quantity="1300", section="Other"),
            line(bill),
        ],
    )
    assert report.startswith("Saved 2 BOQ items for the engineer's approval.")
    assert "item 9.9: “A9=9.9 | B9=Imaginary item” is not on Bill.xlsx, page 1" in report
    assert "item 3.1: the quantity 1300 is not in the quote" in report
    assert "item 3.1: it is already in the BOQ" in report

    boq = client.get(f"/tenders/{tender_id}/boq").json()
    assert [(i["item"], i["quantity"], i["status"]) for i in boq["items"]] == [
        ("3.1", "1240.0000", "proposed"),
        ("4.2", "312.4000", "proposed"),
    ]
    assert boq["items"][0]["source"] == {
        "document_id": bill,
        "document_name": "Bill.xlsx",
        "page": 1,
        "quote": "A2=3.1 | B2=Excavation to reduce levels | C2=m3 | D2=1240",
    }
    assert client.get(f"/tenders/{tender_id}/gates").json() == {"boq": 2, "facts": 0}


def test_a_quote_may_skip_words_with_an_ellipsis(client, package, qs):
    tender_id, bill, _ = package
    assert propose(client, tender_id, qs, [line(bill, quote="A2=3.1 | B2=Excavation ... D2=1240")]).startswith(
        "Saved 1"
    )


def test_approving_all_tells_the_manager_and_a_rejection_goes_back(client, package, qs):
    tender_id, bill, _ = package
    with client.app.state.sessions() as session:
        manager = office.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)
        session.commit()
        manager_id = manager.id
    propose(client, tender_id, qs, [line(bill), line(bill, item="4.2", quantity="312.4", quote="A3=4.2 ... D3=312.4")])
    [first, second] = client.get(f"/tenders/{tender_id}/boq").json()["items"]

    client.post(f"/boq/{second['id']}/decision", json={"approve": False, "reason": "Use the drawing quantity."})
    assert client.post(f"/tenders/{tender_id}/boq/approve-all").json() == {"approved": 1}

    items = client.get(f"/tenders/{tender_id}/boq").json()["items"]
    assert [(i["item"], i["status"]) for i in items] == [("3.1", "approved")]
    to_omar = client.get(f"/tenders/{tender_id}/messages", params={"channel": qs}).json()
    assert to_omar[-1]["text"] == "I rejected BOQ item 4.2: Use the drawing quantity."
    to_rania = client.get(f"/tenders/{tender_id}/messages", params={"channel": manager_id}).json()
    assert to_rania[-1]["text"] == "I approved 1 BOQ items."
    assert client.post(f"/boq/{first['id']}/decision", json={"approve": False}).status_code == 400


def test_facts_need_their_source_and_the_newest_approved_wins(client, package, qs):
    tender_id, _, conditions = package
    with client.app.state.sessions() as session:
        from quantix.office.models import Staff

        omar = session.get(Staff, qs)
        with pytest.raises(ValueError, match="is not on Conditions.pdf, page 1"):
            records.propose_fact(session, tender_id, omar, "currency", "SAR", conditions, 1, "Saudi Riyals", False)
        for value in ("One percent", "1% of the tender price"):
            records.propose_fact(
                session, tender_id, omar, "method_of_measurement", value, conditions, 1, "Tender security", False
            )
        session.commit()
    [fact] = client.get(f"/tenders/{tender_id}/boq").json()["facts"]
    assert (fact["label"], fact["value"], fact["status"]) == (
        "Method of measurement",
        "1% of the tender price",
        "proposed",
    )
    client.post(f"/facts/{fact['id']}/decision", json={"approve": True})
    assert client.get(f"/tenders/{tender_id}/boq").json()["facts"][0]["status"] == "approved"


def test_an_autonomous_office_approves_its_own_items(client, package, qs):
    tender_id, bill, _ = package
    propose(client, tender_id, qs, [line(bill)], autonomous=True)
    assert client.get(f"/tenders/{tender_id}/boq").json()["items"][0]["status"] == "office_approved"
    assert client.get(f"/tenders/{tender_id}/gates").json() == {"boq": 0, "facts": 0}


def test_staff_propose_items_through_their_tool(client, package, tmp_path):
    tender_id, bill, _ = package
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})

    def brain(messages, info):
        if info.output_tools:
            return ModelResponse(
                parts=[
                    ToolCallPart(
                        info.output_tools[0].name,
                        {
                            "name": "Rania Farouk",
                            "discipline": "Civil",
                            "experience_years": 19,
                            "background": "b",
                            "working_style": "w",
                            "opinions": "o",
                            "voice": "v",
                        },
                    )
                ]
            )
        done = [p for m in messages for p in m.parts if isinstance(p, ToolReturnPart)]
        if done:
            return ModelResponse(parts=[TextPart("Done.")])
        quote = "A2=3.1 | B2=Excavation to reduce levels | C2=m3 | D2=1240"
        item = {
            "item": "3.1",
            "description": "Excavation",
            "unit": "m3",
            "quantity": "1240",
            "document_id": bill,
            "page": 1,
            "quote": quote,
        }
        return ModelResponse(parts=[ToolCallPart("propose_boq_items", {"items": [item]})])

    client.app.state.office.model = lambda: FunctionModel(brain)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": "team", "text": "Enter the BOQ."})
    wait_for(lambda o: client.get(f"/tenders/{tender_id}/gates").json()["boq"] == 1, client, tender_id)
    [item] = client.get(f"/tenders/{tender_id}/boq").json()["items"]
    assert (item["item"], item["quantity"], item["unit"]) == ("3.1", "1240.0000", "m3")
