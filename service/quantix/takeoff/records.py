"""Scales, measurements and the comparison with the BOQ. Quantix does all the arithmetic."""

import math
import re
from dataclasses import dataclass
from datetime import UTC, datetime
from decimal import ROUND_HALF_UP, Decimal
from pathlib import Path

from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix.boq import records as boq
from quantix.boq.models import BoqItem
from quantix.core.review import APPROVED, LIVE, REVIEWED
from quantix.documents import cad, library
from quantix.documents.models import Document, Page
from quantix.documents.readers import Unreadable
from quantix.office import records as office
from quantix.office.models import ENGINEER
from quantix.takeoff import drawings
from quantix.takeoff.models import Measurement, Scale

UNITS = {"length": ("m", "m2"), "area": ("m2", "m3"), "count": ("nr",)}
MAX_OBJECTS = 20_000  # objects one drawing measurement may take: more means the rule is too wide
TOLERANCE = Decimal("0.02")  # takeoff and BOQ within 2% are a match
_UNIT_NAMES = {
    "m": ("m", "م", "lm", "l.m", "rm", "m.l", "م.ط", "مط"),
    "m2": ("m2", "م2", "m²", "sqm", "sq.m", "م²"),
    "m3": ("m3", "م3", "m³", "cum", "cu.m", "م³"),
    "nr": ("nr", "no", "no.", "nos", "each", "ea", "عدد", "pcs", "item"),
}


def plain_unit(unit: str) -> str:
    cleaned = re.sub(r"\s+", "", unit.lower()).replace("٢", "2").replace("٣", "3")
    return next((name for name, spellings in _UNIT_NAMES.items() if cleaned in spellings), cleaned)


POINT_M = 0.0254 / 72  # one PDF point on paper, in metres


def drawing_ratio(metres_per_point: float) -> int:
    """The scale as printed on a drawing, 1:n, from the sheet at its printed size, so it can be checked against the
    title block."""
    return round(metres_per_point / POINT_M)


_PRINTED_SCALE = re.compile(r"\b1\s*:\s*(\d{2,5})\b")
PAPER_SIZES = (0.5, 2**-0.5, 1, 2**0.5, 2)  # a sheet printed one or two paper sizes smaller or larger


def _fits_printed_scale(page_text: str, ratio: int) -> tuple[bool, list[int]]:
    """Whether a scale agrees, within 20%, with a scale printed on the sheet. A sheet that prints none fits any."""
    printed = sorted({int(n) for n in _PRINTED_SCALE.findall(page_text)})
    fits = not printed or any(abs(ratio / (n * size) - 1) <= 0.2 for n in printed for size in PAPER_SIZES)
    return fits, printed


def _page(session: Session, tender_id: str, document_id: str, number: int) -> tuple[Document, Page]:
    document = session.get(Document, document_id)
    if document is not None and document.kind == "cad":
        raise ValueError(
            f"{document.name} is a CAD drawing: measure its objects with measure_drawing, and set its units with "
            "set_drawing_units."
        )
    if document is None or document.tender_id != tender_id or document.kind != "pdf":
        raise ValueError("Takeoff works on PDF drawings: that document id is not a PDF in this tender.")
    page = library.page(session, document_id, number)
    if page is None or not page.width:
        raise ValueError(f"{document.name} has no page {number}.")
    return document, page


SNAP = 6.0  # page points (about 2 mm on paper) within which a point snaps to the drawing's own geometry


def snap(
    points: list[list[float]], vertices: tuple[tuple[float, float], ...], within: float = SNAP
) -> list[list[float]]:
    """Each point moved onto the nearest drawn corner or line end, if one is close; otherwise left where it is."""
    snapped = []
    for x, y in points:
        best = min(vertices, key=lambda v: (v[0] - x) ** 2 + (v[1] - y) ** 2, default=None)
        close = best is not None and math.dist(best, (x, y)) <= within
        snapped.append([best[0], best[1]] if close else [x, y])
    return snapped


def scale_for(session: Session, document_id: str, page: int) -> Scale | None:
    """The sheet's current scale: the newest approved one, or else the newest one still being decided."""
    query = select(Scale).where(Scale.document_id == document_id, Scale.page == page, Scale.status.in_(LIVE))
    scales = list(session.scalars(query.order_by(Scale.created_at.desc())))
    return next((s for s in scales if s.status in APPROVED), scales[0] if scales else None)


def set_scale(
    session: Session,
    tender_id: str,
    by: str,
    document_id: str,
    page: int,
    line: list[list[float]],
    length_m: float,
    dimension: str,
    status: str = "proposed",
) -> Scale:
    document, found = _page(session, tender_id, document_id, page)
    if dimension.strip() not in found.text:
        raise ValueError(
            f"“{dimension}” is not printed on {document.name}, page {page}. Calibrate on a real dimension."
        )
    if len(line) != 2 or length_m <= 0:
        raise ValueError("A scale needs the two ends of the dimension line and its real length in metres.")
    span = math.dist(line[0], line[1])
    if span < 10:
        raise ValueError("The two points are too close together to set a scale; use a longer dimension.")
    current = scale_for(session, document_id, page)
    if by != ENGINEER and current is not None and current.status == "approved":
        raise ValueError(
            f"The engineer approved this sheet's scale (about 1:{drawing_ratio(current.metres_per_point):,}). "
            "Measure on it; if you think it is wrong, say why with raise_concern."
        )
    ratio = drawing_ratio(length_m / span)
    fits, printed = _fits_printed_scale(found.text, ratio)
    if by != ENGINEER and not fits:
        raise ValueError(
            f"Those two points make the sheet about 1:{ratio:,}, but it prints "
            f"{' and '.join(f'1:{n}' for n in printed)}. Put the points on the two ends of the dimension line itself "
            "(its ticks or extension lines), not on its text or another label, or on the ends of the graphic scale bar."
        )
    scale = Scale(
        tender_id=tender_id,
        document_id=document_id,
        page=page,
        metres_per_point=length_m / span,
        line=line,
        length_m=length_m,
        dimension=dimension.strip(),
        proposed_by=by,
        status=status,
    )
    session.add(scale)
    session.flush()
    _replace_on_older_copies(session, scale)
    if status in APPROVED:
        _replace_older_scales(session, scale)
    return scale


def _replace_on_older_copies(session: Session, record: Scale | Measurement) -> None:
    """Work done on the newer copy of a drawing replaces the same work on its older copies, which no longer holds:
    the sheet's scale, or the measurements of the same BOQ line (or of the same label, if not linked to one)."""
    older = [d.id for d in library.copies(session, session.get(Document, record.document_id)) if d.status == "replaced"]
    if not older:
        return
    model = type(record)
    query = select(model).where(model.document_id.in_(older), model.page == record.page, model.status.in_(LIVE))
    if isinstance(record, Measurement) and record.boq_item_id:
        query = query.where(Measurement.boq_item_id == record.boq_item_id)
    elif isinstance(record, Measurement):
        query = query.where(Measurement.label == record.label)
    for stale in session.scalars(query):
        stale.status = "replaced"


def _replace_older_scales(session: Session, scale: Scale) -> None:
    """A sheet has one approved scale: approving a new one replaces the others."""
    query = select(Scale).where(Scale.document_id == scale.document_id, Scale.page == scale.page, Scale.id != scale.id)
    for older in session.scalars(query.where(Scale.status.in_(LIVE))):
        older.status = "replaced"


def _kind_and_unit(kind: str, unit: str, multiplier: Decimal | None) -> None:
    if kind not in UNITS:
        raise ValueError("A measurement is a length, an area or a count.")
    if unit not in UNITS[kind]:
        raise ValueError(f"A {kind} is measured in {' or '.join(UNITS[kind])}.")
    needs_multiplier = (kind, unit) in (("length", "m2"), ("area", "m3"))
    if needs_multiplier != (multiplier is not None):
        raise ValueError(
            "Give a height (length to m2) or a thickness (area to m3) in metres as the multiplier, and only then."
        )


def measure(
    session: Session,
    tender_id: str,
    by: str,
    document_id: str,
    page: int,
    kind: str,
    label: str,
    points: list[list[float]],
    unit: str,
    multiplier: Decimal | None,
    boq_item: str | None,
    status: str = "proposed",
) -> Measurement:
    document, found = _page(session, tender_id, document_id, page)
    _kind_and_unit(kind, unit, multiplier)
    least = {"length": 2, "area": 3, "count": 1}[kind]
    if len(points) < least:
        raise ValueError(f"A {kind} needs at least {least} points.")
    if any(not (0 <= x <= found.width and 0 <= y <= found.height) for x, y in points):
        raise ValueError(
            f"A point is off the page; {document.name} page {page} is {found.width:.0f} × {found.height:.0f}."
        )
    item_id = None
    if boq_item:
        item_id = boq.find_item(session, tender_id, boq_item).id
    measurement = Measurement(
        tender_id=tender_id,
        document_id=document_id,
        page=page,
        kind=kind,
        label=label.strip(),
        points=points,
        unit=unit,
        multiplier=multiplier,
        boq_item_id=item_id,
        proposed_by=by,
        status=status,
    )
    session.add(measurement)
    session.flush()
    _replace_on_older_copies(session, measurement)
    return measurement


def set_units(
    session: Session,
    home: Path,
    tender_id: str,
    by: str,
    document_id: str,
    metres: float,
    status: str = "proposed",
) -> Scale:
    """A CAD drawing's units, as metres in one drawing unit: the drawing's scale. It is kept as the scale of the
    drawing's model space, so everything measured on the drawing waits for it at the engineer's gate."""
    document = drawings.drawing_document(session, tender_id, document_id)
    if document.status == "replaced":
        raise ValueError(f"A newer copy of {document.name} replaced this one: set the units on the newer copy.")
    if not metres or metres <= 0:
        raise ValueError("Give the metres in one drawing unit, e.g. 0.001 for millimetres.")
    d = drawings.open_drawing(home, document)
    current = drawings.units_record(session, document_id)
    if by != ENGINEER and current is not None and current.status == "approved":
        raise ValueError(
            f"The engineer approved {document.name}'s units ({drawings.unit_name(current.metres_per_point)}). "
            "Measure on them; if you think they are wrong, say why with raise_concern."
        )
    header = d.units
    if by != ENGINEER and header and abs(header[1] - metres) > header[1] * 1e-9:
        raise ValueError(
            f"{document.name}'s header says its units are {header[0]}, not {drawings.unit_name(metres)}. Set what "
            "the drawing says; if you think its header is wrong, say why with raise_concern."
        )
    said = "as the drawing's header says" if header else "the header says none"
    scale = Scale(
        tender_id=tender_id,
        document_id=document_id,
        page=1,
        metres_per_point=metres,
        line=[],
        length_m=metres,
        dimension=f"{drawings.unit_name(metres)}, {said}"[:100],
        proposed_by=by,
        status=status,
    )
    session.add(scale)
    session.flush()
    _replace_on_older_copies(session, scale)
    if status in APPROVED:
        _replace_older_scales(session, scale)
    return scale


def measurable(d: cad.Drawing, found, kind: str) -> list[int]:
    """The objects a measurement of this kind takes: outlines with an area, lines with a length, or anything to
    count."""
    if kind == "area":
        return [int(i) for i in found if d.flags(int(i)) & cad.CLOSED and d.area(int(i)) > 0]
    if kind == "length":
        return [int(i) for i in found if d.length(int(i)) > 0]
    return [int(i) for i in found]


def resolve(
    session: Session, home: Path, document: Document, kind: str, rule: cad.Rule
) -> tuple[list[str], cad.Drawing]:
    """The keys of the objects (or rooms) a rule takes for a measurement of this kind."""
    found, found_rooms = drawings.choose(session, home, document, 1, rule)
    d = drawings.open_drawing(home, document)
    if [t.lower() for t in rule.types] == ["room"]:
        return [drawings.room_key(r) for r in found_rooms], d
    return [d.keys[i] for i in measurable(d, found, kind)], d


def measure_drawing(
    session: Session,
    home: Path,
    tender_id: str,
    by: str,
    document_id: str,
    page: int,
    kind: str,
    label: str,
    rule: cad.Rule,
    unit: str,
    multiplier: Decimal | None,
    boq_item: str | None,
    status: str = "proposed",
) -> Measurement:
    """A measurement of a CAD drawing's own objects, chosen by a rule. Quantix works out which objects the rule
    takes when it is filed, so whoever checks it sees exactly what was counted, and computes the quantity from their
    geometry whenever it is read. A rule that finds nothing, linked to a BOQ line, records that the line's work isn't
    on this drawing."""
    document = drawings.drawing_document(session, tender_id, document_id)
    if document.status == "replaced":
        raise ValueError(f"A newer copy of {document.name} replaced this one: measure on the newer copy.")
    if page != 1:
        raise ValueError("Measure in model space, page 1, where the drawing is full size; layouts show it scaled.")
    _kind_and_unit(kind, unit, multiplier)
    if rule.empty():
        raise ValueError(
            "Say which objects to take: layers, blocks, types, words, attributes, a region, rooms or keys."
        )
    keys, d = resolve(session, home, document, kind, rule)
    if len(keys) > MAX_OBJECTS:
        raise ValueError(f"That rule takes {len(keys):,} objects: narrow it to the work you mean.")
    item_id = boq.find_item(session, tender_id, boq_item).id if boq_item else None
    if not keys and item_id is None:
        what = {"area": "closed outline or hatch", "length": "line", "count": "object"}[kind]
        raise ValueError(
            f"No {what} on {document.name} matches that rule. Look at the layers and blocks with drawing_overview, "
            "or try the rule with query_drawing first."
        )
    measurement = Measurement(
        tender_id=tender_id,
        document_id=document_id,
        page=1,
        kind=kind,
        label=label.strip(),
        points=[],
        unit=unit,
        multiplier=multiplier,
        boq_item_id=item_id,
        proposed_by=by,
        status=status,
        entities=keys,
        rule=rule.model_dump(exclude_defaults=True),
    )
    session.add(measurement)
    session.flush()
    _replace_on_older_copies(session, measurement)
    return measurement


def _drawing_base(session: Session, m: Measurement) -> Decimal | None:
    """The count, or the length or area in square metres, of the objects a drawing measurement took."""
    document = session.get(Document, m.document_id)
    home = library.home_of(session)
    try:
        d = drawings.open_drawing(home, document)
    except (Unreadable, OSError, ValueError):
        return None
    keys = m.entities or []
    room_names = {k.removeprefix("room:") for k in keys if k.startswith("room:")}
    found_rooms = [r for r in drawings.rooms(session, home, document, 1) if r.name in room_names] if room_names else []
    objects = [d.key_index[k] for k in keys if k in d.key_index]
    if m.kind == "count":
        return Decimal(sum(d.copies(i) for i in objects) + len(found_rooms))
    metres = drawings.metres_per_unit(session, m.document_id)
    if metres is None:
        return None
    if m.kind == "length":
        units = sum(d.length(i) for i in objects) + sum(r.perimeter for r in found_rooms)
        return Decimal(str(units * metres))
    units = sum(d.area(i) for i in objects) + sum(r.area for r in found_rooms)
    return Decimal(str(units * metres * metres))


def quantity(session: Session, m: Measurement) -> Decimal | None:
    """Computed from the points and the sheet's current scale, to 3 decimals. None until the sheet has a scale. On a
    CAD drawing, from the geometry of the objects measured and the drawing's units."""
    if m.entities is not None:
        base = _drawing_base(session, m)
        if base is None:
            return None
        total = base * (m.multiplier or 1)
        return total.quantize(Decimal("1" if m.kind == "count" else "0.001"), rounding=ROUND_HALF_UP)
    if m.kind == "count":
        base = Decimal(len(m.points))
    else:
        scale = scale_for(session, m.document_id, m.page)
        if scale is None:
            return None
        if m.kind == "length":
            points = sum(math.dist(a, b) for a, b in zip(m.points, m.points[1:], strict=False))
            base = Decimal(str(points * scale.metres_per_point))
        else:
            xs, ys = [p[0] for p in m.points], [p[1] for p in m.points]
            shoelace = abs(sum(xs[i] * ys[i - 1] - xs[i - 1] * ys[i] for i in range(len(xs)))) / 2
            base = Decimal(str(shoelace * scale.metres_per_point**2))
    total = base * (m.multiplier or 1)
    return total.quantize(Decimal("1" if m.kind == "count" else "0.001"), rounding=ROUND_HALF_UP)


def measurements(session: Session, tender_id: str) -> list[Measurement]:
    query = select(Measurement).where(Measurement.tender_id == tender_id, Measurement.status.in_(LIVE))
    return list(session.scalars(query.order_by(Measurement.created_at)))


@dataclass
class Comparison:
    boq_item_id: str | None
    item: str | None
    description: str
    boq_quantity: Decimal | None
    boq_unit: str | None
    takeoff: Decimal | None
    unit: str
    # matches | differs | unit_differs | no_boq_quantity | not_in_boq | not_on_drawings | no_scale
    result: str
    difference: Decimal | None  # takeoff minus BOQ, as a fraction of the BOQ quantity


def compare(session: Session, tender_id: str) -> list[Comparison]:
    rows: list[Comparison] = []
    by_item: dict[str, list[Measurement]] = {}
    for m in measurements(session, tender_id):
        if m.boq_item_id:
            by_item.setdefault(m.boq_item_id, []).append(m)
            continue
        q = quantity(session, m)
        rows.append(
            Comparison(None, None, m.label, None, None, q, m.unit, "not_in_boq" if q is not None else "no_scale", None)
        )
    for item_id, group in by_item.items():
        item = session.get(BoqItem, item_id)
        values = [quantity(session, m) for m in group]
        units = {m.unit for m in group}
        total = sum(values, Decimal(0)) if None not in values else None
        unit = units.pop() if len(units) == 1 else "mixed"
        if total is None:
            result, difference = "no_scale", None
        elif total == 0 and all(m.entities == [] for m in group):
            result, difference = "not_on_drawings", None  # the office looked, and the drawings don't show it
        elif unit != plain_unit(item.unit):
            result, difference = "unit_differs", None
        elif item.quantity is None or item.quantity == 0:
            result, difference = "no_boq_quantity", None
        else:
            difference = ((total - item.quantity) / item.quantity).quantize(Decimal("0.0001"))
            result = "matches" if abs(difference) <= TOLERANCE else "differs"
        rows.append(
            Comparison(item.id, item.item, item.description, item.quantity, item.unit, total, unit, result, difference)
        )
    return rows


def label(session: Session, record: Scale | Measurement) -> str:
    """How a message names the record."""
    document = session.get(Document, record.document_id)
    where = f"{document.name}, page {record.page}"
    if isinstance(record, Scale):
        return f"the units of {document.name}" if document.kind == "cad" else f"the scale of {where}"
    return f"the measurement “{record.label}” on {where}"


def approve(session: Session, record: Scale | Measurement, status: str = "approved") -> None:
    """Approved by the engineer, or by a fully autonomous office once the Tender Manager has reviewed it."""
    record.status, record.decided_at = status, datetime.now(UTC)
    if isinstance(record, Scale):
        _replace_older_scales(session, record)


def decide(session: Session, record: Scale | Measurement, approve_it: bool, reason: str | None = None) -> None:
    if approve_it:
        approve(session, record)
    else:
        office.send_back(session, record.tender_id, record, label(session, record), reason, ENGINEER)


def waiting(session: Session, tender_id: str) -> int:
    """Scales and measurements the Tender Manager has reviewed, waiting for the engineer."""
    count = 0
    for model in (Scale, Measurement):
        count += len(
            session.scalars(select(model.id).where(model.tender_id == tender_id, model.status == REVIEWED)).all()
        )
    return count
