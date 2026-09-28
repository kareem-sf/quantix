"""Tender queries: what the office found that the client must answer or the bid must allow for. Work drawn or
specified but not in the BOQ, documents that disagree, errors in the BOQ. The office drafts each query with where it
shows; Quantix checks every source exists and was opened, and that the query's figures come from its sources or from
Quantix's own takeoff; the Tender Manager reviews it; the engineer decides whether it goes to the client."""

import re
from datetime import UTC, datetime
from decimal import Decimal
from pathlib import Path
from typing import Any

from pydantic import BaseModel, Field
from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix.boq import records as boq
from quantix.boq.models import BoqItem, Fact
from quantix.core.review import LIVE, REVIEWED
from quantix.documents import evidence, library
from quantix.documents.models import Document
from quantix.office import records as office
from quantix.office.models import ENGINEER
from quantix.review.models import TenderQuery
from quantix.takeoff import drawings
from quantix.takeoff import records as takeoff
from quantix.takeoff.models import Measurement

KINDS = {
    "missing": "Missing from the BOQ",
    "conflict": "Documents disagree",
    "boq": "Error in the BOQ",
    "clarification": "Clarification",
}
SIMILAR = 0.6  # a query sharing this share of its title's words with another is probably the same query
COVERED = 0.5  # a BOQ description sharing this share of a missing item's words may already cover it
_STOP = set("a an and the of to in on for with by at from or is are be as it this that not no into per each".split())


class QuerySource(BaseModel):
    """Where the query shows: a page of a document, with its words or, on a CAD drawing, its objects."""

    document_id: str
    page: int
    quote: str | None = Field(default=None, description="Words on that page, exactly as read_page shows them")
    objects: list[str] = Field(default=[], description="On a CAD drawing: the objects' keys, as query_drawing lists")


def _words(text: str) -> set[str]:
    return {w for w in re.findall(r"[\w؀-ۿ]+", text.lower()) if w not in _STOP and len(w) > 1}


def _share(a: set[str], b: set[str]) -> float:
    return len(a & b) / len(a) if a else 0.0


def raise_query(
    session: Session,
    home: Path,
    tender_id: str,
    by: str,
    kind: str,
    title: str,
    detail: str,
    wording: str,
    sources: list[QuerySource],
    boq_item: str | None = None,
    measurements: list[str] | None = None,
    governs: str | None = None,
    status: str = "proposed",
) -> TenderQuery:
    if kind not in KINDS:
        raise ValueError(f"A query is one of: {', '.join(KINDS)}.")
    if len(title.split()) < 3:
        raise ValueError("Give the query a title of a few words that says what it is about.")
    if len(wording.split()) < 8:
        raise ValueError("Draft the query as the client will read it: what the documents show and what you ask.")
    if not sources and not boq_item:
        raise ValueError("Show where it is: at least one page of a document, or the BOQ line.")
    if kind == "conflict" and len(sources) < 2:
        raise ValueError("Documents that disagree need both sources: the page of each.")
    item = boq.find_item(session, tender_id, boq_item) if boq_item else None
    if kind == "missing" and item is not None:
        raise ValueError("Work missing from the BOQ has no BOQ line: leave boq_item out, or make it a boq query.")
    shown = [_source(session, home, tender_id, s) for s in sources]
    linked = [_measurement(session, tender_id, ref) for ref in measurements or []]
    query = TenderQuery(
        tender_id=tender_id,
        kind=kind,
        title=title.strip()[:300],
        detail=detail.strip(),
        wording=wording.strip(),
        governs=(governs or "").strip() or None,
        sources=shown,
        boq_item_id=item.id if item else None,
        measurement_ids=[m.id for m in linked],
        proposed_by=by,
        status=status,
    )
    session.add(query)
    session.flush()
    return query


def _source(session: Session, home: Path, tender_id: str, source: QuerySource) -> dict[str, Any]:
    document = session.get(Document, source.document_id)
    if document is None or document.tender_id != tender_id:
        raise ValueError("A source's document id isn't one of this tender's documents: use list_documents.")
    if document.status == "replaced":
        raise ValueError(f"A newer copy of {document.name} replaced the one cited: cite the newer copy.")
    if library.page(session, document.id, source.page) is None:
        raise ValueError(f"{document.name} has no page {source.page}.")
    if not source.quote and not source.objects:
        raise ValueError(f"For {document.name}, page {source.page}: quote its words, or give the drawing's objects.")
    if source.quote:
        evidence.check_quote(session, tender_id, document.id, source.page, source.quote)
    if source.objects:
        d = drawings.open_drawing(home, drawings.drawing_document(session, tender_id, document.id))
        missing = [k for k in source.objects if k not in d.key_index or d.index[d.key_index[k], 0] != source.page]
        if missing:
            raise ValueError(
                f"{document.name}, page {source.page} has no object {', '.join(missing[:5])}: give keys as "
                "query_drawing lists them."
            )
    found: dict[str, Any] = {"document_id": document.id, "page": source.page}
    if source.quote:
        found["quote"] = source.quote.strip()
    if source.objects:
        found["objects"] = source.objects[:200]
    return found


def _measurement(session: Session, tender_id: str, ref: str) -> Measurement:
    short = ref.strip().lower().removeprefix("measurement").strip()
    found = [m for m in takeoff.measurements(session, tender_id) if short and m.id.startswith(short)]
    if len(found) != 1:
        raise ValueError(f"No live measurement is {ref!r}: give it as takeoff_summary or open_record names it.")
    return found[0]


def queries(session: Session, tender_id: str) -> list[TenderQuery]:
    query = select(TenderQuery).where(TenderQuery.tender_id == tender_id, TenderQuery.status.in_(LIVE))
    return list(session.scalars(query.order_by(TenderQuery.created_at)))


def label(session: Session, record: TenderQuery) -> str:
    return f"the query “{record.title}”"


def approve(session: Session, record: TenderQuery, status: str = "approved") -> None:
    record.status, record.decided_at = status, datetime.now(UTC)


def decide(session: Session, record: TenderQuery, approve_it: bool, reason: str | None = None) -> None:
    if approve_it:
        approve(session, record)
    else:
        office.send_back(session, record.tender_id, record, label(session, record), reason, ENGINEER)


def waiting(session: Session, tender_id: str) -> int:
    query = select(TenderQuery.id).where(TenderQuery.tender_id == tender_id, TenderQuery.status == REVIEWED)
    return len(session.scalars(query).all())


def figures(session: Session, record: TenderQuery) -> tuple[list[str], set[Decimal]]:
    """The quantities the query rests on, as Quantix computes them, and every number its sources give."""
    lines, numbers = [], set()
    for mid in record.measurement_ids:
        m = session.get(Measurement, mid)
        if m is None:
            continue
        q = takeoff.quantity(session, m)
        lines.append(f"{m.label}: {q if q is not None else 'no quantity yet'} {m.unit}")
        if q is not None:
            numbers |= {
                q,
                q.normalize(),
                q.quantize(Decimal(1)),
                q.quantize(Decimal("0.1")),
                q.quantize(Decimal("0.01")),
            }
    item = session.get(BoqItem, record.boq_item_id) if record.boq_item_id else None
    if item is not None:
        numbers |= evidence.numbers_in(f"{item.item} {item.description}")
        if item.quantity is not None:
            numbers.add(item.quantity.normalize())
    home = library.home_of(session)
    for source in record.sources:
        numbers |= evidence.numbers_in(source.get("quote") or "")
        document = session.get(Document, source["document_id"])
        if document is not None:
            numbers |= evidence.numbers_in(document.name) | {Decimal(source["page"])}
        if source.get("objects") and document is not None and document.kind == "cad":
            d = drawings.open_drawing(home, document)
            for key in source["objects"]:
                if key in d.key_index:
                    numbers |= evidence.numbers_in(d.text_of(d.key_index[key]))
    return lines, numbers


def problems(session: Session, record: TenderQuery) -> list[tuple[str, str, str]]:
    """What Quantix finds in a query: (key, severity, message)."""
    found = []
    for source in record.sources:
        document = session.get(Document, source["document_id"])
        if document is not None and document.status == "replaced":
            found.append(
                (
                    f"query-older-copy:{record.id}:{document.id}",
                    "blocker",
                    f"It cites an older copy of {document.name}: check the newer copy still shows it, and raise it "
                    "again from there.",
                )
            )
    _, numbers = figures(session, record)
    stated = evidence.numbers_in(f"{record.title} {record.detail} {record.wording}")
    unsupported = sorted(n for n in stated if n.normalize() not in {x.normalize() for x in numbers})
    if unsupported:
        found.append(
            (
                f"query-figures:{record.id}:{','.join(str(n) for n in unsupported)}",
                "warning",
                f"The query gives {', '.join(str(n) for n in unsupported[:6])}, which neither its sources nor "
                "Quantix's takeoff give. Take figures from the documents, or measure them.",
            )
        )
    title = _words(record.title)
    for other in queries(session, record.tender_id):
        if other.id != record.id and _share(title, _words(other.title)) >= SIMILAR:
            found.append(
                (
                    f"query-twice:{record.id}:{other.id}",
                    "warning",
                    f"It looks like the query “{other.title}” already raised: put both in one query.",
                )
            )
            break
    if record.kind == "missing":
        title = _words(record.title)
        for item in boq.items(session, record.tender_id):
            described = _words(item.description)
            shared = {w for w in title & described if len(w) >= 4}
            if shared and len(title & described) / min(len(title), len(described)) >= COVERED:
                found.append(
                    (
                        f"query-covered:{record.id}:{item.id}",
                        "warning",
                        f"BOQ line {boq.reference(item)} (“{item.description[:80]}”) may already cover it: check the "
                        "line, its preambles and what it deems included before raising it.",
                    )
                )
                break
    if record.kind == "conflict" and not record.governs:
        precedence = session.scalars(
            select(Fact).where(Fact.tender_id == record.tender_id, Fact.kind == "precedence", Fact.status.in_(LIVE))
        ).first()
        if precedence is not None:
            found.append(
                (
                    f"query-governs:{record.id}",
                    "warning",
                    f"Say which document governs under the order of precedence ({precedence.value[:80]}).",
                )
            )
    if record.kind == "boq" and record.boq_item_id and record.measurement_ids:
        row = next((c for c in takeoff.compare(session, record.tender_id) if c.boq_item_id == record.boq_item_id), None)
        if row is not None and row.result == "matches":
            found.append(
                (
                    f"query-matches:{record.id}",
                    "warning",
                    f"The takeoff matches the BOQ's {row.boq_quantity} {row.boq_unit} within 2%: there may be nothing "
                    "to query.",
                )
            )
    return found


def describe(session: Session, record: TenderQuery) -> str:
    return f"{KINDS[record.kind]}: {record.title}"


def details(session: Session, record: TenderQuery) -> str:
    lines, _ = figures(session, record)
    parts = [f"{KINDS[record.kind]}: {record.title}", record.detail]
    for source in record.sources:
        document = session.get(Document, source["document_id"])
        where = f"{document.name if document else source['document_id']}, page {source['page']}"
        if source.get("quote"):
            parts.append(f"{where}: “{source['quote']}”")
        if source.get("objects"):
            parts.append(f"{where}: {len(source['objects'])} drawing objects ({', '.join(source['objects'][:5])})")
    item = session.get(BoqItem, record.boq_item_id) if record.boq_item_id else None
    if item is not None:
        parts.append(f"BOQ line {boq.reference(item)}: {item.description[:120]} · {item.quantity} {item.unit}")
    if lines:
        parts.append("Quantix's takeoff: " + "; ".join(lines))
    if record.governs:
        parts.append(f"Governs: {record.governs}")
    parts.append(f"Draft to the client: {record.wording}")
    return "\n".join(parts)
