import uuid
from datetime import UTC, date, datetime

from sqlalchemy import Date, String, select
from sqlalchemy.orm import Mapped, Session, mapped_column

from quantix.core.db import Base, UTCDateTime

LOCAL_OWNER = "local"


class Tender(Base):
    __tablename__ = "tenders"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=lambda: uuid.uuid4().hex)
    owner_id: Mapped[str] = mapped_column(String(64), default=LOCAL_OWNER)
    name: Mapped[str] = mapped_column(String(200))
    due_date: Mapped[date | None] = mapped_column(Date)
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=lambda: datetime.now(UTC))


def create_tender(session: Session, name: str, due_date: date | None) -> Tender:
    tender = Tender(name=name, due_date=due_date)
    session.add(tender)
    session.commit()
    return tender


def list_tenders(session: Session) -> list[Tender]:
    query = select(Tender).where(Tender.owner_id == LOCAL_OWNER).order_by(Tender.created_at.desc())
    return list(session.scalars(query))


def get_tender(session: Session, tender_id: str) -> Tender | None:
    tender = session.get(Tender, tender_id)
    return tender if tender and tender.owner_id == LOCAL_OWNER else None
