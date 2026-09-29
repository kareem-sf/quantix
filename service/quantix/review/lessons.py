"""What the office learned from work that needed correcting. The Tender Manager states the lesson with his review;
from then on the whole office on the tender follows it. The engineer can keep it as a company rule, so every later
team follows it too, or drop it."""

from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix import company
from quantix.office.records import same_subject
from quantix.review.models import Lesson

TENDER, KEPT, DROPPED = "tender", "kept", "dropped"
TOPICS = {  # the company rule's topic, by the kind of work the lesson came from
    "boq": "BOQ",
    "fact": "Tender facts",
    "scale": "Takeoff",
    "measurement": "Takeoff",
    "rate": "Rates",
    "markups": "Markups",
    "draft": "Submission",
    "checklist": "Submission",
    "enquiry": "Subcontract",
    "recommendation": "Subcontract",
}


def lessons(session: Session, tender_id: str, statuses: tuple[str, ...] = (TENDER, KEPT, DROPPED)) -> list[Lesson]:
    query = select(Lesson).where(Lesson.tender_id == tender_id, Lesson.status.in_(statuses))
    return list(session.scalars(query.order_by(Lesson.created_at)))


def suggested(session: Session) -> list[Lesson]:
    """Lessons from every tender that the engineer hasn't kept or dropped yet, newest first: company rules to be."""
    return list(session.scalars(select(Lesson).where(Lesson.status == TENDER).order_by(Lesson.created_at.desc())))


def current(session: Session, tender_id: str) -> list[Lesson]:
    """What the office follows on this tender. A kept lesson is a company rule by then, which every team reads."""
    return lessons(session, tender_id, (TENDER,))


def learn(session: Session, tender_id: str, manager_id: str, kind: str, source: str, text: str) -> str:
    """Record the Manager's lesson, unless the office already follows it or the engineer dropped one like it.
    Returns what happened, for his review report."""
    text = " ".join(text.split())
    if len(text.split()) < 6:
        return f"Lesson not kept: “{text}” is too short to follow. Say it as a rule the whole office can apply."
    for earlier in lessons(session, tender_id):
        if same_subject(text, earlier.text):
            why = "the engineer dropped a lesson like it" if earlier.status == DROPPED else "the office already follows"
            return f"Lesson not kept: {why}: “{earlier.text}”"
    for rule in company.rules(session):
        if same_subject(text, rule.text):
            return f"Lesson not kept: it is already a company rule: “{rule.text}”"
    session.add(Lesson(tender_id=tender_id, text=text, topic=TOPICS[kind], source=source[:300], learned_by=manager_id))
    session.flush()
    return f"Lesson kept: the whole office follows “{text}” from now on."


def decide(session: Session, lesson: Lesson, status: str) -> None:
    """The engineer keeps the lesson as a company rule for later tenders, or drops it."""
    if lesson.status != TENDER:
        raise ValueError("This lesson has already been kept or dropped.")
    if status == KEPT:
        session.add(company.CompanyRule(topic=lesson.topic, text=lesson.text))
    lesson.status = status
