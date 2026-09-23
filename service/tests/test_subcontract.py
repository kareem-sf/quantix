from decimal import Decimal

import pytest
from pydantic_ai.messages import ModelResponse, TextPart, ToolCallPart, ToolReturnPart
from pydantic_ai.models.function import FunctionModel
from test_documents import make_pdf, read_all, upload
from test_estimate import bill
from test_office import wait_for

from quantix import settings
from quantix.boq import records as boq
from quantix.estimate import records as estimate
from quantix.office import records as office
from quantix.subcontract import records as subcontract

GULF = make_pdf(
    [
        [
            "Gulf Groundworks quotation",
            "3.1 Excavation 17.00 SAR per m3",
            "6.3 Waterproofing 35.00 SAR per m2",
            "Excludes dewatering",
        ]
    ]
)
NAJD = make_pdf([["Najd Contracting offer", "Item 3.1 excavation rate 16.50 SAR"]])


@pytest.fixture
def tender(client):
    tender_id = client.post("/tenders", json={"name": "Synthetic school"}).json()["id"]
    upload(client, tender_id, {"Bill.xlsx": bill(), "Gulf.pdf": GULF, "Najd.pdf": NAJD})
    docs = read_all(client, tender_id)
    with client.app.state.sessions() as session:
        manager = office.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)
        omar = office.hire(session, tender_id, "Omar Haddad", "Commercial", {})
        rows = [("3.1", "Excavation", "m3", "1240"), ("4.3", "Slab reinforcement", "t", "28.1")]
        rows.append(("6.3", "Waterproofing", "m2", "980"))
        lines = [
            boq.ItemIn(
                item=i,
                description=d,
                unit=u,
                quantity=Decimal(q),
                document_id=docs["Bill.xlsx"]["id"],
                page=1,
                quote=f"A{n}={i} | B{n}={d} | C{n}={u} | D{n}={q}",
            )
            for n, (i, d, u, q) in enumerate(rows, start=1)
        ]
        boq.propose_items(session, tender_id, omar, lines, False)
        boq.approve_all_items(session, tender_id)
        for item, rate in (("3.1", "18.50"), ("6.3", "38.00")):
            note = "Our own rate from outputs and current prices."
            estimate.propose_rate(session, tender_id, omar.id, item, "estimate", note, unit_rate=Decimal(rate))
        session.commit()
        return tender_id, manager.id, omar.id, docs["Gulf.pdf"]["id"], docs["Najd.pdf"]["id"]


def quoted(item, rate, quote):
    return subcontract.QuoteLine(boq_item=item, rate=Decimal(rate), page=1, quote=quote)


def two_quotes(client, tender):
    tender_id, _, omar, gulf, najd = tender
    with client.app.state.sessions() as session:
        gulf_co = subcontract.add_company(
            session, omar, "Gulf Groundworks", "subcontractor", "Earthworks, waterproofing"
        )
        najd_co = subcontract.add_company(session, omar, "Najd Contracting", "subcontractor", "Earthworks")
        package = subcontract.create_package(session, tender_id, omar, "Groundworks", "subcontract", ["3.1", "6.3"])
        subcontract.record_quote(
            session,
            package,
            gulf_co,
            omar,
            gulf,
            [quoted("3.1", "17.00", "3.1 Excavation 17.00"), quoted("6.3", "35.00", "6.3 Waterproofing 35.00")],
            [
                subcontract.Exclusion(
                    description="Dewatering", amount=Decimal("5000"), page=1, quote="Excludes dewatering"
                )
            ],
        )
        subcontract.record_quote(
            session, package, najd_co, omar, najd, [quoted("3.1", "16.50", "Item 3.1 excavation rate 16.50")], []
        )
        session.commit()
        return package.id


def test_quotes_are_levelled_with_our_rates_and_their_exclusions(client, tender):
    tender_id = tender[0]
    two_quotes(client, tender)
    [package] = client.get(f"/tenders/{tender_id}/packages").json()
    assert [(i["item"], i["our_rate"]) for i in package["items"]] == [("3.1", "18.50"), ("6.3", "38.00")]
    gulf, najd = package["quotes"]
    ids = [i["id"] for i in package["items"]]

    # Gulf: 1240 × 17.00 = 21,080.00 and 980 × 35.00 = 34,300.00; dewatering 5,000 added back.
    assert [gulf["cells"][i]["amount"] for i in ids] == ["21080.00", "34300.00"]
    assert (gulf["quoted_total"], gulf["exclusions_total"], gulf["levelled_total"]) == ("55380.00", "5000", "60380.00")
    # Najd left out waterproofing: our 38.00 × 980 = 37,240.00 fills the gap. 20,460.00 + 37,240.00.
    assert najd["cells"][ids[1]] == {
        "rate": "38.00",
        "amount": "37240.00",
        "plugged": True,
        "page": None,
        "quote": None,
    }
    assert (najd["quoted_total"], najd["levelled_total"]) == ("20460.00", "57700.00")
    assert (najd["rank"], gulf["rank"]) == (1, 2)


def test_a_quote_must_be_read_from_its_document(client, tender):
    tender_id, _, omar, gulf, _ = tender
    package_id = two_quotes(client, tender)
    with client.app.state.sessions() as session:
        package = session.get(subcontract.Package, package_id)
        company = subcontract.find_company(session, "gulf groundworks")
        with pytest.raises(ValueError, match="The rate 16.00 for item 3.1 is not in the quoted line"):
            subcontract.record_quote(
                session, package, company, omar, gulf, [quoted("3.1", "16.00", "3.1 Excavation 17.00")], []
            )
        with pytest.raises(ValueError, match="Item 4.3 is not in the Groundworks package"):
            subcontract.record_quote(
                session, package, company, omar, gulf, [quoted("4.3", "17.00", "3.1 Excavation 17.00")], []
            )
        with pytest.raises(ValueError):
            subcontract.record_quote(
                session, package, company, omar, gulf, [quoted("3.1", "14.00", "Excavation 14.00")], []
            )
        with pytest.raises(ValueError, match="Mystery Co is not in the directory"):
            subcontract.find_company(session, "Mystery Co")
        with pytest.raises(ValueError, match="There is no BOQ item 9.9"):
            subcontract.create_package(session, tender_id, omar, "Other", "supply", ["9.9"])
        # A revised quote from the same company replaces the earlier one.
        subcontract.record_quote(
            session, package, company, omar, gulf, [quoted("3.1", "17.00", "3.1 Excavation 17.00")], []
        )
        session.commit()
    assert [len(q["exclusions"]) for q in client.get(f"/tenders/{tender_id}/packages").json()[0]["quotes"]] == [0, 0]


def test_the_engineer_chooses_and_the_quoted_rates_enter_the_estimate(client, tender):
    tender_id, manager, _, _, _ = tender
    package_id = two_quotes(client, tender)
    with client.app.state.sessions() as session:
        package = session.get(subcontract.Package, package_id)
        subcontract.recommend(
            session, package, tender[2], subcontract.find_company(session, "Najd Contracting"), "Cheapest levelled."
        )
        session.commit()
    assert client.get(f"/tenders/{tender_id}/gates").json()["subcontract"] == 1

    [package] = client.get(f"/tenders/{tender_id}/packages").json()
    gulf = package["quotes"][0]["id"]
    assert client.post(f"/packages/{package_id}/choice", json={"quote_id": gulf}).status_code == 200
    assert client.post(f"/packages/{package_id}/choice", json={"quote_id": gulf}).json()["detail"] == (
        "A quote has already been chosen for this package."
    )
    assert client.get(f"/tenders/{tender_id}/gates").json()["subcontract"] == 0
    [package] = client.get(f"/tenders/{tender_id}/packages").json()
    assert [i["our_rate"] for i in package["items"]] == ["18.50", "38.00"]  # still ours, not the chosen quote's
    assert [q["levelled_total"] for q in package["quotes"]] == ["60380.00", "57700.00"]

    rows = {i["item"]: i for i in client.get(f"/tenders/{tender_id}/estimate").json()["items"]}
    assert (rows["3.1"]["rate"]["basis"], rows["3.1"]["rate"]["status"], rows["3.1"]["amount"]) == (
        "quote",
        "approved",
        "21080.00",
    )
    assert (rows["6.3"]["rate"]["source_document"], rows["6.3"]["rate"]["note"]) == (
        "Gulf.pdf",
        "Subcontract: Gulf Groundworks",
    )
    chat = client.get(f"/tenders/{tender_id}/messages", params={"channel": manager}).json()
    assert chat[-1]["text"] == (
        "I chose Gulf Groundworks for Groundworks. Their rates are now in the estimate. "
        "Their exclusions still need covering: Dewatering"
    )


def test_the_directory_and_enquiries(client, tender):
    tender_id = tender[0]
    two_quotes(client, tender)
    added = client.post(
        "/directory",
        json={"name": "Red Sea Membranes", "kind": "supplier", "trades": "Waterproofing", "email": "q@rsm.sa"},
    )
    assert added.status_code == 201
    assert (
        client.post("/directory", json={"name": "red sea membranes", "kind": "supplier", "trades": "x"}).json()[
            "detail"
        ]
        == "red sea membranes is already in the directory."
    )
    assert [c["name"] for c in client.get("/directory", params={"q": "waterproofing"}).json()] == [
        "Gulf Groundworks",
        "Red Sea Membranes",
    ]
    gulf = client.get("/directory", params={"q": "gulf"}).json()[0]["id"]
    assert client.delete(f"/directory/{gulf}").status_code == 400
    assert client.delete(f"/directory/{added.json()['id']}").status_code == 204

    with client.app.state.sessions() as session:
        package = subcontract.find_package(session, tender_id, "groundworks")
        company = subcontract.find_company(session, "Najd Contracting")
        subcontract.draft_enquiry(
            session, package, company, tender[2], "Groundworks enquiry", "Please price 3.1 and 6.3."
        )
        session.commit()
    [enquiry] = client.get(f"/tenders/{tender_id}/packages").json()[0]["enquiries"]
    assert (enquiry["company"], enquiry["status"]) == ("Najd Contracting", "draft")
    client.post(f"/enquiries/{enquiry['id']}/sent")
    assert client.get(f"/tenders/{tender_id}/packages").json()[0]["enquiries"][0]["status"] == "sent"


def test_staff_level_quotes_through_their_tools_and_an_autonomous_office_chooses(client, tender, tmp_path):
    tender_id, _, omar, gulf, najd = tender
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"}, office_mode="autonomous")
    replies: list[str] = []
    steps = [
        ToolCallPart("add_company", {"name": "Gulf Groundworks", "kind": "subcontractor", "trades": "Earthworks"}),
        ToolCallPart("add_company", {"name": "Najd Contracting", "kind": "subcontractor", "trades": "Earthworks"}),
        ToolCallPart("create_package", {"name": "Groundworks", "kind": "subcontract", "boq_items": ["3.1", "6.3"]}),
        ToolCallPart(
            "draft_enquiry",
            {"package": "Groundworks", "company": "Najd Contracting", "subject": "Enquiry", "body": "Please price."},
        ),
        ToolCallPart(
            "record_quote",
            {
                "package": "Groundworks",
                "company": "Gulf Groundworks",
                "document_id": gulf,
                "lines": [
                    {"boq_item": "3.1", "rate": "17.00", "page": 1, "quote": "3.1 Excavation 17.00"},
                    {"boq_item": "6.3", "rate": "35.00", "page": 1, "quote": "6.3 Waterproofing 35.00"},
                ],
                "exclusions": [
                    {"description": "Dewatering", "amount": "5000", "page": 1, "quote": "Excludes dewatering"}
                ],
            },
        ),
        ToolCallPart(
            "record_quote",
            {
                "package": "Groundworks",
                "company": "Najd Contracting",
                "document_id": najd,
                "lines": [{"boq_item": "3.1", "rate": "16.50", "page": 1, "quote": "Item 3.1 excavation rate 16.50"}],
            },
        ),
        ToolCallPart("levelling", {"package": "Groundworks"}),
        ToolCallPart(
            "recommend_quote", {"package": "Groundworks", "company": "Najd Contracting", "reason": "Cheapest levelled."}
        ),
    ]

    def brain(messages, info):
        if info.output_tools:  # appointing the Tender Manager is already done; nothing to generate
            return ModelResponse(parts=[TextPart("Done.")])
        if "You are Omar Haddad" not in info.instructions:
            return ModelResponse(parts=[TextPart("Done.")])
        done = [str(p.content) for m in messages for p in m.parts if isinstance(p, ToolReturnPart)]
        replies[:] = done
        return (
            ModelResponse(parts=[steps[len(done)]])
            if len(done) < len(steps)
            else ModelResponse(parts=[TextPart("Done.")])
        )

    client.app.state.office.model = lambda: FunctionModel(brain)
    with client.app.state.sessions() as session:
        office.post(session, tender_id, "engineer", omar, "Omar, level the groundworks quotes.")
        session.commit()
    client.app.state.office.engineer_spoke(tender_id)
    wait_for(lambda o: len(replies) == len(steps), client, tender_id)
    assert replies[6] == (
        "1. Najd Contracting: quoted 20460.00, exclusions 0, levelled 57700.00 (our rate used for 6.3)\n"
        "2. Gulf Groundworks: quoted 55380.00, exclusions 5000, levelled 60380.00"
    )
    assert replies[7] == "The office has taken Najd Contracting's quote; its rates are in the estimate."
    rows = {i["item"]: i for i in client.get(f"/tenders/{tender_id}/estimate").json()["items"]}
    assert (rows["3.1"]["rate"]["status"], rows["3.1"]["amount"]) == ("office_approved", "20460.00")
    assert rows["6.3"]["rate"]["basis"] == "estimate"  # the gap stays on our own rate
