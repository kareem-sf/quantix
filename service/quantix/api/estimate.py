from datetime import date
from decimal import Decimal
from typing import Any, Literal

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel, Field
from sqlalchemy.orm import Session

from quantix import tenders
from quantix.api.tenders import DB
from quantix.boq import records as boq
from quantix.documents.models import Document
from quantix.estimate import records
from quantix.estimate.models import LibraryResource, Markups, Rate
from quantix.office import records as office
from quantix.office.models import ENGINEER

router = APIRouter(tags=["estimate"])


class LineOut(BaseModel):
    kind: str
    resource: str
    quantity: Decimal
    unit: str
    rate: Decimal
    wastage: Decimal
    cost: Decimal


class RateOut(BaseModel):
    id: str
    basis: str
    rate: Decimal
    unit_rate: Decimal | None
    lines: list[LineOut] | None
    source_document: str | None
    document_id: str | None
    page: int | None
    quote: str | None
    library_id: str | None
    note: str
    status: str
    proposed_by: str


class PricedItem(BaseModel):
    id: str
    section: str | None
    item: str
    description: str
    unit: str
    quantity: Decimal | None
    item_status: str
    rate: RateOut | None
    amount: Decimal | None


class MarkupsOut(BaseModel):
    id: str
    preliminaries: Decimal
    overheads: Decimal
    profit: Decimal
    adjustment: Decimal
    note: str
    status: str
    proposed_by: str


class SummaryOut(BaseModel):
    currency: str
    priced: int
    items: int
    waiting: int
    net: Decimal
    preliminaries: Decimal
    overheads: Decimal
    profit: Decimal
    adjustment: Decimal
    total: Decimal
    vat_rate: Decimal | None
    vat: Decimal | None
    total_with_vat: Decimal | None
    unpriced: list[str]


class EstimateOut(BaseModel):
    items: list[PricedItem]
    markups: MarkupsOut | None
    summary: SummaryOut


class RateDecision(BaseModel):
    approve: bool
    reason: str | None = None
    save_to_library: bool = False


class DecisionIn(BaseModel):
    approve: bool
    reason: str | None = None


class LibraryIn(BaseModel):
    kind: Literal["labour", "plant", "material", "subcontract", "unit_rate"]
    name: str = Field(min_length=1)
    unit: str = Field(min_length=1)
    rate: Decimal = Field(gt=0)
    currency: str = Field(min_length=1)
    source: str = Field(min_length=1)
    dated: date


class LibraryOut(LibraryIn):
    id: str


class Saved(BaseModel):
    approved: int


def _tender(session: Session, tender_id: str) -> None:
    if tenders.get_tender(session, tender_id) is None:
        raise HTTPException(status_code=404, detail="Tender not found.")


def _rate(session: Session, rate: Rate | None) -> RateOut | None:
    if rate is None:
        return None
    lines = None
    if rate.lines:
        lines = [LineOut(**{"wastage": 0, **line}, cost=records.line_cost(line)) for line in rate.lines]
    document = session.get(Document, rate.document_id) if rate.document_id else None
    return RateOut(
        id=rate.id,
        basis=rate.basis,
        rate=records.rate_of(rate),
        unit_rate=rate.unit_rate,
        lines=lines,
        source_document=document.name if document else None,
        document_id=rate.document_id,
        page=rate.page,
        quote=rate.quote,
        library_id=rate.library_id,
        note=rate.note,
        status=rate.status,
        proposed_by=rate.proposed_by,
    )


@router.get("/tenders/{tender_id}/estimate")
def get_estimate(tender_id: str, session: DB) -> EstimateOut:
    _tender(session, tender_id)
    items = []
    for item in boq.items(session, tender_id):
        rate = records.current_rate(session, item.id)
        amount = records.money(item.quantity * records.rate_of(rate)) if rate and item.quantity is not None else None
        items.append(
            PricedItem(
                id=item.id,
                section=item.section,
                item=item.item,
                description=item.description,
                unit=item.unit,
                quantity=item.quantity,
                item_status=item.status,
                rate=_rate(session, rate),
                amount=amount,
            )
        )
    markups = records.current_markups(session, tender_id)
    return EstimateOut(
        items=items,
        markups=MarkupsOut.model_validate(markups, from_attributes=True) if markups else None,
        summary=SummaryOut(**vars(records.summary(session, tender_id))),
    )


def _proposed(session: Session, model: type[Rate] | type[Markups], record_id: str) -> Any:
    record = session.get(model, record_id)
    if record is None or tenders.get_tender(session, record.tender_id) is None:
        raise HTTPException(status_code=404, detail="Not found.")
    if record.status != "proposed":
        raise HTTPException(status_code=400, detail="This has already been decided.")
    return record


@router.post("/rates/{rate_id}/decision")
def decide_rate(rate_id: str, body: RateDecision, session: DB, request: Request) -> None:
    rate = _proposed(session, Rate, rate_id)
    records.decide(session, rate, body.approve, body.reason)
    if body.approve and body.save_to_library:
        records.save_to_library(session, rate, records.summary(session, rate.tender_id).currency or "—")
    session.commit()
    request.app.state.office.engineer_spoke(rate.tender_id)


@router.post("/tenders/{tender_id}/rates/approve-all")
def approve_all_rates(tender_id: str, session: DB, request: Request) -> Saved:
    _tender(session, tender_id)
    waiting = [
        r
        for item in boq.items(session, tender_id)
        if (r := records.current_rate(session, item.id)) is not None and r.status == "proposed"
    ]
    for rate in waiting:
        records.decide(session, rate, approve=True)
    manager = office.manager(session, tender_id)
    if waiting and manager:
        office.post(session, tender_id, ENGINEER, manager.id, f"I approved {len(waiting)} rates.")
    session.commit()
    request.app.state.office.engineer_spoke(tender_id)
    return Saved(approved=len(waiting))


@router.post("/markups/{markups_id}/decision")
def decide_markups(markups_id: str, body: DecisionIn, session: DB, request: Request) -> None:
    markups = _proposed(session, Markups, markups_id)
    records.decide(session, markups, body.approve, body.reason)
    session.commit()
    request.app.state.office.engineer_spoke(markups.tender_id)


@router.get("/library")
def get_library(session: DB, q: str = "") -> list[LibraryOut]:
    return [LibraryOut.model_validate(r, from_attributes=True) for r in records.library(session, q)]


@router.post("/library", status_code=201)
def add_to_library(body: LibraryIn, session: DB) -> LibraryOut:
    resource = LibraryResource(**body.model_dump())
    session.add(resource)
    session.commit()
    return LibraryOut.model_validate(resource, from_attributes=True)


@router.delete("/library/{resource_id}", status_code=204)
def remove_from_library(resource_id: str, session: DB) -> None:
    resource = session.get(LibraryResource, resource_id)
    if resource is None:
        raise HTTPException(status_code=404, detail="Not found.")
    if session.query(Rate).filter(Rate.library_id == resource_id).first():
        raise HTTPException(status_code=400, detail="A tender's rate is based on this entry, so it is kept.")
    session.delete(resource)
    session.commit()
