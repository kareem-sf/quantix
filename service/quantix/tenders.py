import uuid
from datetime import UTC, date, datetime

from sqlalchemy import Boolean, Date, Integer, String, Text, select
from sqlalchemy.orm import Mapped, Session, mapped_column

from quantix.core.db import Base, UTCDateTime

LOCAL_OWNER = "local"
# Where a due date comes from: the engineer's own date (entered by them, or by the Tender Manager on their word), or
# a page of the tender documents that states it
ENGINEER_DATE, DOCUMENT_DATE = "engineer", "document"


class Tender(Base):
    __tablename__ = "tenders"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=lambda: uuid.uuid4().hex)
    owner_id: Mapped[str] = mapped_column(String(64), default=LOCAL_OWNER)
    name: Mapped[str] = mapped_column(String(200))
    due_date: Mapped[date | None] = mapped_column(Date)
    due_date_basis: Mapped[str | None] = mapped_column(String(20))  # ENGINEER_DATE or DOCUMENT_DATE
    due_date_by: Mapped[str | None] = mapped_column(String(32))  # "engineer", or the staff member who entered it
    due_date_at: Mapped[datetime | None] = mapped_column(UTCDateTime)
    due_date_document_id: Mapped[str | None] = mapped_column(String(32))
    due_date_page: Mapped[int | None] = mapped_column(Integer)
    due_date_quote: Mapped[str | None] = mapped_column(Text)
    outcome: Mapped[str] = mapped_column(String(20), default="open")  # open | submitted | won | lost
    outcome_at: Mapped[datetime | None] = mapped_column(UTCDateTime)  # when the outcome was last set
    archived_at: Mapped[datetime | None] = mapped_column(UTCDateTime)  # put away: out of the sidebar and the Desk
    # stopped by the engineer or paused by the office, until the engineer writes; kept across a restart
    office_paused: Mapped[bool] = mapped_column(Boolean, default=False)
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=lambda: datetime.now(UTC))


def set_due_date(
    tender: Tender,
    due: date | None,
    by: str,
    basis: str = ENGINEER_DATE,
    document_id: str | None = None,
    page: int | None = None,
    quote: str | None = None,
) -> None:
    """The due date and where it comes from, kept together so the date never shows without its source."""
    tender.due_date = due
    tender.due_date_basis, tender.due_date_by = (basis, by) if due else (None, None)
    tender.due_date_at = datetime.now(UTC) if due else None
    source = (document_id, page, quote) if due and basis == DOCUMENT_DATE else (None, None, None)
    tender.due_date_document_id, tender.due_date_page, tender.due_date_quote = source


def create_tender(session: Session, name: str, due_date: date | None) -> Tender:
    tender = Tender(name=name)
    set_due_date(tender, due_date, "engineer")
    session.add(tender)
    session.commit()
    return tender


def list_tenders(session: Session) -> list[Tender]:
    query = select(Tender).where(Tender.owner_id == LOCAL_OWNER).order_by(Tender.created_at.desc())
    return list(session.scalars(query))


def get_tender(session: Session, tender_id: str) -> Tender | None:
    tender = session.get(Tender, tender_id)
    return tender if tender and tender.owner_id == LOCAL_OWNER else None
