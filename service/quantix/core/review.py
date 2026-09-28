"""How a record moves through the office: staff propose it, the Tender Manager reviews it, the engineer decides."""

from datetime import UTC, datetime

from sqlalchemy import String, Text
from sqlalchemy.orm import Mapped, mapped_column

from quantix.core.db import UTCDateTime

PROPOSED = "proposed"  # with the Tender Manager for review
REVIEWED = "reviewed"  # the Manager accepted it; waiting for the engineer
APPROVED = ("approved", "office_approved")  # by the engineer, or by a fully autonomous office after review
UNDECIDED = (PROPOSED, REVIEWED)
LIVE = (*UNDECIDED, *APPROVED)  # rejected, withdrawn and replaced records no longer count


def _now() -> datetime:
    return datetime.now(UTC)


class Reviewed:
    """A record the office proposed, the Tender Manager's review of it and the engineer's decision."""

    status: Mapped[str] = mapped_column(String(20), default=PROPOSED)
    proposed_by: Mapped[str] = mapped_column(String(32))  # a staff id, or "engineer"
    reason: Mapped[str | None] = mapped_column(Text)  # why it was sent back
    reviewed_by: Mapped[str | None] = mapped_column(String(32))  # the Tender Manager
    review_note: Mapped[str | None] = mapped_column(Text)  # what he checked, or the correction he asked for
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=_now)
    reviewed_at: Mapped[datetime | None] = mapped_column(UTCDateTime)
    decided_at: Mapped[datetime | None] = mapped_column(UTCDateTime)
