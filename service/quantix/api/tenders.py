from collections.abc import Iterator
from datetime import date, datetime
from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, Request
from pydantic import BaseModel, ConfigDict, Field, StringConstraints
from sqlalchemy.orm import Session

from quantix import tenders as service
from quantix.core.db import sessions

router = APIRouter(prefix="/tenders", tags=["tenders"])


def db(request: Request) -> Iterator[Session]:
    yield from sessions(request.app.state.sessions)


DB = Annotated[Session, Depends(db)]


class TenderCreate(BaseModel):
    name: Annotated[str, StringConstraints(strip_whitespace=True, min_length=1, max_length=200)]
    due_date: date | None = None


class TenderOut(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    id: str
    name: str
    due_date: date | None = Field(description="Submission deadline, when known")
    created_at: datetime


@router.get("")
def list_tenders(session: DB) -> list[TenderOut]:
    return [TenderOut.model_validate(t) for t in service.list_tenders(session)]


@router.post("", status_code=201)
def create_tender(body: TenderCreate, session: DB) -> TenderOut:
    return TenderOut.model_validate(service.create_tender(session, body.name, body.due_date))


@router.get("/{tender_id}")
def get_tender(tender_id: str, session: DB) -> TenderOut:
    tender = service.get_tender(session, tender_id)
    if tender is None:
        raise HTTPException(status_code=404, detail="Tender not found.")
    return TenderOut.model_validate(tender)
