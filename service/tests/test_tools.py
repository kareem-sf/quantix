"""The office's tools as an agent meets them: what each returns, what it records, and what it refuses and why."""

import io
import json
import re
import time
from datetime import date
from decimal import Decimal

import httpx
import openpyxl
import pytest
import test_drawings
import test_estimate
import test_takeoff
import test_vectors
from drawings import MAP, make_drawing
from PIL import Image
from pydantic_ai import ModelRetry, ToolReturn
from pydantic_ai.exceptions import ModelHTTPError
from test_documents import make_pdf, make_xlsx, read_all, upload
from test_lookup import fake_turn, priced_after_a_send_back
from test_office import DONE, call, returns, scripted, turn_records, wait_for

from quantix import settings
from quantix.ai import connections
from quantix.boq import records as boq
from quantix.documents import cad, web
from quantix.documents.models import Document, Page
from quantix.estimate import records as estimate
from quantix.office import agents, packs, runtime, tools
from quantix.office import records as office
from quantix.office.models import ENGINEER, TEAM, Message, Staff, Task
from quantix.review import lookup, queries
from quantix.review import records as reviews
from quantix.review.package import DocumentNote
from quantix.subcontract import records as subcontract
from quantix.submission import records as submission

tender = test_estimate.tender  # the fixture: a school with an approved BOQ and an estimator, Priya
flat = test_drawings.flat  # a two-room flat drawn in CAD, with its bill and a quantity surveyor, Omar
sheet = test_takeoff.sheet  # a PDF floor plan, with its bill and a quantity surveyor, Omar
site = test_vectors.site  # a site plan printed from CAD to PDF, with a quantity surveyor, Omar
ITT = make_pdf(
    [
        ["Instructions to Tenderers", "Tenders are due by 14 October 2026."],
        ["12.3 The tenderer shall submit a work programme."],
        [],  # a scan
    ]
)


def png(width=400, height=200) -> bytes:
    buffer = io.BytesIO()
    Image.new("RGB", (width, height), "white").save(buffer, format="PNG")
    return buffer.getvalue()


@pytest.fixture
def package(client):
    """A tender with its package read, a Tender Manager and a quantity surveyor, Omar."""
    tender_id = client.post("/tenders", json={"name": "Synthetic school"}).json()["id"]
    upload(client, tender_id, {"ITT.pdf": ITT, "Bill.xlsx": make_xlsx(), "Site photo.png": png()})
    documents = {name: d["id"] for name, d in read_all(client, tender_id).items()}
    with client.app.state.sessions() as session:
        rania = office.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True)
        omar = office.hire(session, tender_id, "Omar Haddad", "Quantity Surveyor", {"work": ["documents", "boq"]})
        session.commit()
        return tender_id, documents, rania.id, omar.id


def opened(client, staff_id, kind, ref) -> bool:
    with client.app.state.sessions() as session:
        return office.has_opened(session, staff_id, kind, ref)


def team_room(client, tender_id) -> list[Message]:
    with client.app.state.sessions() as session:
        return office.messages(session, tender_id, TEAM)


def test_a_new_tender_tells_the_office_plainly_there_is_nothing_yet(client):
    tender_id = client.post("/tenders", json={"name": "Synthetic school"}).json()["id"]
    with client.app.state.sessions() as session:
        omar_id = office.hire(session, tender_id, "Omar Haddad", "Quantity Surveyor", {}).id
        session.commit()
    ctx = fake_turn(client, tender_id, omar_id)
    assert tools.list_documents(ctx) == "No documents have been added yet."
    assert tools.coverage(ctx) == "No documents have been added yet."
    assert (
        tools.search_documents(ctx, "tender security")
        == "Nothing found. Try other words, or the Arabic or English term."
    )
    assert tools.list_boq(ctx) == "The BOQ is empty."
    assert tools.find_records(ctx, "excavation") == (
        "No record matches. Try fewer or other words, or the Arabic or English term."
    )
    assert tools.search_conversation(ctx, "bid bond") == "Nothing said or decided has all those words."
    assert tools.list_requirements(ctx) == "The checklist is empty. Add requirements with add_requirements."
    assert tools.find_problems(ctx) == "Quantix's checks find nothing in the BOQ and across the drawings."
    assert tools.search_library(ctx, "ready mix concrete") == (
        "Nothing in the library matches. Price it from a quote, or estimate it and say how."
    )
    assert tools.search_past_tenders(ctx, "excavation") == "No earlier tender has an approved rate for items like that."
    assert (
        tools.search_directory(ctx, "waterproofing")
        == "Nobody in the directory matches. Add a company with add_company."
    )


def test_reading_stops_at_the_last_page_and_before_too_much_text(client, package, monkeypatch):
    tender_id, documents, _, omar_id = package
    ctx = fake_turn(client, tender_id, omar_id)
    itt = documents["ITT.pdf"]
    with pytest.raises(ModelRetry, match="^ITT.pdf has pages 1 to 3.$"):
        tools.read_page(ctx, itt, 4)

    monkeypatch.setattr(tools, "TEXT_AT_ONCE", 20)
    first, stopped = tools.read_page(ctx, itt, 1, last_page=3).split("\n\n")
    assert first.startswith("ITT.pdf, page 1:\nInstructions to Tenderers")
    assert stopped == "(Stopped before page 2: that is a lot of text at once. Read on from there.)"
    assert opened(client, omar_id, "page", f"{itt}:1") and not opened(client, omar_id, "page", f"{itt}:2")


def test_a_scan_is_looked_at_not_read(client, package):
    tender_id, documents, _, omar_id = package
    ctx = fake_turn(client, tender_id, omar_id)
    itt = documents["ITT.pdf"]
    assert (
        tools.read_page(ctx, itt, 3)
        == "ITT.pdf, page 3 is a scan Quantix hasn't read yet. Use view_page to look at it."
    )
    with client.app.state.sessions() as session:  # OCR read it and found no words
        office_page = session.query(Page).filter_by(document_id=itt, number=3).one()
        office_page.ocr = "empty"
        session.commit()
    assert (
        tools.read_page(ctx, itt, 3) == "ITT.pdf, page 3 is a scan with no words to read. Use view_page to look at it."
    )
    assert not opened(client, omar_id, "page", f"{itt}:3")  # nothing read, so nothing to cite

    assert "· 1 scans, 0 read by OCR" in tools.list_documents(ctx)
    with client.app.state.sessions() as session:  # a newer upload the reader is still on
        session.get(Document, itt).status = "reading"
        session.commit()
    [row] = [r for r in tools.list_documents(ctx).splitlines() if r.startswith(itt)]
    assert row == f"{itt} · ITT.pdf · 3 pages · still being read · 1 scans, 0 read by OCR"


def test_a_page_is_looked_at_whole_or_closer(client, package):
    tender_id, documents, _, omar_id = package
    ctx = fake_turn(client, tender_id, omar_id)
    itt, bill, photo = documents["ITT.pdf"], documents["Bill.xlsx"], documents["Site photo.png"]

    whole = tools.view_page(ctx, itt, 3)
    assert isinstance(whole, ToolReturn) and whole.return_value == "The image of ITT.pdf, page 3 follows."
    assert Image.open(io.BytesIO(whole.content[0].data)).width == tools.VIEW_WIDTH
    assert opened(client, omar_id, "page", f"{itt}:3")  # a scan looked at can be cited

    close = tools.view_page(ctx, itt, 1, region=[100, 50, 900, 450])
    assert close.return_value.startswith(
        "A close-up of ITT.pdf, page 1, from (100, 50) to (900, 450) follows, enlarged 2.0 times. A point at (u, v) "
        "in it is at (100 + u × 0.5000, 50 + v × 0.5000) in view_page pixels"
    )
    for wrong in ([100, 50, 90, 450], [0, 0, 10]):
        with pytest.raises(ModelRetry, match="Give the region as \\[left, top, right, bottom\\]"):
            tools.view_page(ctx, itt, 1, region=wrong)
    with pytest.raises(ModelRetry, match="Give the region as \\[left, top, right, bottom\\]"):
        tools.view_page(ctx, photo, 1, region=[400, 0, 0, 400])
    with pytest.raises(ModelRetry, match="^Only PDF pages and images can be viewed; ITT.pdf has 3 pages"):
        tools.view_page(ctx, itt, 4)
    with pytest.raises(ModelRetry, match="^Only PDF pages and images can be viewed; Bill.xlsx has 1 pages"):
        tools.view_page(ctx, bill, 1)


def test_a_sheet_says_when_no_rows_have_the_words(client, package):
    tender_id, documents, _, omar_id = package
    ctx = fake_turn(client, tender_id, omar_id)
    bill = documents["Bill.xlsx"]
    assert tools.read_sheet(ctx, bill, words="blockwork") == "Bill.xlsx, page 1 (Sheet: BOQ): no rows with “blockwork”."
    assert tools.read_sheet(ctx, bill, first_row=50) == "Bill.xlsx, page 1 (Sheet: BOQ): no rows in that range."
    with pytest.raises(ModelRetry, match="ITT.pdf isn't a spreadsheet with a sheet 1: read it with read_page."):
        tools.read_sheet(ctx, documents["ITT.pdf"])


def test_copies_of_a_document_are_compared_page_by_page(client, package):
    tender_id, documents, _, omar_id = package
    ctx = fake_turn(client, tender_id, omar_id)
    assert tools.compare_copies(ctx, documents["ITT.pdf"]) == "ITT.pdf has only one copy."

    upload(client, tender_id, {"ITT.pdf": ITT + b"% saved again\n"})  # a new file with the same words
    newer = read_all(client, tender_id)["ITT.pdf"]["id"]
    same = tools.compare_copies(ctx, newer)
    assert same.startswith("ITT.pdf: the copy added ") and same.endswith(" have the same text on every page.")
    assert tools.compare_copies(ctx, documents["ITT.pdf"]) == same  # from the older copy, the same two

    revised = make_pdf([["Instructions to Tenderers", "Tenders are due by 21 October 2026."], ["12.3"]])
    upload(client, tender_id, {"ITT.pdf": revised})
    latest = read_all(client, tender_id)["ITT.pdf"]["id"]
    changed = tools.compare_copies(ctx, latest).splitlines()
    assert changed[1:] == [
        "Page 1 changed:",
        "  was: Tenders are due by 14 October 2026.",
        "  now: Tenders are due by 21 October 2026.",
        "Page 2 changed:",
        "  was: 12.3 The tenderer shall submit a work programme.",
        "  now: 12.3",
        "Page 3 is gone.",
    ]


def test_the_package_map_and_how_much_of_it_the_office_has_read(client, package):
    tender_id, documents, rania_id, omar_id = package
    ctx = fake_turn(client, tender_id, omar_id)
    itt = documents["ITT.pdf"]
    tools.read_page(ctx, itt, 1, last_page=2)
    notes = [
        {"document_id": itt, "kind": "Contract", "summary": "How to tender: the due date and what to submit."},
        {"document_id": documents["Bill.xlsx"], "kind": "invoice", "summary": "The client's bill of quantities."},
        {"document_id": "nope", "kind": "boq", "summary": "Nothing."},
    ]
    report = tools.describe_documents(ctx, [DocumentNote(**n) for n in notes])
    assert report == (
        "Described 1 documents.\nNot done: Bill.xlsx: kind is one of contract, specification, drawing, boq, addendum, "
        "quote, report, form, other; nope: no document of this tender has that id"
    )
    [row] = [r for r in tools.list_documents(ctx).splitlines() if r.startswith(itt)]
    assert row.endswith("· Contract and conditions: How to tender: the due date and what to submit.")

    read = tools.coverage(ctx).splitlines()
    assert read[0] == "3 documents, 5 pages; the office opened 2 of them."
    assert (
        "ITT.pdf: 3 pages, 2 readable (0 by OCR, 1 scans still to read); the office opened 2, its work cites 0" in read
    )
    with client.app.state.sessions() as session:  # looked at, so it can be given as a source
        assert lookup.cited(session, tender_id, omar_id, "coverage") == {"label": lookup.SUMMARIES["coverage"]}
        with pytest.raises(ValueError, match="You haven't looked at coverage yet"):
            lookup.cited(session, tender_id, rania_id, "coverage")


def test_a_tool_called_after_stop_does_nothing(client, package):
    tender_id, _, _, omar_id = package
    ctx = fake_turn(client, tender_id, omar_id)
    ctx.deps.stop.set()
    with pytest.raises(tools.Stopped):
        tools.post_to_team(ctx, "Carrying on.")
    assert team_room(client, tender_id) == []


def test_a_concern_is_raised_in_the_team_room_for_everyone(client, package):
    tender_id, _, rania_id, omar_id = package
    concern = "The bill has no item for dewatering, and the site is near the creek."
    assert tools.raise_concern(fake_turn(client, tender_id, omar_id), concern) == "Your concern is in the team room."
    [said] = team_room(client, tender_id)
    assert (said.sender, said.kind, said.text) == (omar_id, "concern", concern)
    with client.app.state.sessions() as session:
        brief = agents.situation(session, session.get(Staff, rania_id), [said])
    assert f"New for you:\n- Omar (concern), team room: {concern}" in brief


def test_next_steps_are_few_and_follow_ups_dont_pile_up(client, package):
    tender_id, _, rania_id, _ = package
    with client.app.state.sessions() as session:  # promised for an earlier question, and not done yet
        for step in ("check the bond wording", "read the addendum"):
            session.add(Task(tender_id=tender_id, staff_id=rania_id, title=f"{tools.FOLLOW_UP}{step}", brief="Do it."))
        session.commit()
        office.post(session, tender_id, ENGINEER, rania_id, "Is the programme 12 months?")
        session.commit()
    ctx = fake_turn(client, tender_id, rania_id)
    with pytest.raises(ModelRetry, match="^Give at most 3 next steps: the ones you will do yourself next.$"):
        tools.message_engineer(ctx, "Checking.", next_steps=["one", "two", "three", "four"])
    with pytest.raises(ModelRetry) as refused:
        tools.message_engineer(ctx, "Checking.", next_steps=["read clause 12", "check the drawings"])
    assert str(refused.value) == (
        "You already have 2 follow-ups open: Follow up: check the bond wording; Follow up: read the addendum. Do "
        "them and complete them before you promise more."
    )
    assert tools.message_engineer(ctx, "Checking.", next_steps=[" ", "read clause 12"]) == (
        "Sent. 1 next step is now your task."
    )


def test_a_question_for_the_engineer_has_two_to_four_options(client, package):
    tender_id, _, rania_id, _ = package
    ctx = fake_turn(client, tender_id, rania_id)
    for options in (["Yes"], ["A", "B", "C", "D", "E"]):
        with pytest.raises(ModelRetry, match="^Give between 2 and 4 options.$"):
            tools.ask_engineer(ctx, "Retention", "5% or 10%?", options)
    assert tools.ask_engineer(ctx, "Retention", "5% or 10%?", ["5%", "10%"]) == (
        "The question is waiting for the engineer."
    )


def test_a_task_to_produce_work_is_done_only_once_the_work_is_filed(client, package):
    tender_id, _, rania_id, omar_id = package
    with client.app.state.sessions() as session:
        rania, omar = session.get(Staff, rania_id), session.get(Staff, omar_id)
        task_id = office.assign(session, tender_id, rania, omar, "enter the BOQ", "All of it.").id
        session.commit()
    ctx = fake_turn(client, tender_id, omar_id)
    with pytest.raises(ModelRetry, match="^You haven't filed anything since this task began."):
        tools.complete_task(ctx, task_id, "Entered.")
    assert tools.complete_task(ctx, task_id, "The BOQ is a scan: nothing to enter.", only_reported=True) == (
        "Done. The Manager has your result."
    )
    said = team_room(client, tender_id)[-1]
    assert (said.sender, said.text) == (omar_id, "Finished: enter the BOQ. The BOQ is a scan: nothing to enter.")


PROFILE = {
    "role": "Estimator",
    "discipline": "Quantity surveying",
    "experience_years": 12,
    "background": "Priced roads and schools in Riyadh.",
    "working_style": "Builds every rate up from outputs.",
    "opinions": "Quotes are only as good as their exclusions.",
    "voice": "Short and plain.",
}


def test_the_manager_hires_real_people_for_work_the_office_does(client, package):
    tender_id, _, rania_id, _ = package
    ctx = fake_turn(client, tender_id, rania_id)
    with pytest.raises(ModelRetry, match="^Give their work as one or more of documents, boq, takeoff, drawings"):
        tools.hire(ctx, "Layla Nasser", work=["plumbing"], **PROFILE)
    with pytest.raises(ModelRetry) as placeholder:
        tools.hire(ctx, "Layla", work=["pricing"], **(PROFILE | {"background": "Background_Text"}))
    assert str(placeholder.value) == (
        "Make them a real person. name: Value error, give a full name, first and last; background: Value error, "
        "write real words, not a label or placeholder."
    )
    with pytest.raises(ModelRetry, match="^Omar Haddad is already on the team. Choose a different name.$"):
        tools.hire(ctx, "Omar Haddad", work=["pricing"], **PROFILE)

    assert tools.hire(ctx, "Layla Nasser", work=[" Pricing"], **PROFILE) == (
        "Layla Nasser has joined. Give them a task with assign_task."
    )
    with client.app.state.sessions() as session:
        layla = office.find_staff(session, tender_id, "layla")
        assert (layla.role, layla.now, layla.profile["work"]) == ("Estimator", "Just joined the team", ["pricing"])
    said = team_room(client, tender_id)[-1]
    assert (said.sender, said.kind, said.text) == (rania_id, "task", "Layla Nasser joins the team as Estimator.")


def test_only_someone_on_the_team_is_given_work_or_released(client, package):
    tender_id, _, rania_id, _ = package
    ctx = fake_turn(client, tender_id, rania_id)
    for name in ("Hadi Nassar", "Rania"):  # nobody by that name, and the Manager himself
        with pytest.raises(ModelRetry, match="^No one on the team has that name. Hire them first"):
            tools.assign_task(ctx, name, "price the fencing", "From the drawings.")
        with pytest.raises(ModelRetry, match="^No one on the team has that name.$"):
            tools.release(ctx, name, "Done.")
    assert tools.release(ctx, "omar", "His takeoff is finished.") == "Omar has been released."
    said = team_room(client, tender_id)[-1]
    assert (said.kind, said.text) == ("task", "Omar Haddad has left the team: His takeoff is finished.")
    with pytest.raises(ModelRetry, match="^No one on the team has that name."):  # released: no longer on the team
        tools.assign_task(ctx, "Omar", "measure the kerbs", "Both sheets.")


def test_the_due_date_rests_on_a_page_or_the_engineer_and_everyone_sees_where_from(client, package):
    tender_id, documents, rania_id, omar_id = package
    ctx = fake_turn(client, tender_id, rania_id)
    tools.coverage(ctx)  # a summary looked at is a source, but not a page that states the date
    with pytest.raises(ModelRetry, match='^Give the page of the tender documents that states the deadline, or "'):
        tools.set_due_date(ctx, date(2026, 10, 14), "coverage")
    tools.read_page(ctx, documents["ITT.pdf"], 1)
    tools.set_due_date(ctx, date(2026, 10, 14), "ITT.pdf, page 1", "Tenders are due by 14 October 2026.")
    with client.app.state.sessions() as session:
        brief = agents.situation(session, session.get(Staff, omar_id), [])
    assert brief.startswith(
        "Tender: Synthetic school. Due: 14 October 2026, from ITT.pdf, page 1: “Tenders are due by 14 October 2026.”."
    )
    tools.set_due_date(ctx, date(2026, 10, 7), "the engineer")
    with client.app.state.sessions() as session:
        brief = agents.situation(session, session.get(Staff, omar_id), [])
    assert brief.startswith(
        "Tender: Synthetic school. Due: 7 October 2026, the engineer's date, not from the tender documents."
    )


def test_the_office_looks_back_through_what_it_said_and_decided(client, package):
    tender_id, _, rania_id, omar_id = package
    with client.app.state.sessions() as session:
        rania, omar = session.get(Staff, rania_id), session.get(Staff, omar_id)
        task = office.assign(session, tender_id, rania, omar, "check the bid bond clause", "In the ITT.")
        office.complete(session, omar, task.id, "Clause 7.3: a bid bond of 1%.")
        question = office.ask(session, tender_id, rania, "Bid bond", "Price the bond at 1%?", ["Yes", "No"])
        office.answer(session, question, "Yes, 1%.")
        office.post(session, tender_id, ENGINEER, TEAM, "Keep the tender security in mind.")
        session.commit()
    ctx = fake_turn(client, tender_id, omar_id)
    found = [line.split(" · ", 1)[1] for line in tools.search_conversation(ctx, "bid bond").splitlines()]
    assert sorted(found) == [
        "Decision “Bid bond”: Price the bond at 1%? The engineer answered: Yes, 1%.",
        "Omar in the team room: Finished: check the bid bond clause. Clause 7.3: a bid bond of 1%.",
        "Rania in the team room: Rania asked Omar to check the bid bond clause",
        "Task for Omar: check the bid bond clause (done). Result: Clause 7.3: a bid bond of 1%.",
        "The engineer in the chat between Rania and the engineer: About “Bid bond”: Yes, 1%.",
    ]

    with client.app.state.sessions() as session:
        for n in range(30):
            office.post(session, tender_id, rania_id, TEAM, f"Bid bond note {n}.")
        session.commit()
    found = tools.search_conversation(ctx, "bid bond").splitlines()
    assert len(found) == tools.activity.SHOWN + 1 and found[-1] == "… and 5 older."


def test_open_record_says_which_references_it_cannot_find(client, tender):
    tender_id, priya_id, _ = tender
    ctx = fake_turn(client, tender_id, priya_id)
    assert tools.open_record(ctx, []) == "Give at least one reference."
    missing, line = tools.open_record(ctx, ["rate 0badc0de", "3.1"]).split("\n\n", 1)
    assert (
        missing == "rate 0badc0de: There is no rate 0badc0de on this tender. Use find_records to look it up by words."
    )
    assert "Excavation" in line
    with client.app.state.sessions() as session:
        assert office.has_opened(session, priya_id, "boq", boq.find_item(session, tender_id, "3.1").id)
    assert tools.find_records(ctx, "curtain walling") == (
        "No record matches. Try fewer or other words, or the Arabic or English term."
    )


def test_only_your_own_work_the_manager_has_not_reviewed_can_be_withdrawn(client, tender):
    tender_id, priya_id, _ = tender
    with client.app.state.sessions() as session:
        omar_id = office.hire(session, tender_id, "Omar Haddad", "Buyer", {}).id
        note = "Plant 4.5 m3/hr at 83.25 per hour; no disposal off site."
        rate = estimate.propose_rate(session, tender_id, priya_id, "3.1", "estimate", note, unit_rate=Decimal("18.50"))
        package = subcontract.create_package(session, tender_id, priya_id, "Groundworks", "subcontract", ["3.1"])
        session.commit()
        rate_ref, package_ref = f"rate {rate.id[:8]}", f"package {package.id[:8]}"
    assert tools.withdraw(fake_turn(client, tender_id, omar_id), [rate_ref], "Wrong line.") == (
        f"Withdrew 0.\nNot withdrawn: {rate_ref}: only your own work the Tender Manager hasn't reviewed yet"
    )
    report = tools.withdraw(fake_turn(client, tender_id, priya_id), [rate_ref, package_ref, "4.3"], " Wrong line. ")
    assert report == (
        f"Withdrew 1.\nNot withdrawn: {package_ref}: only BOQ lines, facts, scales, measurements, rates, markups and "
        "drafts; 4.3: only your own work the Tender Manager hasn't reviewed yet"  # approved by the engineer
    )
    with client.app.state.sessions() as session:
        _, withdrawn = lookup.find(session, tender_id, rate_ref)
        assert (withdrawn.status, withdrawn.reason) == ("withdrawn", "Wrong line.")


def test_the_priced_boq_is_quantix_figures_and_can_be_given_as_a_source(client, tender):
    tender_id, priya_id, _ = tender
    ctx = fake_turn(client, tender_id, priya_id)
    assert tools.list_boq(ctx, section="Roofing") == "No BOQ line is in a bill called “Roofing”."
    head, *rows, foot = tools.priced_boq(ctx).splitlines()
    assert head.startswith("3 lines, 0 priced") and rows[0] == "3.1 | Excavation | 1,240 m3 | not priced"
    assert foot == "Lines 1 to 3 of 3."
    with client.app.state.sessions() as session:
        assert lookup.cited(session, tender_id, priya_id, "priced_boq") == {"label": "The priced BOQ"}


def test_measuring_a_pdf_drawing_needs_it_opened_and_its_scale_set(client, sheet):
    tender_id, drawing, omar_id = sheet
    ctx = fake_turn(client, tender_id, omar_id)
    with client.app.state.sessions() as session:
        bill = next(d.id for d in session.query(Document).filter_by(tender_id=tender_id, name="Bill.xlsx"))
    with pytest.raises(
        ModelRetry, match="^Takeoff works on PDF pages of this tender; check the document id and page.$"
    ):
        tools.find_on_page(ctx, bill, 1, "Doors")
    kerb = [[100, 100], [500, 100]]
    with pytest.raises(ModelRetry, match="^You cite A-101.pdf, page 1, but you haven't opened it: read it first, then"):
        tools.measure(ctx, drawing, 1, "length", "Kerb", kerb, "m")
    assert tools.find_on_page(ctx, drawing, 1, "Grid F") == (
        "“Grid F” is not printed on that page as text. Look at the page with view_page instead."
    )
    # find_on_page opened the sheet, so its maker may measure on it
    assert (
        tools.measure(ctx, drawing, 1, "length", "Kerb", kerb, "m")
        == "Saved, but the sheet has no scale yet: set_scale first."
    )
    assert test_takeoff.scale(client, tender_id, drawing).status_code == 201  # the engineer sets it
    assert tools.measure(ctx, drawing, 1, "length", "Kerb", kerb, "m") == "Measured Kerb: 15.300 m."


def test_query_drawing_counts_and_groups_what_a_rule_takes(client, flat):
    tender_id, drawing, omar_id = flat
    ctx = fake_turn(client, tender_id, omar_id)
    assert tools.query_drawing(ctx, drawing, cad.Rule(layers=["S-STEEL"])) == (
        "Nothing on A-101.dwg, page 1 matches that rule."
    )
    assert tools.query_drawing(ctx, drawing, cad.Rule(types=["Room"])).startswith(
        "No rooms: map the layers that outline rooms (room_boundary)"
    )
    placed = cad.Rule(layers=["A-GLAZ", "A-DOOR"], types=["Insert"])
    by_block = tools.query_drawing(ctx, drawing, placed, group_by="block")
    assert "By block:\n- WIN-1200: 3 objects" in by_block and "\n- DOOR-900: 1 object" in by_block
    by_type = tools.query_drawing(ctx, drawing, cad.Rule(layers=["A-ROOM", "A-FLOR"]), group_by="type")
    hatch = "- Hatch: 1 object, length 28,700.000 drawing units, closed area 35,100,000.000 square units"
    assert f"By type:\n- Text: 2 objects\n{hatch}\n" in by_type
    assert "· “LIVING”" in by_type and "· area 35,100,000.0" in by_type

    client.post(f"/tenders/{tender_id}/units", json={"document_id": drawing, "units": "millimetres"})
    test_drawings.approve_map(client, tender_id, drawing)
    rooms = tools.query_drawing(ctx, drawing, cad.Rule(types=["Room"])).splitlines()
    assert rooms[0] == "2 rooms on A-101.dwg, page 1:"
    assert any(r.startswith("- BEDROOM: area 36.10 m2, perimeter 24.70 m (from its ") for r in rooms)
    names = cad.Rule(layers=["A-ROOM"], rooms=["living", "bedroom"])
    by_room = tools.query_drawing(ctx, drawing, names, group_by="room")
    assert "By room:\n- " in by_room and "- LIVING: 1 object" in by_room and "- BEDROOM: 1 object" in by_room
    floor = tools.query_drawing(ctx, drawing, cad.Rule(layers=["A-FLOR"]))
    assert floor.startswith(
        "A-101.dwg, page 1: 1 object, length 28,700.000 drawing units = 28.700 m, closed area 35,100,000.000 square "
        "units = 35.100 m2 (Quantix's figures)."
    )


def test_a_picture_of_a_drawing_numbers_what_a_rule_takes(client, flat):
    tender_id, drawing, omar_id = flat
    ctx = fake_turn(client, tender_id, omar_id)
    whole = tools.view_drawing(ctx, drawing)
    assert isinstance(whole, ToolReturn) and whole.content[0].data[:4] == b"\x89PNG"
    assert re.fullmatch(r"A-101.dwg, page 1, from \(.+\) to \(.+\) in drawing units, follows\.", whole.return_value)
    assert opened(client, omar_id, "page", f"{drawing}:1")

    marked = tools.view_drawing(ctx, drawing, rule=cad.Rule(blocks=["WIN-1200"]))
    numbered = marked.return_value.split(" Numbered: ")[1].split("; ")
    assert [n.split(" = ")[0] for n in numbered] == ["1", "2", "3"]
    close = tools.view_drawing(ctx, drawing, region=[0, 0, 5000, 4000])
    assert close.return_value.startswith("A-101.dwg, page 1, from (0, ")
    with pytest.raises(ModelRetry, match="^Give the region as \\[left, bottom, right, top\\] in drawing units.$"):
        tools.view_drawing(ctx, drawing, region=[0, 0, 5000])


def test_a_drawing_is_measured_once_read_and_its_units_set(client, flat):
    tender_id, drawing, omar_id = flat
    ctx = fake_turn(client, tender_id, omar_id)
    windows = cad.Rule(blocks=["WIN-1200"])
    with pytest.raises(ModelRetry, match="^You cite A-101.dwg, page 1, but you haven't opened it: read it first"):
        tools.measure_drawing(ctx, drawing, "count", "Windows", windows, "nr", boq_item="8.1")
    overview = tools.drawing_overview(ctx, drawing).splitlines()
    assert overview[1].startswith("Units: not set yet (set them with set_drawing_units).")
    assert tools.measure_drawing(ctx, drawing, "length", "Walls", cad.Rule(layers=["A-WALL"]), "m") == (
        "Took 7 objects, but A-101.dwg has no units yet: set them with set_drawing_units."
    )
    with pytest.raises(ModelRetry, match="^Give the units as one of "):
        tools.set_drawing_units(ctx, drawing, "cubits")
    tools.set_drawing_units(ctx, drawing, "MM")
    assert "\nUnits: millimetres (waiting for approval).\n" in tools.drawing_overview(ctx, drawing)

    skirting = cad.Rule(layers=["A-SKRT"])
    assert tools.measure_drawing(ctx, drawing, "length", "Skirting", skirting, "m", boq_item="8.3") == (
        "Recorded that Skirting isn't on A-101.dwg: nothing there matches the rule."
    )
    assert "BOQ line 8.3 (Skirting) is billed but the office found nothing of it" in tools.find_problems(ctx)
    temporary = cad.Rule(layers=["A-TEMP"])
    assert tools.measure_drawing(ctx, drawing, "length", "Temporary lines", temporary, "m", boq_item="8.4") == (
        "Measured Temporary lines: 2 objects, 2.000 m (Quantix's figure). That is 0.03 times the BOQ's 70.200 m: "
        "check the rule and the units, and raise a query if the BOQ looks wrong."
    )


def test_a_layer_map_and_a_tender_query_rest_on_pages_their_maker_opened(client, flat):
    tender_id, drawing, omar_id = flat
    ctx = fake_turn(client, tender_id, omar_id)
    blocks = {"DOOR-900": "doors", "WIN-1200": "windows"}
    with pytest.raises(ModelRetry, match="^You cite A-101.dwg, page 1, but you haven't opened it"):
        tools.propose_layer_map(ctx, drawing, MAP, "From what each layer holds.", blocks)
    tools.drawing_overview(ctx, drawing)
    filed = tools.propose_layer_map(ctx, drawing, MAP, "From what each layer holds.", blocks)
    assert re.fullmatch(r"Layer map \w{8} filed for the Tender Manager's review: .+\.", filed)

    living = queries.QuerySource(document_id=drawing, page=1, quote="LIVING")
    layout = queries.QuerySource(document_id=drawing, page=2, quote="DRAWING NO A-101")
    wording = "The drawings show skirting round both rooms; no BOQ item covers it. Please add an item or confirm."
    detail = "Skirting runs round both rooms but no line bills it."
    with pytest.raises(ModelRetry, match="^You cite A-101.dwg, page 2, but you haven't opened it"):
        tools.raise_query(ctx, "missing", "Skirting to both rooms", detail, wording, [living, layout])
    raised = tools.raise_query(ctx, "missing", "Skirting to both rooms", detail, wording, [living])
    assert re.fullmatch(r"Query \w{8} filed for the Tender Manager's review\.", raised)
    with client.app.state.sessions() as session:
        assert {"layers", "query"} <= {p.kind for p in reviews.pending(session, tender_id)}


GRID = {"name": "GRID", "base": [0, 0], "entities": [{"type": "line", "from": [0, 0], "to": [10000, 0]}]}
STRUCTURE = {  # 41 kinds of fitting, a grid clipped to part of it, a base plan the package lacks and a steel beam
    "insunits": 4,
    "layers": ["A-FURN", "X-REF", "GRID", "S-STEEL"],
    "blocks": [
        *[
            {"name": f"FIT-{n:02d}", "base": [0, 0], "entities": [{"type": "line", "from": [0, 0], "to": [100, 0]}]}
            for n in range(41)
        ],
        {"name": "BASE", "base": [0, 0], "xref": "..\\Base\\Base-Plan.dwg", "entities": []},
        GRID,
    ],
    "entities": [
        *[{"type": "insert", "block": f"FIT-{n:02d}", "layer": "A-FURN", "at": [n * 200, 0]} for n in range(41)],
        {"type": "insert", "block": "BASE", "layer": "X-REF", "at": [0, 0]},
        {
            "type": "insert",
            "block": "GRID",
            "layer": "GRID",
            "at": [0, 5000],
            "clip": [[0, 4000], [5000, 4000], [5000, 6000], [0, 6000]],
        },
        {"type": "insert", "block": "NOWHERE", "layer": "A-FURN", "at": [0, 9000]},
        {"type": "box", "layer": "S-STEEL", "at": [1000, 1000, 50], "size": [2000, 300, 100]},
    ],
}


def frozen(dxf: bytes, layer: str) -> bytes:
    """The drawing with one layer frozen, as a designer leaves a layer that shouldn't print."""
    record = f"  2\r\n{layer}\r\n 70\r\n     0\r\n".encode()
    assert dxf.count(record) == 1
    return dxf.replace(record, f"  2\r\n{layer}\r\n 70\r\n     1\r\n".encode())


@pytest.fixture
def hall(client):
    tender_id = client.post("/tenders", json={"name": "Synthetic hall"}).json()["id"]
    upload(client, tender_id, {"S-101.dxf": frozen(make_drawing(STRUCTURE, ".dxf"), "GRID")})
    drawing = read_all(client, tender_id)["S-101.dxf"]["id"]
    with client.app.state.sessions() as session:
        omar_id = office.hire(session, tender_id, "Omar Haddad", "Quantity Surveyor", {}).id
        session.commit()
    return tender_id, drawing, omar_id


def test_the_overview_says_what_quantix_could_not_read_or_find(client, hall):
    tender_id, drawing, omar_id = hall
    ctx = fake_turn(client, tender_id, omar_id)
    overview = tools.drawing_overview(ctx, drawing).splitlines()
    assert "Not read: 1 references to blocks the drawing doesn't define." in overview
    assert "1 block references are clipped: only what shows inside their clip boundaries is read." in overview
    assert "It refers to drawings it doesn't hold: ..\\Base\\Base-Plan.dwg." in overview
    assert "- GRID: 1 Insert, 1 Line; length 5,000; blocks GRID ×1; doesn't print (off or frozen)" in overview
    assert "43 blocks are placed here (copies, wherever they sit):" in overview  # the one it lacks isn't placed
    assert overview[-1] == "… and 3 more: find them with query_drawing and types ['Insert']."

    client.post(f"/tenders/{tender_id}/units", json={"document_id": drawing, "units": "millimetres"})
    steel = tools.query_drawing(ctx, drawing, cad.Rule(types=["Solid3D"]))
    assert "1 object, 3D solid volume 60,000,000.000 cubic units = 0.060 m3 (Quantix's figures)." in steel


@pytest.mark.xfail(strict=True, reason="Bug: query_drawing says it lists `show` objects but lists at most 60")
def test_query_drawing_lists_as_many_objects_as_it_says(client, hall):
    tender_id, drawing, omar_id = hall
    found = tools.query_drawing(fake_turn(client, tender_id, omar_id), drawing, cad.Rule(layers=["A-FURN"]), show=100)
    head, listed = found.split("\nThe first ")[1].split(" objects:\n")
    assert int(head) == len(listed.splitlines())


@pytest.mark.xfail(strict=True, reason="Bug: group_by room finds no rooms unless the rule itself names rooms")
def test_query_drawing_groups_any_rule_by_room(client, flat):
    tender_id, drawing, omar_id = flat
    client.post(f"/tenders/{tender_id}/units", json={"document_id": drawing, "units": "millimetres"})
    test_drawings.approve_map(client, tender_id, drawing)
    ctx = fake_turn(client, tender_id, omar_id)
    by_room = tools.query_drawing(ctx, drawing, cad.Rule(layers=["A-ROOM"]), group_by="room")
    assert "By room:\n- " in by_room and "- LIVING: 1 object" in by_room and "- BEDROOM: 1 object" in by_room


def test_a_pdf_drawn_in_lines_is_measured_once_its_scale_is_set(client, site):
    tender_id, drawing, omar_id = site
    ctx = fake_turn(client, tender_id, omar_id)
    overview = tools.drawing_overview(ctx, drawing).splitlines()
    assert overview[1] == "Scale: not set yet (set it with set_scale on a dimension printed on the sheet)."
    road = cad.Rule(layers=["green*"])
    assert tools.measure_drawing(ctx, drawing, "length", "Road edge", road, "m") == (
        "Took 1 objects, but page 1 of Site.pdf has no scale yet: set it with set_scale."
    )
    test_vectors._scale(client, tender_id, drawing)
    assert tools.measure_drawing(ctx, drawing, "length", "Road edge", road, "m") == (
        "Measured Road edge: 1 objects, 22.500 m (Quantix's figure)."
    )


def test_an_approved_rate_serves_later_tenders_once_the_engineer_keeps_it(client, tender):
    tender_id, priya_id, _ = tender
    rania_id, rate_id, _ = priced_after_a_send_back(client, tender_id, priya_id)  # 4.3, built up and approved
    with client.app.state.sessions() as session:
        note = "Plant 4.5 m3/hr at 83.25 per hour; no disposal off site."
        waiting = estimate.propose_rate(
            session, tender_id, priya_id, "3.1", "estimate", note, unit_rate=Decimal("18.5")
        )
        session.commit()
        waiting_ref = f"rate {waiting.id[:8]}"
    ctx = fake_turn(client, tender_id, rania_id)
    with pytest.raises(ModelRetry, match="^Only an approved rate can go to the library.$"):
        tools.suggest_library(ctx, waiting_ref, "A sound excavation rate.")
    assert tools.suggest_library(ctx, f"rate {rate_id[:8]}", "A clean build-up for slab rebar.") == (
        "The engineer will decide whether to keep it in the library."
    )
    assert tools.suggest_library(ctx, "4.3", "Again.") == "The engineer has already been asked about this rate."
    [decision] = client.get(f"/tenders/{tender_id}/decisions").json()
    assert decision["title"] == "Keep the rate for 4.3 in the library?"
    assert decision["options"] == [estimate.KEEP_IN_LIBRARY, "Don't keep it"]
    kept = client.post(f"/decisions/{decision['id']}/answer", json={"answer": estimate.KEEP_IN_LIBRARY})
    assert kept.status_code == 200
    again = client.post(f"/decisions/{decision['id']}/answer", json={"answer": "Don't keep it"})
    assert (again.status_code, again.json()["detail"]) == (400, "This has already been decided.")

    clinic = client.post("/tenders", json={"name": "Synthetic clinic"}).json()["id"]
    with client.app.state.sessions() as session:
        nora_id = office.hire(session, clinic, "Nora Al-Otaibi", "Estimator", {}).id
        session.commit()
    later = fake_turn(client, clinic, nora_id)
    wire = tools.search_library(later, "tie wire").splitlines()[0]
    assert wire.endswith(
        f"· material · Tie wire · 6.0000 SAR per kg · dated {date.today()} · Approved for 4.3 Slab reinforcement"
    )
    head, built, note = tools.search_past_tenders(later, "slab reinforcement").splitlines()
    assert head.startswith("Synthetic school (open, ")
    assert head.endswith(") · 4.3 Slab reinforcement · 3488.00 per t (estimate)")
    assert built == (
        "  Built up from: Steel fixer gang 16 hr at 62; Rebar B500B cut and bent 1 t at 2300; Tie wire 12 kg at 6; "
        "Bar bending machine 0.5 hr at 18"
    )
    assert note == "  Note: Fixed by a gang of four, rebar with 5% wastage."


def test_a_build_up_is_reused_only_from_an_approved_rate_of_the_office_own(client, tender):
    tender_id, priya_id, quote_id = tender
    ctx = fake_turn(client, tender_id, priya_id)
    assert tools.check_rate(ctx, "3.1") == "3.1 isn't priced yet."
    with pytest.raises(ModelRetry, match="^3.1 has no approved rate to reuse.$"):
        tools.apply_buildup(ctx, "3.1", ["6.3"], "Same work.")
    with client.app.state.sessions() as session:
        quoted = "Rebar B500B cut and bent 2,300.00 SAR per t"
        estimate.propose_rate(
            session,
            tender_id,
            priya_id,
            "4.3",
            "quote",
            "The supplier's price, delivered.",
            unit_rate=Decimal("2300"),
            document_id=quote_id,
            page=1,
            quote=quoted,
            status="approved",
        )
        session.commit()
    with pytest.raises(ModelRetry, match="^A rate from a quote or a web price prices its own line only"):
        tools.apply_buildup(ctx, "4.3", ["6.3"], "Same supplier.")


def test_figures_are_worked_out_by_quantix_never_in_the_head(client, package):
    tender_id, _, _, omar_id = package
    ctx = fake_turn(client, tender_id, omar_id)
    assert tools.calculate(ctx, "L * W * D", {"L": 420, "W": 12, "D": 0.3}, "m3") == (
        "L * W * D = 1,512 m3 (Quantix's figure)."
    )
    with pytest.raises(ModelRetry, match="^That divides by zero.$"):
        tools.calculate(ctx, "L / (W - 12)", {"L": 420, "W": 12})
    with pytest.raises(ModelRetry, match="^D has no value: give it in values.$"):
        tools.calculate(ctx, "L * D", {"L": 420})

    ground = [[101.0, 101.0], [101.0, 101.0]]
    assert tools.earthwork_volumes(ctx, 10.0, ground, formation_level=100.5) == (
        "Over 1 grid squares of 10.0 m: cut 50 m3, fill 0 m3, net 50 m3 (Quantix's figures, in place, before bulking "
        "or compaction)."
    )
    raised = [[101.5, 101.5], [101.5, 101.5]]
    assert "cut 0 m3, fill 50 m3, net -50 m3" in tools.earthwork_volumes(ctx, 10.0, ground, formation_levels=raised)
    for levels in ({}, {"formation_level": 100.5, "formation_levels": raised}):
        with pytest.raises(ModelRetry, match="^Give either one formation_level or formation_levels on the same grid.$"):
            tools.earthwork_volumes(ctx, 10.0, ground, **levels)
    with pytest.raises(ModelRetry, match="^Give the formation levels on the same grid as the ground levels.$"):
        tools.earthwork_volumes(ctx, 10.0, ground, formation_levels=[[100.0, 100.0, 100.0]] * 2)


def test_a_package_is_levelled_only_once_quotes_are_in(client, tender):
    tender_id, priya_id, _ = tender
    ctx = fake_turn(client, tender_id, priya_id)
    assert tools.list_packages(ctx) == "No packages yet. Group lines to price from outside with create_package."
    assert tools.create_package(ctx, "Groundworks", "subcontract", ["3.1", "6.3"]) == (
        "The Groundworks package has 2 items. Draft enquiries with draft_enquiry."
    )
    assert tools.levelling(ctx, "groundworks") == "No quotes recorded yet."
    assert re.fullmatch(
        r"package \w{8} · Groundworks \(subcontract\), 2 lines · 0 enquiries, 0 sent · quotes: none yet · .+",
        tools.list_packages(ctx),
    )
    with client.app.state.sessions() as session:
        assert lookup.cited(session, tender_id, priya_id, "list_packages") == {
            "label": "The subcontract and supply packages"
        }


URL = "https://prices.example/ready-mix"


def test_the_web_tools_say_plainly_what_they_could_not_find_or_read(client, package, monkeypatch):
    long_page = "# Ready mix prices\n\n" + "C35/20 OPC at 230.00 SAR per m3, delivered in Riyadh.\n" * 300

    def answer(request: httpx.Request) -> httpx.Response:
        if request.url.path == "/v2/search":
            return httpx.Response(200, json={"success": True, "data": {"web": []}})
        if json.loads(request.content)["url"] != URL:
            return httpx.Response(503)
        metadata = {"title": "Ready mix prices", "sourceURL": URL, "statusCode": 200}
        return httpx.Response(200, json={"success": True, "data": {"markdown": long_page, "metadata": metadata}})

    monkeypatch.setattr(web, "client", httpx.Client(transport=httpx.MockTransport(answer)))
    tender_id, _, _, omar_id = package
    ctx = fake_turn(client, tender_id, omar_id)
    assert tools.search_web(ctx, "ready mix concrete price Riyadh") == "The web has nothing for that. Try other words."
    assert tools.read_web_page(ctx, "https://down.example/prices") == (
        "That page can't be read right now. Try another result, or carry on without it."
    )
    with pytest.raises(ModelRetry, match="^Give the page's full address, starting with https://$"):
        tools.read_web_page(ctx, "prices.example/ready-mix")

    parts = -(-len(long_page) // tools.WEB_PART)
    last = tools.read_web_page(ctx, URL, part=parts)
    assert last.splitlines()[0].endswith(f"· part {parts} of {parts}")
    assert last.endswith(long_page[(parts - 1) * tools.WEB_PART :])
    with pytest.raises(ModelRetry, match=f"^The page has parts 1 to {parts}.$"):
        tools.read_web_page(ctx, URL, part=parts + 1)


def test_a_work_schedule_takes_its_days_from_the_boq_and_goes_for_review(client, tender):
    tender_id, priya_id, _ = tender
    upload(client, tender_id, {"ITT.pdf": ITT})
    itt = read_all(client, tender_id)["ITT.pdf"]["id"]
    with client.app.state.sessions() as session:
        clause = "12.3 The tenderer shall submit a work programme."
        programme = submission.RequirementIn(
            section="Technical", title="Work programme", document_id=itt, page=2, quote=clause
        )
        submission.add_requirements(session, tender_id, priya_id, [programme])
        session.commit()
    ctx = fake_turn(client, tender_id, priya_id)
    lines = [
        submission.ActivityIn(boq_item="3.1", output=Decimal(100), crews=2),
        submission.ActivityIn(boq_item="6.3", output=Decimal(500), crews=1),
    ]
    with pytest.raises(ModelRetry, match="^There is no requirement called Programme of works."):
        tools.draft_work_schedule(ctx, "Programme of works", "Programme", lines, "In order.", 9)
    drafted = tools.draft_work_schedule(
        ctx, "Work programme", "Work programme", lines, "Excavation, then waterproofing.", 9
    )
    assert drafted == (
        "Durations, from the BOQ quantities and the assumed outputs:\n"
        "- 3.1, Excavation: 1,240 m3 at 100 m3 a day × 2 crews = 7 days\n"
        "- 6.3, Waterproofing: 980 m2 at 500 m2 a day × 1 crew = 2 days\n\n"
        "Excavation, then waterproofing.\nOverall duration: 9 working days.\n\n"
        "It is with the Tender Manager for review. 9 days if every line ran one after another."
    )
    with client.app.state.sessions() as session:
        assert "draft" in {p.kind for p in reviews.pending(session, tender_id)}


def test_the_review_queue_shows_sixty_at_a_time_and_counts_the_rest(client):
    rows = [(f"9.{n}", f"Precast kerb type {n}", "m", n) for n in range(1, 62)]
    workbook = openpyxl.Workbook()
    for row in rows:
        workbook.active.append(row)
    buffer = io.BytesIO()
    workbook.save(buffer)
    tender_id = client.post("/tenders", json={"name": "Synthetic estate"}).json()["id"]
    upload(client, tender_id, {"Bill.xlsx": buffer.getvalue()})
    bill = read_all(client, tender_id)["Bill.xlsx"]["id"]
    with client.app.state.sessions() as session:
        rania_id = office.hire(session, tender_id, "Rania Farouk", "Tender Manager", {}, is_manager=True).id
        omar = office.hire(session, tender_id, "Omar Haddad", "Quantity Surveyor", {})
        lines = [
            boq.ItemIn(
                item=i,
                description=d,
                unit=u,
                quantity=Decimal(q),
                document_id=bill,
                page=1,
                quote=f"A{q}={i} | B{q}={d} | C{q}={u} | D{q}={q}",
            )
            for i, d, u, q in rows
        ]
        assert boq.propose_items(session, tender_id, omar, lines).startswith("Saved 61")
        session.commit()
    queue = tools.review_queue(fake_turn(client, tender_id, rania_id)).splitlines()
    assert queue[0] == "61 waiting for your review:" and len(queue) == 62
    assert queue[-1] == "… and 1 more after these."


def test_every_tool_is_in_someones_hands():
    import inspect

    defined = {
        name
        for name, tool in vars(tools).items()
        if inspect.isfunction(tool) and not name.startswith("_") and "ctx" in inspect.signature(tool).parameters
    }
    assert packs.every_tool() == defined


def test_a_refused_key_stops_the_office_at_once_and_the_message_waits_unread(client, package, tmp_path):
    tender_id, _, rania_id, _ = package
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})
    asked = []

    def refused(messages, info):
        asked.append(1)
        raise ModelHTTPError(401, "brain", body={"error": {"message": "invalid x-api-key"}})

    client.app.state.office.model = lambda: scripted(refused)
    client.post(f"/tenders/{tender_id}/messages", json={"channel": rania_id, "text": "Is the bond 1%?"})
    paused = wait_for(lambda o: o["state"] == "paused", client, tender_id)
    assert (
        paused["notice"]
        == "The office stopped: The key was refused. Check it and try again. Send a message to try again."
    )
    assert asked == [1]  # a refused key doesn't clear on its own: no retries
    [turn] = turn_records(client, tender_id)
    assert turn.ended == "failed" and turn.note.startswith("ModelHTTPError: status_code: 401")
    with client.app.state.sessions() as session:  # the engineer's question is new again for when the office carries on
        assert office.inbox(session, session.get(Staff, rania_id))[0].text == "Is the bond 1%?"


def test_the_turn_log_shows_a_page_looked_at_in_words(client, package, tmp_path):
    tender_id, documents, _, omar_id = package
    settings.save(tmp_path, office_ai={"connection_id": "scripted", "model": "brain"})

    def brain(messages, info):
        if "You are Omar Haddad" not in info.instructions:
            return DONE
        return [call("view_page", document_id=documents["ITT.pdf"], page=3), DONE][len(returns(messages))]

    client.app.state.office.model = lambda: scripted(brain)
    client.app.state.office.sees_images = lambda: True
    client.post(f"/tenders/{tender_id}/messages", json={"channel": omar_id, "text": "Look at the scan."})
    wait_for(
        lambda o: any(t.staff_id == omar_id and t.ended for t in turn_records(client, tender_id)), client, tender_id
    )
    turn = client.get(f"/tenders/{tender_id}/turns", params={"staff_id": omar_id}).json()[0]
    brief, looked, done = client.get(f"/turns/{turn['id']}").json()["log"]  # the image itself is no step of its own
    assert brief["kind"] == "brief" and done["kind"] == "note"
    assert (looked["tool"], looked["doing"]) == ("view_page", "Looking at ITT.pdf, page 3")
    assert looked["result"] == "The image of ITT.pdf, page 3 follows."


def test_the_office_works_with_the_ai_chosen_in_settings(client, tmp_path):
    assert runtime.office_model(tmp_path) is None
    connection = connections.add(tmp_path, "anthropic", "Office", "sk-ant-synthetic", None)
    settings.save(tmp_path, office_ai={"connection_id": connection["id"], "model": "claude-synthetic"})
    assert runtime.office_model(tmp_path).model_name == "claude-synthetic"
    assert runtime.office_sees_images(tmp_path) is False  # until its check read the number in the image
    connections.record_check(tmp_path, connection["id"], "claude-synthetic", True, "Answered.", sees_images=True)
    assert runtime.office_sees_images(tmp_path) is True


def test_the_office_carries_on_after_a_failure_of_its_own(client, monkeypatch, caplog):
    office_runtime = client.app.state.office
    passes = []

    async def work():
        passes.append(1)
        if len(passes) == 1:
            raise RuntimeError("The database is locked.")

    monkeypatch.setattr(office_runtime, "_work", work)
    for expected in (1, 2):
        office_runtime.wake()
        deadline = time.monotonic() + 10
        while len(passes) < expected and time.monotonic() < deadline:
            time.sleep(0.01)
    assert len(passes) == 2 and office_runtime._thread.is_alive()
    assert "The office loop failed" in caplog.text


def test_the_office_answers_only_for_tenders_and_decisions_that_exist(client):
    missing = client.get("/tenders/nope/office")
    assert (missing.status_code, missing.json()["detail"]) == (404, "Tender not found.")
    unknown = client.post("/decisions/nope/answer", json={"answer": "Yes"})
    assert (unknown.status_code, unknown.json()["detail"]) == (404, "Decision not found.")
    tender_id = client.post("/tenders", json={"name": "Synthetic school"}).json()["id"]
    assert client.delete(f"/tenders/{tender_id}").status_code == 204
    client.app.state.office.stop(tender_id)  # deleted while the office worked on it: nothing to stop
    assert client.app.state.office.status(tender_id) == "idle"
