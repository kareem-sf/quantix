"""Reading and writing the office's records. Used by both the engineer's API and the agents' tools."""

from datetime import UTC, datetime
from typing import Any

from sqlalchemy import or_, select
from sqlalchemy.orm import Session

from quantix.office.models import ENGINEER, TEAM, Decision, Message, Staff, Task

MAX_STAFF = 8


def team(session: Session, tender_id: str, include_released: bool = False) -> list[Staff]:
    query = select(Staff).where(Staff.tender_id == tender_id)
    if not include_released:
        query = query.where(Staff.status == "active")
    return list(session.scalars(query.order_by(Staff.is_manager.desc(), Staff.created_at)))


def manager(session: Session, tender_id: str) -> Staff | None:
    return session.scalars(select(Staff).where(Staff.tender_id == tender_id, Staff.is_manager)).first()


def find_staff(session: Session, tender_id: str, name: str) -> Staff | None:
    """A team member by full or first name, ignoring case."""
    wanted = name.strip().lower()
    for member in team(session, tender_id):
        if wanted in (member.name.lower(), member.first_name.lower()):
            return member
    return None


def hire(session: Session, tender_id: str, name: str, role: str, profile: dict[str, Any], is_manager=False) -> Staff:
    if find_staff(session, tender_id, name):
        raise ValueError(f"{name} is already on the team. Choose a different name.")
    if not is_manager and len(team(session, tender_id)) - 1 >= MAX_STAFF:
        raise ValueError(f"The team already has {MAX_STAFF} staff. Release someone before hiring.")
    member = Staff(tender_id=tender_id, name=name.strip(), role=role.strip(), profile=profile, is_manager=is_manager)
    session.add(member)
    session.flush()
    return member


def post(session: Session, tender_id: str, sender: str, channel: str, text: str, kind: str = "message") -> Message:
    message = Message(tender_id=tender_id, sender=sender, channel=channel, kind=kind, text=text.strip())
    session.add(message)
    session.flush()
    return message


def messages(session: Session, tender_id: str, channel: str, limit: int = 200) -> list[Message]:
    query = (
        select(Message)
        .where(Message.tender_id == tender_id, Message.channel == channel)
        .order_by(Message.id.desc())
        .limit(limit)
    )
    return list(reversed(session.scalars(query).all()))


def inbox(session: Session, member: Staff) -> list[Message]:
    """What this person hasn't seen yet and should act on.

    The Manager follows the whole team room. Staff act on their tasks, on messages that name them, and on anything
    the engineer sends them directly. Their own messages never wake them."""
    query = select(Message).where(
        Message.tender_id == member.tender_id,
        Message.id > member.last_read,
        Message.sender != member.id,
        or_(Message.channel == TEAM, Message.channel == member.id),
    )
    new = list(session.scalars(query.order_by(Message.id)))
    if member.is_manager:
        return new
    name = member.first_name.lower()
    return [m for m in new if m.channel == member.id or name in m.text.lower()]


def mark_read(session: Session, member: Staff) -> None:
    newest = session.scalars(
        select(Message.id).where(Message.tender_id == member.tender_id).order_by(Message.id.desc()).limit(1)
    ).first()
    member.last_read = newest or 0


def assign(session: Session, tender_id: str, by: Staff, to: Staff, title: str, brief: str) -> Task:
    task = Task(tender_id=tender_id, staff_id=to.id, title=title.strip(), brief=brief.strip())
    session.add(task)
    post(session, tender_id, by.id, TEAM, f"{by.first_name} asked {to.first_name} to {title.strip()}", kind="task")
    session.flush()
    return task


def open_tasks(session: Session, member: Staff) -> list[Task]:
    return list(
        session.scalars(select(Task).where(Task.staff_id == member.id, Task.status == "open").order_by(Task.created_at))
    )


def all_tasks(session: Session, tender_id: str) -> list[Task]:
    return list(session.scalars(select(Task).where(Task.tender_id == tender_id).order_by(Task.created_at)))


def complete(session: Session, member: Staff, task_id: str, result: str) -> Task:
    task = session.get(Task, task_id)
    if task is None or task.staff_id != member.id:
        raise ValueError("That task isn't one of yours. Check the task list you were given.")
    task.status, task.result, task.done_at = "done", result.strip(), datetime.now(UTC)
    post(session, member.tender_id, member.id, TEAM, f"Finished: {task.title}. {result.strip()}")
    return task


def ask(session: Session, tender_id: str, by: Staff, title: str, text: str, options: list[str]) -> Decision:
    decision = Decision(tender_id=tender_id, raised_by=by.id, title=title.strip(), text=text.strip(), options=options)
    session.add(decision)
    session.flush()
    return decision


def decisions(session: Session, tender_id: str, waiting_only: bool = False) -> list[Decision]:
    query = select(Decision).where(Decision.tender_id == tender_id)
    if waiting_only:
        query = query.where(Decision.status == "waiting")
    return list(session.scalars(query.order_by(Decision.created_at)))


def answer(session: Session, decision: Decision, text: str) -> None:
    """Record the engineer's answer and tell whoever asked, in their chat with the engineer."""
    decision.status, decision.answer, decision.decided_at = "answered", text.strip(), datetime.now(UTC)
    post(session, decision.tender_id, ENGINEER, decision.raised_by, f"About “{decision.title}”: {text.strip()}")
