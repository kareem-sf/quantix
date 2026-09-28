"""The Tender Manager's reasons for accepting what Quantix's checks warned about, and the lessons he draws from work
that needed correcting."""

import uuid
from datetime import UTC, datetime
from typing import Any

from sqlalchemy import JSON, ForeignKey, String, Text
from sqlalchemy.orm import Mapped, mapped_column

from quantix.core.db import Base, UTCDateTime
from quantix.core.review import Reviewed


def _id() -> str:
    return uuid.uuid4().hex


def _now() -> datetime:
    return datetime.now(UTC)


class Acceptance(Base):
    """A warning the Manager accepted, and why. The key names the check and the records, so a changed record is
    checked again."""

    __tablename__ = "acceptances"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    tender_id: Mapped[str] = mapped_column(ForeignKey("tenders.id", ondelete="CASCADE"))
    key: Mapped[str] = mapped_column(String(300))
    reason: Mapped[str] = mapped_column(Text)
    accepted_by: Mapped[str] = mapped_column(String(32))
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=_now)


class Lesson(Base):
    """What the Tender Manager learned from work that needed correcting: a rule the whole office follows on this
    tender from then on. The engineer can keep it as a company rule for later tenders, or drop it."""

    __tablename__ = "lessons"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    tender_id: Mapped[str] = mapped_column(ForeignKey("tenders.id", ondelete="CASCADE"))
    text: Mapped[str] = mapped_column(Text)
    topic: Mapped[str] = mapped_column(String(100))  # the company rule's topic, if the engineer keeps it
    source: Mapped[str] = mapped_column(String(300))  # the work it came from, e.g. "the rate for BOQ item 3.1"
    learned_by: Mapped[str] = mapped_column(String(32))
    status: Mapped[str] = mapped_column(String(20), default="tender")  # tender | kept | dropped
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=_now)


class TenderQuery(Reviewed, Base):
    """Something the office found that the client has to answer or the bid has to allow for: work drawn or specified
    but not in the BOQ, documents that disagree, or an error in the BOQ. The office drafts it; the engineer decides
    whether it goes to the client."""

    __tablename__ = "tender_queries"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    tender_id: Mapped[str] = mapped_column(ForeignKey("tenders.id", ondelete="CASCADE"))
    kind: Mapped[str] = mapped_column(String(20))  # missing | conflict | boq | clarification
    title: Mapped[str] = mapped_column(String(300))
    detail: Mapped[str] = mapped_column(Text)  # what was found, for the engineer
    wording: Mapped[str] = mapped_column(Text)  # the query as the client would read it
    governs: Mapped[str | None] = mapped_column(Text)  # which document governs, and the clause that says so
    # where it shows: [{document_id, page, quote?, objects?: [keys]}]
    sources: Mapped[list[dict[str, Any]]] = mapped_column(JSON)
    boq_item_id: Mapped[str | None] = mapped_column(ForeignKey("boq_items.id"))
    measurement_ids: Mapped[list[str]] = mapped_column(JSON)  # the takeoff that gives its quantities
