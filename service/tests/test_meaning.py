import io
import textwrap

import docx
from test_company import priced_tender
from test_documents import indexed, make_pdf, upload

from quantix import company
from quantix.boq import records as boq
from quantix.documents import meaning

CLAUSES = [
    "If the Subcontractor fails to complete the works by the completion date, he shall pay the Contractor 0.5 "
    "percent of the subcontract price for every week of delay.",
    "The Contractor shall hold back five percent of each interim payment until the end of the defects liability "
    "period.",
    "Concrete for foundations shall be grade C35 ready mixed, supplied from an approved batching plant and placed "
    "within 90 minutes.",
    "Before entering the site the Subcontractor shall insure the works, his plant and his workers against all risks "
    "for the full contract period.",
    "The Contractor will provide site offices, stores and a laydown area; the Subcontractor provides his own welfare "
    "facilities and water for curing.",
]
ARABIC = "يدفع المقاول من الباطن غرامة تأخير قدرها نصف بالمائة من قيمة العقد عن كل أسبوع تأخير في إنجاز الأعمال"


def arabic_docx() -> bytes:
    document = docx.Document()
    document.add_paragraph(ARABIC)
    buffer = io.BytesIO()
    document.save(buffer)
    return buffer.getvalue()


def test_pages_are_indexed_in_passages_of_whole_lines_and_garbled_pages_are_left_out():
    clause = "The Subcontractor shall compact each layer of backfill to 95 percent of maximum dry density.\n"
    text = clause * 20
    spans = meaning.passages(text)
    assert all(b - a <= meaning.PASSAGE and text[b - 1] == "\n" for a, b in spans)  # never stops mid-line
    assert "".join(text[a:b] for a, b in spans) == text

    garbled = "ZE>t>>^EKhEZzt>>^t/>> K&Dh^K>/>K<^t/d,^dKE >/E'͘ • KhEZzt>>, /',dϯD d ZΘt>> ^/'E^W Z^&'h/\n"
    assert meaning.passages(garbled * 5) == []  # text from a PDF font Quantix can't map
    assert meaning.passages("Sheet 3 of 12, drawn by the engineer") == []  # too little to mean anything


def test_search_finds_pages_that_say_it_in_other_words_or_other_arabic_forms(client):
    tender = client.post("/tenders", json={"name": "Synthetic school"}).json()["id"]
    pdf = make_pdf([textwrap.wrap(c, 60) for c in CLAUSES])
    upload(client, tender, {"Conditions.pdf": pdf, "Arabic.docx": arabic_docx()})
    indexed(client, tender)

    def search(q):
        return [
            (h["name"], h["page"], h["snippet"])
            for h in client.get(f"/tenders/{tender}/search", params={"q": q}).json()
        ]

    assert search("who provides the stores") == [("Conditions.pdf", 5, CLAUSES[4])]  # "who" isn't on the page
    assert search("concrete strength") == [("Conditions.pdf", 3, CLAUSES[2])]
    assert search("غرامة التأخير") == [("Arabic.docx", 1, ARABIC)]  # the page says تأخير, not التأخير
    assert search("nothing like this") == []

    both = search("site offices")  # words and meaning agree on the same page, which is shown once
    assert [(name, page) for name, page, _ in both] == [("Conditions.pdf", 5)] and "[site] [offices,]" in both[0][2]


def test_the_library_directory_and_past_tenders_offer_entries_close_in_meaning(client):
    for name in ("Termite treatment below slab", "Excavation in ordinary soil", "Ready-mix concrete C35"):
        entry = {"kind": "unit_rate", "name": name, "unit": "m2", "rate": "12", "currency": "SAR"}
        client.post("/library", json=entry | {"source": "Riyadh school 2025", "dated": "2025-11-02"})
    for name, kind, trades in (
        ("Gulf Membranes", "subcontractor", "bitumen membranes, damp proofing"),
        ("Riyadh Readymix", "supplier", "ready-mixed concrete"),
        ("Secure Fencing", "subcontractor", "fencing, gates"),
    ):
        client.post("/directory", json={"name": name, "kind": kind, "trades": trades})
    for _ in range(100):  # the model loads in the background when Quantix starts
        if meaning.loaded():
            break
        indexed(client, client.post("/tenders", json={"name": "Waiting"}).json()["id"])

    def library(q):
        return [r["name"] for r in client.get("/library", params={"q": q}).json()]

    def directory(q):
        return [c["name"] for c in client.get("/directory", params={"q": q}).json()]

    assert library("anti-termite") == ["Termite treatment below slab"]
    assert library("concrete") == ["Ready-mix concrete C35"]  # the word, found the old way
    assert directory("waterproofing membrane") == ["Gulf Membranes"]
    assert directory("concrete supply") == ["Riyadh Readymix"]
    assert directory("tower crane hire") == []

    old, _ = priced_tender(client, "Riyadh school 2025", "18.50", "approved")
    current, _ = priced_tender(client, "Synthetic school", "19.00", "approved")
    with client.app.state.sessions() as session:
        boq.items(session, old)[0].description = "Termite treatment below the ground floor slab"
        session.commit()
        assert [p.description for p in company.past_rates(session, current, "anti-termite")] == [
            "Termite treatment below the ground floor slab"
        ]
        assert company.past_rates(session, current, "blockwork") == []
