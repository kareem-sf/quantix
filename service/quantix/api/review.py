from datetime import datetime
from typing import Literal

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel, ConfigDict, Field

from quantix import tenders
from quantix.api.tenders import DB
from quantix.office import records as office
from quantix.office.models import Staff
from quantix.review import audit, lessons, records
from quantix.review.models import Lesson

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


class LessonOut(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    id: str
    text: str
    topic: str
    source: str  # the work it came from, e.g. "the rate for BOQ item 3.1"
    status: str  # tender: the office follows it on this tender | kept: a company rule too | dropped
    created_at: datetime


class LessonDecision(BaseModel):
    status: Literal["kept", "dropped"]


@router.get("/tenders/{tender_id}/lessons")
def tender_lessons(tender_id: str, session: DB) -> list[LessonOut]:
    """What the office learned on this tender from work that needed correcting, and whether the engineer kept it."""
    if tenders.get_tender(session, tender_id) is None:
        raise HTTPException(status_code=404, detail="Tender not found.")
    found = lessons.lessons(session, tender_id, (lessons.TENDER, lessons.KEPT))
    return [LessonOut.model_validate(lesson) for lesson in found]


@router.patch("/lessons/{lesson_id}")
def decide_lesson(lesson_id: str, body: LessonDecision, session: DB) -> LessonOut:
    """The engineer keeps a lesson as a company rule for later tenders, or drops it."""
    lesson = session.get(Lesson, lesson_id)
    if lesson is None:
        raise HTTPException(status_code=404, detail="Not found.")
    try:
        lessons.decide(session, lesson, body.status)
    except ValueError as error:
        raise HTTPException(status_code=400, detail=str(error)) from error
    session.commit()
    return LessonOut.model_validate(lesson)
