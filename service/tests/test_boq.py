from decimal import Decimal

import pytest
from pydantic_ai.messages import ModelResponse, TextPart, ToolCallPart, ToolReturnPart
from test_documents import PDF, make_xlsx, read_all, upload
from test_office import manager_accepts, scripted, wait_for

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


def propose(client, tender_id, staff_id, lines):
    with client.app.state.sessions() as session:
        from quantix.office.models import Staff

        report = records.propose_items(session, tender_id, session.get(Staff, staff_id), lines)
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
    assert report.startswith("Saved 2 BOQ items for the Tender Manager's review.")
    assert "item 9.9: “A9=9.9 | B9=Imaginary item” is not on Bill.xlsx, page 1" in report
    assert "item 3.1: the quantity 1300 is not in the quote" in report
    assert "item 3.1: that line of the client's BOQ is already in" in report

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
    assert client.get(f"/tenders/{tender_id}/gates").json() == {
        "manager": 2,  # with the Tender Manager for review first
        "boq": 0,
        "facts": 0,
        "takeoff": 0,
        "drawings": 0,
        "pricing": 0,
        "subcontract": 0,
        "submission": 0,
    }


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
    assert client.post(f"/tenders/{tender_id}/boq/approve-all").json() == {"approved": 0}  # nothing reviewed yet
    assert manager_accepts(client, tender_id) == "Accepted 2 (waiting for the engineer). Sent back 0."
    [first, second] = client.get(f"/tenders/{tender_id}/boq").json()["items"]
    assert (first["status"], first["reviewed_by"], first["review_note"]) == (
        "reviewed",
        manager_id,
        "Checked it against its source.",
    )

    client.post(f"/boq/{second['id']}/decision", json={"approve": False, "reason": "Use the drawing quantity."})
    assert client.post(f"/tenders/{tender_id}/boq/approve-all").json() == {"approved": 1}

    items = client.get(f"/tenders/{tender_id}/boq").json()["items"]
    assert [(i["item"], i["status"]) for i in items] == [("3.1", "approved")]
    team = client.get(f"/tenders/{tender_id}/messages", params={"channel": "team"}).json()
    # in the team room, naming Omar: he redoes it, and the Manager sees what came back
    assert (team[-1]["sender"], team[-1]["text"]) == (
        "engineer",
        "Omar, I sent back BOQ item 4.2: Use the drawing quantity.",
    )
    to_rania = client.get(f"/tenders/{tender_id}/messages", params={"channel": manager_id}).json()
    assert to_rania[-1]["text"] == "I approved 1 BOQ items."
    assert client.post(f"/boq/{first['id']}/decision", json={"approve": False}).status_code == 400


def test_facts_need_their_source_and_the_newest_approved_wins(client, package, qs):
    tender_id, _, conditions = package
    with client.app.state.sessions() as session:
        from quantix.office.models import Staff

        omar = session.get(Staff, qs)
        with pytest.raises(ValueError, match="is not on Conditions.pdf, page 1"):
            records.propose_fact(session, tender_id, omar, "currency", "SAR", conditions, 1, "Saudi Riyals")
        for value in ("One percent", "1% of the tender price"):
            records.propose_fact(
                session, tender_id, omar, "method_of_measurement", value, conditions, 1, "Tender security"
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
    with client.app.state.sessions() as session, pytest.raises(ValueError, match="The engineer approved the method"):
        records.propose_fact(
            session, tender_id, session.get(Staff, qs), "method_of_measurement", "1%", conditions, 1, "Tender"
        )


def test_in_an_autonomous_office_the_managers_review_approves(client, package, qs):
    tender_id, bill, _ = package
    propose(client, tender_id, qs, [line(bill)])
    assert client.get(f"/tenders/{tender_id}/boq").json()["items"][0]["status"] == "proposed"  # nobody self-approves
    manager_accepts(client, tender_id, autonomous=True)
    assert client.get(f"/tenders/{tender_id}/boq").json()["items"][0]["status"] == "office_approved"
    assert client.get(f"/tenders/{tender_id}/gates").json() == {
        "manager": 0,
        "boq": 0,
        "facts": 0,
        "takeoff": 0,
        "drawings": 0,
        "pricing": 0,
        "subcontract": 0,
        "submission": 0,
    }


def test_staff_propose_items_through_their_tool(client, package, qs, tmp_path):
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
                            "background": "Priced civil works for schools and clinics.",
                            "working_style": "Checks every figure twice.",
                            "opinions": "Distrusts quantities nobody has measured.",
                            "voice": "Short and direct.",
                        },
                    )
                ]
            )
        done = [p for m in messages for p in m.parts if isinstance(p, ToolReturnPart)]
        if len(done) > 1 or "You are Omar Haddad" not in info.instructions:  # the Manager doesn't enter BOQ lines
            return ModelResponse(parts=[TextPart("Done.")])
        if not done:  # he enters only what he has read
            return ModelResponse(parts=[ToolCallPart("read_page", {"document_id": bill, "page": 1})])
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

    client.app.state.office.model = lambda: scripted(brain)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": qs, "text": "Omar, enter the BOQ."})
    wait_for(lambda o: client.get(f"/tenders/{tender_id}/gates").json()["manager"] == 1, client, tender_id)
    [item] = client.get(f"/tenders/{tender_id}/boq").json()["items"]
    assert (item["item"], item["quantity"], item["unit"]) == ("3.1", "1240.0000", "m3")


def test_bills_that_reuse_item_numbers_are_told_apart_by_section(client, package, qs):
    tender_id, bill, _ = package
    upload(client, tender_id, {"Bill 8486.xlsx": make_xlsx()})  # a second substation's bill, same item numbers
    bill_8486 = read_all(client, tender_id)["Bill 8486.xlsx"]["id"]
    report = propose(
        client, tender_id, qs, [line(bill, section="8485 · Earthwork"), line(bill_8486, section="8485 · Earthwork")]
    )
    assert "start the section with that bill's name" in report
    propose(client, tender_id, qs, [line(bill_8486, section="8486 · Earthwork")])
    with client.app.state.sessions() as session:
        with pytest.raises(
            ValueError, match="Item 3.1 is in more than one bill: “8485 · Earthwork”, “8486 · Earthwork”"
        ):
            records.find_item(session, tender_id, "3.1")
        assert records.find_item(session, tender_id, "8486 · earthwork / 3.1").section == "8486 · Earthwork"
        with pytest.raises(ValueError) as wrong_bill:
            records.find_item(session, tender_id, "8487 / 3.1")
        assert str(wrong_bill.value) == (  # says what the number is, so the next try is right
            "There is no BOQ item 8487 / 3.1. That number is “8485 · Earthwork / 3.1” or “8486 · Earthwork / 3.1”."
        )
        second = records.find_item(session, tender_id, "8486 · Earthwork / 3.1")
        records.decide(session, second, False, "Wrong unit.")
        session.commit()
    team = client.get(f"/tenders/{tender_id}/messages", params={"channel": "team"}).json()
    assert team[-1]["text"] == "Omar, I sent back BOQ item 8486 · Earthwork / 3.1: Wrong unit."  # which bill


def test_a_line_the_client_left_unnumbered_is_named_by_its_row(client, package, qs):
    tender_id, bill, _ = package
    quote = "B2=Excavation to reduce levels | C2=m3 | D2=1240"
    assert propose(client, tender_id, qs, [line(bill, item="", section="8486 · Earthwork", quote=quote)]).startswith(
        "Saved 1"
    )
    with client.app.state.sessions() as session:
        found = records.find_item(session, tender_id, "8486 · Earthwork / row 2")
        assert records.reference(found) == "8486 · Earthwork / row 2"
        with pytest.raises(ValueError, match="There is no BOQ item 8486 · Earthwork / row 3"):
            records.find_item(session, tender_id, "8486 · Earthwork / row 3")


def test_staff_withdraw_their_own_undecided_lines(client, package, qs):
    import threading
    from types import SimpleNamespace

    from quantix.office import tools

    def turn_of(staff_id):
        state = client.app.state
        return SimpleNamespace(
            deps=tools.Turn(state.home, state.sessions, tender_id, staff_id, False, threading.Event()),
            tool_call_id="call",
        )

    tender_id, bill, _ = package
    propose(client, tender_id, qs, [line(bill, section="Earthwork")])
    with client.app.state.sessions() as session:
        someone = office.hire(session, tender_id, "Layla Nasser", "Estimator", {})
        session.commit()
        someone_id = someone.id
    assert "Not withdrawn: Earthwork / 3.1: only your own work" in tools.withdraw(
        turn_of(someone_id), ["Earthwork / 3.1"], "Wrong bill."
    )
    assert tools.withdraw(turn_of(qs), ["Earthwork / 3.1"], "Wrong bill.") == "Withdrew 1."
    assert client.get(f"/tenders/{tender_id}/boq").json()["items"] == []
    assert client.get(f"/tenders/{tender_id}/gates").json()["boq"] == 0
    assert propose(client, tender_id, qs, [line(bill, section="8485 · Earthwork")]).startswith("Saved 1")
