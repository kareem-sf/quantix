from decimal import Decimal
from typing import Literal

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel
from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix import tenders
from quantix.api.tenders import DB
from quantix.boq.models import BoqItem
from quantix.documents import library, readers
from quantix.documents.models import Document
from quantix.office.models import ENGINEER
from quantix.takeoff import records
from quantix.takeoff.models import Measurement, Scale

router = APIRouter(tags=["takeoff"])


class ScaleOut(BaseModel):
    id: str
    metres_per_point: float
    line: list[list[float]]
    length_m: float
    dimension: str
    status: str
    proposed_by: str


class Sheet(BaseModel):
    document_id: str
    name: str
    page: int
    width: float
    height: float
    scale: ScaleOut | None


class MeasurementOut(BaseModel):
    id: str
    document_id: str
    page: int
    kind: str
    label: str
    points: list[list[float]]
    unit: str
    multiplier: Decimal | None
    quantity: Decimal | None
    boq_item: str | None
    status: str
    proposed_by: str


class ComparisonOut(BaseModel):
    boq_item_id: str | None
    item: str | None
    description: str
    boq_quantity: Decimal | None
    boq_unit: str | None
    takeoff: Decimal | None
    unit: str
    result: str
    difference: Decimal | None


class Takeoff(BaseModel):
    sheets: list[Sheet]
    measurements: list[MeasurementOut]
    comparison: list[ComparisonOut]


class ScaleIn(BaseModel):
    document_id: str
    page: int
    line: list[list[float]]
    length_m: float
    dimension: str


class MeasurementIn(BaseModel):
    document_id: str
    page: int
    kind: Literal["length", "area", "count"]
    label: str
    points: list[list[float]]
    unit: str
    multiplier: Decimal | None = None
    boq_item: str | None = None


class DecisionIn(BaseModel):
    approve: bool
    reason: str | None = None


def _tender(session: Session, tender_id: str) -> None:
    if tenders.get_tender(session, tender_id) is None:
        raise HTTPException(status_code=404, detail="Tender not found.")


def _scale(scale: Scale | None) -> ScaleOut | None:
    if scale is None:
        return None
    return ScaleOut(
        id=scale.id,
        metres_per_point=scale.metres_per_point,
        line=scale.line,
        length_m=scale.length_m,
        dimension=scale.dimension,
        status=scale.status,
        proposed_by=scale.proposed_by,
    )


def _measurement(session: Session, m: Measurement) -> MeasurementOut:
    item = session.get(BoqItem, m.boq_item_id) if m.boq_item_id else None
    return MeasurementOut(
        id=m.id,
        document_id=m.document_id,
        page=m.page,
        kind=m.kind,
        label=m.label,
        points=m.points,
        unit=m.unit,
        multiplier=m.multiplier,
        quantity=records.quantity(session, m),
        boq_item=item.item if item else None,
        status=m.status,
        proposed_by=m.proposed_by,
    )


def _sheets(session: Session, tender_id: str, measured: list[Measurement]) -> list[Sheet]:
    """Every PDF page that has a scale or a measurement, in document order."""
    scaled = session.execute(
        select(Scale.document_id, Scale.page).where(Scale.tender_id == tender_id, Scale.status.in_(records.LIVE))
    ).all()
    keys = sorted({(m.document_id, m.page) for m in measured} | {tuple(k) for k in scaled})
    sheets = []
    for document_id, number in keys:
        document = session.get(Document, document_id)
        page = library.page(session, document_id, number)
        sheets.append(
            Sheet(
                document_id=document_id,
                name=document.name,
                page=number,
                width=page.width,
                height=page.height,
                scale=_scale(records.scale_for(session, document_id, number)),
            )
        )
    return sheets


@router.get("/tenders/{tender_id}/takeoff")
def get_takeoff(tender_id: str, session: DB) -> Takeoff:
    _tender(session, tender_id)
    measured = records.measurements(session, tender_id)
    return Takeoff(
        sheets=_sheets(session, tender_id, measured),
        measurements=[_measurement(session, m) for m in measured],
        comparison=[ComparisonOut(**vars(c)) for c in records.compare(session, tender_id)],
    )


@router.get("/documents/{document_id}/pages/{number}/sheet")
def get_sheet(document_id: str, number: int, session: DB) -> Sheet:
    """Any PDF page as a sheet the engineer can measure on."""
    document = session.get(Document, document_id)
    page = library.page(session, document_id, number) if document else None
    if document is None or page is None or not page.width or tenders.get_tender(session, document.tender_id) is None:
        raise HTTPException(status_code=404, detail="That page can't be measured.")
    return Sheet(
        document_id=document_id,
        name=document.name,
        page=number,
        width=page.width,
        height=page.height,
        scale=_scale(records.scale_for(session, document_id, number)),
    )


@router.get("/documents/{document_id}/pages/{number}/vertices")
def get_vertices(document_id: str, number: int, session: DB, request: Request) -> list[list[float]]:
    """The drawing's own corners and line ends, for snapping the engineer's clicks."""
    document = session.get(Document, document_id)
    if document is None or document.kind != "pdf" or tenders.get_tender(session, document.tender_id) is None:
        raise HTTPException(status_code=404, detail="That page can't be measured.")
    path = library.stored_file(request.app.state.home, document)
    return [list(p) for p in readers.vector_points(path, number)]


@router.post("/tenders/{tender_id}/scales", status_code=201)
def set_scale(tender_id: str, body: ScaleIn, session: DB) -> ScaleOut:
    _tender(session, tender_id)
    try:
        scale = records.set_scale(
            session,
            tender_id,
            ENGINEER,
            body.document_id,
            body.page,
            body.line,
            body.length_m,
            body.dimension,
            "approved",
        )
    except ValueError as error:
        raise HTTPException(status_code=400, detail=str(error)) from error
    session.commit()
    return _scale(scale)


@router.post("/tenders/{tender_id}/measurements", status_code=201)
def add_measurement(tender_id: str, body: MeasurementIn, session: DB) -> MeasurementOut:
    _tender(session, tender_id)
    try:
        m = records.measure(
            session,
            tender_id,
            ENGINEER,
            body.document_id,
            body.page,
            body.kind,
            body.label,
            body.points,
            body.unit,
            body.multiplier,
            body.boq_item,
            "approved",
        )
    except ValueError as error:
        raise HTTPException(status_code=400, detail=str(error)) from error
    session.commit()
    return _measurement(session, m)


def _record(session: Session, model: type[Scale] | type[Measurement], record_id: str) -> Scale | Measurement:
    record = session.get(model, record_id)
    if record is None or tenders.get_tender(session, record.tender_id) is None:
        raise HTTPException(status_code=404, detail="Not found.")
    return record


@router.post("/measurements/{measurement_id}/decision")
def decide_measurement(measurement_id: str, body: DecisionIn, session: DB, request: Request) -> None:
    m = _record(session, Measurement, measurement_id)
    if m.status != "proposed":
        raise HTTPException(status_code=400, detail="This has already been decided.")
    records.decide(session, m, body.approve, body.reason)
    session.commit()
    request.app.state.office.engineer_spoke(m.tender_id)


@router.post("/scales/{scale_id}/decision")
def decide_scale(scale_id: str, body: DecisionIn, session: DB, request: Request) -> None:
    scale = _record(session, Scale, scale_id)
    if scale.status != "proposed":
        raise HTTPException(status_code=400, detail="This has already been decided.")
    records.decide(session, scale, body.approve, body.reason)
    session.commit()
    request.app.state.office.engineer_spoke(scale.tender_id)


@router.delete("/measurements/{measurement_id}", status_code=204)
def delete_measurement(measurement_id: str, session: DB) -> None:
    """The engineer removes a measurement to redo it. It is kept as rejected, not erased."""
    m = _record(session, Measurement, measurement_id)
    records.decide(session, m, approve=False, reason="Removed by the engineer to be measured again.")
    session.commit()
