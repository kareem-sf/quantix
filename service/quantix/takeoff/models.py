"""Sheet scales and measurements. Quantities are never stored: they are computed from geometry and scale."""

import uuid
from datetime import UTC, datetime
from decimal import Decimal

from sqlalchemy import JSON, Float, ForeignKey, Integer, Numeric, String, Text
from sqlalchemy.orm import Mapped, mapped_column

from quantix.core.db import Base, UTCDateTime


def _id() -> str:
    return uuid.uuid4().hex


def _now() -> datetime:
    return datetime.now(UTC)


class Reviewed:
    status: Mapped[str] = mapped_column(
        String(20), default="proposed"
    )  # proposed | approved | office_approved | rejected | replaced
    proposed_by: Mapped[str] = mapped_column(String(32))  # a staff id, or "engineer"
    reason: Mapped[str | None] = mapped_column(Text)
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=_now)
    decided_at: Mapped[datetime | None] = mapped_column(UTCDateTime)


class Scale(Reviewed, Base):
    """A sheet's scale, calibrated on a dimension printed on the drawing."""

    __tablename__ = "scales"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    tender_id: Mapped[str] = mapped_column(ForeignKey("tenders.id", ondelete="CASCADE"))
    document_id: Mapped[str] = mapped_column(ForeignKey("documents.id"))
    page: Mapped[int] = mapped_column(Integer)
    metres_per_point: Mapped[float] = mapped_column(Float)
    line: Mapped[list[list[float]]] = mapped_column(JSON)  # the calibrated line, in page points from the top left
    length_m: Mapped[float] = mapped_column(Float)
    dimension: Mapped[str] = mapped_column(String(100))  # the dimension text on the drawing, e.g. "40.00"


class Measurement(Reviewed, Base):
    __tablename__ = "measurements"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    tender_id: Mapped[str] = mapped_column(ForeignKey("tenders.id", ondelete="CASCADE"))
    document_id: Mapped[str] = mapped_column(ForeignKey("documents.id"))
    page: Mapped[int] = mapped_column(Integer)
    kind: Mapped[str] = mapped_column(String(10))  # length | area | count
    label: Mapped[str] = mapped_column(String(300))
    points: Mapped[list[list[float]]] = mapped_column(JSON)  # page points from the top left
    multiplier: Mapped[Decimal | None] = mapped_column(Numeric(12, 4))  # a height, depth or thickness in metres
    unit: Mapped[str] = mapped_column(String(10))  # m | m2 | m3 | nr
    boq_item_id: Mapped[str | None] = mapped_column(ForeignKey("boq_items.id"))
