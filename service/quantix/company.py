"""Company knowledge every new team reads: the firm's rules, and what it priced on earlier tenders."""

import uuid
from dataclasses import dataclass
from datetime import UTC, date, datetime
from decimal import Decimal
from io import BytesIO
from pathlib import Path

from PIL import Image, UnidentifiedImageError
from sqlalchemy import String, Text, select
from sqlalchemy.orm import Mapped, Session, mapped_column

from quantix import settings
from quantix.boq.models import APPROVED, BoqItem
from quantix.core.db import Base, UTCDateTime
from quantix.documents import meaning
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
    lines: list[dict] | None = None  # the build-up, when it was built up
    note: str = ""  # its assumptions


def past_rates(session: Session, tender_id: str, words: str, limit: int = 20) -> list[PastRate]:
    """Approved rates from the firm's other tenders for items whose description has every word, newest first, then
    for items close to the words in meaning."""
    terms = words.lower().split()
    query = (
        select(BoqItem, Tender)
        .join(Tender, Tender.id == BoqItem.tender_id)
        .where(Tender.owner_id == LOCAL_OWNER, Tender.id != tender_id, BoqItem.status.in_(APPROVED))
        .order_by(Tender.created_at.desc())
    )
    rows = list(session.execute(query).tuples())
    matched = [all(t in item.description.lower() for t in terms) for item, _ in rows]
    others = [row for row, m in zip(rows, matched, strict=True) if not m]
    close = meaning.closest(session, words, others, lambda row: row[0].description)
    found = []
    for item, tender in [row for row, m in zip(rows, matched, strict=True) if m] + close:
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
                rate.lines,
                rate.note,
            )
        )
        if len(found) == limit:
            break
    return found


@dataclass
class Profile:
    """Who the firm is, for the title block and letterhead of every document Quantix writes."""

    name: str = ""
    address: str = ""
    cr_number: str = ""  # commercial registration
    vat_number: str = ""
    logo: Path | None = None


def _logo_path(home: Path) -> Path:
    return home / "company" / "logo.png"


def profile(home: Path) -> Profile:
    stored = settings.load(home).get("company") or {}
    logo = _logo_path(home)
    return Profile(**{k: stored.get(k, "") for k in ("name", "address", "cr_number", "vat_number")},
                   logo=logo if logo.exists() else None)  # fmt: skip


def save_profile(home: Path, name: str, address: str, cr_number: str, vat_number: str) -> Profile:
    fields = {"name": name, "address": address, "cr_number": cr_number, "vat_number": vat_number}
    settings.save(home, company={k: v.strip() for k, v in fields.items()})
    return profile(home)


def save_logo(home: Path, content: bytes) -> None:
    """Keep the firm's logo as a PNG no wider than 800 pixels, whatever image the engineer chose."""
    try:
        image = Image.open(BytesIO(content))
        image.load()
    except (UnidentifiedImageError, OSError) as error:
        raise ValueError("That file isn't an image Quantix can read. Choose a PNG or JPEG.") from error
    image.thumbnail((800, 800))
    path = _logo_path(home)
    path.parent.mkdir(parents=True, exist_ok=True)
    image.convert("RGBA").save(path, "PNG")


def remove_logo(home: Path) -> None:
    _logo_path(home).unlink(missing_ok=True)
