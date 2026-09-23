"""The rate library, rates per BOQ item and markups. Rates, amounts and totals are computed, never stored."""

import uuid
from datetime import UTC, date, datetime
from decimal import Decimal
from typing import Any

from sqlalchemy import JSON, Date, ForeignKey, Integer, Numeric, String, Text
from sqlalchemy.orm import Mapped, mapped_column

from quantix.core.db import Base, UTCDateTime
from quantix.tenders import LOCAL_OWNER


def _id() -> str:
    return uuid.uuid4().hex


def _now() -> datetime:
    return datetime.now(UTC)


class Reviewed:
    # proposed | approved | office_approved | rejected | replaced
    status: Mapped[str] = mapped_column(String(20), default="proposed")
    proposed_by: Mapped[str] = mapped_column(String(32))  # a staff id, or "engineer"
    reason: Mapped[str | None] = mapped_column(Text)
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=_now)
    decided_at: Mapped[datetime | None] = mapped_column(UTCDateTime)


class LibraryResource(Base):
    """A rate the firm reuses across tenders: labour, plant, material, subcontract or a whole unit rate."""

    __tablename__ = "library"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    owner_id: Mapped[str] = mapped_column(String(64), default=LOCAL_OWNER)
    kind: Mapped[str] = mapped_column(String(20))
    name: Mapped[str] = mapped_column(String(300))
    unit: Mapped[str] = mapped_column(String(40))
    rate: Mapped[Decimal] = mapped_column(Numeric(18, 4))
    currency: Mapped[str] = mapped_column(String(10))
    source: Mapped[str] = mapped_column(Text)
    dated: Mapped[date] = mapped_column(Date)
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=_now)


class Rate(Reviewed, Base):
    """How one BOQ item is priced: a unit rate, or a build-up of resource lines."""

    __tablename__ = "rates"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    tender_id: Mapped[str] = mapped_column(ForeignKey("tenders.id", ondelete="CASCADE"))
    boq_item_id: Mapped[str] = mapped_column(ForeignKey("boq_items.id"))
    basis: Mapped[str] = mapped_column(String(20))  # quote | library | estimate
    unit_rate: Mapped[Decimal | None] = mapped_column(Numeric(18, 4))
    # A build-up: [{kind, resource, quantity, unit, rate, wastage}] per one unit of the BOQ item
    lines: Mapped[list[dict[str, Any]] | None] = mapped_column(JSON)
    document_id: Mapped[str | None] = mapped_column(ForeignKey("documents.id"))
    page: Mapped[int | None] = mapped_column(Integer)
    quote: Mapped[str | None] = mapped_column(Text)
    library_id: Mapped[str | None] = mapped_column(ForeignKey("library.id"))
    note: Mapped[str] = mapped_column(Text)


class Markups(Reviewed, Base):
    __tablename__ = "markups"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    tender_id: Mapped[str] = mapped_column(ForeignKey("tenders.id", ondelete="CASCADE"))
    preliminaries: Mapped[Decimal] = mapped_column(Numeric(8, 4))  # fractions: 0.08 is 8%
    overheads: Mapped[Decimal] = mapped_column(Numeric(8, 4))
    profit: Mapped[Decimal] = mapped_column(Numeric(8, 4))
    adjustment: Mapped[Decimal] = mapped_column(Numeric(18, 2))  # a lump sum added to (or taken off) the price
    note: Mapped[str] = mapped_column(Text)
