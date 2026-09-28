"""The office reads the package beyond single pages: a spreadsheet's rows, several pages at once, what changed in a
newer copy, what each document is, and how much of the package it has read, opened and cited."""

import io
import threading
from decimal import Decimal
from types import SimpleNamespace

import openpyxl
import pytest
from test_documents import PDF, make_pdf, read_all, upload

from quantix.boq import records as boq
from quantix.office import records as office
from quantix.office import tools
from quantix.review import package


def bill(rows: int) -> bytes:
    workbook = openpyxl.Workbook()
    sheet = workbook.active
    sheet.title = "BOQ"
    sheet.append(["Item", "Description", "Unit", "Qty"])
    for n in range(1, rows + 1):
        sheet.append([f"C.{n}", "Supply and place backfill" if n % 50 == 0 else f"Excavation stage {n}", "m3", n * 10])
    buffer = io.BytesIO()
    workbook.save(buffer)
    return buffer.getvalue()


@pytest.fixture
def staffed(client):
    tender_id = client.post("/tenders", json={"name": "Substation"}).json()["id"]
    upload(
        client,
        tender_id,
        {
            "Bill.xlsx": bill(120),
            "Conditions.pdf": PDF,
            "Spec.pdf": make_pdf([["Section one: earthworks"], ["Section two: asphalt paving"]]),
        },
    )
    documents = read_all(client, tender_id)
    with client.app.state.sessions() as session:
        omar = office.hire(session, tender_id, "Omar Haddad", "Quantity Surveyor", {})
        session.commit()
        omar_id = omar.id
    turn = tools.Turn(client.app.state.home, client.app.state.sessions, tender_id, omar_id, False, threading.Event())
    return tender_id, SimpleNamespace(deps=turn, tool_call_id="call"), documents


def test_a_sheet_is_read_a_part_at_a_time_or_by_its_words(client, staffed):
    _, ctx, documents = staffed
    bill_id = documents["Bill.xlsx"]["id"]
    first = tools.read_sheet(ctx, bill_id)
    assert first.startswith("Bill.xlsx, page 1 (Sheet: BOQ), 121 rows:\nA1=Item | B1=Description | C1=Unit | D1=Qty")
    assert "A80=C.79" in first and "A81=" not in first
    assert first.endswith("… 41 more rows: ask for them from the next row on.")
    part = tools.read_sheet(ctx, bill_id, first_row=100, last_row=102)
    assert part.splitlines()[1:] == [
        "A100=C.99 | B100=Excavation stage 99 | C100=m3 | D100=990",
        "A101=C.100 | B101=Supply and place backfill | C101=m3 | D101=1000",
        "A102=C.101 | B102=Excavation stage 101 | C102=m3 | D102=1010",
    ]
    found = tools.read_sheet(ctx, bill_id, words="backfill")
    assert found.splitlines()[0] == "Bill.xlsx, page 1 (Sheet: BOQ), 2 rows:"
    with pytest.raises(Exception, match="isn't a spreadsheet"):
        tools.read_sheet(ctx, documents["Conditions.pdf"]["id"])


def test_pages_are_read_several_at_a_time(client, staffed):
    _, ctx, documents = staffed
    text = tools.read_page(ctx, documents["Spec.pdf"]["id"], 1, last_page=9)
    assert text == "Spec.pdf, page 1:\nSection one: earthworks\n\nSpec.pdf, page 2:\nSection two: asphalt paving"


def test_a_newer_copy_says_what_changed(client, staffed):
    tender_id, ctx, documents = staffed
    upload(
        client,
        tender_id,
        {"Conditions.pdf": make_pdf([["Conditions of Contract", "4.2 Tender security of two percent"]])},
    )
    newer = next(d for d in read_all(client, tender_id).values() if d["name"] == "Conditions.pdf")
    text = tools.compare_copies(ctx, newer["id"])
    assert (
        "Page 1 changed:\n  was: 4.2 Tender security of one percent\n  now: 4.2 Tender security of two percent" in text
    )
    assert "Page 2 is gone." in text
    assert tools.compare_copies(ctx, documents["Bill.xlsx"]["id"]) == "Bill.xlsx has only one copy."


def test_the_package_map_and_how_much_was_read(client, staffed):
    tender_id, ctx, documents = staffed
    bill_id, conditions = documents["Bill.xlsx"]["id"], documents["Conditions.pdf"]["id"]
    report = tools.describe_documents(
        ctx,
        [
            package.DocumentNote(document_id=bill_id, kind="boq", summary="The client's bill: 120 earthwork lines."),
            package.DocumentNote(document_id=conditions, kind="poem", summary="The conditions of contract."),
        ],
    )
    assert report == "Described 1 documents.\nNot done: Conditions.pdf: kind is one of " + ", ".join(package.KINDS)
    assert "Bills of quantities: The client's bill: 120 earthwork lines." in tools.list_documents(ctx)
    listed = client.get(f"/tenders/{tender_id}/documents").json()
    assert next(d for d in listed if d["id"] == bill_id)["group_name"] == "Bills of quantities"

    tools.read_page(ctx, conditions, 1)
    with client.app.state.sessions() as session:
        omar = office.find_staff(session, tender_id, "Omar")
        quote = "A2=C.1 | B2=Excavation stage 1 | C2=m3 | D2=10"
        line = boq.ItemIn(
            item="C.1", description="Excavation stage 1", unit="m3", quantity=Decimal(10), document_id=bill_id,
            page=1, quote=quote,
        )  # fmt: skip
        boq.propose_items(session, tender_id, omar, [line])
        session.commit()
    text = tools.coverage(ctx)
    assert text.splitlines()[0] == "3 documents, 5 pages; the office opened 1 of them."
    assert "Bill.xlsx: 1 pages, 1 readable; the office opened 0, its work cites 1" in text
    assert "Conditions.pdf: 2 pages, 1 readable (0 by OCR, 1 scans still to read); the office opened 1" in text
