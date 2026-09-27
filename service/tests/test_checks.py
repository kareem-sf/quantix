"""Quantix checks what the office proposes before the Tender Manager can accept it: he can't accept a blocker, and
accepts a warning only with his reason."""

import io
from datetime import date
from decimal import Decimal
from pathlib import Path

import openpyxl
import pytest
from test_documents import make_pdf, read_all, upload

from quantix.boq import records as boq
from quantix.estimate import records as estimate
from quantix.estimate.models import LibraryResource
from quantix.office import records as office
from quantix.office.models import ENGINEER, Staff
from quantix.review import records as reviews
from quantix.submission import records as submission
from quantix.takeoff import records as takeoff

PLAN = (Path(__file__).parent / "fixtures" / "synthetic-plan.pdf").read_bytes()  # a 40 × 20 m yard, 800 m²
CONDITIONS = make_pdf([["VAT at fifteen percent", "12.3 The tenderer shall submit a work programme."]])
ROWS = [
    ("8485 · Earthwork", "C.1", "Excavation in all soils", "m3", "1000"),
    ("8485 · Earthwork", "C.2", "Asphalt wearing course", "m2", "800"),
    ("8486 · Earthwork", "C.1", "Excavation in all soils", "m3", "600"),
]
NOTE = "Own rate from crew outputs and current prices."
YARD = [[85.04, 170.08], [481.89, 170.08], [481.89, 368.51], [85.04, 368.51]]  # the drawing's own corners


def bill() -> bytes:
    workbook = openpyxl.Workbook()
    for _, item, description, unit, quantity in ROWS:
        workbook.active.append([item, description, unit, int(quantity)])
    buffer = io.BytesIO()
    workbook.save(buffer)
    return buffer.getvalue()


@pytest.fixture
def tender(client):
    """Two substations' bills, approved by the engineer. Omar works for Rania, the Tender Manager."""
    tender_id = client.post("/tenders", json={"name": "Synthetic substations"}).json()["id"]
    upload(client, tender_id, {"Bill.xlsx": bill(), "A-102.pdf": PLAN, "Conditions.pdf": CONDITIONS})
    docs = {name: d["id"] for name, d in read_all(client, tender_id).items()}
    with client.app.state.sessions() as session:
        office.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)
        omar = office.hire(session, tender_id, "Omar Haddad", "Quantity Surveyor", {})
        lines = [
            boq.ItemIn(section=s, item=i, description=d, unit=u, quantity=Decimal(q), document_id=docs["Bill.xlsx"],
                       page=1, quote=f"A{n}={i} | B{n}={d} | C{n}={u} | D{n}={q}")
            for n, (s, i, d, u, q) in enumerate(ROWS, start=1)
        ]  # fmt: skip
        boq.propose_items(session, tender_id, omar, lines)
        for item in boq.items(session, tender_id):
            boq.approve(session, item)
        session.commit()
        return tender_id, omar.id, docs


def decide(client, tender_id, record_id, accept=True, note="Checked it against its source.", reason=None) -> str:
    """The Manager's verdict on one record in his queue."""
    with client.app.state.sessions() as session:
        [ref] = [p.ref for p in reviews.pending(session, tender_id) if p.record.id == record_id]
        verdict = reviews.Verdict(record=ref, accept=accept, note=note, warnings_reason=reason)
        manager = office.manager(session, tender_id)
        report = reviews.review(session, client.app.state.home, tender_id, manager, [verdict], autonomous=False)
        session.commit()
        return report


def findings(client, kind, record_id) -> list[tuple[str, str]]:
    return [(f["severity"], f["message"]) for f in client.get(f"/records/{kind}/{record_id}/findings").json()]


def test_a_rate_is_checked_against_the_same_work_and_the_firms_own_rates(client, tender):
    tender_id, omar, _ = tender
    with client.app.state.sessions() as session:
        session.add(
            LibraryResource(kind="unit_rate", name="Excavation in all soils", unit="m3", rate=Decimal("24"),
                            currency="SAR", source="Riyadh depot tender", dated=date(2026, 5, 1))
        )  # fmt: skip
        first = estimate.propose_rate(
            session, tender_id, omar, "8485 · Earthwork / C.1", "estimate", NOTE, unit_rate=Decimal("25")
        )
        second = estimate.propose_rate(
            session, tender_id, omar, "8486 · Earthwork / C.1", "estimate", NOTE, unit_rate=Decimal("32")
        )
        session.commit()
        first_id, second_id = first.id, second.id

    same_work = "8485 · Earthwork / C.1 is the same item at 25.00 per m3; 8486 · Earthwork / C.1 is 32.00."
    assert findings(client, "rate", second_id) == [
        ("warning", f"{same_work} The same work normally carries one rate."),
        (
            "warning",
            "32.00 is +33% from the firm's own rates for items like it (middle 24.00): 24.00 library: Excavation in "
            "all soils (Riyadh depot tender, 01 May 2026)",
        ),
    ]
    assert len(findings(client, "rate", first_id)) == 1  # 25.00 is within 30% of the library's 24.00

    refused = decide(client, tender_id, second_id)
    assert refused.startswith("Accepted 0 (waiting for the engineer). Sent back 0.\nNot done:\nrate ")
    assert f"Quantix warns: {same_work}" in refused
    assert "Accept it with warnings_reason saying why each is acceptable" in refused

    reason = "The 8486 yard is rock: the geotechnical report prices it higher."
    assert decide(client, tender_id, second_id, reason=reason) == "Accepted 1 (waiting for the engineer). Sent back 0."
    [shown, _] = client.get(f"/records/rate/{second_id}/findings").json()
    assert (shown["accepted_by"], shown["reason"]) == ("Rania", reason)  # the engineer sees why he accepted it


def test_a_blocker_stops_the_manager_accepting(client, tender):
    tender_id, omar, _ = tender
    with client.app.state.sessions() as session:
        entry = LibraryResource(kind="unit_rate", name="Asphalt wearing course 50 mm", unit="m3", rate=Decimal("40"),
                                currency="SAR", source="Jeddah tender", dated=date(2026, 3, 1))  # fmt: skip
        session.add(entry)
        session.flush()
        asphalt = estimate.propose_rate(
            session,
            tender_id,
            omar,
            "8485 · Earthwork / C.2",
            "library",
            "",
            unit_rate=Decimal(40),
            library_id=entry.id,
        )
        fill = estimate.LineIn(kind="material", resource="Fill", quantity=1, unit="m3", rate=20, wastage=Decimal("0.8"))
        excavation = estimate.propose_rate(
            session, tender_id, omar, "8486 · Earthwork / C.1", "estimate", NOTE, lines=[fill]
        )
        session.commit()
        asphalt_id, excavation_id = asphalt.id, excavation.id

    blocker = "The library entry is per m3; 8485 · Earthwork / C.2 is measured per m2."
    assert findings(client, "rate", asphalt_id) == [("blocker", blocker)]
    assert findings(client, "rate", excavation_id) == [("warning", "Fill carries 80% wastage.")]
    refused = decide(client, tender_id, asphalt_id, reason="The library rate is close enough.")
    assert refused.endswith(f"Quantix's checks stop it: {blocker} Send it back saying exactly what to correct.")
    assert decide(client, tender_id, asphalt_id, accept=False, note="Use a rate per m2.").endswith("Sent back 1.")


def test_measuring_the_same_area_twice_is_a_blocker(client, tender):
    tender_id, omar, docs = tender
    drawing = docs["A-102.pdf"]
    body = {"document_id": drawing, "page": 1, "line": [[85.04, 141.73], [481.89, 141.73]], "length_m": 40}
    assert client.post(f"/tenders/{tender_id}/scales", json={**body, "dimension": "40.00"}).status_code == 201
    with client.app.state.sessions() as session:

        def area(label, points, item="8485 · Earthwork / C.2"):
            return takeoff.measure(session, tender_id, omar, drawing, 1, "area", label, points, "m2", None, item).id

        yard, again = area("Asphalt yard", YARD), area("Asphalt yard again", YARD)
        loose = area("Asphalt patch", [[200, 200], [300, 200], [300, 300], [200, 300]], item=None)
        session.commit()

    assert findings(client, "measurement", again) == [
        (
            "blocker",
            "It covers the same area as “Asphalt yard”, already measured for 8485 · Earthwork / C.2: measuring it "
            "again counts it twice.",
        ),
        ("warning", "The drawings measure 1,600.04 m2 for 8485 · Earthwork / C.2, +100.0% from the BOQ's 800."),
    ]
    assert findings(client, "measurement", loose) == [
        ("warning", "None of its points is on a line of the drawing: check them against the sheet.")
    ]
    assert "Quantix's checks stop it: It covers the same area" in decide(client, tender_id, again)
    decide(client, tender_id, again, accept=False, note="The yard is already measured; remove this one.")
    assert findings(client, "measurement", yard) == []  # 800.02 m2 against the BOQ's 800
    assert decide(client, tender_id, yard) == "Accepted 1 (waiting for the engineer). Sent back 0."


def test_a_fact_must_say_what_its_clause_says(client, tender):
    tender_id, omar, docs = tender
    with client.app.state.sessions() as session:
        fact = boq.propose_fact(
            session, tender_id, session.get(Staff, omar), "vat", "15%", docs["Conditions.pdf"], 1, "VAT at fifteen"
        )
        session.commit()
        fact_id = fact.id
    assert findings(client, "fact", fact_id) == [
        ("warning", "VAT: the value gives 15, which the quoted clause doesn't say.")
    ]


def test_the_schedule_and_the_markups_are_checked_against_each_other(client, tender):
    tender_id, omar, docs = tender
    activity = submission.ActivityIn
    with client.app.state.sessions() as session:
        for item, rate in (
            ("8485 · Earthwork / C.1", 25),
            ("8485 · Earthwork / C.2", 40),
            ("8486 · Earthwork / C.1", 25),
        ):
            estimate.propose_rate(
                session, tender_id, ENGINEER, item, "estimate", NOTE, unit_rate=Decimal(rate), status="approved"
            )  # net 25,000 + 32,000 + 15,000 = 72,000
        quote = "12.3 The tenderer shall submit a work programme."
        wanted = submission.RequirementIn(
            section="Technical", title="Work programme", document_id=docs["Conditions.pdf"], page=1, quote=quote
        )
        submission.add_requirements(session, tender_id, omar, [wanted])
        requirement = submission.find_requirement(session, tender_id, "Work programme")
        rows = submission.durations(
            session,
            tender_id,
            [
                activity(boq_item="8485 · Earthwork / C.1", output=100, crews=2),
                activity(boq_item="C.2", output=400, crews=1),
            ],
        )  # 5 days and 2 days; the 8486 excavation is left out

        def programme(text, overall):
            record = submission.schedule_record(rows, overall)
            return submission.draft(session, requirement, omar, "Programme", text, schedule=record).id

        short = programme("Excavation from [start date], then asphalt.", 4)
        session.commit()
    left_out = "1 BOQ line with a quantity is not in the schedule: 8486 · Earthwork / C.1"
    assert findings(client, "draft", short) == [
        ("blocker", "It still has text to fill in: “[start date]”."),
        ("warning", left_out),
        ("blocker", "The overall duration, 4 working days, is shorter than its longest line (5 days)."),
    ]

    with client.app.state.sessions() as session:
        long = programme("Excavation, then asphalt, with the client's shutdowns.", 104)
        site = estimate.PreliminaryIn(item="Site engineer", quantity=2, unit="month", rate=10000)
        zero = Decimal(0)
        markups = estimate.propose_markups(session, tender_id, omar, [site], zero, zero, zero, "Site staff.")
        session.commit()
        markups_id = markups.id
    assert findings(client, "draft", long) == [
        ("warning", left_out),
        (
            "warning",
            "The overall duration, 104 working days, is longer than every line run one after another (7 days).",
        ),
    ]
    assert findings(client, "markups", markups_id) == [
        (
            "warning",
            "Preliminaries are 20,000.00, 27.8% of the net cost of 72,000.00; the office usually sees 5% to 15%.",
        ),
        (
            "warning",
            "Site engineer: 2 month is about 52 working days at 26 a month; the work schedule is 104 working days.",
        ),
    ]
