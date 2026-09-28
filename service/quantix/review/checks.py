"""Quantix's checks on the office's work. Every finding is computed from the records, with the sources it rests on,
never the model's opinion. A blocker must be fixed before the Tender Manager can accept the work; he accepts a
warning only with his reason. The limits below are plausibility checks, not standards: each finding states them."""

import re
import statistics
from collections import Counter
from dataclasses import dataclass, field
from datetime import datetime
from decimal import Decimal
from pathlib import Path
from typing import Any

from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix import company
from quantix.boq import records as boq
from quantix.boq.models import FACT_KINDS, BoqItem, Fact
from quantix.core.review import LIVE
from quantix.documents import library, readers
from quantix.documents.evidence import numbers_in
from quantix.documents.models import Document
from quantix.estimate import records as estimate
from quantix.estimate.models import LibraryResource, Markups, Rate
from quantix.review import queries, revisions
from quantix.review.models import TenderQuery
from quantix.subcontract import records as subcontract
from quantix.subcontract.models import Package, Quote
from quantix.submission.models import Draft
from quantix.takeoff import drawings
from quantix.takeoff import records as takeoff
from quantix.takeoff.models import LayerMap, Measurement

BLOCKER, WARNING = "blocker", "warning"
SAME_ITEM = Decimal("0.01")  # the same item priced more than 1% differently elsewhere in the tender
BENCHMARK_BAND = Decimal("0.30")  # a rate more than 30% from the firm's own rates for items like it
PRELIMINARIES = (Decimal("0.05"), Decimal("0.15"))  # preliminaries as a share of the net cost
WASTAGE_MAX = Decimal("0.5")
OVERLAP = 0.5  # two measurements of one line whose outlines share half their extent
ON_THE_DRAWING = 0.5  # page points: a point this close to a drawn corner or line end was placed on the drawing
WORKING_DAYS = {"month": 26, "week": 6}  # to compare time-related preliminaries with a programme in working days
PLACEHOLDER = re.compile(
    r"\[[^\]]*[^\W\d_][^\]]*\]|\bto be (?:inserted|filled|completed|confirmed)\b|\btb[dc]\b", re.IGNORECASE
)
# Words that belong to the office, not to a document the client reads: its sources, the files Quantix keeps, its
# people and its instructions
INTERNAL = re.compile(
    r"^\s*sources?\s*:|\S+\.(?:docx|xlsx|xls|pdf)\b|\b(?:tender manager|quantix|sen[dt] back|as instructed|as filed)\b",
    re.IGNORECASE,
)


@dataclass
class Ref:
    """Where a finding comes from: a page of a document, or a record the engineer can open."""

    label: str
    document_id: str | None = None
    page: int | None = None


@dataclass
class Finding:
    key: str  # the check and the records it is about
    severity: str
    message: str
    refs: list[Ref] = field(default_factory=list)
    # when what the record is compared with was settled: approving the record before then didn't see this warning
    since: datetime | None = None


def for_record(session: Session, home: Path, kind: str, record: Any, blockers_only: bool = False) -> list[Finding]:
    """What the checks find in one record the office proposed. Blockers only skips reading the drawing, which only
    ever warns."""
    found = _older_copy(session, record)
    if kind == "measurement" and record.entities is not None:
        found += _drawing_measurement(session, home, record)
    elif kind == "measurement":
        found += _measurement(session, home, record, read_drawing=not blockers_only)
    else:
        check = {
            "boq": _item,
            "fact": _fact,
            "rate": _rate,
            "markups": _markups,
            "draft": _draft,
            "recommendation": _recommendation,
            "layers": _layer_map,
            "query": _query,
        }.get(kind)
        found += check(session, home, record) if check else []
    return [f for f in found if f.severity == BLOCKER] if blockers_only else found


def _older_copy(session: Session, record: Any) -> list[Finding]:
    """Work that rests on a document the engineer has replaced by a newer copy, and didn't move onto it."""
    if isinstance(record, LayerMap | TenderQuery):  # a map applies by name; a query checks its own sources
        return []
    if isinstance(record, Package):  # a recommendation rests on the quote it recommends
        record = session.get(Quote, record.recommended_quote_id) if record.recommended_quote_id else None
    why = revisions.problem(session, record) if record is not None else None
    if why is None:
        return []
    page = next((p["page"] for p in record.lines + record.exclusions), 1) if isinstance(record, Quote) else record.page
    return [Finding(f"older-copy:{record.id}", BLOCKER, why, _page_ref(session, record.document_id, page))]


def _money(value: Decimal) -> str:
    return f"{estimate.money(value):,}"


def _qty(value: Decimal | None) -> str:
    return f"{value:,.3f}".rstrip("0").rstrip(".")


def _lines(n: int) -> str:
    return f"{n} BOQ line{'s' if n > 1 else ''}"


def _same(text: str) -> str:
    return " ".join(re.sub(r"[^\w\s]", " ", text.lower()).split())


def _page_ref(session: Session, document_id: str | None, page: int | None) -> list[Ref]:
    document = session.get(Document, document_id) if document_id else None
    return [Ref(f"{document.name}, page {page}", document_id, page)] if document else []


def _item(session: Session, home: Path, item: BoqItem) -> list[Finding]:
    row = next((c for c in takeoff.compare(session, item.tender_id) if c.boq_item_id == item.id), None)
    if row is None or row.result != "differs":
        return []
    return [
        Finding(
            f"takeoff-differs:{item.id}:{row.takeoff}",  # a new total needs a new reason
            WARNING,
            f"The drawings measure {_qty(row.takeoff)} {row.unit} for {boq.reference(item)}, {row.difference:+.1%} "
            f"from the BOQ's {_qty(row.boq_quantity)}.",
            _page_ref(session, item.document_id, item.page),
        )
    ]


def _fact(session: Session, home: Path, fact: Fact) -> list[Finding]:
    missing = numbers_in(fact.value) - numbers_in(fact.quote)
    if not missing:
        return []
    return [
        Finding(
            f"fact-not-in-quote:{fact.id}",
            WARNING,
            f"{FACT_KINDS[fact.kind]}: the value gives {', '.join(str(n) for n in sorted(missing))}, which the quoted "
            "clause doesn't say.",
            _page_ref(session, fact.document_id, fact.page),
        )
    ]


def _box(points: list[list[float]]) -> tuple[float, float, float, float]:
    xs, ys = [p[0] for p in points], [p[1] for p in points]
    return min(xs), min(ys), max(xs), max(ys)


def _shared(a: tuple[float, float, float, float], b: tuple[float, float, float, float]) -> float:
    """How much of the smaller outline's extent the two share."""
    width, height = min(a[2], b[2]) - max(a[0], b[0]), min(a[3], b[3]) - max(a[1], b[1])
    if width <= 0 or height <= 0:
        return 0.0
    smaller = min((a[2] - a[0]) * (a[3] - a[1]), (b[2] - b[0]) * (b[3] - b[1]))
    return width * height / smaller if smaller else 0.0


def _named(measurements: list[Measurement]) -> str:
    """Measurements by their labels: “Yard” (2 measurements) and “Access road”."""
    counts = Counter(m.label for m in measurements)
    return " and ".join(f"“{label}”" + (f" ({n} measurements)" if n > 1 else "") for label, n in counts.items())


def _near(point: list[float], vertices: tuple[tuple[float, float], ...]) -> bool:
    x, y = point
    return any(abs(vx - x) <= ON_THE_DRAWING and abs(vy - y) <= ON_THE_DRAWING for vx, vy in vertices)


def _measurement(session: Session, home: Path, m: Measurement, read_drawing: bool = True) -> list[Finding]:
    found: list[Finding] = []
    item = session.get(BoqItem, m.boq_item_id) if m.boq_item_id else None
    where = _page_ref(session, m.document_id, m.page)
    if item is not None and m.kind == "area":
        others = session.scalars(
            select(Measurement).where(
                Measurement.boq_item_id == item.id, Measurement.id != m.id, Measurement.status.in_(LIVE)
            )
        )
        mine = takeoff.quantity(session, m)
        twice, alike = [], []
        for other in others:
            theirs = takeoff.quantity(session, other)
            same_sheet = other.document_id == m.document_id and other.page == m.page
            if same_sheet and _shared(_box(m.points), _box(other.points)) >= OVERLAP:
                twice.append(other)
            elif mine and theirs and abs(mine - theirs) <= max(mine, theirs) * Decimal("0.05"):
                alike.append(other)
        if twice:
            found.append(
                Finding(
                    f"measured-twice:{m.id}:{','.join(sorted(o.id for o in twice))}",
                    BLOCKER,
                    f"It covers the same area as {_named(twice)}, already measured for {boq.reference(item)}: "
                    "measuring it again counts it twice.",
                    where,
                )
            )
        if alike:
            found.append(
                Finding(
                    f"maybe-measured-twice:{m.id}:{','.join(sorted(o.id for o in alike))}",
                    WARNING,
                    f"{_named(alike)} for {boq.reference(item)} measure{'s' if len(alike) == 1 else ''} within 5% of "
                    f"this {_qty(mine)} {m.unit}: check it isn't the same area measured twice.",
                    where + [r for o in alike for r in _page_ref(session, o.document_id, o.page) if r not in where],
                )
            )
    if m.kind != "count" and read_drawing:
        document = session.get(Document, m.document_id)
        vertices = readers.vector_points(library.stored_file(home, document), m.page)
        if vertices:
            exact = set(vertices)  # snapping puts a point exactly on a corner or line end
            off = [p for p in m.points if tuple(p) not in exact and not _near(p, vertices)]
            if len(off) * 2 > len(m.points):
                which = "None of its points is" if len(off) == len(m.points) else f"{len(off)} of its points aren't"
                found.append(
                    Finding(
                        f"off-the-drawing:{m.id}",
                        WARNING,
                        f"{which} on a line of the drawing: check them against the sheet.",
                        where,
                    )
                )
    if item is not None:
        found += [Finding(f.key, f.severity, f.message, where + f.refs) for f in _item(session, home, item)]
    return found


def _drawing_measurement(session: Session, home: Path, m: Measurement) -> list[Finding]:
    """A measurement of a CAD drawing's objects: they must all still be in the drawing, none may be measured twice
    for the same BOQ line, and none should sit on a layer that doesn't print."""
    found: list[Finding] = []
    where = _page_ref(session, m.document_id, m.page)
    document = session.get(Document, m.document_id)
    try:
        d = drawings.open_drawing(home, document)
    except (ValueError, OSError):
        return [Finding(f"drawing-unread:{m.id}", BLOCKER, f"{document.name} can't be read now.", where)]
    keys = m.entities or []
    missing = [k for k in keys if not k.startswith("room:") and k not in d.key_index]
    room_names = {k.removeprefix("room:") for k in keys if k.startswith("room:")}
    if room_names:
        known = {r.name for r in drawings.rooms(session, home, document, 1)}
        missing += [f"room:{n}" for n in room_names - known]
    if missing:
        found.append(
            Finding(
                f"drawing-missing:{m.id}",
                BLOCKER,
                f"{len(missing)} of the {len(keys)} things it measured aren't in {document.name} as Quantix reads it "
                "now: measure it again.",
                where,
            )
        )
    item = session.get(BoqItem, m.boq_item_id) if m.boq_item_id else None
    if item is not None:
        others = session.scalars(
            select(Measurement).where(
                Measurement.boq_item_id == item.id, Measurement.id != m.id, Measurement.status.in_(LIVE)
            )
        )
        mine, twice, formats = set(keys), [], []
        stem = document.name.rsplit(".", 1)[0].lower()
        for other in others:
            if other.document_id == m.document_id and mine & set(other.entities or []):
                twice.append(other)
                continue
            other_document = session.get(Document, other.document_id)
            if other_document.kind != document.kind and other_document.name.rsplit(".", 1)[0].lower() == stem:
                formats.append(other)
        if twice:
            found.append(
                Finding(
                    f"measured-twice:{m.id}:{','.join(sorted(o.id for o in twice))}",
                    BLOCKER,
                    f"It takes objects {_named(twice)} already measured for {boq.reference(item)}: measuring them "
                    "again counts them twice.",
                    where,
                )
            )
        if formats:
            found.append(
                Finding(
                    f"two-formats:{m.id}:{','.join(sorted(o.id for o in formats))}",
                    WARNING,
                    f"{_named(formats)} measure {boq.reference(item)} on the same drawing in another format: check the "
                    "work isn't taken off twice.",
                    where + [r for o in formats for r in _page_ref(session, o.document_id, o.page) if r not in where],
                )
            )
    hidden = {name for name, layer in d.layers.items() if layer.get("off") or layer.get("frozen")}
    on_hidden = sorted({d.layer_of(d.key_index[k]) for k in keys if k in d.key_index} & hidden)
    if on_hidden:
        found.append(
            Finding(
                f"hidden-layers:{m.id}:{','.join(on_hidden)}",
                WARNING,
                f"Some of what it measured is on layers that are off or frozen ({', '.join(on_hidden[:5])}): "
                "they don't print, so check that work is part of the tender.",
                where,
            )
        )
    if item is not None:
        found += [Finding(f.key, f.severity, f.message, where + f.refs) for f in _item(session, home, item)]
    return found


def _layer_map(session: Session, home: Path, layer_map: LayerMap) -> list[Finding]:
    where = _page_ref(session, layer_map.document_id, 1)
    return [
        Finding(key, severity, message, where)
        for key, severity, message in drawings.map_problems(session, home, layer_map)
    ]


def _query(session: Session, home: Path, query: TenderQuery) -> list[Finding]:
    where = [r for s in query.sources for r in _page_ref(session, s["document_id"], s["page"])]
    return [Finding(key, severity, message, where) for key, severity, message in queries.problems(session, query)]


def _benchmarks(session: Session, tender_id: str, item: BoqItem) -> list[tuple[Decimal, str]]:
    """The firm's own rates for items like this one, in the same unit: its library and its other tenders."""
    unit = takeoff.plain_unit(item.unit)
    words = _same(item.description)
    marks = [
        (entry.rate, f"library: {entry.name} ({entry.source}, {entry.dated:%d %b %Y})")
        for entry in estimate.library(session)
        if entry.kind == "unit_rate" and takeoff.plain_unit(entry.unit) == unit and _alike(words, _same(entry.name))
    ]
    marks += [
        (past.rate, f"{past.tender}: {past.item or ''} {past.description[:60]} ({past.dated:%d %b %Y}, {past.outcome})")
        for past in company.past_rates(session, tender_id, "", limit=500)
        if takeoff.plain_unit(past.unit) == unit and _alike(words, _same(past.description))
    ]
    return marks


def _alike(a: str, b: str) -> bool:
    """Most of the shorter description's words are in the other."""
    first, second = ({w for w in text.split() if len(w) > 2} for text in (a, b))
    shorter = min(first, second, key=len)
    return bool(shorter) and len(first & second) / len(shorter) >= 0.6


def _rate(session: Session, home: Path, rate: Rate) -> list[Finding]:
    found: list[Finding] = []
    item = session.get(BoqItem, rate.boq_item_id)
    value = estimate.rate_of(rate)
    for other in boq.items(session, rate.tender_id):
        if other.id == item.id or _same(other.description) != _same(item.description):
            continue
        if takeoff.plain_unit(other.unit) != takeoff.plain_unit(item.unit):
            continue
        theirs = estimate.current_rate(session, other.id)
        if theirs is None:
            continue
        other_value = estimate.rate_of(theirs)
        if abs(value - other_value) > other_value * SAME_ITEM:
            found.append(
                Finding(
                    f"same-item-rate:{rate.id}:{other.id}",
                    WARNING,
                    f"{boq.reference(other)} is the same item at {_money(other_value)} per {other.unit}; "
                    f"{boq.reference(item)} is {_money(value)}. The same work normally carries one rate.",
                    [Ref(boq.reference(other))],
                )
            )
    marks = _benchmarks(session, rate.tender_id, item)
    if marks:
        middle = statistics.median(m[0] for m in marks)
        if middle and abs(value - middle) > middle * BENCHMARK_BAND:
            found.append(
                Finding(
                    f"off-benchmark:{rate.id}",
                    WARNING,
                    f"{_money(value)} is {(value - middle) / middle:+.0%} from the firm's own rates for items like it "
                    f"(middle {_money(middle)}): " + "; ".join(f"{_money(r)} {label}" for r, label in marks[:5]),
                )
            )
    for line in rate.lines or []:
        wastage = Decimal(str(line.get("wastage", 0)))
        if not 0 <= wastage <= WASTAGE_MAX:
            found.append(
                Finding(
                    f"wastage:{rate.id}:{line['resource']}",
                    WARNING,
                    f"{line['resource']} carries {wastage:.0%} wastage.",
                )
            )
    if rate.basis == "library" and rate.library_id:
        entry = session.get(LibraryResource, rate.library_id)
        if entry is not None and takeoff.plain_unit(entry.unit) != takeoff.plain_unit(item.unit):
            found.append(
                Finding(
                    f"library-unit:{rate.id}",
                    BLOCKER,
                    f"The library entry is per {entry.unit}; {boq.reference(item)} is measured per {item.unit}.",
                )
            )
    return found


def _schedule(session: Session, tender_id: str) -> Draft | None:
    """The newest work schedule the office drafted and hasn't had sent back. A draft without one holds JSON null,
    which SQL doesn't see as NULL, so the schedule is looked for here."""
    query = select(Draft).where(Draft.tender_id == tender_id, Draft.status.in_(LIVE))
    return next((d for d in session.scalars(query.order_by(Draft.created_at.desc())) if d.schedule), None)


def _markups(session: Session, home: Path, markups: Markups) -> list[Finding]:
    found: list[Finding] = []
    net = estimate.summary(session, markups.tender_id).net
    prelims = sum((estimate.preliminary_cost(i) for i in markups.preliminary_items), Decimal(0))
    if net:
        share = prelims / net
        low, high = PRELIMINARIES
        if not low <= share <= high:
            found.append(
                Finding(
                    f"preliminaries-share:{markups.id}",
                    WARNING,
                    f"Preliminaries are {_money(prelims)}, {share:.1%} of the net cost of {_money(net)}; the office "
                    f"usually sees {low:.0%} to {high:.0%}.",
                )
            )
    schedule = _schedule(session, markups.tender_id)
    if schedule is not None:
        overall = schedule.schedule["overall_days"]
        apart: dict[tuple[str, str, str], list[str]] = {}  # items priced for the same time, named together
        for i in markups.preliminary_items:
            period = next((p for p in WORKING_DAYS if p in i["unit"].lower()), None)
            if period is None:
                continue
            priced = Decimal(str(i["quantity"])) * WORKING_DAYS[period]
            if abs(priced - overall) > WORKING_DAYS["month"]:
                apart.setdefault((period, str(i["quantity"]), i["unit"]), []).append(i["item"])
        for (period, quantity, unit), items in apart.items():
            priced = Decimal(quantity) * WORKING_DAYS[period]
            names = items[0] if len(items) == 1 else f"{', '.join(items[:-1])} and {items[-1]}"
            found.append(
                Finding(
                    f"time-related:{markups.id}:{quantity}:{unit}:{overall}",
                    WARNING,
                    f"{names}: {quantity} {unit} is about {_qty(priced)} working days at {WORKING_DAYS[period]} a "
                    f"{period}; the work schedule is {overall} working days.",
                    [Ref(f"the work schedule “{schedule.title}”")],
                    since=schedule.decided_at or schedule.created_at,
                )
            )
    return found


def _draft(session: Session, home: Path, draft: Draft) -> list[Finding]:
    found: list[Finding] = []
    placeholders = sorted({m.group(0) for m in PLACEHOLDER.finditer(draft.body)})
    if placeholders:
        found.append(
            Finding(
                f"placeholder:{draft.id}",
                BLOCKER,
                "It still has text to fill in: " + ", ".join(f"“{p}”" for p in placeholders[:5]) + ".",
            )
        )
    notes = [line.strip() for line in draft.body.splitlines() if INTERNAL.search(line)]
    if notes:
        found.append(
            Finding(
                f"internal:{draft.id}",
                WARNING,
                "It reads like a note to the office, not to the client: "
                + "; ".join(f"“{line[:90]}”" for line in notes[:3])
                + ". Name the tender's documents as the client does (Annexure E, clause 7.3) and leave notes to the "
                "team out.",
            )
        )
    if draft.schedule:
        lines = draft.schedule["lines"]
        covered = {line["item_id"] for line in lines}
        missing = [
            i for i in boq.items(session, draft.tender_id) if i.quantity and i.quantity > 0 and i.id not in covered
        ]
        if missing:
            found.append(
                Finding(
                    f"schedule-missing:{draft.id}",
                    WARNING,
                    f"{_lines(len(missing))} with a quantity {'is' if len(missing) == 1 else 'are'} not in the "
                    "schedule: " + ", ".join(boq.reference(i) for i in missing[:10]),
                )
            )
        overall, days = draft.schedule["overall_days"], [line["days"] for line in lines]
        if days and overall < max(days):
            found.append(
                Finding(
                    f"schedule-too-short:{draft.id}",
                    BLOCKER,
                    f"The overall duration, {overall} working days, is shorter than its longest line "
                    f"({max(days)} days).",
                )
            )
        elif days and overall > sum(days):
            found.append(
                Finding(
                    f"schedule-too-long:{draft.id}",
                    WARNING,
                    f"The overall duration, {overall} working days, is longer than every line run one after "
                    f"another ({sum(days)} days).",
                )
            )
    return found


def _recommendation(session: Session, home: Path, package: Package) -> list[Finding]:
    found: list[Finding] = []
    result = subcontract.level(session, package)
    chosen = next((c for c in result.columns if c.quote_id == package.recommended_quote_id), None)
    if chosen is None:
        return []
    key = f"{package.id}:{chosen.quote_id}"  # a new recommendation is checked again
    first = next((c for c in result.columns if c.rank == 1), None)
    if chosen.levelled_total is None:
        found.append(
            Finding(
                f"recommend-incomplete:{key}",
                WARNING,
                f"{chosen.company}'s quote can't be levelled: a line it left out has no rate of ours to fill it.",
            )
        )
    elif first is not None and first.quote_id != chosen.quote_id:
        found.append(
            Finding(
                f"recommend-not-lowest:{key}",
                WARNING,
                f"{chosen.company} levels at {_money(chosen.levelled_total)}; {first.company} is lower at "
                f"{_money(first.levelled_total)}.",
            )
        )
    gaps = [boq.reference(i) for i in result.items if chosen.cells[i.id].plugged]
    if gaps:
        found.append(
            Finding(
                f"recommend-gaps:{key}",
                WARNING,
                f"{chosen.company} left out {', '.join(gaps)}; our own rate stands in for them.",
            )
        )
    quote = session.get(Quote, chosen.quote_id)
    unpriced = [e for e in quote.exclusions if not Decimal(str(e["amount"]))]
    if unpriced:
        found.append(
            Finding(
                f"exclusions-unpriced:{key}",
                WARNING,
                f"{chosen.company} excludes {', '.join(e['description'] for e in unpriced)} with nothing allowed for "
                "it, so the comparison isn't like with like.",
                [r for e in unpriced for r in _page_ref(session, quote.document_id, e["page"])],
            )
        )
    return found
