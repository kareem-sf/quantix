"""The engineer's desk: every tender at a glance, for the Desk and the tender register."""

from collections.abc import Iterator
from datetime import date, datetime
from decimal import Decimal
from typing import Annotated, Literal

from fastapi import APIRouter, Depends, Request
from pydantic import BaseModel, Field
from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix import tenders
from quantix.api.boq import gates
from quantix.core.db import sessions
from quantix.documents.models import Document
from quantix.estimate import records as estimate
from quantix.office import records as office
from quantix.subcontract import records as subcontract
from quantix.submission import records as submission

router = APIRouter(tags=["desk"])


def db(request: Request) -> Iterator[Session]:
    yield from sessions(request.app.state.sessions)


DB = Annotated[Session, Depends(db)]


class TenderGlance(BaseModel):
    """One tender as the Desk and the register show it. Quantix counts every figure; nothing here is the AI's."""

    id: str
    name: str
    due_date: date | None
    outcome: Literal["open", "submitted", "won", "lost"]
    outcome_at: datetime | None
    archived: bool
    created_at: datetime
    team: Literal["working", "paused", "idle"]
    doing: str | None = Field(description="What someone on the team is doing now, with their first name")
    waiting: int = Field(description="Approvals at every gate and questions, waiting for the engineer")
    documents: int
    read: int = Field(description="Documents read, or that can't be read, out of the current ones")
    items: int
    priced: int
    currency: str
    total: Decimal | None = Field(description="The tender total before VAT, once an item is priced")
    packages: int
    chosen: int
    requirements: int
    ready: int


@router.get("/desk")
def desk(session: DB, request: Request) -> list[TenderGlance]:
    return [_glance(session, request, t) for t in tenders.list_tenders(session)]


def _glance(session: Session, request: Request, tender: tenders.Tender) -> TenderGlance:
    state = request.app.state.office.status(tender.id)
    busy = next((m for m in office.team(session, tender.id) if m.now), None) if state == "working" else None
    waiting = gates(tender.id, session).model_dump()
    waiting.pop("manager")  # still with the Tender Manager, not yet the engineer's
    documents = list(session.scalars(select(Document.status).where(Document.tender_id == tender.id)))
    current = [s for s in documents if s != "replaced"]
    price = estimate.summary(session, tender.id)
    packages = subcontract.packages(session, tender.id)
    checklist = [submission.state(session, r) for r in submission.requirements(session, tender.id)]
    return TenderGlance(
        id=tender.id,
        name=tender.name,
        due_date=tender.due_date,
        outcome=tender.outcome,
        outcome_at=tender.outcome_at,
        archived=tender.archived_at is not None,
        created_at=tender.created_at,
        team=state,
        doing=f"{busy.first_name}: {busy.now}" if busy else None,
        waiting=sum(waiting.values()) + len(office.decisions(session, tender.id, waiting_only=True)),
        documents=len(current),
        read=sum(s not in ("waiting", "reading") for s in current),
        items=price.items,
        priced=price.priced,
        currency=price.currency,
        total=price.total if price.priced else None,
        packages=len(packages),
        chosen=sum(p.selected_quote_id is not None for p in packages),
        requirements=len(checklist),
        ready=checklist.count("ready"),
    )
