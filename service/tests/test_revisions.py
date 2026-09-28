"""A newer copy of a tender document takes over the office's work wherever what the work cites is unchanged. The rest
must be done again from the newer copy, and until it is, Quantix's checks block it and the audit holds the release."""

import hashlib
import io
from decimal import Decimal
from pathlib import Path

import openpyxl
import pytest
from test_documents import make_pdf, read_all, upload

from quantix.boq import records as boq
from quantix.boq.models import BoqItem, Fact
from quantix.documents import readers
from quantix.estimate import records as estimate
from quantix.office import agents
from quantix.office import records as office
from quantix.office.models import Staff
from quantix.review import revisions
from quantix.submission import records as submission
from quantix.submission.models import Requirement
from quantix.takeoff import records as takeoff
from quantix.takeoff.models import Measurement

PLAN = (Path(__file__).parent / "fixtures" / "synthetic-plan.pdf").read_bytes()  # a 40 × 20 m yard, 800 m²
YARD = [[85.04, 170.08], [481.89, 170.08], [481.89, 368.51], [85.04, 368.51]]  # the drawing's own corners
KERB = [[100.0, 500.0], [300.0, 500.0]]
BILL = [
    ["C.1", "Excavation in all soils", "m3", 1000],
    ["C.2", "Asphalt wearing course", "m2", 800],
    ["C.3", "Precast kerbs", "m", 120],
]
# Addendum 1 adds site clearance above every line, and more kerbs
REVISED_BILL = [["C.0", "Site clearance", "m2", 5000], *BILL[:2], ["C.3", "Precast kerbs", "m", 150]]
CONDITIONS = make_pdf([["VAT at fifteen percent", "12.3 The tenderer shall submit a work programme."]])
REVISED_CONDITIONS = make_pdf([["VAT at five percent", "12.4 The tenderer shall submit a work programme."]])
NOTE = "Own rate from crew outputs and current prices."


def workbook(rows: list[list]) -> bytes:
    book = openpyxl.Workbook()
    for row in rows:
        book.active.append(row)
    buffer = io.BytesIO()
    book.save(buffer)
    return buffer.getvalue()


@pytest.fixture
def tender(client):
    """A bill, a drawing and the conditions; the bill's lines approved and two of them priced. Omar works for Rania,
    the Tender Manager."""
    tender_id = client.post("/tenders", json={"name": "Synthetic depot"}).json()["id"]
    upload(client, tender_id, {"Bill.xlsx": workbook(BILL), "A-102.pdf": PLAN, "Conditions.pdf": CONDITIONS})
    docs = {name: d["id"] for name, d in read_all(client, tender_id).items()}
    with client.app.state.sessions() as session:
        office.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)
        omar = office.hire(session, tender_id, "Omar Haddad", "Quantity Surveyor", {})
        lines = [
            boq.ItemIn(item=i, description=d, unit=u, quantity=Decimal(q), document_id=docs["Bill.xlsx"], page=1,
                       quote=f"A{n}={i} | B{n}={d} | C{n}={u} | D{n}={q}")
            for n, (i, d, u, q) in enumerate(BILL, start=1)
        ]  # fmt: skip
        boq.propose_items(session, tender_id, omar, lines)
        for item in boq.items(session, tender_id):
            boq.approve(session, item)
        for item in ("C.1", "C.3"):
            estimate.approve(
                session, estimate.propose_rate(session, tender_id, omar.id, item, "estimate", NOTE, Decimal("25"))
            )
        session.commit()
        return tender_id, omar.id, docs


def newer(client, tender_id, name) -> str:
    """The current copy of a file, once every copy has been read."""
    read_all(client, tender_id)
    documents = client.get(f"/tenders/{tender_id}/documents").json()
    return next(d for d in documents if d["path"] == name and d["status"] != "replaced")


def findings(client, kind, record_id) -> list[tuple[str, str]]:
    return [(f["severity"], f["message"]) for f in client.get(f"/records/{kind}/{record_id}/findings").json()]


def audit(client, tender_id) -> list[str]:
    return [f["message"] for f in client.get(f"/tenders/{tender_id}/audit").json() if "older copy" in f["message"]]


def test_a_revised_bill_takes_over_the_lines_it_still_has(client, tender):
    tender_id, omar_id, docs = tender
    with client.app.state.sessions() as session:
        ids = {i.item: i.id for i in boq.items(session, tender_id)}
    upload(client, tender_id, {"Bill.xlsx": workbook(REVISED_BILL)})
    bill = newer(client, tender_id, "Bill.xlsx")
    assert bill["note"] == (
        "It replaces an older copy; 2 of the 3 pieces of work citing it are unchanged here and now rest on it, and 1 "
        "must be done again from it."
    )

    with client.app.state.sessions() as session:
        excavation, kerbs = session.get(BoqItem, ids["C.1"]), session.get(BoqItem, ids["C.3"])
        # a row added above it moves the line down a row: the same cells, so the line moves with its approval
        assert (excavation.document_id, excavation.status) == (bill["id"], "approved")
        assert excavation.quote == "A2=C.1 | B2=Excavation in all soils | C2=m3 | D2=1000"
        assert kerbs.document_id == docs["Bill.xlsx"]  # 150 m now, not 120: it stays on the older copy
        assert revisions.news(session, tender_id, since=None)  # wakes the Tender Manager, who hears what to redo
        briefing = agents.situation(session, office.manager(session, tender_id), [])
        assert "must be done again from the newer ones" in briefing and "- BOQ item C.3, by Omar" in briefing
        assert "- Work on older copies of documents, to do again from the newer copies: 1." in briefing

    why = (
        "The newer copy of Bill.xlsx doesn't say what it cites (“A3=C.3 | B3=Precast kerbs | C3=m | D3=120”, page 1 "
        "of the older copy): do it again from the newer copy."
    )
    assert findings(client, "boq", ids["C.3"]) == [("blocker", why)]
    assert audit(client, tender_id) == [f"BOQ item C.3: {why}"]  # approved, but it holds the release

    with client.app.state.sessions() as session:
        row = "A4=C.3 | B4=Precast kerbs | C4=m | D4=150"
        again = boq.ItemIn(item="C.3", description="Precast kerbs", unit="m", quantity=150, document_id=bill["id"],
                           page=1, quote=row)  # fmt: skip
        report = boq.propose_items(session, tender_id, session.get(Staff, omar_id), [again])
        session.commit()
        assert report == (
            "Saved 0 BOQ items for the Tender Manager's review. Revised 1 from the newer copy of their document, for "
            "his review."
        )
        kerbs = session.get(BoqItem, ids["C.3"])  # the same line, so its rate stays with it
        assert (kerbs.status, kerbs.quantity, kerbs.document_id) == ("proposed", 150, bill["id"])
        was = "“A3=C.3 | B3=Precast kerbs | C3=m | D3=120”"
        assert kerbs.reason == f"Entered again from the newer copy of Bill.xlsx. The older copy said: {was}"
        assert estimate.current_rate(session, kerbs.id).unit_rate == 25
        assert not revisions.news(session, tender_id, since=None)
    assert findings(client, "boq", ids["C.3"]) == [] and audit(client, tender_id) == []


def test_a_revised_drawing_keeps_the_measurements_where_it_is_unchanged(client, tender, monkeypatch):
    tender_id, omar_id, docs = tender
    drawing = docs["A-102.pdf"]
    body = {"document_id": drawing, "page": 1, "line": [[85.04, 141.73], [481.89, 141.73]], "length_m": 40}
    assert client.post(f"/tenders/{tender_id}/scales", json={**body, "dimension": "40.00"}).status_code == 201
    with client.app.state.sessions() as session:
        yard = takeoff.measure(session, tender_id, omar_id, drawing, 1, "area", "Yard", YARD, "m2", None, "C.2")
        kerb = takeoff.measure(session, tender_id, omar_id, drawing, 1, "length", "Kerb", KERB, "m", None, "C.3")
        takeoff.approve(session, yard)
        takeoff.approve(session, kerb)
        session.commit()
        yard_id, kerb_id = yard.id, kerb.id

    revised = PLAN + b"\n% Addendum 1: a kerb moved\n"
    drawn = readers.vector_points

    def kerb_moved(path, number, limit=2_000_000):  # the revision redraws the drawing around the kerb only
        points = drawn(path, number, limit)
        return points + ((200.0, 505.0),) if path.stem == hashlib.sha256(revised).hexdigest() else points

    monkeypatch.setattr(readers, "vector_points", kerb_moved)
    upload(client, tender_id, {"A-102.pdf": revised})
    plan = newer(client, tender_id, "A-102.pdf")

    with client.app.state.sessions() as session:
        yard, kerb = session.get(Measurement, yard_id), session.get(Measurement, kerb_id)
        assert yard.document_id == plan["id"] and takeoff.quantity(session, yard) == Decimal("800.020")
        assert takeoff.scale_for(session, plan["id"], 1) is not None  # the scale line is untouched too
        assert kerb.document_id == drawing
    why = "The drawing around it has changed in the newer copy of A-102.pdf, page 1: do it again on the newer copy."
    assert [f for f in findings(client, "measurement", kerb_id) if f[0] == "blocker"] == [("blocker", why)]

    with client.app.state.sessions() as session:  # measured again on the newer copy, it replaces the older measurement
        takeoff.measure(session, tender_id, omar_id, plan["id"], 1, "length", "Kerb", KERB, "m", None, "C.3")
        session.commit()
        assert session.get(Measurement, kerb_id).status == "replaced"
        assert not revisions.stale(session, tender_id)


def test_a_fact_and_a_checklist_item_are_done_again_from_the_newer_conditions(client, tender):
    tender_id, omar_id, docs = tender
    with client.app.state.sessions() as session:
        omar = session.get(Staff, omar_id)
        vat = boq.propose_fact(session, tender_id, omar, "vat", "15%", docs["Conditions.pdf"], 1, "VAT at fifteen")
        boq.approve(session, vat)
        clause = "12.3 The tenderer shall submit a work programme."
        programme = submission.RequirementIn(section="Technical", title="Work programme",
                                             document_id=docs["Conditions.pdf"], page=1, quote=clause)  # fmt: skip
        submission.add_requirements(session, tender_id, omar_id, [programme])
        session.commit()
        vat_id = vat.id
    upload(client, tender_id, {"Conditions.pdf": REVISED_CONDITIONS})
    conditions = newer(client, tender_id, "Conditions.pdf")
    assert conditions["note"] == "It replaces an older copy; none of the 2 pieces of work citing it is unchanged here."
    assert audit(client, tender_id) == [
        "The vat: The newer copy of Conditions.pdf doesn't say what it cites (“VAT at fifteen”, page 1 of the older "
        "copy): do it again from the newer copy.",
        "The checklist item “Work programme”: The newer copy of Conditions.pdf doesn't say what it cites (“12.3 The "
        "tenderer shall submit a work programme.”, page 1 of the older copy): do it again from the newer copy.",
    ]

    with client.app.state.sessions() as session:
        omar = session.get(Staff, omar_id)
        # the engineer approved 15%, but that no longer stands: a fact from the newer copy may replace it
        five = boq.propose_fact(session, tender_id, omar, "vat", "5%", conditions["id"], 1, "VAT at five percent")
        boq.approve(session, five)
        programme = programme.model_copy(update={"document_id": conditions["id"], "quote": "12.4 The tenderer shall"})
        assert submission.add_requirements(session, tender_id, omar_id, [programme]).startswith("1 requirements")
        session.commit()
        assert session.get(Fact, vat_id).status == "replaced"
        [requirement] = session.query(Requirement).filter_by(tender_id=tender_id).all()
        assert (requirement.document_id, requirement.reviewed_by) == (conditions["id"], None)  # back to the Manager
        assert not revisions.stale(session, tender_id)
    assert audit(client, tender_id) == []
