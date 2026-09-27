"""Scales, measurements and the comparison with the BOQ. Quantix does all the arithmetic."""

import math
import re
from dataclasses import dataclass
from datetime import UTC, datetime
from decimal import ROUND_HALF_UP, Decimal

from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix.boq import records as boq
from quantix.boq.models import BoqItem
from quantix.core.review import APPROVED, LIVE, REVIEWED
from quantix.documents import library
from quantix.documents.models import Document, Page
from quantix.office import records as office
from quantix.office.models import ENGINEER
from quantix.takeoff.models import Measurement, Scale

UNITS = {"length": ("m", "m2"), "area": ("m2", "m3"), "count": ("nr",)}
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
    if status in APPROVED:
        _replace_older_scales(session, scale)
    return scale


def _replace_older_scales(session: Session, scale: Scale) -> None:
    """A sheet has one approved scale: approving a new one replaces the others."""
    query = select(Scale).where(Scale.document_id == scale.document_id, Scale.page == scale.page, Scale.id != scale.id)
    for older in session.scalars(query.where(Scale.status.in_(LIVE))):
        older.status = "replaced"


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
    if kind not in UNITS:
        raise ValueError("A measurement is a length, an area or a count.")
    if unit not in UNITS[kind]:
        raise ValueError(f"A {kind} is measured in {' or '.join(UNITS[kind])}.")
    needs_multiplier = (kind, unit) in (("length", "m2"), ("area", "m3"))
    if needs_multiplier != (multiplier is not None):
        raise ValueError(
            "Give a height (length to m2) or a thickness (area to m3) in metres as the multiplier, and only then."
        )
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
    return measurement


def quantity(session: Session, m: Measurement) -> Decimal | None:
    """Computed from the points and the sheet's current scale, to 3 decimals. None until the sheet has a scale."""
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
    result: str  # matches | differs | unit_differs | no_boq_quantity | not_in_boq | no_scale
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
    return f"the scale of {where}" if isinstance(record, Scale) else f"the measurement “{record.label}” on {where}"


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
