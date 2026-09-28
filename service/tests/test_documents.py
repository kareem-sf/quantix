import datetime
import io
import os
import time

import docx
import openpyxl
import pytest
from sqlalchemy import exists, func, select

from quantix.api import documents as documents_api
from quantix.documents import library, meaning, readers, sheets
from quantix.documents.models import Document, Page, PageChunk


def make_pdf(pages: list[list[str]]) -> bytes:
    """A small, valid PDF with one Helvetica text line per entry; an empty page stands in for a scan."""
    objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]
    kids = []
    for lines in pages:
        stream = "BT /F1 12 Tf 72 720 Td 16 TL " + " ".join(f"({line}) '" for line in lines) + " ET"
        objects.append(f"<< /Length {len(stream)} >>\nstream\n{stream}\nendstream")
        content = len(objects)
        objects.append(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
            f"/Resources << /Font << /F1 3 0 R >> >> /Contents {content} 0 R >>"
        )
        kids.append(f"{len(objects)} 0 R")
    objects[1] = f"<< /Type /Pages /Kids [{' '.join(kids)}] /Count {len(kids)} >>"
    out, offsets = "%PDF-1.4\n", []
    for number, body in enumerate(objects, start=1):
        offsets.append(len(out))
        out += f"{number} 0 obj\n{body}\nendobj\n"
    xref = len(out)
    out += f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n" + "".join(f"{o:010d} 00000 n \n" for o in offsets)
    out += f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
    return out.encode("latin-1")


def make_xlsx() -> bytes:
    workbook = openpyxl.Workbook()
    sheet = workbook.active
    sheet.title = "BOQ"
    sheet.append(["Item", "Description", "Unit", "Qty"])
    sheet.append(["3.1", "Excavation to reduce levels", "m3", 1240.0])
    sheet.append(["4.2", "خرسانة مُسلّحة للبلاطة", "م3", 312.4])
    buffer = io.BytesIO()
    workbook.save(buffer)
    return buffer.getvalue()


def make_docx() -> bytes:
    document = docx.Document()
    document.add_paragraph("Instructions to Tenderers")
    document.add_paragraph("The bid bond shall be one percent of the tender price.")
    table = document.add_table(rows=1, cols=2)
    table.rows[0].cells[0].text = "Validity"
    table.rows[0].cells[1].text = "120 days"
    buffer = io.BytesIO()
    document.save(buffer)
    return buffer.getvalue()


PDF = make_pdf([["Conditions of Contract", "4.2 Tender security of one percent"], []])


@pytest.fixture
def tender(client):
    return client.post("/tenders", json={"name": "Synthetic school"}).json()["id"]


def upload(client, tender_id, files: dict[str, bytes]):
    return client.post(
        f"/tenders/{tender_id}/documents",
        files=[("files", (name, content, "application/octet-stream")) for name, content in files.items()],
    )


def read_all(client, tender_id):
    for _ in range(100):
        documents = client.get(f"/tenders/{tender_id}/documents").json()
        if all(d["status"] not in ("waiting", "reading") for d in documents):
            return {d["path"]: d for d in documents}
        time.sleep(0.05)
    raise AssertionError("The documents were not read in time.")


def indexed(client, tender_id):
    """Waits until the tender's pages are indexed by meaning."""
    read_all(client, tender_id)
    for _ in range(600):
        with client.app.state.sessions() as session:
            waiting = session.execute(
                select(func.count(Page.id))
                .join(Document, Document.id == Page.document_id)
                .where(
                    Document.tender_id == tender_id,
                    Document.status == "read",
                    Page.has_text,
                    ~exists().where(PageChunk.page_id == Page.id),
                )
            ).scalar_one()
        if meaning.loaded() and not waiting:
            return
        time.sleep(0.1)
    raise AssertionError("The pages were not indexed in time.")


def test_a_package_is_stored_and_read(client, tender, tmp_path):
    response = upload(
        client,
        tender,
        {
            "Package/Conditions.pdf": PDF,
            "Package/BOQ/Bill.xlsx": make_xlsx(),
            "Package/ITT.docx": make_docx(),
            "Package/Drawings/A-101.dwg": b"AC1027 not really a drawing",
        },
    )
    assert response.json() == {"added": 4, "unchanged": 0}
    documents = read_all(client, tender)

    pdf = documents["Package/Conditions.pdf"]
    assert (pdf["status"], pdf["page_count"], pdf["name"]) == ("read", 2, "Conditions.pdf")
    assert pdf["note"] == "1 of 2 pages are scans without text. Quantix reads their words by OCR in the background."
    first = client.get(f"/documents/{pdf['id']}/pages/1").json()
    assert "Tender security of one percent" in first["text"] and first["has_text"] is True
    assert client.get(f"/documents/{pdf['id']}/pages/2").json()["has_text"] is False

    sheet = client.get(f"/documents/{documents['Package/BOQ/Bill.xlsx']['id']}/pages/1").json()["text"]
    assert "Sheet: BOQ" in sheet and "A2=3.1 | B2=Excavation to reduce levels | C2=m3 | D2=1240" in sheet

    word = client.get(f"/documents/{documents['Package/ITT.docx']['id']}/pages/1").json()["text"]
    assert "one percent of the tender price" in word and "Validity | 120 days" in word

    dwg = documents["Package/Drawings/A-101.dwg"]
    assert dwg["status"] == "unreadable" and dwg["note"].startswith("This drawing can't be opened")
    kept = (tmp_path / "tenders" / tender / "files").iterdir()
    assert sorted(f.suffix for f in kept) == [".docx", ".dwg", ".pdf", ".xlsx"]


def test_search_finds_pages_in_english_and_arabic(client, tender):
    upload(client, tender, {"Conditions.pdf": PDF, "Bill.xlsx": make_xlsx()})
    indexed(client, tender)

    hits = client.get(f"/tenders/{tender}/search", params={"q": "tender security"}).json()
    assert [(h["name"], h["page"]) for h in hits] == [("Conditions.pdf", 1)]
    assert "4.2 [Tender] [security] of one percent" == hits[0]["snippet"]

    arabic = client.get(f"/tenders/{tender}/search", params={"q": "خرسانة مسلحة"}).json()
    assert [(h["name"], h["page"]) for h in arabic] == [("Bill.xlsx", 1)]
    assert "مُسلّحة" in arabic[0]["snippet"]  # the document's own spelling, marks included
    assert client.get(f"/tenders/{tender}/search", params={"q": "nothing like this"}).json() == []


def test_the_same_file_is_ignored_and_a_changed_one_replaces_it(client, tender):
    upload(client, tender, {"Conditions.pdf": PDF})
    assert upload(client, tender, {"Conditions.pdf": PDF}).json() == {"added": 0, "unchanged": 1}

    upload(client, tender, {"Conditions.pdf": make_pdf([["Revised conditions"]])})
    read_all(client, tender)
    documents = client.get(f"/tenders/{tender}/documents").json()
    assert sorted(d["status"] for d in documents) == ["read", "replaced"]
    current = next(d for d in documents if d["status"] == "read")
    assert "Revised conditions" in client.get(f"/documents/{current['id']}/pages/1").json()["text"]


def test_page_images_and_originals(client, tender):
    upload(client, tender, {"Conditions.pdf": PDF, "ITT.docx": make_docx()})
    documents = read_all(client, tender)

    image = client.get(f"/documents/{documents['Conditions.pdf']['id']}/pages/1/image")
    assert image.headers["content-type"] == "image/png" and image.content.startswith(b"\x89PNG")
    assert client.get(f"/documents/{documents['ITT.docx']['id']}/pages/1/image").status_code == 404
    assert client.get(f"/documents/{documents['Conditions.pdf']['id']}/file").content == PDF


def test_a_close_up_enlarges_part_of_a_page(tmp_path):
    from PIL import Image

    path = tmp_path / "sheet.pdf"
    path.write_bytes(PDF)
    whole = Image.open(io.BytesIO(readers.render_page(path, 1, width=612)))
    close = Image.open(io.BytesIO(readers.render_page(path, 1, width=612, region=(72, 60, 225, 90))))
    assert whole.size == (612, 792)
    assert close.size == (612, 120)  # 153 x 30 points, four times larger
    assert close.convert("L").getextrema()[0] < 128  # the title's letters are in it


def test_rejects_paths_that_leave_the_package(client, tender):
    assert upload(client, tender, {"../outside.pdf": PDF}).status_code == 400


def test_broken_characters_in_pdf_text_are_repaired():
    split_emoji, lone_half = "😀", "\udc00"  # how some PDFs hand characters over
    assert readers.clean_text(f"a{split_emoji}b{lone_half}c") == "a\U0001f600b�c"


def test_one_file_that_cannot_be_saved_never_stops_the_reader(client, tender, monkeypatch):
    real = readers.read_file

    def read_file(path, kind):
        pages = real(path, kind)
        if "Broken text" in pages[0].text:
            pages[0].text += " with a lone \udc00 half"  # what crashed the reader on a real package
        return pages

    monkeypatch.setattr(library.readers, "read_file", read_file)
    upload(client, tender, {"Bad.pdf": make_pdf([["Broken text"]]), "Good.pdf": make_pdf([["Good text"]])})
    read_all(client, tender)
    statuses = {d["name"]: d["status"] for d in client.get(f"/tenders/{tender}/documents").json()}
    assert statuses == {"Bad.pdf": "failed", "Good.pdf": "read"}


def make_styled_xlsx() -> bytes:
    """A small bill as an estimator lays one out: a merged, filled title, a frozen heading row, formatted numbers,
    a hidden column, and a second sheet that reads right to left."""
    from openpyxl.styles import Alignment, Border, Font, PatternFill, Side

    workbook = openpyxl.Workbook()
    bill = workbook.active
    bill.title = "Bill 1"
    bill["A1"] = "Earthworks"
    bill.merge_cells("A1:D1")
    bill["A1"].font = Font(bold=True, size=14)
    bill["A1"].fill = PatternFill("solid", fgColor="FFFF00")
    bill["A1"].alignment = Alignment(horizontal="center")
    bill.append(["Item", "Description", "Qty", "Rate"])
    bill.append(["3.1", "Excavation to reduce levels", 16480, 12.5])
    bill["C3"].number_format = "#,##0"
    bill["D3"].number_format = "#,##0.00"
    bill["B3"].border = Border(bottom=Side(style="thin", color="FF0000"))
    bill.column_dimensions["B"].width = 40
    bill.column_dimensions["E"].hidden = True
    bill["E3"] = "internal note"
    bill.row_dimensions[3].height = 30
    bill.freeze_panes = "A3"
    arabic = workbook.create_sheet("جدول")
    arabic.sheet_view.rightToLeft = True
    arabic["A1"] = "خرسانة"
    buffer = io.BytesIO()
    workbook.save(buffer)
    return buffer.getvalue()


def test_a_sheet_shows_as_excel_shows_it(client, tender):
    upload(client, tender, {"BOQ.xlsx": make_styled_xlsx()})
    document = read_all(client, tender)["BOQ.xlsx"]

    sheet = client.get(f"/documents/{document['id']}/sheets/1").json()
    assert sheet["sheets"] == [{"name": "Bill 1", "hidden": False}, {"name": "جدول", "hidden": False}]
    assert [c["letter"] for c in sheet["columns"]] == ["A", "B", "C", "D"]  # E is hidden, as in Excel
    assert sheet["columns"][1]["width"] == 285  # 40 characters
    assert sheet["hidden_columns"] == 1 and sheet["frozen_rows"] == 2
    title, heading, line = sheet["rows"]
    assert title["cells"] == [{"column": 0, "text": "Earthworks", "style": 1, "rows": 1, "columns": 4}]
    title_style = {k: v for k, v in sheet["styles"][1].items() if v not in (None, False, 0)}
    assert title_style == {"bold": True, "size": 14.0, "fill": "#ffff00", "align": "center"}
    assert line["height"] == 40  # 30 points
    texts = [c["text"] for c in line["cells"]]
    assert texts == ["3.1", "Excavation to reduce levels", "16,480", "12.50"]
    description, quantity = line["cells"][1], line["cells"][2]
    assert sheet["styles"][description["style"]]["bottom"] == "1px solid #ff0000"
    assert sheet["styles"][quantity["style"]]["align"] == "end"  # numbers sit at the end of the cell

    shown = client.get(f"/documents/{document['id']}/sheets/1", params={"hidden": True}).json()
    assert shown["columns"][4] == {"letter": "E", "width": 96.0, "hidden": True}  # openpyxl saves 13 characters
    assert shown["rows"][2]["cells"][4]["text"] == "internal note"

    arabic = client.get(f"/documents/{document['id']}/sheets/2").json()
    assert arabic["right_to_left"] and arabic["rows"][0]["cells"][0]["text"] == "خرسانة"
    assert client.get(f"/documents/{document['id']}/sheets/3").status_code == 404


@pytest.mark.parametrize(
    ("value", "number_format", "shown"),
    [
        (16480, "#,##0", "16,480"),
        (-1234.5, "#,##0.00;(#,##0.00)", "(1,234.50)"),
        (-3, "0", "-3"),
        (0, r'_(* #,##0.00_);_(* \(#,##0.00\);_(* "-"??_);_(@_)', "-"),
        (-2.5, r'_(* #,##0.00_);_(* \(#,##0.00\);_(* "-"??_);_(@_)', "(2.50)"),
        (0.125, "0.0%", "12.5%"),
        (1234.5, "[$SAR-401] #,##0.00", "SAR 1,234.50"),
        (0.1 + 0.2, "General", "0.3"),
        (1500000, '#,##0,"K"', "1,500K"),
        (7, "000", "007"),
        (True, "General", "TRUE"),
        (datetime.datetime(2026, 9, 28, 14, 5), "dd/mm/yyyy", "28/09/2026"),
        (datetime.datetime(2026, 9, 28, 14, 5), "d-mmm-yy h:mm AM/PM", "28-Sep-26 2:05 PM"),
    ],
)
def test_numbers_and_dates_read_as_excel_shows_them(value, number_format, shown):
    assert sheets.display(value, number_format) == shown


def test_a_tiff_shows_as_png(client, tender):
    from PIL import Image

    buffer = io.BytesIO()
    Image.new("RGB", (40, 30), "white").save(buffer, format="TIFF")
    upload(client, tender, {"Scan.tif": buffer.getvalue()})
    document = read_all(client, tender)["Scan.tif"]
    image = client.get(f"/documents/{document['id']}/pages/1/image")
    assert image.headers["content-type"] == "image/png"
    assert Image.open(io.BytesIO(image.content)).size == (40, 30)


def test_a_file_opens_in_its_app_as_a_read_only_copy(client, tender, tmp_path, monkeypatch):
    opened = []
    monkeypatch.setattr(documents_api, "_open_with_app", opened.append)
    upload(client, tender, {"Package/Conditions.pdf": PDF})
    document = read_all(client, tender)["Package/Conditions.pdf"]

    assert client.post(f"/documents/{document['id']}/open").status_code == 204
    [copy] = opened
    assert copy.name == "Conditions.pdf" and copy.read_bytes() == PDF
    assert not os.access(copy, os.W_OK)  # changes made in the app never reach the tender's files
    assert client.post(f"/documents/{document['id']}/open").status_code == 204  # the same copy again

    monkeypatch.setattr(documents_api, "_open_with_app", lambda path: (_ for _ in ()).throw(OSError()))
    refused = client.post(f"/documents/{document['id']}/open")
    assert refused.status_code == 409 and refused.json()["detail"] == "No app on this computer opens .pdf files."

    assert client.delete(f"/tenders/{tender}").status_code == 204
    assert not (tmp_path / "tenders" / tender).exists()  # read-only copies go with the tender
