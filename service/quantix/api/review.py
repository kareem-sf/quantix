from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel, Field

from quantix import tenders
from quantix.api.tenders import DB
from quantix.office import records as office
from quantix.office.models import Staff
from quantix.review import audit, records

router = APIRouter(tags=["review"])


class Waiting(BaseModel):
    ref: str
    kind: str
    record_id: str
    producer: str
    line: str


class ReopenIn(BaseModel):
    reason: str = Field(min_length=1)


class SourceOut(BaseModel):
    label: str
    document_id: str | None
    page: int | None


class FindingOut(BaseModel):
    severity: str  # blocker | warning
    message: str
    refs: list[SourceOut]
    accepted_by: str | None  # the Manager's first name, when he accepted the warning
    reason: str | None


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


@router.get("/records/{kind}/{record_id}/findings")
def findings(kind: str, record_id: str, session: DB, request: Request) -> list[FindingOut]:
    """What Quantix's checks find in a record now, with the Manager's reason for each warning he accepted."""
    try:
        found = records.record_findings(session, request.app.state.home, kind, record_id)
    except LookupError as error:
        raise HTTPException(status_code=404, detail=str(error)) from error
    out = []
    for finding, acceptance in found:
        person = session.get(Staff, acceptance.accepted_by) if acceptance else None
        out.append(
            FindingOut(
                severity=finding.severity,
                message=finding.message,
                refs=[SourceOut(label=r.label, document_id=r.document_id, page=r.page) for r in finding.refs],
                accepted_by=person.first_name if person else None,
                reason=acceptance.reason if acceptance else None,
            )
        )
    return out


@router.get("/tenders/{tender_id}/audit")
def tender_audit(tender_id: str, session: DB, request: Request) -> list[FindingOut]:
    """What keeps the tender from release, blockers first, with the Manager's reason for each warning he accepted."""
    if tenders.get_tender(session, tender_id) is None:
        raise HTTPException(status_code=404, detail="Tender not found.")
    settled = records.accepted(session, tender_id)
    out = []
    for finding in audit.findings(session, request.app.state.home, tender_id):
        acceptance = settled.get(finding.key) if finding.severity == "warning" else None
        person = session.get(Staff, acceptance.accepted_by) if acceptance else None
        out.append(
            FindingOut(
                severity=finding.severity,
                message=finding.message,
                refs=[SourceOut(label=r.label, document_id=r.document_id, page=r.page) for r in finding.refs],
                accepted_by=person.first_name if person else None,
                reason=acceptance.reason if acceptance else None,
            )
        )
    return out
