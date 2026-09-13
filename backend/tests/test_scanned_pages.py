"""Scanned pages are read by local text recognition at import, or reported honestly."""

from pathlib import Path

import pypdfium2 as pdfium

from quantix import documents, extraction_worker, tesseract_runtime


def _blank_pdf(path: Path, pages: int = 1) -> Path:
    document = pdfium.PdfDocument.new()
    for _ in range(pages):
        page = document.new_page(200, 100)
        page.close()
    document.save(path)
    document.close()
    return path


def _install_recognition(monkeypatch, text, languages=("eng", "ara"), confidence=91.0):
    seen = {}

    def ocr_page(image, *, home=None, languages="eng+ara", cancelled=None):
        seen["languages"] = languages
        return extraction_worker.OCRPage(text, confidence)

    monkeypatch.setattr(extraction_worker, "ocr_available", lambda **_: {"available": True})
    monkeypatch.setattr(extraction_worker, "render_page_png", lambda *_args, **_kwargs: b"png")
    monkeypatch.setattr(extraction_worker, "ocr_page", ocr_page)
    monkeypatch.setattr(tesseract_runtime, "tesseract_languages", lambda *_a, **_k: frozenset(languages))
    return seen


def test_scanned_pages_are_read_and_marked(tmp_path, monkeypatch):
    seen = _install_recognition(monkeypatch, "موعد الزيارة الميدانية 19/08/2026")

    result = documents.extract_document(_blank_pdf(tmp_path / "scan.pdf"), "scan.pdf", lambda: False)

    assert [segment.metadata["method"] for segment in result.segments] == ["ocr"]
    assert "19/08/2026" in result.segments[0].text
    assert result.metadata["extraction_scope"] == "embedded_text+ocr"
    assert result.metadata["ocr_pages"] == 1
    assert seen["languages"] == "eng+ara"
    assert result.warnings == []


def test_missing_arabic_recognition_is_reported_once(tmp_path, monkeypatch):
    seen = _install_recognition(monkeypatch, "site visit", languages=("eng",))

    result = documents.extract_document(
        _blank_pdf(tmp_path / "scan.pdf", pages=2), "scan.pdf", lambda: False
    )

    assert seen["languages"] == "eng"
    assert len(result.segments) == 2
    assert [warning["code"] for warning in result.warnings] == ["ocr_language_missing"]


def test_pages_stay_exceptions_without_recognition(tmp_path, monkeypatch):
    monkeypatch.setattr(extraction_worker, "ocr_available", lambda **_: {"available": False})

    result = documents.extract_document(_blank_pdf(tmp_path / "scan.pdf"), "scan.pdf", lambda: False)

    assert result.segments == []
    assert [warning["code"] for warning in result.warnings] == ["pdf_no_text"]


def test_every_scanned_page_is_read_without_a_page_cap(tmp_path, monkeypatch):
    _install_recognition(monkeypatch, "Bill of quantities page")

    result = documents.extract_document(
        _blank_pdf(tmp_path / "scan.pdf", pages=30), "scan.pdf", lambda: False
    )

    assert [segment.page for segment in result.segments] == list(range(1, 31))
    assert result.metadata["ocr_pages"] == 30
    assert all(segment.metadata["ocr_confidence"] == 91.0 for segment in result.segments)
    assert result.warnings == []


def test_uncertain_recognition_is_kept_and_flagged_for_a_second_reading(tmp_path, monkeypatch):
    _install_recognition(monkeypatch, "blurred stamp", confidence=41.5)

    result = documents.extract_document(_blank_pdf(tmp_path / "scan.pdf"), "scan.pdf", lambda: False)

    assert result.segments[0].text == "blurred stamp"
    assert [(warning["code"], warning["locator"]) for warning in result.warnings] == [
        ("ocr_low_confidence", "page:1")
    ]


def test_tsv_confidence_is_weighted_by_word_length():
    header = ["level", "page_num", "block_num", "par_num", "line_num", "word_num",
              "left", "top", "width", "height", "conf", "text"]
    rows = [header, ["5", "1", "1", "1", "1", "1", "0", "0", "1", "1", "90", "concrete"],
            ["5", "1", "1", "1", "1", "2", "0", "0", "1", "1", "30", "C30"],
            ["4", "1", "1", "1", "1", "0", "0", "0", "1", "1", "-1", ""]]
    tsv = chr(10).join(chr(9).join(row) for row in rows)
    assert extraction_worker._tsv_confidence(tsv) == round((90 * 8 + 30 * 3) / 11, 1)
