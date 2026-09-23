"""The firm's directory, and a tender's packages, enquiries and quotes. Levelling is computed, never stored."""

import uuid
from datetime import UTC, datetime
from typing import Any

from sqlalchemy import JSON, ForeignKey, String, Text
from sqlalchemy.orm import Mapped, mapped_column

from quantix.core.db import Base, UTCDateTime
from quantix.tenders import LOCAL_OWNER


def _id() -> str:
    return uuid.uuid4().hex


def _now() -> datetime:
    return datetime.now(UTC)


class Company(Base):
    """A subcontractor or supplier the firm works with, kept across tenders."""

    __tablename__ = "companies"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    owner_id: Mapped[str] = mapped_column(String(64), default=LOCAL_OWNER)
    name: Mapped[str] = mapped_column(String(200))
    kind: Mapped[str] = mapped_column(String(20))  # subcontractor | supplier
    trades: Mapped[str] = mapped_column(Text)
    email: Mapped[str | None] = mapped_column(String(200))
    phone: Mapped[str | None] = mapped_column(String(60))
    notes: Mapped[str | None] = mapped_column(Text)
    added_by: Mapped[str] = mapped_column(String(32))  # "engineer" or the staff member who added it
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=_now)


class Package(Base):
    """BOQ items let to one subcontractor (kind "subcontract") or bought from one supplier (kind "supply")."""

    __tablename__ = "packages"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    tender_id: Mapped[str] = mapped_column(ForeignKey("tenders.id", ondelete="CASCADE"))
    name: Mapped[str] = mapped_column(String(200))
    kind: Mapped[str] = mapped_column(String(20))
    items: Mapped[list[str]] = mapped_column(JSON)  # BOQ item ids
    recommended_quote_id: Mapped[str | None] = mapped_column(String(32))
    recommendation: Mapped[str | None] = mapped_column(Text)
    recommended_by: Mapped[str | None] = mapped_column(String(32))
    selected_quote_id: Mapped[str | None] = mapped_column(String(32))
    created_by: Mapped[str] = mapped_column(String(32))
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=_now)
    decided_at: Mapped[datetime | None] = mapped_column(UTCDateTime)


class Enquiry(Base):
    """An enquiry email drafted by the office. The engineer sends it from their own mail program."""

    __tablename__ = "enquiries"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    package_id: Mapped[str] = mapped_column(ForeignKey("packages.id", ondelete="CASCADE"))
    company_id: Mapped[str] = mapped_column(ForeignKey("companies.id"))
    subject: Mapped[str] = mapped_column(String(300))
    body: Mapped[str] = mapped_column(Text)
    status: Mapped[str] = mapped_column(String(20), default="draft")  # draft | sent
    created_by: Mapped[str] = mapped_column(String(32))
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=_now)
    sent_at: Mapped[datetime | None] = mapped_column(UTCDateTime)


class Quote(Base):
    """A company's price for a package, as read from their reply in the tender's documents."""

    __tablename__ = "quotes"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    package_id: Mapped[str] = mapped_column(ForeignKey("packages.id", ondelete="CASCADE"))
    company_id: Mapped[str] = mapped_column(ForeignKey("companies.id"))
    document_id: Mapped[str] = mapped_column(ForeignKey("documents.id"))
    # [{boq_item_id, rate, page, quote}]: each quoted rate with the line it was read from
    lines: Mapped[list[dict[str, Any]]] = mapped_column(JSON)
    # [{description, amount, page, quote}]: what the quote leaves out, priced back in to compare like with like
    exclusions: Mapped[list[dict[str, Any]]] = mapped_column(JSON)
    proposed_by: Mapped[str] = mapped_column(String(32))
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=_now)
