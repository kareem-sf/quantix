import os
import shutil
import stat
from collections.abc import Callable, Iterator
from datetime import UTC, date, datetime
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
    outcome_at: datetime | None = Field(default=None, description="When the outcome was last set")
    archived: bool = Field(default=False, description="Put away: kept in the register, out of the sidebar and the Desk")
    created_at: datetime


def _out(session: Session, tender: service.Tender) -> TenderOut:
    out = TenderOut.model_validate(tender)
    out.archived = tender.archived_at is not None
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
    archived: bool | None = None


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
def change_tender(tender_id: str, body: TenderChange, session: DB, request: Request) -> TenderOut:
    tender = service.get_tender(session, tender_id)
    if tender is None:
        raise HTTPException(status_code=404, detail="Tender not found.")
    if body.outcome is not None and body.outcome != tender.outcome:
        tender.outcome, tender.outcome_at = body.outcome, datetime.now(UTC)
    if body.archived is not None and body.archived != (tender.archived_at is not None):
        tender.archived_at = datetime.now(UTC) if body.archived else None
        if body.archived:
            request.app.state.office.stop(tender_id)  # an archived tender's team stops; writing to it starts it again
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
    shutil.rmtree(request.app.state.home / "tenders" / tender_id, onexc=_remove_anyway)


def _remove_anyway(function: Callable[[str], object], path: str, _error: BaseException) -> None:
    """Copies opened in their apps are read-only: make them writable and try again. A file still open stays."""
    try:
        os.chmod(path, stat.S_IWRITE)
        function(path)
    except OSError:
        pass
