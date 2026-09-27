from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel, Field

from quantix import tenders
from quantix.api.tenders import DB
from quantix.office import records as office
from quantix.review import records

router = APIRouter(tags=["review"])


class Waiting(BaseModel):
    ref: str
    kind: str
    record_id: str
    producer: str
    line: str


class ReopenIn(BaseModel):
    reason: str = Field(min_length=1)


@router.get("/tenders/{tender_id}/review")
def review_queue(tender_id: str, session: DB) -> list[Waiting]:
    """What the staff proposed that the Tender Manager hasn't reviewed yet."""
    if tenders.get_tender(session, tender_id) is None:
        raise HTTPException(status_code=404, detail="Tender not found.")
    names = {m.id: m.first_name for m in office.team(session, tender_id, include_released=True)}
    return [
        Waiting(
            ref=p.ref, kind=p.kind, record_id=p.record.id, producer=p.producer, line=records.describe(session, p, names)
        )
        for p in records.pending(session, tender_id)
    ]


@router.post("/records/{kind}/{record_id}/reopen", status_code=204)
def reopen(kind: str, record_id: str, body: ReopenIn, session: DB, request: Request) -> None:
    """The engineer sends back work the office, or the engineer, already approved."""
    try:
        record = records.reopen(session, kind, record_id, body.reason)
    except LookupError as error:
        raise HTTPException(status_code=404, detail=str(error)) from error
    except ValueError as error:
        raise HTTPException(status_code=400, detail=str(error)) from error
    session.commit()
    request.app.state.office.engineer_spoke(record.tender_id)
