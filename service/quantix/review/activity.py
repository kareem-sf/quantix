"""What the office has said and done: its conversation searched by words, what changed since a moment, and what
Quantix's checks find in someone's own work before the Tender Manager sees it."""

from datetime import UTC, datetime, timedelta
from pathlib import Path

from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix.documents.arabic import searchable
from quantix.office import records as office
from quantix.office.models import ENGINEER, TEAM, Decision, Message, Task
from quantix.review import records as reviews

SHOWN = 30  # conversation lines at a time


def _names(session: Session, tender_id: str) -> dict[str, str]:
    names = {m.id: m.first_name for m in office.team(session, tender_id, include_released=True)}
    names[ENGINEER] = "The engineer"
    return names


def conversation(session: Session, tender_id: str, words: str) -> list[str]:
    """Messages, tasks and decisions with every word, newest first."""
    wanted = searchable(words).split()
    names = _names(session, tender_id)

    def has(*texts: str | None) -> bool:
        joined = searchable(" ".join(t for t in texts if t))
        return all(w in joined for w in wanted)

    found: list[tuple[datetime, str]] = []
    for m in session.scalars(select(Message).where(Message.tender_id == tender_id)):
        if has(m.text):
            where = (
                "team room" if m.channel == TEAM else f"chat between {names.get(m.channel, 'someone')} and the engineer"
            )
            found.append((m.created_at, f"{names.get(m.sender, 'Quantix')} in the {where}: {m.text[:300]}"))
    for t in office.all_tasks(session, tender_id):
        if has(t.title, t.brief, t.result):
            result = f" Result: {t.result[:200]}" if t.result else ""
            found.append(
                (t.created_at, f"Task for {names.get(t.staff_id, 'someone')}: {t.title} ({t.status}).{result}")
            )
    for d in office.decisions(session, tender_id):
        if has(d.title, d.text, d.answer):
            answer = f" The engineer answered: {d.answer}" if d.answer else " Waiting for the engineer."
            found.append((d.created_at, f"Decision “{d.title}”: {d.text[:200]}{answer}"))
    return [f"{at:%d %b %H:%M} · {text}" for at, text in sorted(found, key=lambda f: f[0], reverse=True)]


def last_word(session: Session, tender_id: str) -> datetime | None:
    """When the engineer last wrote to the office."""
    query = select(Message.created_at).where(Message.tender_id == tender_id, Message.sender == ENGINEER)
    return session.scalars(query.order_by(Message.id.desc()).limit(1)).first()


def changed(session: Session, tender_id: str, since: datetime) -> str:
    """What the office filed, what the Manager decided, what the engineer decided and which tasks and questions
    closed, since a moment."""
    names = _names(session, tender_id)
    filed: dict[str, dict[str, int]] = {}
    reviewed = {"accepted": 0, "sent back": 0}
    decided = {"approved": 0, "sent back": 0}
    for kind, (model, _) in reviews.REVIEWED_KINDS.items():
        for r in session.scalars(select(model).where(model.tender_id == tender_id, model.created_at >= since)):
            who = filed.setdefault(names.get(r.proposed_by, "Someone"), {})
            who[kind] = who.get(kind, 0) + 1
        for r in session.scalars(select(model).where(model.tender_id == tender_id, model.reviewed_at >= since)):
            reviewed["sent back" if r.status == "rejected" and not r.decided_at else "accepted"] += 1
        for r in session.scalars(select(model).where(model.tender_id == tender_id, model.decided_at >= since)):
            decided["sent back" if r.status == "rejected" else "approved"] += 1
    tasks = select(Task).where(Task.tender_id == tender_id, Task.done_at >= since)
    finished = [f"{names.get(t.staff_id, 'Someone')}: {t.title}" for t in session.scalars(tasks)]
    answered = select(Decision).where(Decision.tender_id == tender_id, Decision.decided_at >= since)
    questions = [d.title for d in session.scalars(answered)]
    lines = [f"Since {since:%d %b %H:%M}:"]
    if filed:
        lines.append(
            "Filed: "
            + "; ".join(
                f"{who} {', '.join(f'{n} {reviews.NAMES[k][n != 1]}' for k, n in kinds.items())}"
                for who, kinds in filed.items()
            )  # fmt: skip
        )
    if any(reviewed.values()):
        lines.append(f"The Tender Manager accepted {reviewed['accepted']} and sent back {reviewed['sent back']}.")
    if any(decided.values()):
        lines.append(f"The engineer approved {decided['approved']} and sent back {decided['sent back']}.")
    if finished:
        lines.append("Tasks finished: " + "; ".join(finished))
    if questions:
        lines.append("Questions the engineer answered: " + "; ".join(questions))
    return "\n".join(lines) if len(lines) > 1 else f"Nothing has changed since {since:%d %b %H:%M}."


def since(session: Session, tender_id: str, hours: float | None) -> datetime:
    if hours:
        return datetime.now(UTC) - timedelta(hours=hours)
    return last_word(session, tender_id) or datetime.now(UTC) - timedelta(days=1)


def precheck(session: Session, home: Path, tender_id: str, staff_id: str) -> str:
    """What Quantix's checks find now in this person's work that waits for the Tender Manager."""
    mine = [p for p in reviews.pending(session, tender_id) if p.producer == staff_id]
    if not mine:
        return "Nothing of yours is waiting for the Tender Manager."
    names = _names(session, tender_id)
    parts = []
    for p in mine:
        found = reviews.findings(session, home, tender_id, p)
        text = reviews.findings_text(found).strip() or "Quantix's checks find nothing in it."
        parts.append(f"{reviews.describe(session, p, names)}\n{text}")
    return "\n\n".join(parts)
