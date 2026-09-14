"""Page-level reprocessing. Originals are never rewritten."""

from __future__ import annotations

import hashlib
from collections.abc import Callable
from contextlib import closing
from pathlib import Path

import pypdfium2 as pdfium

from .db import dump, new_id, now
from .documents import _pdf_lock
from .extraction_models import ReprocessRequest, ReprocessResult
from .extraction_publication import publish_extraction

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS extraction_versions(
        id TEXT PRIMARY KEY,
        artifact_id TEXT,
        original_hash TEXT NOT NULL,
        reader_id TEXT NOT NULL,
        reader_version TEXT NOT NULL,
        extracted_pages INTEGER NOT NULL,
        exception_pages INTEGER NOT NULL,
        segments_json TEXT NOT NULL,
        created_at TEXT NOT NULL
    )
    """,
)


class ExtractionService:
    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)
            columns = {row[1] for row in conn.execute("PRAGMA table_info(extraction_versions)")}
            extras = {
                "tender_id": "TEXT",
                "configuration_json": "TEXT NOT NULL DEFAULT '{}'",
                "published_at": "TEXT",
            }
            for name, definition in extras.items():
                if name not in columns:
                    conn.execute(f"ALTER TABLE extraction_versions ADD COLUMN {name} {definition}")

    def reprocess(
        self,
        original_hash: str,
        pages: int | None = None,
        successful_pages: int | None = None,
        *,
        path: Path | None = None,
        request: ReprocessRequest | None = None,
        cancelled: Callable[[], bool] | None = None,
        tender_id: str | None = None,
    ) -> dict:
        source = Path(path) if path is not None else self.repo.objects / original_hash
        if not source.is_file():
            raise ValueError("The original file is missing; restore it before reprocessing.")
        before = source.read_bytes()
        digest = hashlib.sha256(before).hexdigest()
        if digest != original_hash:
            raise ValueError("The original hash does not match the saved file.")
        limit = (
            successful_pages
            if successful_pages is not None
            else (request.page_limit if request else None)
        )
        segments = []
        extracted = 0
        exceptions = 0
        from .extraction_worker import OCRTimeoutError, ocr_available, ocr_png, render_page_png

        def check_cancelled() -> None:
            if cancelled and cancelled():
                raise InterruptedError("Extraction reprocessing cancelled")

        # Keep the PDFium lock only while the current document/page handles are alive.
        with _pdf_lock(cancelled), pdfium.PdfDocument(source) as document:
            document_pages = len(document)
        total = pages if pages is not None else document_pages
        for index in range(total):
            check_cancelled()
            if limit is not None and index >= limit:
                exceptions += 1
                segments.append({"page": index + 1, "state": "timeout"})
                continue
            if index >= document_pages:
                exceptions += 1
                segments.append({"page": index + 1, "state": "missing"})
                continue
            try:
                with _pdf_lock(cancelled), pdfium.PdfDocument(source) as document:
                    with closing(document[index]) as page, closing(page.get_textpage()) as textpage:
                        text = textpage.get_text_bounded(errors="replace").replace("\r\n", "\n")
            except InterruptedError:
                raise
            except Exception as error:
                exceptions += 1
                segments.append(
                    {
                        "page": index + 1,
                        "state": "embedded_error",
                        "text": "",
                        "detail": type(error).__name__,
                    }
                )
                continue
            if text.strip():
                extracted += 1
                segments.append(
                    {
                        "page": index + 1,
                        "state": "extracted",
                        "text": text,
                        "method": "embedded_text",
                    }
                )
                continue

            ocr = ocr_available(home=self.repo.home)
            require_ocr = request is not None and request.reader_id == "ocr"
            if not ocr.get("available"):
                exceptions += 1
                segments.append(
                    {
                        "page": index + 1,
                        "state": "ocr_unavailable" if require_ocr else "no_text",
                        "text": "",
                        "method": "ocr" if require_ocr else "embedded_text",
                    }
                )
                continue
            try:
                check_cancelled()
                png = render_page_png(source, index + 1, home=self.repo.home, cancelled=cancelled)
                ocr_text = ocr_png(png, home=self.repo.home, cancelled=cancelled)
            except InterruptedError:
                raise
            except OCRTimeoutError:
                exceptions += 1
                segments.append(
                    {"page": index + 1, "state": "ocr_timeout", "text": "", "method": "ocr"}
                )
                continue
            except Exception as error:
                exceptions += 1
                segments.append(
                    {
                        "page": index + 1,
                        "state": "ocr_error",
                        "text": "",
                        "method": "ocr",
                        "detail": type(error).__name__,
                    }
                )
                continue
            if ocr_text:
                extracted += 1
                segments.append(
                    {
                        "page": index + 1,
                        "state": "extracted",
                        "text": ocr_text,
                        "method": "ocr",
                    }
                )
            else:
                exceptions += 1
                segments.append(
                    {"page": index + 1, "state": "ocr_empty", "text": "", "method": "ocr"}
                )
        check_cancelled()
        reader = request.reader_id if request else "pdfium-embedded-text"
        version = request.reader_version if request else "pypdfium2-5.13.0"
        configuration = {
            "reader_id": reader,
            "reader_version": version,
            "page_limit": limit,
        }
        targets = []
        if tender_id:
            self.repo.get_tender(tender_id)
            wanted = request.artifact_id if request else None
            for artifact in self.repo.list_artifacts(tender_id, current_only=True):
                if artifact["content_hash"] != original_hash:
                    continue
                if wanted and artifact["id"] != wanted:
                    continue
                targets.append(artifact)
            if wanted and not targets:
                raise ValueError("The source artifact is not a current file in this Tender.")
            if not targets:
                raise ValueError("The source hash is not a current file in this Tender.")
        stamp = now()
        published_artifact_ids: list[str] = []
        published_evidence_ids: list[str] = []
        retained_locators: list[str] = []
        identifier = new_id()
        generation = None
        with self.repo.atomic() as conn:
            check_cancelled()
            if not targets:
                conn.execute(
                    """INSERT INTO extraction_versions(
                        id,artifact_id,original_hash,reader_id,reader_version,extracted_pages,
                        exception_pages,segments_json,created_at,tender_id,configuration_json,published_at
                    ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)""",
                    (
                        identifier,
                        None,
                        original_hash,
                        reader,
                        version,
                        extracted,
                        exceptions,
                        dump(segments),
                        stamp,
                        tender_id,
                        dump(configuration),
                        None,
                    ),
                )
            else:
                first_id = None
                for artifact in targets:
                    check_cancelled()
                    extraction_id = new_id()
                    first_id = first_id or extraction_id
                    conn.execute(
                        """INSERT INTO extraction_versions(
                            id,artifact_id,original_hash,reader_id,reader_version,extracted_pages,
                            exception_pages,segments_json,created_at,tender_id,configuration_json,published_at
                        ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)""",
                        (
                            extraction_id,
                            artifact["id"],
                            original_hash,
                            reader,
                            version,
                            extracted,
                            exceptions,
                            dump(segments),
                            stamp,
                            tender_id,
                            dump(configuration),
                            stamp,
                        ),
                    )
                    outcome = publish_extraction(
                        conn,
                        artifact=artifact,
                        extraction_id=extraction_id,
                        segments=segments,
                        stamp=stamp,
                    )
                    published_artifact_ids.append(artifact["id"])
                    published_evidence_ids.extend(item["id"] for item in outcome["published"])
                    retained_locators.extend(outcome["retained_locators"])
                identifier = first_id
                generation = self.repo.advance_retrieval_generation(tender_id, conn)
        if generation is not None:
            self.repo.notify_retrieval_generation(tender_id)
        after = source.read_bytes()
        return ReprocessResult(
            id=identifier,
            original_hash_unchanged=hashlib.sha256(after).hexdigest() == original_hash
            and after == before,
            extracted_pages=extracted,
            exception_pages=exceptions,
            reader_id=reader,
            reader_version=version,
            published_artifact_ids=published_artifact_ids,
            published_evidence_ids=published_evidence_ids,
            retained_locators=list(dict.fromkeys(retained_locators)),
            retrieval_generation=generation,
        ).model_dump()
