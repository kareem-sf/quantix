"""Scanned pages are read by OCR on this computer, and join the search and the office's reading."""

import io
import threading
import time
from types import SimpleNamespace

from PIL import Image, ImageDraw, ImageFont
from test_documents import read_all, upload

from quantix.documents import ocr
from quantix.documents.models import Page
from quantix.office import records as office
from quantix.office import tools


def scanned_pdf(lines: list[str]) -> bytes:
    """A page with no text of its own: printed words, as a scanner saves them."""
    image = Image.new("RGB", (1240, 1754), "white")  # A4 at 150 dpi
    draw = ImageDraw.Draw(image)
    font = ImageFont.load_default(size=40)
    for n, line in enumerate(lines):
        draw.text((120, 160 + n * 90), line, fill="black", font=font)
    buffer = io.BytesIO()
    image.save(buffer, format="PDF", resolution=150)
    return buffer.getvalue()


def read_by_ocr(client, document_id, number=1, seconds=90.0) -> Page:
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        with client.app.state.sessions() as session:
            page = session.query(Page).filter_by(document_id=document_id, number=number).one()
            if page.ocr is not None:
                return page
        time.sleep(0.2)
    raise AssertionError("The scan was not read in time.")


def test_a_scanned_page_is_read_and_can_be_searched_and_cited(client):
    tender_id = client.post("/tenders", json={"name": "Substation"}).json()["id"]
    lines = ["DATA SCHEDULE", "Type of Cable", "XLPE", "Conductor Size", "Suitable up to 2500"]
    upload(client, tender_id, {"Data schedule.pdf": scanned_pdf(lines)})
    document = read_all(client, tender_id)["Data schedule.pdf"]
    assert (
        document["note"] == "1 of 1 pages are scans without text. Quantix reads their words by OCR in the background."
    )

    page = read_by_ocr(client, document["id"])
    assert (page.ocr, page.has_text) == ("en", True) and page.ocr_score > 0.8
    for words in lines:
        assert words in page.text
    hits = client.get(f"/tenders/{tender_id}/search", params={"q": "XLPE"}).json()
    assert [(h["name"], h["page"]) for h in hits] == [("Data schedule.pdf", 1)]

    with client.app.state.sessions() as session:
        omar = office.hire(session, tender_id, "Omar Haddad", "Quantity Surveyor", {})
        session.commit()
        omar_id = omar.id
    turn = tools.Turn(client.app.state.home, client.app.state.sessions, tender_id, omar_id, False, threading.Event())
    text = tools.read_page(SimpleNamespace(deps=turn), document["id"], 1)
    assert text.startswith("Data schedule.pdf, page 1, read from the scan by OCR (")
    assert "Check figures that matter on the image with view_page" in text and "Suitable up to 2500" in text
    with client.app.state.sessions() as session:
        assert office.has_opened(session, omar_id, "page", f"{document['id']}:1")


def test_a_blank_scan_has_no_words_to_read(client):
    tender_id = client.post("/tenders", json={"name": "Substation"}).json()["id"]
    upload(client, tender_id, {"Blank.pdf": scanned_pdf([])})
    document = read_all(client, tender_id)["Blank.pdf"]
    page = read_by_ocr(client, document["id"])
    assert (page.ocr, page.has_text, page.text) == ("empty", False, "")


def test_the_same_file_in_another_tender_is_read_once(client, monkeypatch):
    reads = []
    real = ocr.Ocr._read
    monkeypatch.setattr(ocr.Ocr, "_read", lambda self, png, language: reads.append(1) or real(self, png, language))
    scan = scanned_pdf(["Conductor Size", "Suitable up to 2500"])
    pages = []
    for name in ("Substation 8485", "Substation 8486"):
        tender_id = client.post("/tenders", json={"name": name}).json()["id"]
        upload(client, tender_id, {"Data schedule.pdf": scan})
        pages.append(read_by_ocr(client, read_all(client, tender_id)["Data schedule.pdf"]["id"]))
    assert pages[0].text == pages[1].text and "Suitable up to 2500" in pages[1].text
    assert len(reads) == 1


def box(left, top, right, bottom):
    return [[left, top], [right, top], [right, bottom], [left, bottom]]


def test_words_are_laid_out_in_rows_in_reading_order():
    boxes = [box(400, 100, 500, 130), box(100, 102, 300, 128), box(100, 200, 300, 230), box(420, 198, 520, 232)]
    words = ["XLPE", "Type of cable", "النوع", "الكابل"]
    text, score = ocr.lines(boxes, words, [0.9, 0.95, 0.4, 0.99])
    assert text == "Type of cable | XLPE\nالكابل"  # a row of Arabic runs right to left; unsure words are left out
    assert round(score, 3) == round((0.9 + 0.95 + 0.99) / 3, 3)
