"""How the office reads the package beyond single pages: a spreadsheet's rows, what changed in a newer copy of a
document, what each document is, and how much of the package Quantix read, the office opened and its work cites.
Imported, read, opened and cited are kept apart."""

import difflib
import re
from dataclasses import dataclass

from pydantic import BaseModel, Field
from sqlalchemy import and_, case, func, select
from sqlalchemy.orm import Session

from quantix.boq.models import BoqItem, Fact
from quantix.core.review import LIVE
from quantix.documents import library
from quantix.documents.arabic import searchable
from quantix.documents.models import Document, Page
from quantix.estimate.models import Rate
from quantix.office.models import Opened
from quantix.subcontract.models import Package, Quote
from quantix.submission.models import PricingColumns, Requirement
from quantix.takeoff.models import Measurement, Scale

ROWS = 80  # spreadsheet rows at a time
_ROW = re.compile(r"^[A-Z]{1,3}(\d+)=")
KINDS = ("contract", "specification", "drawing", "boq", "addendum", "quote", "report", "form", "other")


def sheet_rows(
    page_text: str, first_row: int = 1, last_row: int | None = None, words: str = ""
) -> tuple[str, list[str]]:
    """The sheet's name line, and its rows as read_page shows them: a range of rows, or the rows with every word."""
    lines = page_text.splitlines()
    name = lines[0] if lines and lines[0].startswith("Sheet:") else ""
    rows = [line for line in lines if _ROW.match(line)]
    if words.strip():
        wanted = searchable(words).split()
        return name, [line for line in rows if all(w in searchable(line) for w in wanted)]
    last = last_row if last_row is not None else 10**9
    return name, [line for line in rows if first_row <= int(_ROW.match(line).group(1)) <= last]


def _pages(session: Session, document_id: str) -> dict[int, str]:
    rows = session.execute(select(Page.number, Page.text).where(Page.document_id == document_id))
    return {number: text for number, text in rows}


def changes(session: Session, older: Document, newer: Document, per_page: int = 12) -> list[str]:
    """Page by page, what the newer copy adds, removes or says differently, in the documents' own words."""
    before, after = _pages(session, older.id), _pages(session, newer.id)
    found = []
    for number in sorted(set(before) | set(after)):
        if number not in before:
            found.append(f"Page {number} is new.")
        elif number not in after:
            found.append(f"Page {number} is gone.")
        elif before[number] != after[number]:
            diff = [
                line
                for line in difflib.unified_diff(before[number].splitlines(), after[number].splitlines(), lineterm="")
                if line[:1] in "+-" and not line.startswith(("+++", "---")) and line[1:].strip()
            ]
            shown = [f"  {'now' if line[0] == '+' else 'was'}: {line[1:].strip()}" for line in diff[:per_page]]
            more = [f"  … and {len(diff) - per_page} more changed lines."] if len(diff) > per_page else []
            found += [f"Page {number} changed:", *shown, *more]
    return found


class DocumentNote(BaseModel):
    """What one document is, for the package map."""

    document_id: str
    kind: str = Field(description=f"One of {', '.join(KINDS)}")
    summary: str = Field(description="What it covers and what it matters for, in one or two sentences")


def describe(session: Session, tender_id: str, notes: list[DocumentNote]) -> str:
    done, problems = 0, []
    for note in notes:
        document = session.get(Document, note.document_id)
        kind, summary = note.kind.strip().lower(), " ".join(note.summary.split())
        if document is None or document.tender_id != tender_id:
            problems.append(f"{note.document_id}: no document of this tender has that id")
        elif kind not in KINDS:
            problems.append(f"{document.name}: kind is one of {', '.join(KINDS)}")
        elif not 5 <= len(summary.split()) <= 60:
            problems.append(f"{document.name}: say what it covers in one or two sentences")
        else:
            document.group_name, document.description = GROUPS[kind], summary
            done += 1
    return f"Described {done} documents." + ("\nNot done: " + "; ".join(problems) if problems else "")


GROUPS = {
    "contract": "Contract and conditions",
    "specification": "Specifications",
    "drawing": "Drawings",
    "boq": "Bills of quantities",
    "addendum": "Addenda and clarifications",
    "quote": "Quotes",
    "report": "Reports",
    "form": "Forms and schedules",
    "other": "Other",
}


@dataclass
class Coverage:
    document: Document
    pages: int
    own_text: int  # pages with text of their own
    scans: int  # pages without: read by OCR, still waiting, or with no words
    read_by_ocr: int
    ocr_waiting: int
    opened: int  # pages someone in the office opened
    cited: int  # pages the office's work cites


def _cited(session: Session, tender_id: str) -> set[tuple[str, int]]:
    found: set[tuple[str, int]] = set()
    for model in (BoqItem, Fact, Scale, Measurement):
        rows = select(model.document_id, model.page).where(model.tender_id == tender_id, model.status.in_(LIVE))
        found |= set(session.execute(rows).all())
    rates = select(Rate.document_id, Rate.page).where(
        Rate.tender_id == tender_id, Rate.status.in_(LIVE), Rate.document_id.is_not(None)
    )
    found |= set(session.execute(rates).all())
    requirements = select(Requirement.document_id, Requirement.page).where(
        Requirement.tender_id == tender_id, Requirement.document_id.is_not(None)
    )
    found |= set(session.execute(requirements).all())
    columns = select(PricingColumns.document_id, PricingColumns.sheet).where(PricingColumns.tender_id == tender_id)
    found |= set(session.execute(columns).all())
    quotes = select(Quote).join(Package, Package.id == Quote.package_id).where(Package.tender_id == tender_id)
    for quote in session.scalars(quotes):
        found |= {(quote.document_id, line["page"]) for line in quote.lines + quote.exclusions}
    return {(d, p) for d, p in found if d and p}


def coverage(session: Session, tender_id: str) -> list[Coverage]:
    """Each current document: its pages, how many Quantix read from their own text or by OCR, how many the office
    opened and how many its work cites."""
    counts = {
        document_id: (total, own, ocr, waiting)
        for document_id, total, own, ocr, waiting in session.execute(
            select(
                Page.document_id,
                func.count(Page.id),
                func.sum(case((and_(Page.has_text, Page.ocr.is_(None)), 1), else_=0)),
                func.sum(case((Page.ocr.in_(("en", "ar")), 1), else_=0)),
                func.sum(case((and_(Page.has_text.is_(False), Page.ocr.is_(None)), 1), else_=0)),
            )
            .join(Document, Document.id == Page.document_id)
            .where(Document.tender_id == tender_id)
            .group_by(Page.document_id)
        )
    }
    opened: dict[str, int] = {}
    refs = select(Opened.ref).where(Opened.tender_id == tender_id, Opened.kind == "page").distinct()
    for ref in session.scalars(refs):
        document_id = ref.split(":")[0]
        opened[document_id] = opened.get(document_id, 0) + 1
    cited: dict[str, int] = {}
    for document_id, _ in _cited(session, tender_id):
        cited[document_id] = cited.get(document_id, 0) + 1
    result = []
    for document in library.documents(session, tender_id):
        if document.status == "replaced":
            continue
        total, own, ocr, waiting = counts.get(document.id, (0, 0, 0, 0))
        own, ocr = own or 0, ocr or 0
        waiting = (waiting or 0) if document.kind in ("pdf", "image") else 0
        result.append(
            Coverage(
                document, total, own, total - own, ocr, waiting, opened.get(document.id, 0), cited.get(document.id, 0)
            )
        )
    return result
