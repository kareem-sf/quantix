"""Newer copies of tender documents. When the engineer adds a newer copy of a file, Quantix moves the office's work
onto it wherever what the work cites is unchanged there: the quoted words on a page, a workbook row that only moved
because rows were added or taken out above it, or the drawing around a measurement or a scale. Whatever still
rests on an older copy has to be done again from the newer one: until it is, Quantix's checks block it and the
audit holds the release."""

import re
from bisect import bisect_left, bisect_right
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any

from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix.boq import records as boq
from quantix.boq.models import BoqItem, Fact
from quantix.core.review import LIVE
from quantix.documents import cad, library, readers
from quantix.documents.evidence import missing_piece
from quantix.documents.models import Document, Page
from quantix.estimate import records as estimate
from quantix.estimate.models import Rate
from quantix.subcontract.models import Company, Package, Quote
from quantix.submission.models import PricingColumns, Requirement
from quantix.takeoff import drawings
from quantix.takeoff import records as takeoff
from quantix.takeoff.models import Measurement, Scale

AROUND = 20.0  # page points: the drawing this far around a measurement or a scale line must be unchanged to move it
REVIEWED = {BoqItem: "boq", Fact: "fact", Scale: "scale", Measurement: "measurement", Rate: "rate"}
OTHERS = {Requirement: "checklist", PricingColumns: "columns", Quote: "quote"}
_CELL = re.compile(r"\b([A-Z]{1,3})\d+=")
_ROW = re.compile(r"\b[A-Z]{1,3}(\d+)=")


def resting_on(session: Session, document_ids: list[str]) -> list[Any]:
    """The office's live work that cites any of these documents."""
    found: list[Any] = []
    for model in REVIEWED:
        found += session.scalars(select(model).where(model.document_id.in_(document_ids), model.status.in_(LIVE)))
    for model in OTHERS:
        found += session.scalars(select(model).where(model.document_id.in_(document_ids)))
    return found


def _moved_row(page_text: str, quote: str) -> str | None:
    """A quoted workbook row with its row number as it is on this page, when exactly one row has the same cells."""
    if not _CELL.search(quote):
        return None
    found = set()
    for line in page_text.splitlines():
        row = _ROW.search(line)
        if row:
            moved = _CELL.sub(rf"\g<1>{row.group(1)}=", quote)
            if missing_piece(line, moved) is None:
                found.add(moved)
    return found.pop() if len(found) == 1 else None


def _within(points: list[tuple[float, float]], box: tuple[float, float, float, float]) -> set[tuple[float, float]]:
    left, top, right, bottom = box
    start, end = bisect_left(points, (left, float("-inf"))), bisect_right(points, (right, float("inf")))
    return {p for p in points[start:end] if top <= p[1] <= bottom}


class _NewerCopy:
    """A newer copy's pages and drawings, each read once, to compare the work on older copies with."""

    def __init__(self, session: Session, home: Path, document: Document):
        self.session, self.home, self.document = session, home, document
        pages = session.scalars(select(Page).where(Page.document_id == document.id))
        self.pages = {p.number: p for p in pages}
        self._drawn: dict[tuple[str, int], list[tuple[float, float]]] = {}

    def find(self, page: int, quote: str) -> tuple[int, str] | None:
        """Where the quoted words are on this copy: the same page, else the one page that has them, else the one
        workbook row with the same cells."""
        if page in self.pages and missing_piece(self.pages[page].text, quote) is None:
            return page, quote
        found = [(n, quote) for n, p in self.pages.items() if missing_piece(p.text, quote) is None]
        if not found:
            found = [(n, moved) for n, p in self.pages.items() if (moved := _moved_row(p.text, quote))]
        return found[0] if len(found) == 1 else None

    def _points(self, document: Document, page: int) -> list[tuple[float, float]]:
        key = (document.id, page)
        if key not in self._drawn:
            self._drawn[key] = sorted(readers.vector_points(library.stored_file(self.home, document), page))
        return self._drawn[key]

    def same_drawing(self, older: Document, page: int, points: list[list[float]]) -> bool:
        """Whether the drawing around these points is the same on this copy as on the older one. A scan has no
        drawing to compare, so work on it never moves."""
        new, old = self.pages.get(page), library.page(self.session, older.id, page)
        if self.document.kind != "pdf" or new is None or old is None:
            return False
        if (new.width, new.height) != (old.width, old.height):
            return False
        before, after = self._points(older, page), self._points(self.document, page)
        if not before or not after:
            return False
        if before == after:
            return True
        xs, ys = [p[0] for p in points], [p[1] for p in points]
        box = (min(xs) - AROUND, min(ys) - AROUND, max(xs) + AROUND, max(ys) + AROUND)
        return _within(before, box) == _within(after, box)

    def same_objects(self, older: Document, record: Scale | Measurement) -> bool:
        """On a CAD drawing, or a PDF page measured by its lines: whether the newer copy states the same units, or
        whether the measurement's rule takes exactly the same objects there, each with the same extent, length and
        area. An object added that the rule takes (a door drawn in by an addendum) keeps the measurement on the older
        copy, to be done again."""
        if self.document.kind != older.kind or older.kind not in ("cad", "pdf"):
            return False
        try:
            before = drawings.open_page(self.home, older, record.page)
            after = drawings.open_page(self.home, self.document, record.page)
        except (ValueError, OSError, readers.Unreadable):
            return False
        if isinstance(record, Scale):
            return before.info["units"] == after.info["units"]
        if record.rule is None:
            return False
        keys = record.entities or []
        try:
            again, _ = takeoff.resolve(
                self.session, self.home, self.document, record.kind, cad.Rule(**record.rule), record.page
            )
        except (ValueError, readers.Unreadable):
            return False
        if sorted(again) != sorted(keys):
            return False
        for key in keys:
            if key.startswith("room:"):
                continue  # a room is its walls, which the rule's other objects and the map already compare
            i, j = before.key_index.get(key), after.key_index.get(key)
            if i is None or j is None:
                return False
            a, b = before.num[i], after.num[j]
            if not all(abs(x - y) <= 1e-6 * max(abs(x), abs(y), 1.0) for x, y in zip(a, b, strict=True)):
                return False
        return True

    def moved(self, record: Any) -> dict[str, Any] | None:
        """What changes when the record moves onto this copy, or None if what it cites isn't unchanged here."""
        older = self.session.get(Document, record.document_id)
        if _by_objects(record, older):
            return {"document_id": self.document.id} if self.same_objects(older, record) else None
        if isinstance(record, Scale | Measurement):
            points = record.line if isinstance(record, Scale) else record.points
            return {"document_id": self.document.id} if self.same_drawing(older, record.page, points) else None
        if isinstance(record, Quote):
            lines, exclusions = [], []
            for kept, parts in ((lines, record.lines), (exclusions, record.exclusions)):
                for part in parts:
                    found = self.find(part["page"], part["quote"])
                    if found is None:
                        return None
                    kept.append({**part, "page": found[0], "quote": found[1]})
            return {"document_id": self.document.id, "lines": lines, "exclusions": exclusions}
        page = "sheet" if isinstance(record, PricingColumns) else "page"
        found = self.find(getattr(record, page), record.quote)
        return None if found is None else {"document_id": self.document.id, page: found[0], "quote": found[1]}


def _by_objects(record: Any, document: Document) -> bool:
    """Takeoff that rests on a drawing's objects: a CAD drawing's units or measurements, or objects measured on a PDF
    page's lines."""
    if not isinstance(record, Scale | Measurement):
        return False
    return document.kind == "cad" or getattr(record, "entities", None) is not None


def _pieces(n: int) -> str:
    return f"{n} piece{'' if n == 1 else 's'} of work"


def carry_over(session: Session, home: Path, document: Document) -> str | None:
    """Move the work on older copies of `document` onto it where what the work cites is unchanged. Returns a note for
    the document saying what moved and what didn't; None when no work rested on an older copy."""
    older = [d.id for d in library.copies(session, document) if d.status == "replaced"]
    work = resting_on(session, older) if older else []
    if not work:
        return None
    newer = _NewerCopy(session, home, document)
    moves = [(record, changes) for record in work if (changes := newer.moved(record)) is not None]
    for record, changes in moves:  # decided first, then moved, so a failure part-way moves nothing
        for name, value in changes.items():
            setattr(record, name, value)
    left = len(work) - len(moves)
    if not left:
        return f"It replaces an older copy; the {_pieces(len(work))} citing it are unchanged here and now rest on it."
    if not moves:
        return f"It replaces an older copy; none of the {_pieces(len(work))} citing it is unchanged here."
    return (
        f"It replaces an older copy; {len(moves)} of the {_pieces(len(work))} citing it are unchanged here and now "
        f"rest on it, and {left} must be done again from it."
    )


def problem(session: Session, record: Any) -> str | None:
    """Why a piece of work can't stand while it rests on an older copy of a document; None if it doesn't."""
    document = session.get(Document, record.document_id) if getattr(record, "document_id", None) else None
    if document is None or document.status != "replaced":
        return None
    newer = library.newer_copy(session, document)
    if newer is None or newer.status in ("waiting", "reading"):
        return (
            f"It rests on an older copy of {document.name}. The newer copy is being read, and Quantix moves this "
            "onto it if what it cites is unchanged there."
        )
    if newer.status != "read":
        return f"It rests on an older copy of {document.name}, and Quantix couldn't read the newer copy."
    if _by_objects(record, document):
        return (
            f"What it measured has changed in the newer copy of {document.name}, or objects were added that its "
            "rule takes: do it again on the newer copy."
        )
    if isinstance(record, Scale | Measurement):
        return (
            f"The drawing around it has changed in the newer copy of {document.name}, page {record.page}: do it "
            "again on the newer copy."
        )
    if isinstance(record, Quote):
        return f"Some of what it cites isn't in the newer copy of {document.name}: record it again from the newer copy."
    page = record.sheet if isinstance(record, PricingColumns) else record.page
    return (
        f"The newer copy of {document.name} doesn't say what it cites (“{record.quote[:100]}”, page {page} of the "
        "older copy): do it again from the newer copy."
    )


@dataclass
class Stale:
    kind: str
    record: Any
    label: str  # as a message names it
    by: str  # who made it
    problem: str


def _label(session: Session, record: Any) -> str:
    if isinstance(record, BoqItem | Fact):
        return boq.label(session, record)
    if isinstance(record, Scale | Measurement):
        return takeoff.label(session, record)
    if isinstance(record, Rate):
        return estimate.label(session, record)
    if isinstance(record, Requirement):
        return f"the checklist item “{record.title}”"
    if isinstance(record, PricingColumns):
        return f"the pricing columns of {session.get(Document, record.document_id).name}, sheet {record.sheet}"
    package = session.get(Package, record.package_id)
    return f"{session.get(Company, record.company_id).name}'s quote for the {package.name} package"


def stale(session: Session, tender_id: str) -> list[Stale]:
    """The office's live work that still rests on older copies of documents, oldest first."""
    older = [d.id for d in library.documents(session, tender_id) if d.status == "replaced"]
    found = []
    for record in resting_on(session, older) if older else []:
        by = getattr(record, "proposed_by", None) or getattr(record, "added_by", "")
        kind = REVIEWED.get(type(record)) or OTHERS[type(record)]
        found.append(Stale(kind, record, _label(session, record), by, problem(session, record) or ""))
    return sorted(found, key=lambda s: s.record.created_at)


def news(session: Session, tender_id: str, since: datetime | None) -> bool:
    """Whether a newer copy was read since `since` that left work on the older copy."""
    query = select(Document).where(Document.tender_id == tender_id, Document.read_at.is_not(None))
    if since is not None:
        query = query.where(Document.read_at > since)
    for document in session.scalars(query):
        older = [d.id for d in library.copies(session, document) if d.status == "replaced"]
        if older and resting_on(session, older):
            return True
    return False
