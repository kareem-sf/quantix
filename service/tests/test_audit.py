"""The tender audit before release: what the tender still lacks and every open finding on the office's work."""

import io
from datetime import date
from decimal import Decimal

import openpyxl
import pytest
from pydantic_ai.messages import ModelResponse, TextPart, ToolCallPart, ToolReturnPart
from test_documents import make_pdf, read_all, upload
from test_office import scripted, wait_for

from quantix import settings
from quantix.boq import records as boq
from quantix.estimate import records as estimate
from quantix.estimate.models import LibraryResource
from quantix.office import records as office
from quantix.office.models import ENGINEER
from quantix.review import audit
from quantix.review import records as reviews
from quantix.submission import records as submission

CONDITIONS = make_pdf(
    [
        [
            "Prices shall be in Saudi Riyals (SAR)",
            "VAT at 15% shall be shown separately",
            "Measured in accordance with POMI",
            "7.9 The tenderer shall submit a programme.",
        ]
    ]
)
NOTE = "Own rate from crew outputs and current prices."


def bill() -> bytes:
    workbook = openpyxl.Workbook()
    for row in (
        ["Item", "Description", "Unit", "Qty"],
        ["3.1", "Excavation", "m3", 1240],
        ["4.3", "Slab reinforcement", "t", 28.1],
        ["6.3", "Waterproofing", "m2", 980],
    ):
        workbook.active.append(row)
    buffer = io.BytesIO()
    workbook.save(buffer)
    return buffer.getvalue()


@pytest.fixture
def tender(client):
    """Priced and nearly ready: the office left the waterproofing row out and a date unfilled in the programme."""
    tender_id = client.post("/tenders", json={"name": "Synthetic school"}).json()["id"]
    upload(client, tender_id, {"Bill.xlsx": bill(), "Conditions.pdf": CONDITIONS, "Site.kmz": b"PK not a drawing"})
    docs = {name: d["id"] for name, d in read_all(client, tender_id).items()}
    with client.app.state.sessions() as session:
        rania = office.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)
        layla = office.hire(session, tender_id, "Layla Nasser", "Estimator", {})
        rows = [("3.1", "Excavation", "m3", "1240"), ("4.3", "Slab reinforcement", "t", "28.1")]
        lines = [
            boq.ItemIn(item=i, description=d, unit=u, quantity=Decimal(q), document_id=docs["Bill.xlsx"], page=1,
                       quote=f"A{n}={i} | B{n}={d} | C{n}={u} | D{n}={q}")
            for n, (i, d, u, q) in enumerate(rows, start=2)
        ]  # fmt: skip
        boq.propose_items(session, tender_id, layla, lines)
        for item in boq.items(session, tender_id):
            boq.approve(session, item)
        for kind, value, quote in (
            ("currency", "SAR", "Saudi Riyals (SAR)"),
            ("vat", "15%", "VAT at 15%"),
            ("method_of_measurement", "POMI", "in accordance with POMI"),
        ):
            boq.approve(
                session, boq.propose_fact(session, tender_id, layla, kind, value, docs["Conditions.pdf"], 1, quote)
            )
        for name, unit, rate in (("Excavation", "m3", "10"), ("Slab reinforcement", "t", "2000")):
            session.add(LibraryResource(kind="unit_rate", name=name, unit=unit, rate=Decimal(rate), currency="SAR",
                                        source="Depot tender", dated=date(2026, 6, 1)))  # fmt: skip
        excavation = estimate.propose_rate(session, tender_id, layla.id, "3.1", "estimate", NOTE, unit_rate=Decimal(25))
        estimate.approve(session, excavation)  # the engineer approved it: its warning is settled
        rebar = estimate.propose_rate(session, tender_id, layla.id, "4.3", "estimate", NOTE, unit_rate=Decimal(3488))
        estimate.approve(session, rebar, "office_approved")  # the office approved it: its warning stays open
        zero = Decimal(0)
        site = estimate.PreliminaryIn(item="Site costs", quantity=1, unit="sum", rate=Decimal("12000"))
        estimate.approve(
            session, estimate.propose_markups(session, tender_id, layla.id, [site], zero, zero, zero, "Site")
        )
        wanted = submission.RequirementIn(
            section="Technical", title="Programme", document_id=docs["Conditions.pdf"], page=1, quote="7.9 The tenderer"
        )
        submission.add_requirements(session, tender_id, ENGINEER, [wanted])
        programme = submission.find_requirement(session, tender_id, "Programme")
        submission.draft(session, programme, layla.id, "Programme", "Mobilise on [start date].", status="approved")
        session.commit()
        return tender_id, rania.id, docs


def test_the_audit_finds_what_keeps_the_tender_from_release(client, tender):
    tender_id, rania_id, docs = tender
    with client.app.state.sessions() as session:
        found = audit.open_findings(session, client.app.state.home, tender_id)
    assert [(f.severity, f.message) for f in found] == [
        (
            "blocker",
            "A row of the client's BOQ has a quantity but isn't in the BOQ, so it would go back unpriced: row 4 (6.3 · "
            "Waterproofing: 980 m2)",
        ),
        ("blocker", "The draft “Programme”: It still has text to fill in: “[start date]”."),  # approved, but blocked
        (
            "warning",
            "Quantix couldn't read 1 document, so the office hasn't seen what they say: Site.kmz (This type of file "
            "isn't read.)",
        ),
        (
            "warning",
            "The rate for BOQ item 4.3: 3,488.00 is +74% from the firm's own rates for items like it (middle "
            "2,000.00): 2,000.00 library: Slab reinforcement (Depot tender, 01 Jun 2026)",
        ),
    ]
    assert [(r.label, r.document_id) for r in found[0].refs] == [("Bill.xlsx, page 1", docs["Bill.xlsx"])]

    built = client.post(f"/tenders/{tender_id}/export", json={"spread_markups": False}).json()
    assert built["not_ready"] == [found[0].message, found[1].message]  # the engineer sees the blockers


def test_the_manager_accepts_a_warning_with_his_reason_and_the_engineer_sees_it(client, tender):
    tender_id, rania_id, _ = tender
    reason = "The KMZ only shows the site's location; the drawings cover the works."
    with client.app.state.sessions() as session:
        rania = office.manager(session, tender_id)
        [kmz] = [f for f in audit.open_findings(session, client.app.state.home, tender_id) if "Site.kmz" in f.message]
        refused = audit.accept(
            session,
            client.app.state.home,
            tender_id,
            rania,
            [
                audit.Accepted(finding="abc123", reason=reason),
                audit.Accepted(finding=audit.short(kmz.key), reason="ok"),
            ],
        )
        assert refused[0].startswith(  # the refusal names the warnings he can accept
            f"abc123: no open warning has that name. The open warnings are: {audit.short(kmz.key)} (Quantix couldn't"
        )
        assert refused[1] == f"{audit.short(kmz.key)}: say why it needs no correction"
        assert (  # named as the model tends to: the report's label, or the file it is about
            audit.accept(
                session,
                client.app.state.home,
                tender_id,
                rania,
                [audit.Accepted(finding=f"WARNING, short name {audit.short(kmz.key)}", reason=reason)],
            )
            == []
        )
        session.commit()
        still_open = audit.open_findings(session, client.app.state.home, tender_id)
        assert not any("Site.kmz" in f.message for f in still_open)
        assert audit.report(still_open).startswith("The audit found 2 blockers and 1 warning:\n- BLOCKER: ")

    shown = client.get(f"/tenders/{tender_id}/audit").json()
    [accepted] = [f for f in shown if f["reason"]]
    assert (accepted["severity"], accepted["accepted_by"], accepted["reason"]) == ("warning", "Rania", reason)


def test_the_manager_audits_through_his_tool(client, tender, tmp_path):
    tender_id, _, _ = tender
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})
    returned: list[str] = []

    def brain(messages, info):
        if "You are Rania Farouk" not in info.instructions:
            return ModelResponse(parts=[TextPart("Done.")])
        returned[:] = [str(p.content) for m in messages for p in m.parts if isinstance(p, ToolReturnPart)]
        return ModelResponse(parts=[TextPart("Done.")] if returned else [ToolCallPart("audit_tender", {})])

    client.app.state.office.model = lambda: scripted(brain)
    with client.app.state.sessions() as session:
        office.post(session, tender_id, ENGINEER, office.manager(session, tender_id).id, "Is the tender ready?")
        session.commit()
    client.app.state.office.engineer_spoke(tender_id)
    wait_for(lambda o: returned, client, tender_id)
    assert returned[0].startswith("The audit found 2 blockers and 2 warnings:\n- BLOCKER: A row of the client's BOQ")
    assert returned[0].endswith(
        'Accept a warning only with your reason, as accept_warnings=[{"finding": "<short name>", "reason": "why it '
        'needs no correction"}].'
    )


def test_markups_approved_before_the_programme_are_checked_against_it(client, tender):
    """The engineer's approval settles the warnings they saw. Markups priced for longer than a programme approved
    after them is news, so the audit shows it until the markups are approved again, or redone."""
    tender_id, _, docs = tender
    with client.app.state.sessions() as session:
        layla = next(m for m in office.team(session, tender_id) if m.first_name == "Layla")
        earlier = estimate.current_markups(session, tender_id)
        reviews.reopen(session, "markups", earlier.id, "Price the site staff by the month.")
        zero = Decimal(0)
        staff = [
            estimate.PreliminaryIn(item=item, quantity=4, unit="month", rate=Decimal("9000"))
            for item in ("Site engineer", "Foreman")
        ]
        markups = estimate.propose_markups(session, tender_id, layla.id, staff, zero, zero, zero, "Four months.")
        estimate.approve(session, markups)
        quote = "7.9 The tenderer shall submit a programme."
        wanted = submission.RequirementIn(
            section="Technical", title="Work schedule", document_id=docs["Conditions.pdf"], page=1, quote=quote
        )
        submission.add_requirements(session, tender_id, ENGINEER, [wanted])
        rows = submission.durations(session, tender_id, [submission.ActivityIn(boq_item="3.1", output=31, crews=1)])
        requirement = submission.find_requirement(session, tender_id, "Work schedule")
        record = submission.schedule_record(rows, 40)
        submission.draft(session, requirement, layla.id, "Work schedule", "Excavate first.", "approved", record)
        session.commit()

    def audited() -> list[str]:
        with client.app.state.sessions() as session:
            return [f.message for f in audit.open_findings(session, client.app.state.home, tender_id)]

    news = (
        "The markups: Site engineer and Foreman: 4 month is about 104 working days at 26 a month; the work schedule "
        "is 40 working days."
    )
    assert news in audited()  # one finding for both, not one each
    with client.app.state.sessions() as session:
        estimate.approve(session, session.get(type(markups), markups.id))  # approved again, with the programme known
        session.commit()
    assert news not in audited()
