"""What the tender asks the bidder to submit, the office's drafts, and where prices go in the client's BOQ."""

import uuid
from datetime import UTC, datetime

from sqlalchemy import ForeignKey, Integer, String, Text
from sqlalchemy.orm import Mapped, mapped_column

from quantix.core.db import Base, UTCDateTime
from quantix.estimate.models import Reviewed


def _id() -> str:
    return uuid.uuid4().hex


def _now() -> datetime:
    return datetime.now(UTC)


class Requirement(Base):
    """One thing the tender requires in the submission, with the clause that requires it."""

    __tablename__ = "requirements"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    tender_id: Mapped[str] = mapped_column(ForeignKey("tenders.id", ondelete="CASCADE"))
    section: Mapped[str] = mapped_column(String(100))
    title: Mapped[str] = mapped_column(String(300))
    # The source clause; empty only for requirements the engineer added themselves
    document_id: Mapped[str | None] = mapped_column(ForeignKey("documents.id"))
    page: Mapped[int | None] = mapped_column(Integer)
    quote: Mapped[str | None] = mapped_column(Text)
    added_by: Mapped[str] = mapped_column(String(32))
    ready_note: Mapped[str | None] = mapped_column(Text)  # set when the engineer marks it ready themselves
    file_name: Mapped[str | None] = mapped_column(String(300))  # a file the engineer attached
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=_now)


class Draft(Reviewed, Base):
    """A document the office drafted for a requirement, for the engineer to review."""

    __tablename__ = "drafts"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    tender_id: Mapped[str] = mapped_column(ForeignKey("tenders.id", ondelete="CASCADE"))
    requirement_id: Mapped[str] = mapped_column(ForeignKey("requirements.id", ondelete="CASCADE"))
    title: Mapped[str] = mapped_column(String(300))
    body: Mapped[str] = mapped_column(Text)


class PricingColumns(Base):
    """Where rates and amounts go in one sheet of the client's BOQ workbook, as the office read its header."""

    __tablename__ = "pricing_columns"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    tender_id: Mapped[str] = mapped_column(ForeignKey("tenders.id", ondelete="CASCADE"))
    document_id: Mapped[str] = mapped_column(ForeignKey("documents.id"))
    sheet: Mapped[int] = mapped_column(Integer)
    rate_column: Mapped[str] = mapped_column(String(3))
    amount_column: Mapped[str] = mapped_column(String(3))
    quote: Mapped[str] = mapped_column(Text)
    proposed_by: Mapped[str] = mapped_column(String(32))
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=_now)
