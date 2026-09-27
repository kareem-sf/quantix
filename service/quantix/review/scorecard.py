"""How each AI model has done in the office, from the turn records: how its turns ended, how often Quantix sent a
tool call back, how much of the work filed on its turns was accepted, and the tokens that took. For choosing the
office's AI in Settings; never shown in the work areas."""

from bisect import bisect_right
from dataclasses import dataclass

from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix.core.review import APPROVED, REVIEWED
from quantix.office.models import ENGINEER, TurnRecord
from quantix.review.records import REVIEWED_KINDS


@dataclass
class Score:
    model: str
    turns: int = 0
    finished: int = 0  # turns that ended done, rather than cut short or failed
    calls: int = 0
    calls_sent_back: int = 0  # tool calls Quantix sent back with a reason
    accepted: int = 0  # records filed on its turns that the Tender Manager or the engineer accepted
    sent_back: int = 0  # records filed on its turns that were sent back
    tokens: int = 0


def scores(session: Session) -> list[Score]:
    """Every model the office has worked with, the most recently used first."""
    found: dict[str, Score] = {}
    turns: dict[str, list[TurnRecord]] = {}  # each person's turns, oldest first
    for turn in session.scalars(select(TurnRecord).order_by(TurnRecord.started_at.desc())):
        score = found.setdefault(turn.model, Score(turn.model))
        score.turns += 1
        score.finished += turn.ended == "done"
        score.calls += len(turn.calls)
        score.calls_sent_back += sum(bool(call["sent_back"]) for call in turn.calls)
        score.tokens += turn.input_tokens + turn.output_tokens
        turns.setdefault(turn.staff_id, []).insert(0, turn)
    starts = {person: [t.started_at for t in theirs] for person, theirs in turns.items()}
    for model, _ in REVIEWED_KINDS.values():
        query = select(model.proposed_by, model.created_at, model.status).where(model.proposed_by != ENGINEER)
        for person, made, status in session.execute(query):
            at = bisect_right(starts.get(person, []), made) - 1
            if at < 0:  # filed before its turns were recorded
                continue
            score = found[turns[person][at].model]  # the turn it was filed in
            score.accepted += status in (REVIEWED, *APPROVED)
            score.sent_back += status == "rejected"
    return list(found.values())
