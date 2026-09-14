"""Bounded, local document evidence extraction. Source files are never rewritten."""

from __future__ import annotations

import io
import json
import math
import os
import posixpath
import re
import threading
import warnings
import zipfile
from collections.abc import Callable
from contextlib import closing, contextmanager
from dataclasses import dataclass, field
from datetime import date, datetime, time
from pathlib import Path
from xml.etree import ElementTree as ET

import openpyxl
import pypdfium2 as pdfium
from docx import Document
from docx.table import Table
from docx.text.paragraph import Paragraph
from openpyxl.utils import coordinate_to_tuple, get_column_letter

MAX_FILE_BYTES = 256 * 1024 * 1024
MAX_ZIP_BYTES = 128 * 1024 * 1024
MAX_ZIP_ENTRY_BYTES = 32 * 1024 * 1024
MAX_ZIP_MEMBERS = 10_000
MAX_PDF_PAGES = 2_000
# Text recognition is slow, so only the first scanned pages of a document are read.
# Every scanned page is recognised; pages below this word confidence are flagged
# for a second reading (AI vision when the Tender's AI supports images).
OCR_LOW_CONFIDENCE = 60.0
OCR_WORKERS = max(1, min(4, (os.cpu_count() or 2) // 2))
MAX_ROWS = 20_000
MAX_COLUMNS = 256
MAX_CELLS = 500_000
MAX_SEGMENTS = 50_000
MAX_OUTPUT_BYTES = 12 * 1024 * 1024
MAX_SEGMENT_CHARS = 200_000
MAX_WARNINGS = 1_000
MAX_RENDER_PIXELS = 12_000_000
MAX_RENDER_SIDE = 4_096
PDFIUM_LOCK = threading.Lock()
_SPREADSHEET_LOCK = threading.Lock()

_S = "{http://schemas.openxmlformats.org/spreadsheetml/2006/main}"
_W = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"
_HEADING_STYLE = re.compile(r"^heading\s*(\d+)$", re.I)


def _docx_paragraph_structure(paragraph) -> dict:
    """Record observed Word heading/style. Do not invent a heading from short lines."""

    text = (paragraph.text or "").strip()
    style = ""
    try:
        style = str(getattr(paragraph.style, "name", "") or "")
    except (AttributeError, ValueError):
        style = ""
    match = _HEADING_STYLE.match(style.strip())
    if match:
        return {
            "block_kind": "heading",
            "heading": text,
            "heading_level": int(match.group(1)),
            "style_name": style,
        }
    lowered = style.strip().casefold()
    if lowered in {"title", "subtitle"}:
        return {
            "block_kind": "heading",
            "heading": text,
            "heading_level": 0 if lowered == "title" else 1,
            "style_name": style,
        }
    outline = None
    try:
        properties = paragraph._element.pPr
        if properties is not None and properties.outlineLvl is not None:
            outline = int(properties.outlineLvl.val)
    except (AttributeError, TypeError, ValueError):
        outline = None
    if outline is not None and 0 <= outline <= 8 and text:
        return {
            "block_kind": "heading",
            "heading": text,
            "heading_level": outline + 1,
            "style_name": style or None,
            "heading_source": "outline_level",
        }
    return {
        "block_kind": "word_paragraph",
        "style_name": style or None,
    }


def _observed_sheet_header(cells: list[dict]) -> tuple[list[dict], bool]:
    """Use a row as headers only when it is observed text without formulas."""

    if not cells:
        return [], True
    if any(item.get("formula") for item in cells):
        return [], True
    labels = []
    for item in cells:
        value = item.get("value")
        if not isinstance(value, str) or not value.strip():
            continue
        coordinate = str(item.get("coordinate") or "")
        column = "".join(ch for ch in coordinate if ch.isalpha())
        labels.append({"column": column, "label": value.strip(), "coordinate": coordinate})
    if not labels:
        return [], True
    return labels, False


@dataclass
class Segment:
    locator: str
    text: str
    page: int | None = None
    sheet: str | None = None
    cell_range: str | None = None
    kind: str = "text"
    metadata: dict = field(default_factory=dict)


@dataclass
class Extraction:
    kind: str
    status: str
    segments: list[Segment] = field(default_factory=list)
    warnings: list[dict] = field(default_factory=list)
    metadata: dict = field(default_factory=dict)


class _LimitReached(Exception):
    pass


def _cancel(cancelled: Callable[[], bool] | None) -> None:
    if cancelled and cancelled():
        raise InterruptedError("Document processing cancelled")


def _warn(result: Extraction, code: str, message: str, locator: str | None = None) -> None:
    result.status = "needs_attention"
    if len(result.warnings) >= MAX_WARNINGS:
        result.metadata["additional_warning_count"] = (
            result.metadata.get("additional_warning_count", 0) + 1
        )
        return
    warning = {"code": code, "message": message}
    if locator:
        warning["locator"] = locator
    result.warnings.append(warning)


class _Collector:
    def __init__(self, result: Extraction):
        self.result = result
        self.output_bytes = 0

    def add(self, segment: Segment) -> None:
        size = len(segment.text.encode("utf-8")) + len(
            json.dumps(segment.metadata, ensure_ascii=False, allow_nan=False).encode("utf-8")
        )
        if (
            len(segment.text) > MAX_SEGMENT_CHARS
            or self.output_bytes + size > MAX_OUTPUT_BYTES
            or len(self.result.segments) >= MAX_SEGMENTS
        ):
            _warn(
                self.result,
                "output_limit",
                "Extraction stopped at the evidence output limit; remaining content was not indexed.",
                segment.locator,
            )
            raise _LimitReached
        self.output_bytes += size
        self.result.segments.append(segment)


@contextmanager
def _pdf_lock(cancelled=None):
    while not PDFIUM_LOCK.acquire(timeout=0.1):
        _cancel(cancelled)
    try:
        _cancel(cancelled)
        yield
    finally:
        PDFIUM_LOCK.release()


def _archive(path: Path) -> zipfile.ZipFile:
    archive = zipfile.ZipFile(path)
    infos = archive.infolist()
    if (
        len(infos) > MAX_ZIP_MEMBERS
        or sum(i.file_size for i in infos) > MAX_ZIP_BYTES
        or any(i.file_size > MAX_ZIP_ENTRY_BYTES or i.flag_bits & 1 for i in infos)
    ):
        archive.close()
        raise _LimitReached("Office package exceeds the size limit or contains encrypted members")
    return archive


def _xml(archive: zipfile.ZipFile, name: str) -> bytes:
    data = archive.read(name)
    declaration_scan = data.upper().replace(b"\x00", b"")
    if b"<!DOCTYPE" in declaration_scan or b"<!ENTITY" in declaration_scan:
        raise ValueError("XML entity declarations are not supported")
    return data


def _kind_from_name(name: str) -> str:
    suffix = Path(name).suffix.lower()
    if suffix == ".pdf":
        return "pdf"
    if suffix in {".xlsx", ".xlsm", ".xls", ".xlsb"}:
        return "spreadsheet"
    if suffix in {".docx", ".doc", ".docm", ".rtf"}:
        return "word"
    if suffix in {".dwg", ".dxf"}:
        return "cad"
    return "other"


def extract_document(
    path: Path, original_name: str, cancelled: Callable[[], bool] | None = None
) -> Extraction:
    """Extract exact evidence, or report a visible format/coverage exception."""
    _cancel(cancelled)
    path = Path(path)
    result = Extraction(_kind_from_name(original_name), "extracted")
    collector = _Collector(result)
    try:
        size = path.stat().st_size
        result.metadata["size_bytes"] = size
        if size > MAX_FILE_BYTES:
            _warn(result, "file_limit", "This file exceeds the local extraction size limit.")
            return result
        with path.open("rb") as source:
            header = source.read(1024)
        _cancel(cancelled)
        if header.startswith(b"%PDF-"):
            result.kind = "pdf"
            _extract_pdf(path, result, collector, cancelled)
        elif header.startswith(b"PK"):
            with _archive(path) as archive:
                names = set(archive.namelist())
                # Preflight all XML before handing the package to parser libraries.
                for name in names:
                    _cancel(cancelled)
                    if name.endswith((".xml", ".rels")):
                        _xml(archive, name)
                if "xl/workbook.xml" in names:
                    result.kind = "spreadsheet"
                    _extract_spreadsheet(path, archive, result, collector, cancelled)
                elif "word/document.xml" in names:
                    result.kind = "word"
                    _extract_docx(path, archive, result, collector, cancelled)
                else:
                    result.status = "unsupported"
                    result.warnings.append(
                        {
                            "code": "unsupported_archive",
                            "message": "This archive is not a supported Excel or Word document.",
                        }
                    )
        elif Path(original_name).suffix.lower() == ".dxf":
            result.kind = "cad"
            from .cad_reader import CadReaderService

            try:
                inspection = CadReaderService().inspect(path=path)
                result.metadata["cad"] = inspection
                if not inspection.get("complete_coverage", True):
                    result.status = "needs_attention"
                    result.warnings.append(
                        {
                            "code": "cad_incomplete",
                            "message": "DXF coverage is incomplete. Missing xrefs or units are recorded as exceptions.",
                        }
                    )
            except Exception as exc:
                result.status = "needs_attention"
                result.warnings.append(
                    {
                        "code": "dxf_read_error",
                        "message": f"The DXF drawing could not be fully read ({type(exc).__name__}).",
                    }
                )
        elif header[:2] == b"AC" and header[2:6].isdigit():
            result.kind = "cad"
            result.metadata["dwg_version"] = header[:6].decode("ascii")
            from .legacy_converter import CadConversionUnavailable, convert_dwg

            try:
                dxf = convert_dwg(path)
                from .cad_reader import CadReaderService

                inspection = CadReaderService().inspect(path=dxf)
                result.metadata["cad"] = inspection
                if not inspection.get("complete_coverage", True):
                    result.status = "needs_attention"
            except CadConversionUnavailable as exc:
                result.status = "unsupported"
                result.warnings.append(
                    {
                        "code": "dwg_conversion_required",
                        "message": str(exc),
                    }
                )
        elif (
            header.startswith(b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1")
            and Path(original_name).suffix.lower() == ".doc"
        ):
            from .document_word import WordConversionUnavailable, convert_legacy_word

            result.kind = "word"
            result.metadata["format"] = "ole_word"
            try:
                converted = convert_legacy_word(path, cancelled)
                with _archive(io.BytesIO(converted)) as archive:
                    for name in archive.namelist():
                        _cancel(cancelled)
                        if name.endswith((".xml", ".rels")):
                            _xml(archive, name)
                    _extract_docx(io.BytesIO(converted), archive, result, collector, cancelled)
                result.metadata.update(
                    {
                        "format": "ole_word",
                        "conversion_engine": "Microsoft Word",
                        "source_locator_basis": "converted_docx_structure",
                    }
                )
            except WordConversionUnavailable as exc:
                result.status = "unsupported"
                result.warnings.append({"code": "doc_conversion_unavailable", "message": str(exc)})
        elif header.lstrip().startswith(b"{\\rtf"):
            result.kind = "word"
            result.metadata["format"] = "rtf"
            result.status = "unsupported"
            result.warnings.append(
                {
                    "code": "rtf_conversion_required",
                    "message": "RTF content requires local conversion before text can be indexed.",
                }
            )
        elif result.kind in {"pdf", "spreadsheet", "word"} and Path(
            original_name
        ).suffix.lower() in {".pdf", ".xlsx", ".xlsm", ".docx"}:
            result.status = "failed"
            result.warnings.append(
                {
                    "code": "format_mismatch",
                    "message": "File contents do not match the declared document format.",
                }
            )
        else:
            result.status = "unsupported"
            result.warnings.append(
                {
                    "code": "unsupported_format",
                    "message": "This file format is registered but is not supported for text extraction.",
                }
            )
    except InterruptedError:
        raise
    except _LimitReached as exc:
        if not result.warnings or result.warnings[-1]["code"] != "output_limit":
            _warn(
                result,
                "package_limit",
                str(exc) or "The document reached a processing limit; coverage is incomplete.",
            )
    except Exception as exc:
        result.status = "needs_attention" if result.segments else "failed"
        result.warnings.append(
            {
                "code": "parse_error",
                "message": f"The document could not be fully read ({type(exc).__name__}).",
            }
        )
    result.metadata["segment_count"] = len(result.segments)
    return result


def _extract_pdf(path, result, collector, cancelled):
    result.metadata["extraction_scope"] = "embedded_text"
    scanned = []
    with _pdf_lock(cancelled), pdfium.PdfDocument(path) as document:
        result.metadata["page_count"] = len(document)
        if not len(document):
            _warn(result, "pdf_no_pages", "The PDF contains no pages to index.")
        for index in range(min(len(document), MAX_PDF_PAGES)):
            _cancel(cancelled)
            locator = f"page:{index + 1}"
            with closing(document[index]) as page, closing(page.get_textpage()) as textpage:
                text = textpage.get_text_bounded(errors="replace").replace("\r\n", "\n")
                if not text.strip():
                    # Rendering needs the same PDFium lock, so read these after.
                    scanned.append((index + 1, page.get_width(), page.get_height()))
                else:
                    collector.add(
                        Segment(
                            locator,
                            text,
                            page=index + 1,
                            metadata={
                                "page_width": page.get_width(),
                                "page_height": page.get_height(),
                                "method": "embedded_text",
                                "block_kind": "page",
                                "structure_uncertain": True,
                            },
                        )
                    )
                    if text.count("\ufffd") > max(2, len(text) // 100):
                        _warn(
                            result,
                            "pdf_text_encoding",
                            "Some text characters could not be decoded; compare with the page image.",
                            locator,
                        )
            result.metadata["pages_processed"] = index + 1
        if len(document) > MAX_PDF_PAGES:
            _warn(
                result,
                "page_limit",
                f"Only the first {MAX_PDF_PAGES} pages were indexed; remaining pages need processing.",
            )
    _read_scanned_pages(path, result, collector, cancelled, scanned)


def _read_scanned_pages(path, result, collector, cancelled, pages):
    """Read pages with no stored text using local text recognition, when it is installed."""

    if not pages:
        return
    from concurrent.futures import ThreadPoolExecutor

    from . import extraction_worker
    from .extraction_worker import OCRProcessError, OCRTimeoutError
    from .tesseract_runtime import tesseract_languages

    unreadable = "This page has no text Quantix can read. It may be blank or scanned; check the page preview."
    if extraction_worker.ocr_available().get("available") is not True:
        for number, _width, _height in pages:
            _warn(result, "pdf_no_text", unreadable, f"page:{number}")
        return
    installed = tesseract_languages()
    languages = "+".join(code for code in ("eng", "ara") if code in installed) or "eng"
    stop = cancelled or (lambda: False)

    def recognise(number):
        # Rendering shares the PDFium lock; recognition runs in parallel processes.
        image = extraction_worker.render_page_png(path, number, cancelled=stop, for_ocr=True)
        return extraction_worker.ocr_page(image, languages=languages, cancelled=stop)

    recognised = 0
    with ThreadPoolExecutor(max_workers=OCR_WORKERS, thread_name_prefix="quantix-ocr") as pool:
        futures = [
            (number, width, height, pool.submit(recognise, number))
            for number, width, height in pages
        ]
        for number, width, height, future in futures:
            _cancel(cancelled)
            locator = f"page:{number}"
            try:
                page = future.result()
            except InterruptedError:
                for *_rest, pending in futures:
                    pending.cancel()
                raise
            except (OCRTimeoutError, OCRProcessError, OSError, ValueError):
                page = None
            if page is None or not page.text.strip():
                _warn(result, "pdf_no_text", unreadable, locator)
                continue
            collector.add(
                Segment(
                    locator,
                    page.text,
                    page=number,
                    metadata={
                        "page_width": width,
                        "page_height": height,
                        "method": "ocr",
                        "ocr_confidence": page.confidence,
                        "block_kind": "page",
                        "structure_uncertain": True,
                    },
                )
            )
            if page.confidence < OCR_LOW_CONFIDENCE:
                _warn(
                    result,
                    "ocr_low_confidence",
                    "Text recognition on this page is uncertain; it will be read again before it is relied on.",
                    locator,
                )
            recognised += 1
    if recognised:
        result.metadata["extraction_scope"] = "embedded_text+ocr"
        result.metadata["ocr_pages"] = recognised
        if "ara" not in installed:
            _warn(
                result,
                "ocr_language_missing",
                "Arabic text recognition is not installed, so Arabic pages may be read incorrectly. "
                "Add the Arabic language data to Tesseract and import again.",
            )


def render_pdf_page(path: Path, page: int, max_width: int = 1600) -> bytes:
    """Render one page under the same PDFium lock used for extraction."""
    if isinstance(page, bool) or not isinstance(page, int) or page < 1:
        raise ValueError("Page must be a positive integer")
    if isinstance(max_width, bool) or not isinstance(max_width, int) or max_width < 1:
        raise ValueError("Preview width must be a positive integer")
    path = Path(path)
    if path.stat().st_size > MAX_FILE_BYTES:
        raise ValueError("File exceeds the preview size limit")
    with _pdf_lock(), pdfium.PdfDocument(path) as document:
        if page > len(document):
            raise ValueError("Page is outside the document")
        with closing(document[page - 1]) as pdf_page:
            width, height = pdf_page.get_size()
            if width <= 0 or height <= 0 or not math.isfinite(width * height):
                raise ValueError("Page has invalid dimensions")
            scale = min(
                max_width / width,
                MAX_RENDER_SIDE / width,
                MAX_RENDER_SIDE / height,
                math.sqrt(MAX_RENDER_PIXELS / (width * height)),
            )
            with closing(pdf_page.render(scale=scale)) as bitmap:
                output = io.BytesIO()
                bitmap.to_pil().save(output, format="PNG")
                return output.getvalue()


def _json_value(value):
    if value is None or isinstance(value, (str, bool, int)):
        return value
    if isinstance(value, float):
        return value if math.isfinite(value) else str(value)
    if isinstance(value, (date, datetime, time)):
        return value.isoformat()
    if hasattr(value, "text") and hasattr(value, "ref"):
        return {"text": value.text, "ref": value.ref}
    return str(value)


def _sheet_context(archive, cancelled):
    relationships = ET.fromstring(_xml(archive, "xl/_rels/workbook.xml.rels"))
    targets = {}
    for relationship in relationships:
        if relationship.get("TargetMode") == "External":
            continue
        target = relationship.get("Target", "")
        targets[relationship.get("Id")] = posixpath.normpath(
            target.lstrip("/") if target.startswith("/") else posixpath.join("xl", target)
        )
    workbook = ET.fromstring(_xml(archive, "xl/workbook.xml"))
    result = {}
    for sheet in workbook.findall(f"{_S}sheets/{_S}sheet"):
        _cancel(cancelled)
        relationship_id = sheet.get(
            "{http://schemas.openxmlformats.org/officeDocument/2006/relationships}id"
        )
        target = targets.get(relationship_id)
        context = {
            "hidden_rows": set(),
            "hidden_columns": [],
            "merged_ranges": [],
            "max_row": 0,
            "max_column": 0,
        }
        if target:
            for _, element in ET.iterparse(io.BytesIO(_xml(archive, target)), events=("end",)):
                if element.tag == f"{_S}c" and element.get("r"):
                    row_number, column_number = coordinate_to_tuple(element.get("r"))
                    context["max_row"] = max(context["max_row"], row_number)
                    context["max_column"] = max(context["max_column"], column_number)
                elif element.tag == f"{_S}row":
                    _cancel(cancelled)
                    row_number = int(element.get("r", "0"))
                    if row_number <= MAX_ROWS and element.get("hidden") in {"1", "true"}:
                        context["hidden_rows"].add(row_number)
                elif element.tag == f"{_S}col" and element.get("hidden") in {"1", "true"}:
                    context["hidden_columns"].append(
                        (int(element.get("min", "0")), int(element.get("max", "0")))
                    )
                elif element.tag == f"{_S}mergeCell" and len(context["merged_ranges"]) < MAX_ROWS:
                    context["merged_ranges"].append(element.get("ref"))
                element.clear()
        result[sheet.get("name")] = context
    return result


def _extract_spreadsheet(path, archive, result, collector, cancelled):
    # Python 3.12 warning capture is process-global. Serialize our openpyxl
    # readers so a parser's coverage warning stays attached to its own workbook.
    while not _SPREADSHEET_LOCK.acquire(timeout=0.1):
        _cancel(cancelled)
    recorded = []
    try:
        with warnings.catch_warnings(record=True) as recorded:
            warnings.simplefilter("always", UserWarning)
            _read_spreadsheet(path, archive, result, collector, cancelled)
    finally:
        _SPREADSHEET_LOCK.release()
        seen = set()
        for warning in recorded:
            message = str(warning.message)
            if "openpyxl" in Path(warning.filename).parts:
                if message not in seen:
                    _warn(result, "spreadsheet_parser_warning", message)
                    seen.add(message)
            else:
                warnings.warn_explicit(
                    warning.message, warning.category, warning.filename, warning.lineno
                )


def _read_spreadsheet(path, archive, result, collector, cancelled):
    result.metadata.update(
        {
            "has_vba": "xl/vbaProject.bin" in archive.namelist(),
            "formula_cache_verified": False,
            "column_interpretation": "unverified",
        }
    )
    contexts = _sheet_context(archive, cancelled)
    formulas = caches = None
    rows_seen = cells_seen = 0
    try:
        # File handles permit content-sniffed OOXML stored under opaque filenames.
        with path.open("rb") as formula_file, path.open("rb") as cached_file:
            formulas = openpyxl.load_workbook(
                formula_file, read_only=True, data_only=False, keep_vba=False, keep_links=False
            )
            caches = openpyxl.load_workbook(
                cached_file, read_only=True, data_only=True, keep_vba=False, keep_links=False
            )
            result.metadata["sheets"] = [
                {
                    "name": s.title,
                    "state": s.sheet_state,
                    "max_row": contexts[s.title]["max_row"],
                    "max_column": contexts[s.title]["max_column"],
                    "declared_max_row": s.max_row,
                    "declared_max_column": s.max_column,
                }
                for s in formulas
            ]
            for sheet in formulas:
                _cancel(cancelled)
                context = contexts[sheet.title]
                if not context["max_row"] or not context["max_column"]:
                    continue
                columns = min(context["max_column"], MAX_COLUMNS)
                rows = min(context["max_row"], MAX_ROWS)
                observed_header: list[dict] = []
                header_uncertain = True
                if context["max_column"] > MAX_COLUMNS:
                    _warn(
                        result,
                        "column_limit",
                        f"Only the first {MAX_COLUMNS} columns were indexed.",
                        f"sheet:{sheet.title}",
                    )
                if context["max_row"] > MAX_ROWS:
                    _warn(
                        result,
                        "row_limit",
                        f"Only the first {MAX_ROWS} rows were indexed.",
                        f"sheet:{sheet.title}",
                    )
                cached_rows = caches[sheet.title].iter_rows(max_row=rows, max_col=columns)
                for row_number, (row, cached_row) in enumerate(
                    zip(sheet.iter_rows(max_row=rows, max_col=columns), cached_rows), 1
                ):
                    _cancel(cancelled)
                    rows_seen += 1
                    cells_seen += columns
                    if rows_seen > MAX_ROWS or cells_seen > MAX_CELLS:
                        _warn(
                            result,
                            "row_limit",
                            "Extraction stopped at the workbook row/cell limit; remaining cells were not indexed.",
                            f"sheet:{sheet.title}/row:{row_number}",
                        )
                        return
                    cells = []
                    for column, (cell, cached) in enumerate(zip(row, cached_row), 1):
                        if cell.value is None and cached.value is None:
                            continue
                        coordinate = f"{get_column_letter(column)}{row_number}"
                        formula = _json_value(cell.value) if cell.data_type == "f" else None
                        metadata = {
                            "coordinate": coordinate,
                            "value": _json_value(cell.value),
                            "cached_value": _json_value(cached.value),
                            "formula": formula,
                            "number_format": cell.number_format,
                            "data_type": cell.data_type,
                            "cached_data_type": cached.data_type,
                            "column_hidden": any(
                                start <= column <= end for start, end in context["hidden_columns"]
                            ),
                        }
                        source = f"sheet:{sheet.title}/cell:{coordinate}"
                        if (
                            cell.data_type == "e"
                            or cached.data_type == "e"
                            or (formula and "#REF!" in str(formula))
                        ):
                            metadata["error"] = (
                                cached.value if cached.data_type == "e" else cell.value
                            )
                            _warn(
                                result,
                                "spreadsheet_error",
                                "This cell contains a spreadsheet error or an invalid formula reference.",
                                source,
                            )
                        if formula and cached.value is None:
                            _warn(
                                result,
                                "formula_cache_missing",
                                "This formula has no stored result. Its value was not calculated.",
                                source,
                            )
                        cells.append(metadata)
                    if cells:
                        cell_range = f"{cells[0]['coordinate']}:{cells[-1]['coordinate']}"
                        text = " | ".join(
                            f"{c['coordinate']}: {c['value']}"
                            + (f" [stored result: {c['cached_value']}]" if c["formula"] else "")
                            for c in cells
                        )
                        if not observed_header:
                            observed_header, header_uncertain = _observed_sheet_header(cells)
                            row_role = (
                                "observed_header"
                                if observed_header and not header_uncertain
                                else "data"
                            )
                        else:
                            row_role = "data"
                        metadata = {
                            "cells": cells,
                            "row": row_number,
                            "row_hidden": row_number in context["hidden_rows"],
                            "sheet_state": sheet.sheet_state,
                            "merged_ranges": [
                                r
                                for r in context["merged_ranges"]
                                if openpyxl.worksheet.cell_range.CellRange(r).min_row
                                <= row_number
                                <= openpyxl.worksheet.cell_range.CellRange(r).max_row
                            ],
                            "column_interpretation": "unverified",
                            "block_kind": "spreadsheet_row",
                            "row_role": row_role,
                            "header_uncertain": header_uncertain,
                        }
                        if row_role == "data" and observed_header:
                            metadata["header_cells"] = observed_header
                        collector.add(
                            Segment(
                                f"sheet:{sheet.title}/range:{cell_range}",
                                text,
                                sheet=sheet.title,
                                cell_range=cell_range,
                                kind="spreadsheet_row",
                                metadata=metadata,
                            )
                        )
    finally:
        if formulas:
            formulas.close()
        if caches:
            caches.close()
        result.metadata["rows_processed"] = min(rows_seen, MAX_ROWS)
    if not result.segments:
        _warn(
            result,
            "spreadsheet_no_values",
            "No cell values were available to index in this workbook.",
        )


def _extract_docx(path, archive, result, collector, cancelled):
    result.metadata["format"] = "docx"
    if isinstance(path, Path):
        with path.open("rb") as source:
            document = Document(source)
    else:
        document = Document(path)

    def blocks(container, prefix="", depth=0, section_heading=None, section_level=None):
        if depth > 12:
            _warn(
                result,
                "word_nesting_limit",
                "Deeply nested Word tables were not fully indexed.",
                prefix,
            )
            return section_heading, section_level
        paragraphs = tables = 0
        heading = section_heading
        level = section_level
        for block in container.iter_inner_content():
            _cancel(cancelled)
            if isinstance(block, Paragraph):
                paragraphs += 1
                if block.text.strip():
                    observed = _docx_paragraph_structure(block)
                    kind = (
                        "heading" if observed.get("block_kind") == "heading" else "word_paragraph"
                    )
                    metadata = dict(observed)
                    if kind == "heading":
                        heading = observed.get("heading")
                        level = observed.get("heading_level")
                    else:
                        if heading:
                            metadata["section_heading"] = heading
                            metadata["heading_level"] = level
                    collector.add(
                        Segment(
                            f"{prefix}paragraph:{paragraphs}",
                            block.text,
                            kind=kind,
                            metadata=metadata,
                        )
                    )
            elif isinstance(block, Table):
                tables += 1
                seen_cells = set()
                first_row_labels = []
                for row_index, row in enumerate(block.rows, 1):
                    row_texts = []
                    for cell_index, cell in enumerate(row.cells, 1):
                        if cell._tc in seen_cells:
                            continue
                        seen_cells.add(cell._tc)
                        label = (cell.text or "").strip()
                        if label:
                            row_texts.append(label)
                        cell_meta = {
                            "block_kind": "table_cell",
                            "table_index": tables,
                            "table_row": row_index,
                            "table_cell": cell_index,
                            "header_uncertain": True,
                        }
                        if heading:
                            cell_meta["section_heading"] = heading
                            cell_meta["heading_level"] = level
                        if row_index == 1:
                            first_row_labels = row_texts
                            cell_meta["row_role"] = "first_row"
                        elif first_row_labels:
                            cell_meta["header_cells"] = [
                                {"label": item} for item in first_row_labels
                            ]
                        blocks(
                            cell,
                            f"{prefix}table:{tables}/row:{row_index}/cell:{cell_index}/",
                            depth + 1,
                            heading,
                            level,
                        )
                        # Attach table coordinates onto the segments just added for this cell.
                        cell_prefix = f"{prefix}table:{tables}/row:{row_index}/cell:{cell_index}/"
                        for segment in result.segments:
                            if (
                                segment.locator.startswith(cell_prefix)
                                and "table_index" not in segment.metadata
                            ):
                                segment.metadata.update(cell_meta)
        return heading, level

    blocks(document)
    seen_parts = set()
    for section_index, section in enumerate(document.sections, 1):
        for name in (
            "header",
            "first_page_header",
            "even_page_header",
            "footer",
            "first_page_footer",
            "even_page_footer",
        ):
            part = getattr(section, name)
            if part.is_linked_to_previous:
                continue
            part_name = str(part.part.partname)
            if part_name not in seen_parts:
                seen_parts.add(part_name)
                blocks(part, f"section:{section_index}/{name}/")
    for name in archive.namelist():
        _cancel(cancelled)
        if name.startswith("word/") and name.endswith(".xml"):
            root = ET.fromstring(_xml(archive, name))
            for tag, code, message in (
                (
                    "txbxContent",
                    "word_text_boxes",
                    "Text boxes require visual review; their text is not included in the paragraph/table extraction.",
                ),
                (
                    "ins",
                    "word_revisions",
                    "Tracked insertions are present; review changes in Word before relying on extracted text.",
                ),
                (
                    "del",
                    "word_revisions",
                    "Tracked deletions are present; review changes in Word before relying on extracted text.",
                ),
                (
                    "altChunk",
                    "word_embedded_content",
                    "Embedded document content was not extracted.",
                ),
            ):
                if root.find(f".//{_W}{tag}") is not None:
                    _warn(result, code, message, name)
            if name in {"word/footnotes.xml", "word/endnotes.xml"}:
                if any(e.text and e.text.strip() for e in root.iter(f"{_W}t")):
                    _warn(
                        result,
                        "word_notes",
                        "Footnotes or endnotes require review in the original document.",
                        name,
                    )
    if not result.segments:
        _warn(
            result,
            "word_no_text",
            "No readable paragraphs or table text were found; review the document visually.",
        )
