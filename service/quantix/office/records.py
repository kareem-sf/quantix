"""Reading and writing the office's records. Used by both the engineer's API and the agents' tools."""

import re
from datetime import UTC, datetime
from typing import Any

from sqlalchemy import func, or_, select
from sqlalchemy.orm import Session

from quantix.office.models import ENGINEER, TEAM, Decision, Message, Opened, Staff, Task, TurnRecord

MAX_STAFF = 8
# How a turn ended that leaves the person's work unfinished, so they carry on: cut short, or Quantix itself stopped
# (closed, or ended without a word: `ended` still empty)
UNFINISHED = {None, "interrupted", "out_of_steps", "tool_failed", "ai_failed"}


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


def post(
    session: Session,
    tender_id: str,
    sender: str,
    channel: str,
    text: str,
    kind: str = "message",
    sources: list[dict[str, Any]] | None = None,
) -> Message:
    message = Message(
        tender_id=tender_id, sender=sender, channel=channel, kind=kind, text=text.strip(), sources=sources or None
    )
    session.add(message)
    session.flush()
    return message


def send_back(session: Session, tender_id: str, record: Any, what: str, reason: str | None, by: str) -> None:
    """Send a proposal back to whoever made it. The note goes to the team room, naming them, and the redo becomes
    one of their open tasks, so they know it is theirs to do. `by` is the engineer, or the Manager reviewing his
    staff's work."""
    now = datetime.now(UTC)
    record.status, record.reason = "rejected", reason
    if by == ENGINEER:
        record.decided_at = now
    else:
        record.reviewed_by, record.reviewed_at, record.review_note = by, now, reason
    if record.proposed_by == ENGINEER:
        return
    person = session.get(Staff, record.proposed_by)
    to = f"{person.first_name}, " if person else ""
    post(session, tender_id, by, TEAM, f"{to}I sent back {what}" + (f": {reason}" if reason else "."))
    if person is not None and person.status == "active" and not person.is_manager:
        title, brief = f"Redo {what}", reason or "See the team room."
        query = select(Task).where(Task.staff_id == person.id, Task.status == "open", Task.title == title)
        earlier = session.scalars(query).first()
        if earlier is not None:  # sent back again: the newest correction is the one to follow, from now
            earlier.brief, earlier.created_at = brief, now
        else:
            session.add(Task(tender_id=tender_id, staff_id=person.id, title=title, brief=brief))


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
    if task is None or task.staff_id != member.id or task.status != "open":
        mine = open_tasks(session, member)
        raise ValueError(
            "That isn't one of your open tasks. "
            + (
                f"Yours: {', '.join(f'{t.id} ({t.title})' for t in mine)}."
                if mine
                else "You have none open: report other work in the team room."
            )
        )
    task.status, task.result, task.done_at = "done", result.strip(), datetime.now(UTC)
    post(session, member.tender_id, member.id, TEAM, f"Finished: {task.title}. {result.strip()}")
    return task


_WORD = re.compile(r"[a-z0-9.]+")
_FILLER = {"the", "and", "for", "with", "of", "to", "on", "in", "a", "an", "or", "confirm", "approve", "decision"}


def same_subject(title: str, other: str) -> bool:
    """Two question titles about the same thing: most of the shorter one's words are in the other."""
    words = [{w for w in _WORD.findall(t.lower()) if w not in _FILLER and len(w) > 2} for t in (title, other)]
    shorter = min(words, key=len)
    return bool(shorter) and len(words[0] & words[1]) / len(shorter) >= 0.6


def ask(session: Session, tender_id: str, by: Staff, title: str, text: str, options: list[str]) -> Decision:
    """One question at a time per person, so a retried turn can't ask the engineer the same thing twice."""
    waiting = session.scalars(
        select(Decision).where(
            Decision.tender_id == tender_id, Decision.raised_by == by.id, Decision.status == "waiting"
        )
    ).first()
    if waiting:
        raise ValueError(f"Your question “{waiting.title}” is still waiting for the engineer. Ask the next one after.")
    for earlier in decisions(session, tender_id):
        if earlier.status == "answered" and same_subject(title, earlier.title):
            raise ValueError(
                f"The engineer already decided “{earlier.title}”: {earlier.answer} Act on that. If your question "
                "is different, give it a title that says how."
            )
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


def note_opened(session: Session, tender_id: str, staff_id: str, kind: str, ref: str) -> None:
    """Remember that someone opened a page ("page", "<document id>:<n>") or a record (its kind and id)."""
    query = select(Opened).where(Opened.staff_id == staff_id, Opened.kind == kind, Opened.ref == ref)
    found = session.scalars(query).first()
    if found is None:
        session.add(Opened(tender_id=tender_id, staff_id=staff_id, kind=kind, ref=ref))
    else:
        found.at = datetime.now(UTC)


def has_opened(session: Session, staff_id: str, kind: str, ref: str) -> bool:
    query = select(Opened.id).where(Opened.staff_id == staff_id, Opened.kind == kind, Opened.ref == ref)
    return session.scalars(query.limit(1)).first() is not None


def new_task(session: Session, member: Staff) -> bool:
    """Whether the person has an open task given, sent back or set by themselves since their last turn began. It
    wakes them once; after that the task waits in their briefing like any other. Someone who hasn't taken a turn
    yet is woken by the message that gave them the task."""
    query = select(TurnRecord.started_at).where(TurnRecord.staff_id == member.id).order_by(TurnRecord.id.desc())
    since = session.scalars(query.limit(1)).first()
    if since is None:
        return False
    tasks = select(Task.id).where(Task.staff_id == member.id, Task.status == "open", Task.created_at > since)
    return session.scalars(tasks.limit(1)).first() is not None


def unfinished(session: Session, staff_id: str) -> bool:
    """Whether the person's last turn left their work unfinished, so they carry on without waiting for a message."""
    query = select(TurnRecord.ended).where(TurnRecord.staff_id == staff_id).order_by(TurnRecord.id.desc()).limit(1)
    last = session.execute(query).first()
    return last is not None and last.ended in UNFINISHED


def tokens_used(session: Session, tender_id: str) -> int:
    """The AI tokens the tender's office has used in its turns, read and written."""
    used = select(func.sum(TurnRecord.input_tokens + TurnRecord.output_tokens)).where(TurnRecord.tender_id == tender_id)
    return session.scalar(used) or 0


def turns_since_engineer(session: Session, tender_id: str) -> int:
    """The office's turns since the engineer last wrote to it or answered it."""
    query = select(func.max(Message.created_at)).where(Message.tender_id == tender_id, Message.sender == ENGINEER)
    spoke = session.scalar(query)
    turns = select(func.count()).select_from(TurnRecord).where(TurnRecord.tender_id == tender_id)
    if spoke is not None:
        turns = turns.where(TurnRecord.started_at > spoke)
    return session.scalar(turns)
