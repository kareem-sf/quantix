"""Pricing: rates with their basis, markups, and the totals. Quantix does every sum."""

import re
from dataclasses import dataclass, field
from datetime import UTC, date, datetime
from decimal import ROUND_HALF_UP, Decimal
from typing import Literal

from pydantic import BaseModel, Field
from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix.boq import records as boq
from quantix.boq.models import APPROVED, BoqItem, Fact
from quantix.documents.evidence import check_quote, numbers_in
from quantix.estimate.models import LibraryResource, Markups, Rate
from quantix.office import records as office
from quantix.office.models import ENGINEER
from quantix.tenders import LOCAL_OWNER

LIVE = ("proposed", *APPROVED)
CENT = Decimal("0.01")
KINDS = ("labour", "plant", "material", "subcontract")


def money(value: Decimal) -> Decimal:
    return value.quantize(CENT, rounding=ROUND_HALF_UP)


class LineIn(BaseModel):
    """One resource in a build-up, per one unit of the BOQ item."""

    kind: Literal["labour", "plant", "material", "subcontract"]
    resource: str = Field(description="e.g. Steel fixer gang, Rebar B500B cut and bent")
    quantity: Decimal = Field(description="How much of the resource one BOQ unit takes, e.g. 16 (hours) or 1 (t)")
    unit: str = Field(description="The resource's unit, e.g. hr, t, m3")
    rate: Decimal = Field(description="The resource's rate per its unit")
    wastage: Decimal = Field(default=Decimal(0), description="Wastage as a fraction, e.g. 0.05 for 5%")


def line_cost(line: dict) -> Decimal:
    return money(
        Decimal(str(line["quantity"])) * (1 + Decimal(str(line.get("wastage", 0)))) * Decimal(str(line["rate"]))
    )


def rate_of(rate: Rate) -> Decimal:
    """The item's rate: the unit rate, or the sum of the build-up's line costs."""
    if rate.lines:
        return sum((line_cost(line) for line in rate.lines), Decimal(0))
    return money(rate.unit_rate)


def current_rate(session: Session, item_id: str) -> Rate | None:
    """The item's rate: the newest approved one, or else the newest proposed one."""
    query = select(Rate).where(Rate.boq_item_id == item_id, Rate.status.in_(LIVE)).order_by(Rate.created_at.desc())
    rates = list(session.scalars(query))
    return next((r for r in rates if r.status in APPROVED), rates[0] if rates else None)


def propose_rate(
    session: Session,
    tender_id: str,
    by: str,
    item_number: str,
    basis: str,
    note: str,
    unit_rate: Decimal | None = None,
    lines: list[LineIn] | None = None,
    document_id: str | None = None,
    page: int | None = None,
    quote: str | None = None,
    library_id: str | None = None,
    status: str = "proposed",
) -> Rate:
    item = session.scalars(
        select(BoqItem).where(
            BoqItem.tender_id == tender_id, BoqItem.item == item_number.strip(), BoqItem.status.in_(boq.ACTIVE)
        )
    ).first()
    if item is None:
        raise ValueError(f"There is no BOQ item {item_number}. Use list_boq to see the items.")
    if (unit_rate is None) == (not lines):
        raise ValueError("Give either a unit rate or a build-up of lines, not both.")
    if basis == "quote":
        if not (document_id and page and quote):
            raise ValueError("A quoted rate needs the document, page and the quoted line.")
        check_quote(session, tender_id, document_id, page, quote)
        if unit_rate is not None and unit_rate not in numbers_in(quote):
            raise ValueError(f"The rate {unit_rate} is not in the quoted line.")
    elif basis == "library":
        if library_id is None or session.get(LibraryResource, library_id) is None:
            raise ValueError("That library entry doesn't exist. Use search_library to find it.")
    elif basis == "estimate":
        if len(note.strip()) < 20:
            raise ValueError("An estimated rate needs its reasoning in the note: outputs, prices and assumptions.")
    else:
        raise ValueError("The basis is quote, library or estimate.")
    rate = Rate(
        tender_id=tender_id,
        boq_item_id=item.id,
        basis=basis,
        unit_rate=unit_rate,
        lines=[line.model_dump(mode="json") for line in lines] if lines else None,
        document_id=document_id,
        page=page,
        quote=quote,
        library_id=library_id,
        note=note.strip(),
        proposed_by=by,
        status=status,
    )
    session.add(rate)
    session.flush()
    if status in APPROVED:
        _replace_older(session, rate)
    return rate


def _replace_older(session: Session, record: Rate | Markups) -> None:
    """One approved rate per item, and one approved set of markups per tender: approving replaces the others."""
    if isinstance(record, Rate):
        query = select(Rate).where(Rate.boq_item_id == record.boq_item_id, Rate.id != record.id)
    else:
        query = select(Markups).where(Markups.tender_id == record.tender_id, Markups.id != record.id)
    for older in session.scalars(query.where(type(record).status.in_(LIVE))):
        older.status = "replaced"


def decide(session: Session, record: Rate | Markups, approve: bool, reason: str | None = None) -> None:
    record.status = "approved" if approve else "rejected"
    record.reason = reason
    record.decided_at = datetime.now(UTC)
    if approve:
        _replace_older(session, record)
    if not approve and record.proposed_by != ENGINEER:
        if isinstance(record, Rate):
            what = f"the rate for BOQ item {session.get(BoqItem, record.boq_item_id).item}"
        else:
            what = "the markups"
        office.post(
            session,
            record.tender_id,
            ENGINEER,
            record.proposed_by,
            f"I rejected {what}" + (f": {reason}" if reason else "."),
        )


def save_to_library(session: Session, rate: Rate, currency: str) -> int:
    """Keep an approved rate for later tenders: its build-up resources, or the unit rate itself."""
    item = session.get(BoqItem, rate.boq_item_id)
    source = f"Approved for {item.item} {item.description[:80]}"
    entries = (
        [(line["kind"], line["resource"], line["unit"], Decimal(str(line["rate"]))) for line in rate.lines]
        if rate.lines
        else [("unit_rate", item.description[:300], item.unit, rate.unit_rate)]
    )
    for kind, name, unit, value in entries:
        session.add(
            LibraryResource(
                kind=kind, name=name, unit=unit, rate=value, currency=currency, source=source, dated=date.today()
            )
        )
    return len(entries)


def propose_markups(
    session: Session,
    tender_id: str,
    by: str,
    preliminaries: Decimal,
    overheads: Decimal,
    profit: Decimal,
    adjustment: Decimal,
    note: str,
    status: str = "proposed",
) -> Markups:
    for name, value in (("preliminaries", preliminaries), ("overheads", overheads), ("profit", profit)):
        if not 0 <= value < 1:
            raise ValueError(f"Give {name} as a fraction between 0 and 1, e.g. 0.08 for 8%.")
    for older in session.scalars(select(Markups).where(Markups.tender_id == tender_id, Markups.status == "proposed")):
        older.status = "replaced"
    markups = Markups(
        tender_id=tender_id,
        preliminaries=preliminaries,
        overheads=overheads,
        profit=profit,
        adjustment=adjustment,
        note=note.strip(),
        proposed_by=by,
        status=status,
    )
    session.add(markups)
    session.flush()
    if status in APPROVED:
        _replace_older(session, markups)
    return markups


def current_markups(session: Session, tender_id: str) -> Markups | None:
    query = select(Markups).where(Markups.tender_id == tender_id, Markups.status.in_(LIVE))
    rows = list(session.scalars(query.order_by(Markups.created_at.desc())))
    return next((m for m in rows if m.status in APPROVED), rows[0] if rows else None)


@dataclass
class Summary:
    currency: str
    priced: int
    items: int
    waiting: int
    net: Decimal = Decimal(0)
    preliminaries: Decimal = Decimal(0)
    overheads: Decimal = Decimal(0)
    profit: Decimal = Decimal(0)
    adjustment: Decimal = Decimal(0)
    total: Decimal = Decimal(0)
    vat_rate: Decimal | None = None
    vat: Decimal | None = None
    total_with_vat: Decimal | None = None
    unpriced: list[str] = field(default_factory=list)


def _fact(session: Session, tender_id: str, kind: str) -> Fact | None:
    query = select(Fact).where(Fact.tender_id == tender_id, Fact.kind == kind, Fact.status.in_(APPROVED))
    return session.scalars(query.order_by(Fact.created_at.desc())).first()


def summary(session: Session, tender_id: str) -> Summary:
    """The price, from the BOQ quantities, the current rates and the current markups."""
    currency = _fact(session, tender_id, "currency")
    result = Summary(currency=currency.value if currency else "", priced=0, items=0, waiting=0)
    for item in boq.items(session, tender_id):
        result.items += 1
        rate = current_rate(session, item.id)
        if rate is None or item.quantity is None:
            result.unpriced.append(item.item)
            continue
        result.priced += 1
        result.waiting += rate.status == "proposed"
        result.net += money(item.quantity * rate_of(rate))
    markups = current_markups(session, tender_id)
    if markups:
        result.preliminaries = money(result.net * markups.preliminaries)
        result.overheads = money((result.net + result.preliminaries) * markups.overheads)
        result.profit = money((result.net + result.preliminaries + result.overheads) * markups.profit)
        result.adjustment = markups.adjustment
    result.total = result.net + result.preliminaries + result.overheads + result.profit + result.adjustment
    vat = _fact(session, tender_id, "vat")
    percent = re.search(r"(\d+(?:\.\d+)?)\s*%", vat.value) if vat else None
    if percent:
        result.vat_rate = Decimal(percent.group(1)) / 100
        result.vat = money(result.total * result.vat_rate)
        result.total_with_vat = result.total + result.vat
    return result


def waiting(session: Session, tender_id: str) -> int:
    rows = (
        session.scalars(select(Rate.id).where(Rate.tender_id == tender_id, Rate.status == "proposed")).all()
        + session.scalars(select(Markups.id).where(Markups.tender_id == tender_id, Markups.status == "proposed")).all()
    )
    return len(rows)


def library(session: Session, query: str = "") -> list[LibraryResource]:
    """The firm's rates whose names contain every word of the query."""
    words = query.lower().split()
    query_rows = select(LibraryResource).where(LibraryResource.owner_id == LOCAL_OWNER)
    rows = session.scalars(query_rows.order_by(LibraryResource.kind, LibraryResource.name))
    return [r for r in rows if all(w in r.name.lower() for w in words)]
