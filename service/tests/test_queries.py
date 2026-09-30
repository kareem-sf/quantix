"""Tender queries: what the office finds that the client must answer or the bid must allow for. Each shows where it
is, its figures come from its sources or Quantix's own takeoff, the Tender Manager reviews it and the engineer
decides whether it goes to the client."""

import re
from decimal import Decimal

import pytest
from pydantic_ai import ModelRetry
from test_documents import make_pdf, read_all, upload
from test_lookup import fake_turn
from test_revisions import BILL, PLAN, YARD, workbook

from quantix.boq import records as boq
from quantix.office import records as office
from quantix.office import tools
from quantix.office.models import Task
from quantix.review import lookup, queries
from quantix.review import records as reviews
from quantix.takeoff import records as takeoff

SPEC = make_pdf([["5.2 Thermoplastic road markings to the yard, 450 m", "5.3 Kerbs shall be precast concrete"]])
CONDITIONS = make_pdf(
    [["2.1 The conditions govern, then the specification, then the drawings", "5.3 Kerbs cast in situ"]]
)
MARKINGS = "Thermoplastic road markings to the yard, 450 m"
WORDING = "The specification calls for road markings to the yard, but no BOQ item covers them. Please add an item."


@pytest.fixture
def tender(client):
    """A depot's bill, its drawing at the engineer's scale, the specification and the conditions with their order of
    precedence, all approved. Omar raises queries for Rania, the Tender Manager."""
    tender_id = client.post("/tenders", json={"name": "Synthetic depot"}).json()["id"]
    files = {"Bill.xlsx": workbook(BILL), "A-102.pdf": PLAN, "Spec.pdf": SPEC, "Conditions.pdf": CONDITIONS}
    upload(client, tender_id, files)
    docs = {name: d["id"] for name, d in read_all(client, tender_id).items()}
    body = {"document_id": docs["A-102.pdf"], "page": 1, "line": [[85.04, 141.73], [481.89, 141.73]], "length_m": 40}
    assert client.post(f"/tenders/{tender_id}/scales", json={**body, "dimension": "40.00"}).status_code == 201
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
        order = "The conditions govern, then the specification"
        precedence = boq.propose_fact(
            session, tender_id, omar, "precedence", "Conditions, specification, drawings", docs["Conditions.pdf"], 1,
            order
        )  # fmt: skip
        boq.approve(session, precedence)
        session.commit()
        return tender_id, omar.id, docs


def raise_query(client, tender_id, by, kind, title, wording, sources, detail="Found while measuring.", **more) -> str:
    with client.app.state.sessions() as session:
        home = client.app.state.home
        query = queries.raise_query(session, home, tender_id, by, kind, title, detail, wording, sources, **more)
        session.commit()
        return query.id


def findings(client, record_id) -> list[tuple[str, str]]:
    return [(f["severity"], f["message"]) for f in client.get(f"/records/query/{record_id}/findings").json()]


def decide(client, tender_id, record_id, accept=True, note="Checked it against its sources.") -> str:
    with client.app.state.sessions() as session:
        [ref] = [p.ref for p in reviews.pending(session, tender_id) if p.record.id == record_id]
        verdict = reviews.Verdict(record=ref, accept=accept, note=note)
        manager = office.manager(session, tender_id)
        report = reviews.review(session, client.app.state.home, tender_id, manager, [verdict], autonomous=False)
        session.commit()
        return report


def test_a_query_says_what_it_is_about_and_where_it_shows(client, tender):
    tender_id, omar, docs = tender
    markings = queries.QuerySource(document_id=docs["Spec.pdf"], page=1, quote=MARKINGS)
    refused = [
        ({"kind": "rfi"}, "A query is one of: missing, conflict, boq, clarification."),
        ({"title": "Markings"}, "Give the query a title of a few words that says what it is about."),
        ({"wording": "Please add markings."}, "Draft the query as the client will read it"),
        ({"sources": []}, "Show where it is: at least one page of a document, or the BOQ line."),
        ({"kind": "conflict"}, "Documents that disagree need both sources: the page of each."),
        ({"boq_item": "C.2"}, "Work missing from the BOQ has no BOQ line: leave boq_item out, or make it a boq query."),
        ({"kind": "boq", "boq_item": "C.9"}, "There is no BOQ item C.9."),
        (
            {"sources": [markings.model_copy(update={"document_id": "0" * 32})]},
            "A source's document id isn't one of this tender's documents: use list_documents.",
        ),
        ({"sources": [markings.model_copy(update={"page": 3})]}, "Spec.pdf has no page 3."),
        (
            {"sources": [markings.model_copy(update={"quote": None})]},
            "For Spec.pdf, page 1: quote its words, or give the drawing's objects.",
        ),
        (
            {"sources": [markings.model_copy(update={"quote": "Road markings to the car park"})]},
            "“Road markings to the car park” is not on Spec.pdf, page 1.",
        ),
        (
            {"kind": "boq", "boq_item": "C.2", "measurements": ["measurement 1234abcd"]},
            "No live measurement is 'measurement 1234abcd': give it as takeoff_summary or open_record names it.",
        ),
    ]
    with client.app.state.sessions() as session:
        for change, message in refused:
            asked = {"kind": "missing", "title": "Road markings to the yard", "detail": "Specified, not billed."}
            asked |= {"wording": WORDING, "sources": [markings], **change}
            with pytest.raises(ValueError, match=re.escape(message)):
                queries.raise_query(session, client.app.state.home, tender_id, omar, **asked)
        assert queries.queries(session, tender_id) == []


def test_staff_raise_a_query_from_the_pages_they_read(client, tender):
    tender_id, omar, docs = tender
    spec = docs["Spec.pdf"]
    ctx = fake_turn(client, tender_id, omar)
    detail = "The specification has 450 m of road markings; no BOQ line covers them."
    asked = {"kind": "missing", "title": "Road markings to the yard", "detail": detail, "wording": WORDING}
    asked["sources"] = [queries.QuerySource(document_id=spec, page=1, quote=MARKINGS)]
    with pytest.raises(ModelRetry, match="You cite Spec.pdf, page 1, but you haven't opened it"):
        tools.raise_query(ctx, **asked)
    tools.read_page(ctx, spec, 1)
    filed = tools.raise_query(ctx, **asked)
    assert re.fullmatch(r"Query \w{8} filed for the Tender Manager's review\.", filed)

    [waiting] = client.get(f"/tenders/{tender_id}/review").json()
    assert (waiting["kind"], waiting["line"]) == (
        "query",
        f"{waiting['ref']} · by Omar · Missing from the BOQ: Road markings to the yard",
    )
    assert findings(client, waiting["record_id"]) == []  # 450 m is on the page it cites
    with client.app.state.sessions() as session:
        shown = reviews.details(session, reviews.find(session, tender_id, waiting["ref"]))
    assert shown == (
        f"Missing from the BOQ: Road markings to the yard\n{detail}\nSpec.pdf, page 1: “{MARKINGS}”\nDraft to the "
        f"client: {WORDING}"
    )
    [listed] = client.get(f"/tenders/{tender_id}/queries").json()
    assert (listed["kind_label"], listed["status"], listed["boq_item"], listed["figures"]) == (
        "Missing from the BOQ",
        "proposed",
        None,
        [],
    )
    assert [(s["document_name"], s["page"], s["quote"]) for s in listed["sources"]] == [("Spec.pdf", 1, MARKINGS)]


def yard_query(client, tender) -> str:
    """A query on the asphalt line, resting on the yard Omar measured for it."""
    tender_id, omar, docs = tender
    with client.app.state.sessions() as session:
        yard = takeoff.measure(session, tender_id, omar, docs["A-102.pdf"], 1, "area", "Yard", YARD, "m2", None, "C.2")
        session.commit()
        measured = f"measurement {yard.id[:8]}"
    wording = "The yard is drawn at 800.02 m2 and billed at 800 m2; we believe it should be 812 m2. Please confirm."
    title = "Asphalt area of the yard"
    return raise_query(client, tender_id, omar, "boq", title, wording, [], boq_item="C.2", measurements=[measured])


def test_quantix_checks_a_querys_figures_and_what_it_takes_off(client, tender):
    tender_id = tender[0]
    record = yard_query(client, tender)
    [figures, matches] = findings(client, record)
    assert figures == (
        "warning",
        "The query gives 812, which neither its sources nor Quantix's takeoff give. Take figures from the documents, "
        "or measure them.",
    )  # 800.02 is Quantix's takeoff and 800 the bill's
    assert matches[0] == "warning" and matches[1].startswith("The takeoff matches the BOQ's 800")
    [listed] = client.get(f"/tenders/{tender_id}/queries").json()
    assert (listed["boq_item"], listed["figures"]) == ("C.2", ["Yard: 800.020 m2"])
    with client.app.state.sessions() as session:
        shown = reviews.details(session, reviews.find(session, tender_id, f"query {record[:8]}"))
    assert shown.startswith("Error in the BOQ: Asphalt area of the yard\nFound while measuring.\nBOQ line C.2: ")
    assert "\nQuantix's takeoff: Yard: 800.020 m2\n" in shown


@pytest.mark.xfail(strict=True, reason="Bug: a query prints the bill's quantity as stored, 800.0000 m2")
def test_a_query_gives_the_bills_quantity_as_the_bill_does(client, tender):
    record = yard_query(client, tender)
    assert findings(client, record)[1] == (
        "warning",
        "The takeoff matches the BOQ's 800 m2 within 2%: there may be nothing to query.",
    )
    with client.app.state.sessions() as session:
        shown = reviews.details(session, reviews.find(session, tender[0], f"query {record[:8]}"))
    assert "\nBOQ line C.2: Asphalt wearing course · 800 m2\n" in shown


def test_quantix_asks_which_document_governs_and_whether_the_query_is_raised_already(client, tender):
    tender_id, omar, docs = tender
    kerbs = [
        queries.QuerySource(document_id=docs["Spec.pdf"], page=1, quote="5.3 Kerbs shall be precast concrete"),
        queries.QuerySource(document_id=docs["Conditions.pdf"], page=1, quote="5.3 Kerbs cast in situ"),
    ]
    wording = "Clause 5.3 of the specification asks for precast kerbs, the conditions for kerbs cast in situ. Which?"
    conflict = raise_query(client, tender_id, omar, "conflict", "Precast or in situ kerbs", wording, kerbs)
    assert findings(client, conflict) == [
        ("warning", "Say which document governs under the order of precedence (Conditions, specification, drawings).")
    ]
    governs = "The conditions, by clause 2.1"
    again = raise_query(
        client, tender_id, omar, "conflict", "Kerbs precast or in situ", wording, kerbs, governs=governs
    )
    assert findings(client, again) == [
        ("warning", "It looks like the query “Precast or in situ kerbs” already raised: put both in one query.")
    ]
    edge = "The specification asks for kerbs along the yard edge, which the bill doesn't price. Please add an item."
    missing = raise_query(client, tender_id, omar, "missing", "Precast kerbs to the yard edge", edge, kerbs[:1])
    assert findings(client, missing) == [
        (
            "warning",
            "BOQ line C.3 (“Precast kerbs”) may already cover it: check the line, its preambles and what it deems "
            "included before raising it.",
        )
    ]
    with client.app.state.sessions() as session:
        shown = reviews.details(session, reviews.find(session, tender_id, f"query {again[:8]}"))
    assert shown.endswith(f"Governs: {governs}\nDraft to the client: {wording}")
    assert "\nConditions.pdf, page 1: “5.3 Kerbs cast in situ”\n" in shown


def test_the_engineer_decides_whether_a_query_goes_to_the_client(client, tender):
    tender_id, omar, docs = tender
    markings = [queries.QuerySource(document_id=docs["Spec.pdf"], page=1, quote=MARKINGS)]
    kerbs = [queries.QuerySource(document_id=docs["Spec.pdf"], page=1, quote="Kerbs shall be precast concrete")]
    wording = "The specification asks for precast kerbs; please confirm the type and size the bill should price."
    to_send = raise_query(client, tender_id, omar, "missing", "Road markings to the yard", WORDING, markings)
    to_redo = raise_query(client, tender_id, omar, "clarification", "Type of precast kerb", wording, kerbs)
    assert decide(client, tender_id, to_send).startswith(
        "Accepted 1 (waiting for the engineer). Sent back 0. Once your review is done, tell the engineer with "
        "message_engineer what now waits for their approval and where: 1 tender query on the Queries screen."
    )
    decide(client, tender_id, to_redo)
    assert client.get(f"/tenders/{tender_id}/gates").json()["drawings"] == 2

    assert client.post(f"/queries/{to_send}/decision", json={"approve": True}).status_code == 200
    again = client.post(f"/queries/{to_send}/decision", json={"approve": False})
    assert (again.status_code, again.json()["detail"]) == (400, "This has already been decided.")
    reason = "Ask for the kerb size too."
    assert client.post(f"/queries/{to_redo}/decision", json={"approve": False, "reason": reason}).status_code == 200
    assert client.post(f"/queries/{'0' * 32}/decision", json={"approve": True}).status_code == 404

    [sent] = client.get(f"/tenders/{tender_id}/queries").json()  # the one sent back is Omar's to raise again
    assert (sent["id"], sent["status"]) == (to_send, "approved")
    assert client.get(f"/tenders/{tender_id}/gates").json()["drawings"] == 0
    with client.app.state.sessions() as session:
        [task] = session.query(Task).filter_by(tender_id=tender_id).all()
        assert (task.staff_id, task.title, task.brief) == (omar, "Redo the query “Type of precast kerb”", reason)
        assert lookup.search(session, tender_id, "markings", "query") == [
            f"query {to_send[:8]} · Missing from the BOQ: Road markings to the yard · approved"
        ]
        assert lookup.search(session, tender_id, "kerb", "query") == []  # sent back: no longer the office's

        redone = raise_query(client, tender_id, omar, "clarification", "Type of precast kerb", wording, kerbs)
        kind, record = lookup.find(session, tender_id, f"query {redone[:8]}")
        opened = lookup.explain(session, client.app.state.home, tender_id, kind, record)
    assert opened.startswith(f"query {redone[:8]} · with the Tender Manager for review\nClarification: Type of")
    assert "\nMade by Omar on " in opened
    history = opened.split("\nOther versions of the same work, newest first:\n")[1]
    assert re.fullmatch(
        rf"- query {to_redo[:8]} · \d\d \w{{3}} \d\d:\d\d · “Type of precast kerb” · sent back: {reason}", history
    )


def test_a_query_on_an_older_copy_is_raised_again_from_the_newer_one(client, tender):
    tender_id, omar, docs = tender
    markings = [queries.QuerySource(document_id=docs["Spec.pdf"], page=1, quote=MARKINGS)]
    record = raise_query(client, tender_id, omar, "missing", "Road markings to the yard", WORDING, markings)
    upload(client, tender_id, {"Spec.pdf": make_pdf([["5.2 Thermoplastic road markings to the yard, 520 m"]])})
    read_all(client, tender_id)
    assert findings(client, record) == [
        (
            "blocker",
            "It cites an older copy of Spec.pdf: check the newer copy still shows it, and raise it again from there.",
        )
    ]
    with client.app.state.sessions() as session:
        with pytest.raises(ValueError, match="A newer copy of Spec.pdf replaced the one cited: cite the newer copy."):
            queries.raise_query(
                session, client.app.state.home, tender_id, omar, "missing", "Road markings to the yard",
                "Specified, not billed.", WORDING, markings
            )  # fmt: skip
