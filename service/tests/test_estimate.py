import io
from decimal import Decimal

import openpyxl
import pytest
from pydantic_ai.messages import ModelResponse, TextPart, ToolCallPart, ToolReturnPart
from pydantic_ai.models.function import FunctionModel
from test_documents import make_pdf, read_all, upload
from test_office import wait_for

from quantix import settings
from quantix.boq import records as boq
from quantix.estimate import records as estimate
from quantix.office import records as office

QUOTE = make_pdf([["Al-Rajhi Steel quotation 17 September", "Rebar B500B cut and bent 2,300.00 SAR per t"]])
CONDITIONS = make_pdf([["Prices shall be in Saudi Riyals (SAR)", "VAT at 15% shall be shown separately"]])
BUILD_UP = [
    {"kind": "labour", "resource": "Steel fixer gang", "quantity": "16", "unit": "hr", "rate": "62"},
    {
        "kind": "material",
        "resource": "Rebar B500B cut and bent",
        "quantity": "1",
        "unit": "t",
        "rate": "2300",
        "wastage": "0.05",
    },
    {"kind": "material", "resource": "Tie wire", "quantity": "12", "unit": "kg", "rate": "6"},
    {"kind": "plant", "resource": "Bar bending machine", "quantity": "0.5", "unit": "hr", "rate": "18"},
]


def bill() -> bytes:
    workbook = openpyxl.Workbook()
    sheet = workbook.active
    for row in (
        ["3.1", "Excavation", "m3", 1240],
        ["4.3", "Slab reinforcement", "t", 28.1],
        ["6.3", "Waterproofing", "m2", 980],
    ):
        sheet.append(row)
    buffer = io.BytesIO()
    workbook.save(buffer)
    return buffer.getvalue()


@pytest.fixture
def tender(client):
    tender_id = client.post("/tenders", json={"name": "Synthetic school"}).json()["id"]
    upload(client, tender_id, {"Bill.xlsx": bill(), "Quote.pdf": QUOTE, "Conditions.pdf": CONDITIONS})
    docs = read_all(client, tender_id)
    with client.app.state.sessions() as session:
        priya = office.hire(session, tender_id, "Priya Nair", "Estimator", {})
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
            for n, (i, d, u, q) in enumerate(
                [
                    ("3.1", "Excavation", "m3", "1240"),
                    ("4.3", "Slab reinforcement", "t", "28.1"),
                    ("6.3", "Waterproofing", "m2", "980"),
                ],
                start=1,
            )
        ]
        boq.propose_items(session, tender_id, priya, lines, False)
        boq.approve_all_items(session, tender_id)
        for kind, value, quote in (("currency", "SAR", "Saudi Riyals (SAR)"), ("vat", "15%", "VAT at 15%")):
            fact = boq.propose_fact(
                session, tender_id, priya, kind, value, docs["Conditions.pdf"]["id"], 1, quote, False
            )
            boq.decide(session, fact, True)
        session.commit()
        return tender_id, priya.id, docs["Quote.pdf"]["id"]


def price(client, tender_id, by, item, basis, note="", **kw):
    with client.app.state.sessions() as session:
        rate = estimate.propose_rate(session, tender_id, by, item, basis, note, **kw)
        session.commit()
        return rate.id


def test_the_price_is_computed_from_quantities_rates_and_markups(client, tender):
    tender_id, priya, quote = tender
    note = "Plant 4.5 m3/hr at 83.25 per hour; no disposal off site."
    price(client, tender_id, priya, "3.1", "estimate", note, unit_rate=Decimal("18.50"))
    lines = [estimate.LineIn(**line) for line in BUILD_UP]
    price(
        client,
        tender_id,
        priya,
        "4.3",
        "quote",
        "Steel price from Al-Rajhi.",
        lines=lines,
        document_id=quote,
        page=1,
        quote="Rebar B500B cut and bent 2,300.00",
    )
    with client.app.state.sessions() as session:
        estimate.propose_markups(
            session,
            tender_id,
            priya,
            Decimal("0.08"),
            Decimal("0.05"),
            Decimal("0.07"),
            Decimal("-1000"),
            "Company rules",
        )
        session.commit()

    result = client.get(f"/tenders/{tender_id}/estimate").json()
    rows = {i["item"]: i for i in result["items"]}
    assert (rows["3.1"]["rate"]["rate"], rows["3.1"]["amount"]) == ("18.50", "22940.00")
    assert [line["cost"] for line in rows["4.3"]["rate"]["lines"]] == ["992.00", "2415.00", "72.00", "9.00"]
    assert (rows["4.3"]["rate"]["rate"], rows["4.3"]["amount"]) == ("3488.00", "98012.80")
    assert rows["4.3"]["rate"]["source_document"] == "Quote.pdf"
    assert rows["6.3"]["rate"] is None

    s = result["summary"]
    assert (s["currency"], s["priced"], s["items"], s["waiting"], s["unpriced"]) == ("SAR", 2, 3, 2, ["6.3"])
    assert [s[k] for k in ("net", "preliminaries", "overheads", "profit", "adjustment", "total")] == [
        "120952.80",
        "9676.22",
        "6531.45",
        "9601.23",
        "-1000.00",
        "145761.70",
    ]
    assert (s["vat_rate"], s["vat"], s["total_with_vat"]) == ("0.15", "21864.26", "167625.96")


def test_every_rate_needs_a_basis_that_checks_out(client, tender):
    tender_id, priya, quote = tender
    with client.app.state.sessions() as session:
        with pytest.raises(ValueError, match="The rate 2400 is not in the quoted line"):
            estimate.propose_rate(
                session,
                tender_id,
                priya,
                "4.3",
                "quote",
                "x",
                unit_rate=Decimal("2400"),
                document_id=quote,
                page=1,
                quote="Rebar B500B cut and bent 2,300.00",
            )
        with pytest.raises(ValueError, match="needs its reasoning"):
            estimate.propose_rate(session, tender_id, priya, "3.1", "estimate", "cheap", unit_rate=Decimal("18"))
        with pytest.raises(ValueError, match="library entry doesn't exist"):
            estimate.propose_rate(
                session, tender_id, priya, "3.1", "library", "x", unit_rate=Decimal("18"), library_id="no"
            )
        with pytest.raises(ValueError, match="There is no BOQ item 9.9"):
            estimate.propose_rate(session, tender_id, priya, "9.9", "estimate", "x" * 30, unit_rate=Decimal("1"))


def test_approving_keeps_one_rate_and_can_save_it_to_the_library(client, tender):
    tender_id, priya, quote = tender
    lines = [estimate.LineIn(**line) for line in BUILD_UP]
    first = price(
        client, tender_id, priya, "4.3", "estimate", "First try, outputs from the last school job.", lines=lines
    )
    second = price(
        client,
        tender_id,
        priya,
        "4.3",
        "quote",
        "Quoted steel.",
        lines=lines,
        document_id=quote,
        page=1,
        quote="Rebar B500B cut and bent 2,300.00",
    )
    assert client.post(f"/rates/{second}/decision", json={"approve": True, "save_to_library": True}).status_code == 200
    assert (
        client.post(f"/rates/{first}/decision", json={"approve": True}).json()["detail"]
        == "This has already been decided."
    )

    library = client.get("/library", params={"q": "rebar"}).json()
    assert [(r["kind"], r["name"], r["rate"], r["currency"]) for r in library] == [
        ("material", "Rebar B500B cut and bent", "2300.0000", "SAR")
    ]
    assert len(client.get("/library").json()) == 4

    entry = client.post(
        "/library",
        json={
            "kind": "unit_rate",
            "name": "Excavation in soft ground",
            "unit": "m3",
            "rate": "18.5",
            "currency": "SAR",
            "source": "Engineer",
            "dated": "2026-09-01",
        },
    ).json()
    price(
        client,
        tender_id,
        priya,
        "3.1",
        "library",
        "From the library.",
        unit_rate=Decimal("18.5"),
        library_id=entry["id"],
    )
    assert (
        client.delete(f"/library/{entry['id']}").json()["detail"].startswith("A tender's rate is based on this entry")
    )


def test_approve_all_and_send_a_rate_back(client, tender):
    tender_id, priya, _ = tender
    price(client, tender_id, priya, "3.1", "estimate", "Plant 4.5 m3/hr at 83.25 per hour.", unit_rate=Decimal("18.50"))
    rejected = price(
        client, tender_id, priya, "6.3", "estimate", "Membrane 38 per m2 laid, from memory.", unit_rate=Decimal("38")
    )
    client.post(f"/rates/{rejected}/decision", json={"approve": False, "reason": "Get a supplier quote."})
    assert client.post(f"/tenders/{tender_id}/rates/approve-all").json() == {"approved": 1}
    chat = client.get(f"/tenders/{tender_id}/messages", params={"channel": priya}).json()
    assert chat[-1]["text"] == "I rejected the rate for BOQ item 6.3: Get a supplier quote."
    assert client.get(f"/tenders/{tender_id}/gates").json()["pricing"] == 0


def test_staff_price_through_their_tools(client, tender, tmp_path):
    tender_id, priya, _ = tender
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})
    replies: list[str] = []

    persona = {
        "name": "Rania Farouk",
        "discipline": "Civil",
        "experience_years": 19,
        "background": "b",
        "working_style": "w",
        "opinions": "o",
        "voice": "v",
    }

    def brain(messages, info):
        if info.output_tools:  # appointing the Tender Manager
            return ModelResponse(parts=[ToolCallPart(info.output_tools[0].name, persona)])
        if "You are Priya Nair" not in info.instructions:
            return ModelResponse(parts=[TextPart("Done.")])
        done = [str(p.content) for m in messages for p in m.parts if isinstance(p, ToolReturnPart)]
        replies[:] = done
        steps = [
            ToolCallPart(
                "propose_rate",
                {
                    "boq_item": "3.1",
                    "basis": "estimate",
                    "unit_rate": "18.50",
                    "note": "Plant 4.5 m3/hr at 83.25 per hour; no disposal.",
                },
            ),
            ToolCallPart("estimate_summary", {}),
        ]
        return (
            ModelResponse(parts=[steps[len(done)]])
            if len(done) < len(steps)
            else ModelResponse(parts=[TextPart("Done.")])
        )

    client.app.state.office.model = lambda: FunctionModel(brain)
    with client.app.state.sessions() as session:
        office.post(session, tender_id, "engineer", priya, "Priya, price the excavation.")
        session.commit()
    client.app.state.office.engineer_spoke(tender_id)
    wait_for(
        lambda o: client.get(f"/tenders/{tender_id}/gates").json()["pricing"] == 1 and len(replies) == 2,
        client,
        tender_id,
    )
    assert replies[0] == "Item 3.1 priced at 18.50 per unit."
    assert replies[1].startswith("1 of 3 items priced (1 waiting for the engineer).\nNet 22940.00 SAR")
