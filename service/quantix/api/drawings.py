"""CAD drawings for the engineer: what a drawing holds, a compact copy for the Takeoff screen to draw, choosing
objects, the drawing's units and measurements, the layer map, the checks and the tender queries."""

from decimal import Decimal
from typing import Any, Literal

import numpy as np
from fastapi import APIRouter, HTTPException, Request
from fastapi.responses import Response
from pydantic import BaseModel
from sqlalchemy.orm import Session

from quantix import tenders
from quantix.api.documents import Home
from quantix.api.tenders import DB
from quantix.boq.models import BoqItem
from quantix.core.review import UNDECIDED
from quantix.documents import cad, vectors
from quantix.documents.models import Document
from quantix.documents.readers import Unreadable
from quantix.office.models import ENGINEER
from quantix.review import queries
from quantix.review.models import TenderQuery
from quantix.takeoff import drawings, layers
from quantix.takeoff import records as takeoff
from quantix.takeoff.models import LayerMap

router = APIRouter(tags=["drawings"])


class PageOut(BaseModel):
    number: int
    name: str
    kind: str  # model | paper
    extents: list[float] | None
    objects: int


class UnitsOut(BaseModel):
    id: str
    name: str
    metres: float
    status: str
    note: str


class LayerOut(BaseModel):
    name: str
    objects: int
    types: dict[str, int]
    prints: bool
    meaning: str | None
    colour: str | None = None  # a PDF pen's colour, "#RRGGBB"


class BlockOut(BaseModel):
    name: str
    copies: int
    meaning: str | None


class DrawingOut(BaseModel):
    document_id: str
    name: str
    stamp: str  # which reading of the drawing the object numbers belong to
    pages: list[PageOut]
    header_units: str | None
    units: UnitsOut | None
    evidence: list[str]
    not_read: dict[str, int]
    layers: list[LayerOut]
    blocks: list[BlockOut]
    meanings: dict[str, str]


class Choice(BaseModel):
    """Objects of a drawing page: by their numbers on the Takeoff screen, or by a rule."""

    stamp: str
    objects: list[int] = []
    rule: cad.Rule | None = None


class Chosen(BaseModel):
    objects: list[int]  # their numbers on the screen
    keys: list[str]
    count: int
    length_m: float | None
    area_m2: float | None
    volume_m3: float | None


class UnitsIn(BaseModel):
    document_id: str
    units: str


class DrawingMeasurementIn(BaseModel):
    document_id: str
    page: int = 1  # a CAD drawing's model space, or the page of a PDF drawn in lines
    kind: Literal["length", "area", "count", "volume"]
    label: str
    unit: str
    choice: Choice
    multiplier: Decimal | None = None
    boq_item: str | None = None


class RegionIn(BaseModel):
    stamp: str
    point: list[float]  # [x, y] in drawing units
    hidden: list[str] = []  # layers the engineer hid: their lines don't close the region
    within: list[float] | None = None  # [left, bottom, right, top]: only the lines in this part of the page


class RegionOut(BaseModel):
    ring: list[list[float]]  # drawing units
    area_m2: float | None
    perimeter_m: float | None


class ProblemOut(BaseModel):
    severity: str
    message: str
    document_id: str | None
    page: int
    objects: list[str]
    screen_objects: list[int]  # the same objects by their numbers on the Takeoff screen, to show them


class LayerMapOut(BaseModel):
    id: str
    document_id: str
    document_name: str
    layers: dict[str, str]
    blocks: dict[str, str]
    note: str
    status: str
    proposed_by: str
    reviewed_by: str | None
    review_note: str | None


class SourceOut(BaseModel):
    document_id: str
    document_name: str
    page: int
    quote: str | None
    objects: list[str]


class QueryOut(BaseModel):
    id: str
    kind: str
    kind_label: str
    title: str
    detail: str
    wording: str
    governs: str | None
    sources: list[SourceOut]
    boq_item: str | None
    figures: list[str]  # the quantities it rests on, as Quantix computes them
    status: str
    proposed_by: str
    reviewed_by: str | None
    review_note: str | None


class RoomOut(BaseModel):
    number: int
    name: str
    area_m2: float | None
    perimeter_m: float | None
    source: str
    ring: list[list[float]]  # drawing units


class DecisionIn(BaseModel):
    approve: bool
    reason: str | None = None


def stamp(d: cad.Drawing) -> str:
    """Which reading of a drawing the object numbers on the screen belong to: its file, the drawings placed in it
    for its xrefs, and the reader's format."""
    name = d.folder.name
    return f"{name[:16]}{name[64:]}-{cad.FORMAT}"


def _drawing(session: Session, home, document_id: str, page: int = 1) -> tuple[Document, cad.Drawing]:
    """A CAD drawing, or the lines of a PDF page."""
    document = session.get(Document, document_id)
    if document is None or tenders.get_tender(session, document.tender_id) is None:
        raise HTTPException(status_code=404, detail="Document not found.")
    try:
        document = drawings.drawing_page(session, document.tender_id, document_id, page)
        return document, drawings.open_page(home, document, page)
    except (ValueError, Unreadable) as error:
        raise HTTPException(status_code=400, detail=str(error)) from error


def _tender(session: Session, tender_id: str) -> None:
    if tenders.get_tender(session, tender_id) is None:
        raise HTTPException(status_code=404, detail="Tender not found.")


def _units(session: Session, document_id: str) -> UnitsOut | None:
    record = drawings.units_record(session, document_id)
    if record is None:
        return None
    return UnitsOut(
        id=record.id,
        name=drawings.unit_name(record.metres_per_point),
        metres=record.metres_per_point,
        status=record.status,
        note=record.dimension,
    )


@router.get("/documents/{document_id}/drawing")
def get_drawing(document_id: str, session: DB, home: Home, page: int = 1) -> DrawingOut:
    """What a drawing holds: its pages, units, what couldn't be read, and one page's layers and blocks. A PDF page
    drawn in lines is a drawing of one page, whose layers are its pens (or the PDF's own layers)."""
    document, d = _drawing(session, home, document_id, page)
    try:
        d.space(page)
    except ValueError as error:
        raise HTTPException(status_code=404, detail=str(error)) from error
    known = drawings.meanings(session, document.tender_id)
    cad_drawing = document.kind == "cad"
    return DrawingOut(
        document_id=document.id,
        name=document.name,
        stamp=stamp(d),
        pages=[
            PageOut(
                number=s.number,
                name=s.name,
                kind=s.kind,
                extents=list(s.extents) if s.extents else None,
                objects=s.objects,
            )
            for s in d.spaces
        ],
        header_units=d.units[0] if d.units else None,
        units=_units(session, document.id) if cad_drawing else None,
        evidence=drawings.units_evidence(d) if cad_drawing else [],
        not_read=d.info["read"]["not_read"],
        layers=[
            LayerOut(
                name=f.name,
                objects=f.objects,
                types=dict(f.types),
                prints=not (f.off or f.frozen),
                meaning=drawings.meaning_of(known.layers, f.name),
                colour=d.layers.get(f.name, {}).get("colour"),
            )
            for f in drawings.layer_facts(d, page)
        ],
        blocks=[
            BlockOut(name=name, copies=n, meaning=drawings.meaning_of(known.blocks, name))
            for name, n in drawings.block_counts(d, page).most_common()
        ],
        meanings=drawings.MEANINGS,
    )


@router.get("/documents/{document_id}/pages/{number}/screen", response_class=Response)
def screen_copy(document_id: str, number: int, session: DB, home: Home) -> Response:
    """A page of a drawing for the Takeoff screen to draw: its segments, objects and texts, packed."""
    document, d = _drawing(session, home, document_id, number)
    try:
        d.space(number)
    except ValueError as error:
        raise HTTPException(status_code=404, detail=str(error)) from error
    return Response(cad.screen_copy(d, number, stamp(d)), media_type="application/octet-stream")


def _chosen(session: Session, home, document: Document, d: cad.Drawing, page: int, choice: Choice) -> np.ndarray:
    if choice.stamp != stamp(d):
        raise HTTPException(status_code=409, detail="The drawing was read again: reload it and choose again.")
    on_screen = cad.objects_on_screen(d, page)
    if choice.rule is not None and not choice.rule.empty():
        try:
            found, _ = drawings.choose(session, home, document, page, choice.rule)
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error
        return found
    picked = [i for i in choice.objects if 0 <= i < len(on_screen)]
    return on_screen[picked] if picked else np.zeros(0, dtype=int)


@router.post("/documents/{document_id}/pages/{number}/choose")
def choose(document_id: str, number: int, body: Choice, session: DB, home: Home) -> Chosen:
    """What Quantix measures of the objects chosen: how many, their length and closed area."""
    document, d = _drawing(session, home, document_id, number)
    found = _chosen(session, home, document, d, number, body)
    totals = d.totals(found)
    metres = drawings.metres_per_unit(session, document.id, number)
    on_screen = cad.objects_on_screen(d, number)
    return Chosen(
        objects=[int(n) for n in np.searchsorted(on_screen, found)],
        keys=[d.keys[int(i)] for i in found],
        count=int(totals["count"]),
        length_m=round(totals["length"] * metres, 3) if metres else None,
        area_m2=round(totals["area"] * metres * metres, 3) if metres else None,
        volume_m3=round(totals["volume"] * metres**3, 3) if metres else None,
    )


def screen_numbers(home, document: Document, keys: list[str], page: int = 1) -> list[int]:
    """The screen numbers of objects named by key, to show a measurement's objects."""
    try:
        d = drawings.open_page(home, document, page)
    except (ValueError, Unreadable, OSError):
        return []
    on_screen = cad.objects_on_screen(d, page)
    wanted = sorted(d.key_index[k] for k in keys if k in d.key_index)
    return [int(n) for n in np.searchsorted(on_screen, wanted) if n < len(on_screen)]


@router.post("/documents/{document_id}/pages/{number}/region")
def region(document_id: str, number: int, body: RegionIn, session: DB, home: Home) -> RegionOut:
    """The region around a point that the page's lines close off: what the engineer clicked inside, found with
    opencadkernel from the lines themselves, so an area needn't be drawn as one closed outline to be measured."""
    document, d = _drawing(session, home, document_id, number)
    if body.stamp != stamp(d):
        raise HTTPException(status_code=409, detail="The drawing was read again: reload it and click again.")
    if len(body.point) != 2 or (body.within is not None and len(body.within) != 4):
        raise HTTPException(status_code=400, detail="Give the point as [x, y] and the part of the page as 4 numbers.")
    metres = drawings.metres_per_unit(session, document.id, number)
    space = d.space(number)
    if document.kind == "pdf":
        tolerance = vectors.JOIN
    elif metres:
        tolerance = 0.001 / metres  # a millimetre
    else:
        tolerance = max(space.width or 1.0, space.height or 1.0) * 1e-7
    within = tuple(body.within) if body.within else None
    try:
        ring = drawings.enclosure(d, number, (body.point[0], body.point[1]), set(body.hidden), within, tolerance)
    except (ValueError, OSError) as error:
        raise HTTPException(status_code=400, detail=str(error)) from error
    area, perimeter = cad.ring_area(ring), cad.ring_length(ring)
    return RegionOut(
        ring=[[float(x), float(y)] for x, y in ring],
        area_m2=round(area * metres * metres, 3) if metres else None,
        perimeter_m=round(perimeter * metres, 3) if metres else None,
    )


@router.get("/documents/{document_id}/rooms")
def rooms(document_id: str, session: DB, home: Home) -> list[RoomOut]:
    """The rooms Quantix finds in model space from the layer map."""
    document, _ = _drawing(session, home, document_id)
    metres = drawings.metres_per_unit(session, document.id)
    try:
        found = drawings.rooms(session, home, document, 1)
    except ValueError as error:
        raise HTTPException(status_code=400, detail=str(error)) from error
    return [
        RoomOut(
            number=r.number,
            name=r.name,
            area_m2=round(r.area * metres * metres, 3) if metres else None,
            perimeter_m=round(r.perimeter * metres, 3) if metres else None,
            source=r.source,
            ring=[[float(x), float(y)] for x, y in r.ring],
        )
        for r in found
    ]


@router.post("/tenders/{tender_id}/units", status_code=201)
def set_units(tender_id: str, body: UnitsIn, session: DB, home: Home) -> UnitsOut:
    """The engineer sets a drawing's units."""
    _tender(session, tender_id)
    name = body.units.strip().lower()
    if name not in cad.UNIT_NAMES:
        raise HTTPException(status_code=400, detail=f"Units are one of: {', '.join(cad.UNIT_NAMES)}.")
    try:
        takeoff.set_units(session, home, tender_id, ENGINEER, body.document_id, cad.UNIT_NAMES[name], "approved")
    except ValueError as error:
        raise HTTPException(status_code=400, detail=str(error)) from error
    session.commit()
    return _units(session, body.document_id)


@router.post("/tenders/{tender_id}/drawing-measurements", status_code=201)
def add_drawing_measurement(tender_id: str, body: DrawingMeasurementIn, session: DB, home: Home) -> dict[str, Any]:
    """The engineer measures objects they chose on the Takeoff screen, or by a rule."""
    _tender(session, tender_id)
    document, d = _drawing(session, home, body.document_id, body.page)
    found = _chosen(session, home, document, d, body.page, body.choice)
    rule = body.choice.rule if body.choice.rule and not body.choice.rule.empty() else None
    if rule is None:
        if not len(found):
            raise HTTPException(status_code=400, detail="Choose the objects to measure first.")
        rule = cad.Rule(keys=[d.keys[int(i)] for i in found])
    try:
        m = takeoff.measure_drawing(
            session,
            home,
            tender_id,
            ENGINEER,
            body.document_id,
            body.page,
            body.kind,
            body.label,
            rule,
            body.unit,
            body.multiplier,
            body.boq_item,
            "approved",
        )
    except ValueError as error:
        raise HTTPException(status_code=400, detail=str(error)) from error
    session.commit()
    return {"id": m.id, "quantity": takeoff.quantity(session, m)}


@router.get("/tenders/{tender_id}/checks")
def checks(tender_id: str, session: DB, home: Home, document_id: str | None = None) -> list[ProblemOut]:
    """What Quantix's own checks find: in one drawing, or in the BOQ and across the drawings."""
    _tender(session, tender_id)
    document = None
    if document_id:
        document, _ = _drawing(session, home, document_id)
        found = drawings.drawing_problems(session, home, document)
    else:
        found = drawings.boq_problems(session, tender_id) + drawings.grid_problems(session, home, tender_id)
    return [
        ProblemOut(
            severity=p.severity,
            message=p.message,
            document_id=p.document_id,
            page=p.page,
            objects=p.objects,
            screen_objects=screen_numbers(home, document, p.objects, p.page) if document and p.objects else [],
        )
        for p in found
    ]


def _layer_map(session: Session, m: LayerMap) -> LayerMapOut:
    return LayerMapOut(
        id=m.id,
        document_id=m.document_id,
        document_name=session.get(Document, m.document_id).name,
        layers=m.layers,
        blocks=m.blocks,
        note=m.note,
        status=m.status,
        proposed_by=m.proposed_by,
        reviewed_by=m.reviewed_by,
        review_note=m.review_note,
    )


@router.get("/tenders/{tender_id}/layer-maps")
def layer_maps(tender_id: str, session: DB) -> list[LayerMapOut]:
    _tender(session, tender_id)
    return [_layer_map(session, m) for m in drawings.layer_maps(session, tender_id)]


@router.post("/layer-maps/{map_id}/decision")
def decide_layer_map(map_id: str, body: DecisionIn, session: DB, request: Request) -> None:
    record = session.get(LayerMap, map_id)
    if record is None or tenders.get_tender(session, record.tender_id) is None:
        raise HTTPException(status_code=404, detail="Not found.")
    if record.status not in UNDECIDED:
        raise HTTPException(status_code=400, detail="This has already been decided.")
    layers.decide(session, record, body.approve, body.reason)
    session.commit()
    request.app.state.office.engineer_spoke(record.tender_id)


def _query(session: Session, q: TenderQuery) -> QueryOut:
    item = session.get(BoqItem, q.boq_item_id) if q.boq_item_id else None
    figures, _ = queries.figures(session, q)
    return QueryOut(
        id=q.id,
        kind=q.kind,
        kind_label=queries.KINDS[q.kind],
        title=q.title,
        detail=q.detail,
        wording=q.wording,
        governs=q.governs,
        sources=[
            SourceOut(
                document_id=s["document_id"],
                document_name=(doc.name if (doc := session.get(Document, s["document_id"])) else s["document_id"]),
                page=s["page"],
                quote=s.get("quote"),
                objects=s.get("objects", []),
            )
            for s in q.sources
        ],
        boq_item=item.item if item else None,
        figures=figures,
        status=q.status,
        proposed_by=q.proposed_by,
        reviewed_by=q.reviewed_by,
        review_note=q.review_note,
    )


@router.get("/tenders/{tender_id}/queries")
def tender_queries(tender_id: str, session: DB) -> list[QueryOut]:
    """The tender queries the office raised, and the engineer's decisions on them."""
    _tender(session, tender_id)
    return [_query(session, q) for q in queries.queries(session, tender_id)]


@router.post("/queries/{query_id}/decision")
def decide_query(query_id: str, body: DecisionIn, session: DB, request: Request) -> None:
    record = session.get(TenderQuery, query_id)
    if record is None or tenders.get_tender(session, record.tender_id) is None:
        raise HTTPException(status_code=404, detail="Not found.")
    if record.status not in UNDECIDED:
        raise HTTPException(status_code=400, detail="This has already been decided.")
    queries.decide(session, record, body.approve, body.reason)
    session.commit()
    request.app.state.office.engineer_spoke(record.tender_id)
