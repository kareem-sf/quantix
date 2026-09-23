from decimal import Decimal

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel
from sqlalchemy.orm import Session

from quantix import tenders
from quantix.api.tenders import DB
from quantix.boq import records
from quantix.boq.models import FACT_KINDS, BoqItem, Fact
from quantix.documents.models import Document
from quantix.estimate import records as estimate
from quantix.office import records as office
from quantix.office.models import ENGINEER
from quantix.subcontract import records as subcontract
from quantix.submission import records as submission
from quantix.takeoff import records as takeoff

router = APIRouter(tags=["boq"])


class Source(BaseModel):
    document_id: str
    document_name: str
    page: int
    quote: str


class ItemOut(BaseModel):
    id: str
    section: str | None
    item: str
    description: str
    unit: str
    quantity: Decimal | None
    status: str
    proposed_by: str
    reason: str | None
    source: Source


class FactOut(BaseModel):
    id: str
    kind: str
    label: str
    value: str
    status: str
    proposed_by: str
    source: Source


class Boq(BaseModel):
    items: list[ItemOut]
    facts: list[FactOut]


class DecisionIn(BaseModel):
    approve: bool
    reason: str | None = None


class Gates(BaseModel):
    boq: int
    facts: int
    takeoff: int
    pricing: int
    subcontract: int
    submission: int


class Approved(BaseModel):
    approved: int


def _source(session: Session, record: BoqItem | Fact) -> Source:
    document = session.get(Document, record.document_id)
    return Source(document_id=record.document_id, document_name=document.name, page=record.page, quote=record.quote)


def _tender(session: Session, tender_id: str) -> None:
    if tenders.get_tender(session, tender_id) is None:
        raise HTTPException(status_code=404, detail="Tender not found.")


def _record(session: Session, model: type[BoqItem] | type[Fact], record_id: str) -> BoqItem | Fact:
    record = session.get(model, record_id)
    if record is None or tenders.get_tender(session, record.tender_id) is None:
        raise HTTPException(status_code=404, detail="Not found.")
    if record.status != "proposed":
        raise HTTPException(status_code=400, detail="This has already been decided.")
    return record


@router.get("/tenders/{tender_id}/boq")
def get_boq(tender_id: str, session: DB) -> Boq:
    _tender(session, tender_id)
    return Boq(
        items=[
            ItemOut(
                id=i.id,
                section=i.section,
                item=i.item,
                description=i.description,
                unit=i.unit,
                quantity=i.quantity,
                status=i.status,
                proposed_by=i.proposed_by,
                reason=i.reason,
                source=_source(session, i),
            )
            for i in records.items(session, tender_id)
        ],
        facts=[
            FactOut(
                id=f.id,
                kind=f.kind,
                label=FACT_KINDS[f.kind],
                value=f.value,
                status=f.status,
                proposed_by=f.proposed_by,
                source=_source(session, f),
            )
            for f in records.facts(session, tender_id)
        ],
    )


@router.get("/tenders/{tender_id}/gates")
def gates(tender_id: str, session: DB) -> Gates:
    _tender(session, tender_id)
    return Gates(
        **records.waiting_counts(session, tender_id),
        takeoff=takeoff.waiting(session, tender_id),
        pricing=estimate.waiting(session, tender_id),
        subcontract=subcontract.waiting(session, tender_id),
        submission=submission.waiting(session, tender_id),
    )


@router.post("/boq/{item_id}/decision")
def decide_item(item_id: str, body: DecisionIn, session: DB, request: Request) -> None:
    item = _record(session, BoqItem, item_id)
    records.decide(session, item, body.approve, body.reason)
    session.commit()
    request.app.state.office.engineer_spoke(item.tender_id)


@router.post("/tenders/{tender_id}/boq/approve-all")
def approve_all(tender_id: str, session: DB, request: Request) -> Approved:
    _tender(session, tender_id)
    approved = records.approve_all_items(session, tender_id)
    session.commit()
    request.app.state.office.engineer_spoke(tender_id)
    return Approved(approved=approved)


@router.post("/facts/{fact_id}/decision")
def decide_fact(fact_id: str, body: DecisionIn, session: DB, request: Request) -> None:
    fact = _record(session, Fact, fact_id)
    records.decide(session, fact, body.approve, body.reason)
    manager = office.manager(session, fact.tender_id)
    if body.approve and manager:
        office.post(
            session,
            fact.tender_id,
            ENGINEER,
            manager.id,
            f"I approved the {FACT_KINDS[fact.kind].lower()}: {fact.value}.",
        )
    session.commit()
    request.app.state.office.engineer_spoke(fact.tender_id)
