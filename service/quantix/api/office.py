from datetime import datetime
from typing import Annotated, Any, Literal

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel, ConfigDict, StringConstraints
from sqlalchemy.orm import Session

from quantix import settings, tenders
from quantix.api.tenders import DB
from quantix.estimate import records as estimate
from quantix.estimate.models import Rate
from quantix.office import records
from quantix.office.models import ENGINEER, TEAM, Decision, Staff, TurnRecord
from quantix.review import records as reviews

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


class DecisionSource(BaseModel):
    label: str
    document_id: str | None = None
    page: int | None = None
    boq_item_id: str | None = None
    url: str | None = None  # a web page the office read


class MessageOut(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    id: int
    sender: str
    channel: str
    kind: str
    text: str
    sources: list[DecisionSource] | None
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
    # an escalation: the record the office couldn't settle, and where the problem shows
    subject_kind: str | None
    subject_id: str | None
    sources: list[DecisionSource] | None
    status: str
    answer: str | None
    created_at: datetime


class OfficeOut(BaseModel):
    state: Literal["working", "paused", "idle"]
    notice: str | None  # why the office paused itself; none when the engineer stopped it
    ai_ready: bool
    staff: list[StaffOut]
    waiting: int


class TurnOut(BaseModel):
    """One person's turn at work, as the chat shows it folded."""

    id: int
    staff_id: str
    started_at: datetime
    ended_at: datetime | None
    running: bool
    # done | out_of_steps | tool_failed | ai_failed | stopped | failed | interrupted; none if Quantix closed mid-turn
    ended: str | None
    note: str | None
    steps: int
    doing: str | None  # the latest thing done, in words


class TurnStep(BaseModel):
    kind: Literal["brief", "thinking", "note", "tool"]
    text: str | None = None
    tool: str | None = None
    args: str | None = None
    doing: str | None = None
    result: str | None = None
    sent_back: str | None = None


class TurnDetail(TurnOut):
    """The turn opened: what the person was told, their thinking and notes, and each tool call with its answer."""

    log: list[TurnStep]


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
    state = request.app.state.office.status(tender_id)
    staff = [StaffOut.model_validate(m) for m in records.team(session, tender_id, include_released=True)]
    if state != "working":  # what someone was doing when the office stopped isn't what they are doing now
        staff = [m.model_copy(update={"now": None}) for m in staff]
    return OfficeOut(
        state=state,
        notice=records.pause_notice(session, tender_id) if state == "paused" else None,
        ai_ready=settings.load(request.app.state.home)["office_ai"] is not None,
        staff=staff,
        waiting=len(records.decisions(session, tender_id, waiting_only=True)),
    )


# what a step did, for tools that don't say it themselves while they work
DID = {
    "message_engineer": "Wrote to you",
    "ask_engineer": "Asked you to decide",
    "escalate": "Brought a problem to you",
    "post_to_team": "Wrote to the team room",
    "raise_concern": "Raised a concern",
    "review": "Gave the review",
    "complete_task": "Finished a task",
}


ASKED = "The question is waiting for the engineer."  # what ask_engineer answers when the question went to them


def _in_words(step: dict[str, Any]) -> dict[str, Any]:
    if step["kind"] != "tool" or step["doing"]:
        return step
    if step["tool"] == "ask_engineer" and step["result"] not in (None, ASKED):
        return step | {"doing": "Question not sent"}  # refused: the chat said "Asked you to decide" for it
    return step | {"doing": DID.get(step["tool"], step["tool"].replace("_", " ").capitalize())}


def _turn_out(turn: TurnRecord, running: bool) -> dict[str, Any]:
    steps = turn.steps or []
    calls = [_in_words(s) for s in steps if s["kind"] == "tool"]
    done = [s for s in calls if not s["sent_back"]] or calls  # what went through says what the turn did
    last = done[-1] if done else None
    return {
        "id": turn.id,
        "staff_id": turn.staff_id,
        "started_at": turn.started_at,
        "ended_at": turn.ended_at,
        "running": running,
        "ended": turn.ended,
        "note": turn.note,
        "steps": len(steps),
        "doing": last and last["doing"],
    }


def _running(session: Session, request: Request, tender_id: str) -> int | None:
    """The turn under way, if the office is working on this tender: its newest turn, still open."""
    if request.app.state.office.status(tender_id) != "working":
        return None
    latest = records.turns(session, tender_id, limit=1)
    return latest[0].id if latest and latest[0].ended_at is None else None


@router.get("/tenders/{tender_id}/turns")
def list_turns(tender_id: str, session: DB, request: Request, staff_id: str | None = None) -> list[TurnOut]:
    _tender(session, tender_id)
    running = _running(session, request, tender_id)
    return [TurnOut(**_turn_out(t, t.id == running)) for t in records.turns(session, tender_id, staff_id)]


@router.get("/turns/{turn_id}")
def turn_detail(turn_id: int, session: DB, request: Request) -> TurnDetail:
    turn = session.get(TurnRecord, turn_id)
    if turn is None or tenders.get_tender(session, turn.tender_id) is None:
        raise HTTPException(status_code=404, detail="Turn not found.")
    running = _running(session, request, turn.tender_id) == turn.id
    return TurnDetail(**_turn_out(turn, running), log=[TurnStep(**_in_words(s)) for s in turn.steps or []])


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
    reviews.apply_answer(session, decision, body.answer)
    if decision.subject_kind == "library" and body.answer == estimate.KEEP_IN_LIBRARY:
        rate = session.get(Rate, decision.subject_id)
        if rate is not None:
            estimate.save_to_library(session, rate, estimate.summary(session, rate.tender_id).currency or "—")
    session.commit()
    request.app.state.office.engineer_spoke(decision.tender_id)
    return DecisionOut.model_validate(decision)


@router.post("/tenders/{tender_id}/office/stop", status_code=204)
def stop_office(tender_id: str, session: DB, request: Request) -> None:
    _tender(session, tender_id)
    request.app.state.office.stop(tender_id)
