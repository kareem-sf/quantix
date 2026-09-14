"""Reprocessing publishes current evidence without rewriting originals."""

from __future__ import annotations

import hashlib
from pathlib import Path

import pypdfium2 as pdfium
import pytest

from quantix.extraction_adapters import ExtractionService
from quantix.extraction_models import ReprocessRequest
from quantix.repository import Repository
from quantix.research_dependencies import DependencyService


def _blank_pdf(path: Path, pages: int = 1) -> bytes:
    document = pdfium.PdfDocument.new()
    for _ in range(pages):
        page = document.new_page(200, 100)
        page.close()
    document.save(path)
    document.close()
    return path.read_bytes()


def _store(repo: Repository, body: bytes) -> str:
    digest = hashlib.sha256(body).hexdigest()
    (repo.objects / digest).write_bytes(body)
    return digest


def _artifact(repo, tender_id, digest, body, *, name="Sources/scan.pdf", segments=None):
    artifact, _ = repo.register_artifact(
        tender_id,
        name,
        digest,
        len(body),
        {
            "kind": "pdf",
            "status": "needs_attention",
            "segments": segments
            or [
                {
                    "locator": "page:1",
                    "text": "",
                    "page": 1,
                    "metadata": {"method": "embedded_text"},
                }
            ],
        },
    )
    return artifact


def test_reprocessed_text_is_searchable_and_original_bytes_stay(tmp_path, monkeypatch):
    from quantix import extraction_worker

    source = tmp_path / "blank.pdf"
    body = _blank_pdf(source)
    repo = Repository(tmp_path / "home")
    digest = _store(repo, body)
    tender = repo.create_tender("Publish")
    artifact = _artifact(repo, tender["id"], digest, body)
    monkeypatch.setattr(
        extraction_worker,
        "ocr_available",
        lambda **kwargs: {
            "available": True,
            "tesseract": True,
            "engine": "synthetic",
            "executable": "synthetic",
        },
    )
    monkeypatch.setattr(extraction_worker, "render_page_png", lambda *args, **kwargs: b"png")
    monkeypatch.setattr(
        extraction_worker, "ocr_png", lambda png, **kwargs: "Recovered clause 14.7 advance payment."
    )

    result = ExtractionService(repo).reprocess(
        digest,
        path=repo.objects / digest,
        request=ReprocessRequest(
            original_hash=digest, reader_id="ocr", reader_version="synthetic-1"
        ),
        tender_id=tender["id"],
    )

    assert result["original_hash_unchanged"] is True
    assert result["published_artifact_ids"] == [artifact["id"]]
    assert result["published_evidence_ids"]
    assert (repo.objects / digest).read_bytes() == body
    hits = repo.search(tender["id"], "14.7")
    assert hits and "14.7" in hits[0]["text"]
    current = repo.artifact_evidence(tender["id"], artifact["id"])
    assert current[0]["extraction_current"] is True
    assert current[0]["id"] == hits[0]["id"]
    opened = repo.get_evidence(tender["id"], hits[0]["id"])
    assert opened["text"].startswith("Recovered clause 14.7")


def test_same_hash_in_two_tenders_publishes_only_the_selected_tender(tmp_path, monkeypatch):
    from quantix import extraction_worker

    source = tmp_path / "blank.pdf"
    body = _blank_pdf(source)
    repo = Repository(tmp_path / "home")
    digest = _store(repo, body)
    first = repo.create_tender("Alpha")
    second = repo.create_tender("Beta")
    _artifact(repo, first["id"], digest, body, name="A/scan.pdf")
    other = _artifact(repo, second["id"], digest, body, name="B/scan.pdf")
    monkeypatch.setattr(
        extraction_worker,
        "ocr_available",
        lambda **kwargs: {
            "available": True,
            "tesseract": True,
            "engine": "synthetic",
            "executable": "synthetic",
        },
    )
    monkeypatch.setattr(extraction_worker, "render_page_png", lambda *args, **kwargs: b"png")
    monkeypatch.setattr(
        extraction_worker,
        "ocr_png",
        lambda png, **kwargs: "Only tender Alpha recovered this clause.",
    )

    ExtractionService(repo).reprocess(
        digest,
        path=repo.objects / digest,
        request=ReprocessRequest(
            original_hash=digest, reader_id="ocr", reader_version="synthetic-1"
        ),
        tender_id=first["id"],
    )

    assert repo.search(first["id"], "Alpha recovered")
    assert repo.search(second["id"], "Alpha recovered") == []
    assert repo.artifact_evidence(second["id"], other["id"])[0]["text"] == ""


def test_partial_reprocess_keeps_unread_pages_and_does_not_erase_readable_text(
    tmp_path, monkeypatch
):
    from quantix import extraction_worker

    source = tmp_path / "blank.pdf"
    body = _blank_pdf(source, pages=2)
    repo = Repository(tmp_path / "home")
    digest = _store(repo, body)
    tender = repo.create_tender("Partial")
    artifact = _artifact(
        repo,
        tender["id"],
        digest,
        body,
        segments=[
            {"locator": "page:1", "text": "Original page one blinding concrete.", "page": 1},
            {"locator": "page:2", "text": "Original page two formwork timber.", "page": 2},
        ],
    )
    first_ids = {
        row["locator"]: row["id"] for row in repo.artifact_evidence(tender["id"], artifact["id"])
    }
    monkeypatch.setattr(
        extraction_worker,
        "ocr_available",
        lambda **kwargs: {
            "available": True,
            "tesseract": True,
            "engine": "synthetic",
            "executable": "synthetic",
        },
    )
    monkeypatch.setattr(extraction_worker, "render_page_png", lambda *args, **kwargs: b"png")
    monkeypatch.setattr(
        extraction_worker, "ocr_png", lambda png, **kwargs: "OCR replacement for page one only."
    )

    result = ExtractionService(repo).reprocess(
        digest,
        path=repo.objects / digest,
        request=ReprocessRequest(
            original_hash=digest, reader_id="ocr", reader_version="synthetic-1", page_limit=1
        ),
        tender_id=tender["id"],
    )

    assert "page:2" in result["retained_locators"]
    pages = {row["locator"]: row for row in repo.artifact_evidence(tender["id"], artifact["id"])}
    assert "OCR replacement" in pages["page:1"]["text"]
    assert pages["page:2"]["text"] == "Original page two formwork timber."
    assert pages["page:2"]["id"] == first_ids["page:2"]
    old = repo.get_evidence(tender["id"], first_ids["page:1"])
    assert old["extraction_current"] is False
    assert "Original page one" in old["text"]


def test_failed_publication_does_not_leave_a_partial_current_extraction(tmp_path, monkeypatch):
    from quantix import extraction_worker

    source = tmp_path / "blank.pdf"
    body = _blank_pdf(source)
    repo = Repository(tmp_path / "home")
    digest = _store(repo, body)
    tender = repo.create_tender("Fail")
    artifact = _artifact(
        repo,
        tender["id"],
        digest,
        body,
        segments=[{"locator": "page:1", "text": "Keep this original sentence.", "page": 1}],
    )
    original = repo.artifact_evidence(tender["id"], artifact["id"])[0]
    monkeypatch.setattr(
        extraction_worker,
        "ocr_available",
        lambda **kwargs: {
            "available": True,
            "tesseract": True,
            "engine": "synthetic",
            "executable": "synthetic",
        },
    )
    monkeypatch.setattr(extraction_worker, "render_page_png", lambda *args, **kwargs: b"png")
    monkeypatch.setattr(extraction_worker, "ocr_png", lambda png, **kwargs: "Should not persist.")

    def boom(*args, **kwargs):
        raise RuntimeError("synthetic publication failure")

    monkeypatch.setattr("quantix.extraction_adapters.publish_extraction", boom)
    with pytest.raises(RuntimeError, match="publication failure"):
        ExtractionService(repo).reprocess(
            digest,
            path=repo.objects / digest,
            request=ReprocessRequest(
                original_hash=digest, reader_id="ocr", reader_version="synthetic-1"
            ),
            tender_id=tender["id"],
        )
    remaining = repo.artifact_evidence(tender["id"], artifact["id"])
    assert remaining[0]["id"] == original["id"]
    assert remaining[0]["text"] == "Keep this original sentence."
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM extraction_versions").fetchone()[0] == 0


def test_revised_file_is_not_overwritten_by_reprocessing_the_old_hash(tmp_path, monkeypatch):
    from quantix import extraction_worker

    source = tmp_path / "blank.pdf"
    body = _blank_pdf(source)
    repo = Repository(tmp_path / "home")
    digest = _store(repo, body)
    tender = repo.create_tender("Revise")
    _artifact(repo, tender["id"], digest, body, name="Plant/pump.pdf")
    new_body = body + b"\n"
    new_digest = _store(repo, new_body)
    current, _ = repo.register_artifact(
        tender["id"],
        "Plant/pump.pdf",
        new_digest,
        len(new_body),
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [{"locator": "page:1", "text": "Revised pump output 82.", "page": 1}],
        },
    )
    monkeypatch.setattr(
        extraction_worker,
        "ocr_available",
        lambda **kwargs: {
            "available": True,
            "tesseract": True,
            "engine": "synthetic",
            "executable": "synthetic",
        },
    )
    monkeypatch.setattr(extraction_worker, "render_page_png", lambda *args, **kwargs: b"png")
    monkeypatch.setattr(
        extraction_worker, "ocr_png", lambda png, **kwargs: "Stale OCR of the old file."
    )

    with pytest.raises(ValueError, match="current file"):
        ExtractionService(repo).reprocess(
            digest,
            path=repo.objects / digest,
            request=ReprocessRequest(
                original_hash=digest, reader_id="ocr", reader_version="synthetic-1"
            ),
            tender_id=tender["id"],
        )
    assert (
        repo.artifact_evidence(tender["id"], current["id"])[0]["text"] == "Revised pump output 82."
    )


def test_superseded_extraction_marks_saved_dependencies_for_review(tmp_path, monkeypatch):
    from quantix import extraction_worker

    source = tmp_path / "blank.pdf"
    body = _blank_pdf(source)
    repo = Repository(tmp_path / "home")
    digest = _store(repo, body)
    tender = repo.create_tender("Deps")
    artifact = _artifact(
        repo,
        tender["id"],
        digest,
        body,
        segments=[{"locator": "page:1", "text": "Old empty extraction.", "page": 1}],
    )
    evidence = repo.artifact_evidence(tender["id"], artifact["id"])[0]
    dependencies = DependencyService(repo)
    dependencies.capture(tender["id"], "work_product", "draft-one", [evidence["id"]])
    monkeypatch.setattr(
        extraction_worker,
        "ocr_available",
        lambda **kwargs: {
            "available": True,
            "tesseract": True,
            "engine": "synthetic",
            "executable": "synthetic",
        },
    )
    monkeypatch.setattr(extraction_worker, "render_page_png", lambda *args, **kwargs: b"png")
    monkeypatch.setattr(
        extraction_worker, "ocr_png", lambda png, **kwargs: "New readable extraction."
    )

    ExtractionService(repo).reprocess(
        digest,
        path=repo.objects / digest,
        request=ReprocessRequest(
            original_hash=digest, reader_id="ocr", reader_version="synthetic-1"
        ),
        tender_id=tender["id"],
    )
    impact = dependencies.status(tender["id"], "work_product", "draft-one")
    assert impact.state == "needs_review"
    assert "source_extraction_changed" in impact.review_reasons
    assert repo.get_evidence(tender["id"], evidence["id"])["extraction_current"] is False
