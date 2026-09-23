"""Company knowledge every new team reads: the firm's rules, and what it priced on earlier tenders."""

import uuid
from dataclasses import dataclass
from datetime import UTC, date, datetime
from decimal import Decimal

from sqlalchemy import String, Text, select
from sqlalchemy.orm import Mapped, Session, mapped_column

from quantix.boq.models import APPROVED, BoqItem
from quantix.core.db import Base, UTCDateTime
from quantix.estimate import records as estimate
from quantix.tenders import LOCAL_OWNER, Tender


class CompanyRule(Base):
    """A house rule or preference: standard markups, exclusions, qualifications, house style."""

    __tablename__ = "company_rules"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=lambda: uuid.uuid4().hex)
    owner_id: Mapped[str] = mapped_column(String(64), default=LOCAL_OWNER)
    topic: Mapped[str] = mapped_column(String(100))
    text: Mapped[str] = mapped_column(Text)
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=lambda: datetime.now(UTC))


def rules(session: Session) -> list[CompanyRule]:
    query = select(CompanyRule).where(CompanyRule.owner_id == LOCAL_OWNER)
    return list(session.scalars(query.order_by(CompanyRule.topic, CompanyRule.created_at)))


@dataclass
class PastRate:
    tender: str
    outcome: str
    dated: date
    item: str
    description: str
    unit: str
    rate: Decimal
    basis: str


def past_rates(session: Session, tender_id: str, words: str, limit: int = 20) -> list[PastRate]:
    """Approved rates from the firm's other tenders for items whose description has every word, newest first."""
    terms = words.lower().split()
    query = (
        select(BoqItem, Tender)
        .join(Tender, Tender.id == BoqItem.tender_id)
        .where(Tender.owner_id == LOCAL_OWNER, Tender.id != tender_id, BoqItem.status.in_(APPROVED))
        .order_by(Tender.created_at.desc())
    )
    found = []
    for item, tender in session.execute(query):
        if not all(t in item.description.lower() for t in terms):
            continue
        rate = estimate.current_rate(session, item.id)
        if rate is None or rate.status not in APPROVED:
            continue
        dated = (rate.decided_at or rate.created_at).date()
        found.append(
            PastRate(
                tender.name,
                tender.outcome,
                dated,
                item.item,
                item.description,
                item.unit,
                estimate.rate_of(rate),
                rate.basis,
            )
        )
        if len(found) == limit:
            break
    return found
