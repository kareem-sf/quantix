from datetime import datetime
from typing import Annotated, Any, Literal

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel, ConfigDict, StringConstraints
from sqlalchemy.orm import Session

from quantix import settings, tenders
from quantix.api.tenders import DB
from quantix.office import records
from quantix.office.models import ENGINEER, TEAM, Decision, Staff

router = APIRouter(tags=["office"])
Text = Annotated[str, StringConstraints(strip_whitespace=True, min_length=1, max_length=8000)]


class StaffOut(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    id: str
    name: str
    role: str
    is_manager: bool
    profile: dict[str, Any]
    status: str
    now: str | None


class MessageOut(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    id: int
    sender: str
    channel: str
    kind: str
    text: str
    created_at: datetime


class TaskOut(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    id: str
    staff_id: str
    title: str
    brief: str
    status: str
    result: str | None


class DecisionOut(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    id: str
    raised_by: str
    title: str
    text: str
    options: list[str]
    status: str
    answer: str | None
    created_at: datetime


class OfficeOut(BaseModel):
    state: Literal["working", "paused", "idle"]
    ai_ready: bool
    staff: list[StaffOut]
    waiting: int


class MessageIn(BaseModel):
    channel: str
    text: Text


class AnswerIn(BaseModel):
    answer: Text


def _tender(session: Session, tender_id: str) -> None:
    if tenders.get_tender(session, tender_id) is None:
        raise HTTPException(status_code=404, detail="Tender not found.")


@router.get("/tenders/{tender_id}/office")
def office(tender_id: str, session: DB, request: Request) -> OfficeOut:
    _tender(session, tender_id)
    return OfficeOut(
        state=request.app.state.office.status(tender_id),
        ai_ready=settings.load(request.app.state.home)["office_ai"] is not None,
        staff=[StaffOut.model_validate(m) for m in records.team(session, tender_id, include_released=True)],
        waiting=len(records.decisions(session, tender_id, waiting_only=True)),
    )


@router.get("/tenders/{tender_id}/messages")
def list_messages(tender_id: str, session: DB, channel: str = TEAM) -> list[MessageOut]:
    _tender(session, tender_id)
    return [MessageOut.model_validate(m) for m in records.messages(session, tender_id, channel)]


@router.post("/tenders/{tender_id}/messages", status_code=201)
def send_message(tender_id: str, body: MessageIn, session: DB, request: Request) -> MessageOut:
    _tender(session, tender_id)
    if body.channel != TEAM:
        member = session.get(Staff, body.channel)
        if member is None or member.tender_id != tender_id or member.status != "active":
            raise HTTPException(status_code=400, detail="That person isn't on this tender's team.")
    message = records.post(session, tender_id, ENGINEER, body.channel, body.text)
    session.commit()
    request.app.state.office.engineer_spoke(tender_id)
    return MessageOut.model_validate(message)


@router.get("/tenders/{tender_id}/tasks")
def list_tasks(tender_id: str, session: DB) -> list[TaskOut]:
    _tender(session, tender_id)
    return [TaskOut.model_validate(t) for t in records.all_tasks(session, tender_id)]


@router.get("/tenders/{tender_id}/decisions")
def list_decisions(tender_id: str, session: DB) -> list[DecisionOut]:
    _tender(session, tender_id)
    return [DecisionOut.model_validate(d) for d in records.decisions(session, tender_id)]


@router.post("/decisions/{decision_id}/answer")
def answer_decision(decision_id: str, body: AnswerIn, session: DB, request: Request) -> DecisionOut:
    decision = session.get(Decision, decision_id)
    if decision is None or tenders.get_tender(session, decision.tender_id) is None:
        raise HTTPException(status_code=404, detail="Decision not found.")
    if decision.status != "waiting":
        raise HTTPException(status_code=400, detail="This has already been decided.")
    records.answer(session, decision, body.answer)
    session.commit()
    request.app.state.office.engineer_spoke(decision.tender_id)
    return DecisionOut.model_validate(decision)


@router.post("/tenders/{tender_id}/office/stop", status_code=204)
def stop_office(tender_id: str, session: DB, request: Request) -> None:
    _tender(session, tender_id)
    request.app.state.office.stop(tender_id)
