"""The client's bill of quantities, as the office read it, and the tender facts pricing depends on."""

import uuid
from datetime import UTC, datetime
from decimal import Decimal

from sqlalchemy import ForeignKey, Integer, Numeric, String, Text
from sqlalchemy.orm import Mapped, mapped_column

from quantix.core.db import Base, UTCDateTime

# proposed → approved | rejected. In a fully autonomous office: office_approved. A fact replaced by a newer
# approved one becomes "replaced".
APPROVED = ("approved", "office_approved")
FACT_KINDS = {
    "method_of_measurement": "Method of measurement",
    "currency": "Currency",
    "vat": "VAT",
}


def _id() -> str:
    return uuid.uuid4().hex


def _now() -> datetime:
    return datetime.now(UTC)


class Sourced:
    """A record the office proposed, with the page it came from and the engineer's decision."""

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    tender_id: Mapped[str] = mapped_column(ForeignKey("tenders.id", ondelete="CASCADE"))
    document_id: Mapped[str] = mapped_column(ForeignKey("documents.id"))
    page: Mapped[int] = mapped_column(Integer)
    quote: Mapped[str] = mapped_column(Text)
    status: Mapped[str] = mapped_column(String(20), default="proposed")
    proposed_by: Mapped[str] = mapped_column(String(32))
    reason: Mapped[str | None] = mapped_column(Text)
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=_now)
    decided_at: Mapped[datetime | None] = mapped_column(UTCDateTime)


class BoqItem(Sourced, Base):
    __tablename__ = "boq_items"

    section: Mapped[str | None] = mapped_column(String(300))
    item: Mapped[str] = mapped_column(String(40))
    description: Mapped[str] = mapped_column(Text)
    unit: Mapped[str] = mapped_column(String(40))
    quantity: Mapped[Decimal | None] = mapped_column(Numeric(18, 4))
    position: Mapped[int] = mapped_column(Integer)


class Fact(Sourced, Base):
    __tablename__ = "facts"

    kind: Mapped[str] = mapped_column(String(40))
    value: Mapped[str] = mapped_column(Text)
