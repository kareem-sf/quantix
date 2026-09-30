import io
from decimal import Decimal

import openpyxl
import pytest
from pydantic import ValidationError
from pydantic_ai.messages import ModelResponse, TextPart, ToolCallPart, ToolReturnPart
from sqlalchemy import select
from test_documents import make_pdf, read_all, upload
from test_office import manager_accepts, scripted, wait_for

from quantix import settings
from quantix.boq import records as boq
from quantix.boq.models import BoqItem, Fact
from quantix.documents.models import WebPage
from quantix.estimate import analysis
from quantix.estimate import records as estimate
from quantix.estimate.models import Markups, Rate
from quantix.office import records as office
from quantix.review import records as reviews
from quantix.subcontract import records as subcontract

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
        boq.propose_items(session, tender_id, priya, lines)
        for approved in boq.items(session, tender_id):  # the engineer approved the BOQ
            boq.approve(session, approved)
        for kind, value, quote in (("currency", "SAR", "Saudi Riyals (SAR)"), ("vat", "15%", "VAT at 15%")):
            fact = boq.propose_fact(session, tender_id, priya, kind, value, docs["Conditions.pdf"]["id"], 1, quote)
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
            [
                estimate.PreliminaryIn(item="Site engineer", quantity=Decimal(3), unit="month", rate=Decimal(3000)),
                estimate.PreliminaryIn(
                    item="Plant mobilisation", quantity=Decimal(1), unit="sum", rate=Decimal("676.22")
                ),
            ],
            Decimal("0.05"),
            Decimal("0.07"),
            Decimal("-1000"),
            "Company rules",
        )
        session.commit()

    result = client.get(f"/tenders/{tender_id}/estimate").json()
    assert [(p["item"], p["cost"]) for p in result["markups"]["preliminary_items"]] == [
        ("Site engineer", "9000.00"),
        ("Plant mobilisation", "676.22"),
    ]
    rows = {i["item"]: i for i in result["items"]}
    assert (rows["3.1"]["rate"]["rate"], rows["3.1"]["amount"]) == ("18.50", "22940.00")
    assert [line["cost"] for line in rows["4.3"]["rate"]["lines"]] == ["992.00", "2415.00", "72.00", "9.00"]
    assert (rows["4.3"]["rate"]["rate"], rows["4.3"]["amount"]) == ("3488.00", "98012.80")
    assert rows["4.3"]["rate"]["source_document"] == "Quote.pdf"
    assert rows["6.3"]["rate"] is None

    s = result["summary"]
    assert (s["currency"], s["priced"], s["items"], s["unpriced"]) == ("SAR", 2, 3, ["6.3"])
    assert (s["reviewing"], s["waiting"]) == (2, 0)  # with the Tender Manager, not yet waiting for the engineer
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
        with pytest.raises(ValueError, match="A rate must be more than zero"):
            estimate.propose_rate(session, tender_id, priya, "3.1", "estimate", "x" * 30, unit_rate=Decimal("0"))
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
        # an estimate that also names a library entry or document that doesn't exist keeps only what it rests on
        rate = estimate.propose_rate(
            session, tender_id, priya, "3.1", "estimate", "x" * 30, unit_rate=Decimal("18"), library_id="L-01",
            document_id="made-up", page=3,
        )  # fmt: skip
        session.flush()
        assert (rate.library_id, rate.document_id, rate.page) == (None, None, None)


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


def test_approved_markups_stay_settled(client, tender):
    tender_id, priya, _ = tender
    zero = Decimal(0)
    with client.app.state.sessions() as session:
        markups = estimate.propose_markups(session, tender_id, priya, [], Decimal("0.06"), Decimal("0.08"), zero, "x")
        session.commit()
        markups_id = markups.id
    client.post(f"/markups/{markups_id}/decision", json={"approve": True})
    with client.app.state.sessions() as session, pytest.raises(ValueError, match="The engineer approved the markups"):
        estimate.propose_markups(session, tender_id, priya, [], Decimal("0.05"), Decimal("0.08"), zero, "Again.")


def test_a_new_proposal_replaces_the_one_still_waiting(client, tender):
    tender_id, priya, _ = tender
    first = price(
        client, tender_id, priya, "3.1", "estimate", "Unit rate from the BOQ wording.", unit_rate=Decimal("18")
    )
    second = price(
        client, tender_id, priya, "3.1", "estimate", "Plant 4.5 m3/hr at 83.25 an hour.", unit_rate=Decimal(20)
    )
    with client.app.state.sessions() as session:
        assert [p.kind for p in reviews.pending(session, tender_id)] == ["rate"]  # one rate per line to review
        assert session.get(Rate, first).status == "replaced"
        assert session.get(Rate, second).status == "proposed"


def test_an_approved_line_takes_a_new_estimate_only_after_the_engineer_sends_one_back(client, tender):
    tender_id, priya, quote = tender
    lines = [estimate.LineIn(**line) for line in BUILD_UP]
    approved = price(client, tender_id, priya, "4.3", "estimate", "Outputs from the last school job.", lines=lines)
    client.post(f"/rates/{approved}/decision", json={"approve": True})
    with pytest.raises(ValueError, match="The engineer approved 3488.00 for 4.3; a new estimate doesn't replace it"):
        price(client, tender_id, priya, "4.3", "estimate", "Outputs from another job.", unit_rate=Decimal(3000))
    quoted = price(  # a quote may still challenge it
        client, tender_id, priya, "4.3", "quote", "Quoted.", lines=lines, document_id=quote, page=1,
        quote="Rebar B500B cut and bent 2,300.00",
    )  # fmt: skip
    client.post(f"/rates/{quoted}/decision", json={"approve": False, "reason": "Redo the fixing outputs."})
    price(client, tender_id, priya, "4.3", "estimate", "Fixing outputs redone as asked.", unit_rate=Decimal(3400))


def test_approve_all_and_send_a_rate_back(client, tender):
    tender_id, priya, _ = tender
    price(client, tender_id, priya, "3.1", "estimate", "Plant 4.5 m3/hr at 83.25 per hour.", unit_rate=Decimal("18.50"))
    rejected = price(
        client, tender_id, priya, "6.3", "estimate", "Membrane 38 per m2 laid, from memory.", unit_rate=Decimal("38")
    )
    manager_accepts(client, tender_id)
    client.post(f"/rates/{rejected}/decision", json={"approve": False, "reason": "Get a supplier quote."})
    assert client.post(f"/tenders/{tender_id}/rates/approve-all").json() == {"approved": 1}
    team = client.get(f"/tenders/{tender_id}/messages", params={"channel": "team"}).json()
    assert team[-1]["text"] == "Priya, I sent back the rate for BOQ item 6.3: Get a supplier quote."
    assert client.get(f"/tenders/{tender_id}/gates").json()["pricing"] == 0


def test_staff_price_through_their_tools(client, tender, tmp_path):
    tender_id, priya, _ = tender
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})
    replies: list[str] = []

    persona = {
        "name": "Rania Farouk",
        "discipline": "Civil",
        "experience_years": 19,
        "background": "Priced civil works for schools and clinics.",
        "working_style": "Checks every figure twice.",
        "opinions": "Distrusts quantities nobody has measured.",
        "voice": "Short and direct.",
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

    client.app.state.office.model = lambda: scripted(brain)
    with client.app.state.sessions() as session:
        office.post(session, tender_id, "engineer", priya, "Priya, price the excavation.")
        session.commit()
    client.app.state.office.engineer_spoke(tender_id)
    wait_for(
        lambda o: client.get(f"/tenders/{tender_id}/gates").json()["manager"] == 1 and len(replies) == 2,
        client,
        tender_id,
    )
    assert replies[0] == "Item 3.1 priced at 18.50 per unit, for the Tender Manager's review."
    assert replies[1].startswith(
        "1 of 3 items priced (1 with the Tender Manager, 0 waiting for the engineer).\nNet 22940.00 SAR"
    )


REASONING = "Own outputs and current prices from the last school job."


def add_line(client, tender_id, by, item, description, unit, quantity):
    """A BOQ line the engineer approved, entered directly."""
    with client.app.state.sessions() as session:
        slab = boq.find_item(session, tender_id, "4.3")
        session.add(
            BoqItem(tender_id=tender_id, item=item, description=description, unit=unit, quantity=quantity,
                    document_id=slab.document_id, page=1, quote=slab.quote, proposed_by=by, status="approved",
                    position=len(boq.items(session, tender_id)) + 1)
        )  # fmt: skip
        session.commit()


def row_of(client, tender_id, item):
    return next(i for i in client.get(f"/tenders/{tender_id}/estimate").json()["items"] if i["item"] == item)


def test_vat_is_the_rate_the_engineer_approved_from_the_tender(client, tender):
    tender_id, priya, _ = tender
    price(client, tender_id, priya, "3.1", "estimate", REASONING, unit_rate=Decimal("18.50"))
    with client.app.state.sessions() as session:
        facts = {f.kind: f for f in session.scalars(select(Fact).where(Fact.tender_id == tender_id))}
        vat = facts["vat"]
        for value, rate, amount, total in (
            ("VAT at 15%", "0.15", "3441.00", "26381.00"),  # no markups, so on the net: 1,240 × 18.50 = 22,940.00
            ("5.5 % on all supplies", "0.055", "1261.70", "24201.70"),
            ("0%", "0", "0.00", "22940.00"),
        ):
            vat.value = value
            s = estimate.summary(session, tender_id)
            assert (s.total, s.vat_rate, s.vat, s.total_with_vat) == (
                Decimal("22940.00"),
                Decimal(rate),
                Decimal(amount),
                Decimal(total),
            )
        vat.value = "Exempt"
        assert estimate.summary(session, tender_id).vat is None
        # nothing is assumed: a currency and VAT the engineer hasn't approved aren't in the price
        vat.value, vat.status, facts["currency"].status = "15%", "proposed", "proposed"
        s = estimate.summary(session, tender_id)
        assert (s.currency, s.vat_rate, s.vat, s.total_with_vat, s.total) == ("", None, None, None, Decimal("22940.00"))


@pytest.mark.xfail(strict=True, reason="Bug: the VAT rate is read only before an ASCII %, so ١٥٪ adds no VAT")
def test_a_vat_written_in_arabic_is_applied(client, tender):
    tender_id, priya, _ = tender
    price(client, tender_id, priya, "3.1", "estimate", REASONING, unit_rate=Decimal("18.50"))
    with client.app.state.sessions() as session:
        vat = session.scalars(select(Fact).where(Fact.tender_id == tender_id, Fact.kind == "vat")).one()
        vat.value = "ضريبة القيمة المضافة ١٥٪"
        s = estimate.summary(session, tender_id)
        assert (s.vat_rate, s.vat) == (Decimal("0.15"), Decimal("3441.00"))


def test_a_line_without_a_quantity_is_unpriced_and_a_zero_quantity_costs_nothing(client, tender):
    tender_id, priya, _ = tender
    add_line(client, tender_id, priya, "7.1", "Rock excavation, rate only", "m3", None)
    add_line(client, tender_id, priya, "7.2", "Extra over for hand digging", "m3", Decimal(0))
    for item, rate in (("3.1", "18.50"), ("7.1", "45"), ("7.2", "30")):
        price(client, tender_id, priya, item, "estimate", REASONING, unit_rate=Decimal(rate))
    result = client.get(f"/tenders/{tender_id}/estimate").json()
    rows = {i["item"]: i for i in result["items"]}
    assert [(rows[i]["rate"]["rate"], rows[i]["amount"]) for i in ("7.1", "7.2")] == [
        ("45.00", None),
        ("30.00", "0.00"),
    ]
    s = result["summary"]
    assert (s["items"], s["priced"], s["unpriced"], s["net"]) == (5, 2, ["4.3", "6.3", "7.1"], "22940.00")


def test_each_build_up_line_is_rounded_to_the_cent_before_the_rate_adds_them(client, tender):
    tender_id, priya, _ = tender
    lines = [
        estimate.LineIn(kind="labour", resource="Applicator", quantity=Decimal("0.5"), unit="hr", rate=Decimal("4.69")),
        estimate.LineIn(
            kind="material",
            resource="Torch-on membrane",
            quantity=Decimal(1),
            unit="m2",
            rate=Decimal("12.345"),
            wastage=Decimal("0.15"),
        ),
        estimate.LineIn(kind="plant", resource="Gas torch", quantity=Decimal("0.01"), unit="hr", rate=Decimal("1.005")),
    ]
    price(client, tender_id, priya, "6.3", "estimate", REASONING, lines=lines)
    row = row_of(client, tender_id, "6.3")
    # 0.5 × 4.69 = 2.345, half up to 2.35; 1.15 × 12.345 = 14.19675 to 14.20; 0.01 × 1.005 = 0.01005 to 0.01
    assert [line["cost"] for line in row["rate"]["lines"]] == ["2.35", "14.20", "0.01"]
    assert (row["rate"]["rate"], row["amount"]) == ("16.56", "16228.80")  # 980 × 16.56


def test_the_price_keeps_the_approved_rate_while_a_quote_challenging_it_waits(client, tender):
    tender_id, priya, quote = tender
    lines = [estimate.LineIn(**line) for line in BUILD_UP]
    approved = price(client, tender_id, priya, "4.3", "estimate", "Outputs from the last school job.", lines=lines)
    client.post(f"/rates/{approved}/decision", json={"approve": True})
    quoted = price(
        client, tender_id, priya, "4.3", "quote", "Quoted.", unit_rate=Decimal("2300"), document_id=quote, page=1,
        quote="Rebar B500B cut and bent 2,300.00",
    )  # fmt: skip
    row = row_of(client, tender_id, "4.3")
    assert (row["rate"]["id"], row["amount"]) == (approved, "98012.80")  # 28.1 × 3,488.00 until the engineer decides
    client.post(f"/rates/{quoted}/decision", json={"approve": True})
    row = row_of(client, tender_id, "4.3")
    assert (row["rate"]["id"], row["amount"]) == (quoted, "64630.00")  # 28.1 × 2,300.00
    with client.app.state.sessions() as session:
        assert session.get(Rate, approved).status == "replaced"


def test_a_rate_is_a_unit_rate_or_a_build_up_on_a_basis_quantix_knows(client, tender):
    tender_id, priya, quote = tender
    lines = [estimate.LineIn(**line) for line in BUILD_UP]
    idle = [estimate.LineIn(kind="labour", resource="Gang", quantity=Decimal(0), unit="hr", rate=Decimal(62))]
    with client.app.state.sessions() as session:
        for kw in ({}, {"unit_rate": Decimal(18), "lines": lines}):
            with pytest.raises(ValueError, match="Give either a unit rate or a build-up of lines, not both."):
                estimate.propose_rate(session, tender_id, priya, "3.1", "estimate", REASONING, **kw)
        for kw in ({"unit_rate": Decimal(-18)}, {"lines": idle}):
            with pytest.raises(ValueError, match="A rate must be more than zero"):
                estimate.propose_rate(session, tender_id, priya, "3.1", "estimate", REASONING, **kw)
        with pytest.raises(ValueError, match="A quoted rate needs the document, page and the quoted line."):
            estimate.propose_rate(
                session, tender_id, priya, "4.3", "quote", "x", unit_rate=Decimal(2300), document_id=quote
            )
        with pytest.raises(ValueError, match="The basis is quote, library, web or estimate."):
            estimate.propose_rate(session, tender_id, priya, "3.1", "guess", REASONING, unit_rate=Decimal(18))


def test_a_web_price_rests_on_the_page_quantix_saved(client, tender):
    tender_id, priya, _ = tender
    note = "Supplier list price delivered to site, VAT excluded; laid by our own gang."
    quote = "Torch-on membrane 4 mm, 36.50 SAR per m2"
    with client.app.state.sessions() as session:
        saved = WebPage(url="https://prices.example/membrane", title="Membrane prices", text=f"Price list. {quote}.")
        session.add(saved)
        session.flush()
        for kw, message in (
            ({"quote": quote}, "A web price needs the saved page's id from read_web_page"),
            ({"web_page_id": "nope", "quote": quote}, "No saved web page has the id nope"),
            ({"web_page_id": saved.id, "quote": "Membrane 30.00"}, "is not on the saved page https://prices.example"),
            ({"web_page_id": saved.id, "quote": quote, "unit_rate": Decimal(40)}, "The rate 40 is not in the quoted"),
        ):
            with pytest.raises(ValueError, match=message):
                estimate.propose_rate(
                    session, tender_id, priya, "6.3", "web", note, **{"unit_rate": Decimal("36.5"), **kw}
                )
        with pytest.raises(ValueError, match="A web price needs a note: what it covers and how it becomes"):
            estimate.propose_rate(
                session, tender_id, priya, "6.3", "web", "List price.", unit_rate=Decimal("36.5"), web_page_id=saved.id,
                quote=quote,
            )  # fmt: skip
        estimate.propose_rate(
            session, tender_id, priya, "6.3", "web", note, unit_rate=Decimal("36.5"), web_page_id=saved.id,
            quote=quote, document_id="made-up", page=2,
        )  # fmt: skip
        session.commit()
    row = row_of(client, tender_id, "6.3")
    rate = row["rate"]
    assert (rate["basis"], rate["rate"], rate["quote"], rate["document_id"], rate["page"]) == (
        "web",
        "36.50",
        quote,
        None,  # only the evidence a web price calls for
        None,
    )
    assert (rate["web_page"]["url"], rate["web_page"]["title"]) == (
        "https://prices.example/membrane",
        "Membrane prices",
    )
    assert row["amount"] == "35770.00"  # 980 × 36.50


def test_markups_are_fractions_below_one_and_a_new_proposal_replaces_the_waiting_one(client, tender):
    tender_id, priya, _ = tender
    zero = Decimal(0)
    with client.app.state.sessions() as session:
        for overheads, profit in ((Decimal(1), zero), (zero, Decimal("-0.01")), (Decimal(6), zero)):
            with pytest.raises(ValueError, match="as a fraction between 0 and 1, e.g. 0.06 for 6%"):
                estimate.propose_markups(session, tender_id, priya, [], overheads, profit, zero, "x")
        for quantity, rate in ((Decimal(0), Decimal(5000)), (Decimal(3), Decimal(-1))):
            with pytest.raises(ValidationError):
                estimate.PreliminaryIn(item="Site office", quantity=quantity, unit="month", rate=rate)
        first = estimate.propose_markups(session, tender_id, priya, [], Decimal("0.9999"), zero, zero, "First.")
        second = estimate.propose_markups(session, tender_id, priya, [], Decimal("0.05"), Decimal("0.07"), zero, "x")
        session.commit()
        assert (first.status, second.status) == ("replaced", "proposed")
        assert estimate.current_markups(session, tender_id).id == second.id
        assert estimate.label(session, second) == "the markups"
        assert [m.id for m in session.scalars(select(Markups).where(Markups.status == "proposed"))] == [second.id]


def test_unknown_estimates_rates_markups_and_library_entries_are_not_found(client):
    assert client.get("/tenders/nope/estimate").status_code == 404
    assert client.post("/tenders/nope/rates/approve-all").status_code == 404
    assert client.post("/rates/nope/decision", json={"approve": True}).status_code == 404
    assert client.post("/markups/nope/decision", json={"approve": True}).status_code == 404
    assert client.delete("/library/nope").status_code == 404
    entry = {"kind": "plant", "name": "Excavator 20 t", "unit": "hr", "rate": "0", "currency": "SAR", "source": "Hire"}
    assert client.post("/library", json={**entry, "dated": "2026-09-01"}).status_code == 422  # a rate is more than 0
    kept = client.post("/library", json={**entry, "rate": "83.25", "dated": "2026-09-01"}).json()
    assert client.delete(f"/library/{kept['id']}").status_code == 204  # no tender's rate rests on it
    assert client.get("/library").json() == []


def test_where_the_money_is_lists_the_lines_that_carry_most_of_the_net(client, tender):
    tender_id, priya, _ = tender
    with client.app.state.sessions() as session:
        assert analysis.breakdown(session, tender_id) == "Nothing is priced yet."
    price(client, tender_id, priya, "3.1", "estimate", REASONING, unit_rate=Decimal("18.50"))
    price(client, tender_id, priya, "4.3", "estimate", REASONING, lines=[estimate.LineIn(**line) for line in BUILD_UP])
    price(client, tender_id, priya, "6.3", "estimate", REASONING, unit_rate=Decimal(38))
    with client.app.state.sessions() as session:
        text = analysis.breakdown(session, tender_id)
    # 22,940.00 + 98,012.80 + 37,240.00; 4.3 and 6.3 carry 85.5%, past the 80% the list stops at
    assert text.splitlines() == [
        "Net 158,192.80 SAR; with markups 158,192.80 (Quantix's figures).",
        "By bill:",
        "- (no bill): 158,192.80 (100.0%)",
        "The lines that make up most of the net:",
        "- 4.3 Slab reinforcement: 98,012.80 (62.0%)",
        "- 6.3 Waterproofing: 37,240.00 (23.5%)",
        "By kind of cost, from the build-ups:",
        "- labour: 27,875.20 (17.6%)",  # 28.1 × 992.00
        "- plant: 252.90 (0.2%)",  # 28.1 × 9.00
        "- material: 69,884.70 (44.2%)",  # 28.1 × 2,415.00 + 28.1 × 72.00
        "- unit rates: 60,180.00 (38.0%)",
    ]


def test_what_a_change_would_do_to_the_price_with_its_markups(client, tender):
    tender_id, priya, _ = tender
    price(client, tender_id, priya, "3.1", "estimate", REASONING, unit_rate=Decimal(18))
    price(client, tender_id, priya, "4.3", "estimate", REASONING, lines=[estimate.LineIn(**line) for line in BUILD_UP])
    engineer = estimate.PreliminaryIn(item="Site engineer", quantity=Decimal(3), unit="month", rate=Decimal(3000))
    with client.app.state.sessions() as session:
        estimate.propose_markups(
            session, tender_id, priya, [engineer], Decimal("0.05"), Decimal("0.07"), Decimal(-1000), "Company rules"
        )
        session.commit()
        labour = analysis.what_if(session, tender_id, [analysis.Change(kind="labour", factor=Decimal("1.5"))])
        # Now: 1,240 × 18 + 28.1 × 3,488 = 120,332.80; site 9,000; overheads 5% 6,466.64; profit 7% 9,505.96;
        # less 1,000: 144,305.40. Half as much labour again adds 496.00 a t to 4.3: 28.1 × 496 = 13,937.60, then
        # overheads 7,163.52 and profit 10,530.37 on the new net.
        assert labour.splitlines() == [
            "Net 120,332.80 would be 134,270.40 (+13,937.60, 11.6%).",
            "With markups 144,305.40 would be 159,964.29 (+15,658.89). Nothing was changed.",
            "1 lines move, largest first:",
            "- 4.3: rate 3488.00 → 3984.00 per t, amount +13,937.60",
        ]
        untouched = [analysis.Change(item="3.1", resource="labour", factor=Decimal(2))]  # a unit rate has no lines
        assert analysis.what_if(session, tender_id, untouched) == (
            "No priced line is affected: check the item, the resource words or the kind."
        )
        assert estimate.summary(session, tender_id).total == Decimal("144305.40")


def test_a_rate_beside_similar_lines_and_the_quotes_for_it(client, tender):
    tender_id, priya, quote = tender
    add_line(client, tender_id, priya, "4.4", "Slab reinforcement to ground beams", "t", Decimal(12))
    price(client, tender_id, priya, "4.3", "estimate", REASONING, lines=[estimate.LineIn(**line) for line in BUILD_UP])
    price(client, tender_id, priya, "4.4", "estimate", REASONING, unit_rate=Decimal(3300))
    price(client, tender_id, priya, "6.3", "estimate", REASONING, unit_rate=Decimal(38))
    with client.app.state.sessions() as session:
        steel = subcontract.add_company(session, priya, "Al-Rajhi Steel", "supplier", "Rebar")
        package = subcontract.create_package(session, tender_id, priya, "Rebar", "supply", ["4.3"])
        line = subcontract.QuoteLine(
            boq_item="4.3", rate=Decimal("2300.00"), page=1, quote="Rebar B500B cut and bent 2,300.00"
        )
        subcontract.record_quote(session, package, steel, priya, quote, [line], [])
        session.commit()
        assert analysis.compare_rate(session, tender_id, "4.3").splitlines() == [
            "4.3 Slab reinforcement: ours 3488.00 per t (estimate).",
            "- Here: 4.4 Slab reinforcement to ground beams 3300.00: ours is 6% above",  # 188 over 3,300
            "- Quote from Al-Rajhi Steel: 2300.00: ours is 52% above",  # 1,188 over 2,300
        ]
        assert analysis.compare_rate(session, tender_id, "6.3").splitlines() == [
            "6.3 Waterproofing: ours 38.00 per m2 (estimate).",
            "Nothing in the library, earlier tenders, this tender or the quotes to compare it with.",
        ]
        assert analysis.compare_rate(session, tender_id, "3.1") == "3.1 isn't priced yet."
