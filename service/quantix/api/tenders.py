import shutil
from collections.abc import Iterator
from datetime import date, datetime
from typing import Annotated, Literal

from fastapi import APIRouter, Depends, HTTPException, Request
from pydantic import BaseModel, ConfigDict, Field, StringConstraints
from sqlalchemy.orm import Session

from quantix import tenders as service
from quantix.core.db import sessions
from quantix.documents.models import Document
from quantix.office.models import ENGINEER, Staff

router = APIRouter(prefix="/tenders", tags=["tenders"])


def db(request: Request) -> Iterator[Session]:
    yield from sessions(request.app.state.sessions)


DB = Annotated[Session, Depends(db)]


class TenderCreate(BaseModel):
    name: Annotated[str, StringConstraints(strip_whitespace=True, min_length=1, max_length=200)]
    due_date: date | None = None


class DueSource(BaseModel):
    """Where the due date comes from, shown with it."""

    basis: Literal["engineer", "document"] = Field(description="The engineer's own date, or a tender document's")
    set_by: str | None = Field(description="Who entered it, when not the engineer: the staff member's first name")
    set_at: datetime | None = Field(description="When it was entered; unknown for dates entered before this was kept")
    document_id: str | None = None
    document_name: str | None = None
    page: int | None = None
    quote: str | None = None


class TenderOut(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    id: str
    name: str
    due_date: date | None = Field(description="Submission deadline, when known")
    due_date_source: DueSource | None = None
    outcome: Literal["open", "submitted", "won", "lost"]
    created_at: datetime


def _out(session: Session, tender: service.Tender) -> TenderOut:
    out = TenderOut.model_validate(tender)
    if tender.due_date is None or tender.due_date_basis is None:
        return out
    person = session.get(Staff, tender.due_date_by) if tender.due_date_by != ENGINEER else None
    document = session.get(Document, tender.due_date_document_id) if tender.due_date_document_id else None
    out.due_date_source = DueSource(
        basis=tender.due_date_basis,
        set_by=person.first_name if person else None,
        set_at=tender.due_date_at,
        document_id=document.id if document else None,
        document_name=document.name if document else None,
        page=tender.due_date_page if document else None,
        quote=tender.due_date_quote if document else None,
    )
    return out


class TenderChange(BaseModel):
    """Only the fields sent change; a due date sent as null clears it."""

    outcome: Literal["open", "submitted", "won", "lost"] | None = None
    due_date: date | None = None


@router.get("")
def list_tenders(session: DB) -> list[TenderOut]:
    return [_out(session, t) for t in service.list_tenders(session)]


@router.post("", status_code=201)
def create_tender(body: TenderCreate, session: DB) -> TenderOut:
    return _out(session, service.create_tender(session, body.name, body.due_date))


@router.get("/{tender_id}")
def get_tender(tender_id: str, session: DB) -> TenderOut:
    tender = service.get_tender(session, tender_id)
    if tender is None:
        raise HTTPException(status_code=404, detail="Tender not found.")
    return _out(session, tender)


@router.patch("/{tender_id}")
def change_tender(tender_id: str, body: TenderChange, session: DB) -> TenderOut:
    tender = service.get_tender(session, tender_id)
    if tender is None:
        raise HTTPException(status_code=404, detail="Tender not found.")
    if body.outcome is not None:
        tender.outcome = body.outcome
    if "due_date" in body.model_fields_set:  # the engineer's own date, whatever the documents say
        service.set_due_date(tender, body.due_date, ENGINEER)
    session.commit()
    return _out(session, tender)


@router.delete("/{tender_id}", status_code=204)
def delete_tender(tender_id: str, session: DB, request: Request) -> None:
    """The tender and everything Quantix keeps for it. Built packages in exports and the engineer's own files stay."""
    tender = service.get_tender(session, tender_id)
    if tender is None:
        raise HTTPException(status_code=404, detail="Tender not found.")
    request.app.state.office.stop(tender_id)
    session.delete(tender)  # the database removes its documents, BOQ, rates, packages and the rest with it
    session.commit()
    shutil.rmtree(request.app.state.home / "tenders" / tender_id, ignore_errors=True)
