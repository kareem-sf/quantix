"""The Tender Manager's reasons for accepting what Quantix's checks warned about."""

import uuid
from datetime import UTC, datetime

from sqlalchemy import ForeignKey, String, Text
from sqlalchemy.orm import Mapped, mapped_column

from quantix.core.db import Base, UTCDateTime


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
