"""Proposing, checking and deciding BOQ items and tender facts."""

from datetime import UTC, datetime
from decimal import Decimal

from pydantic import BaseModel, Field
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from quantix.boq.models import APPROVED, FACT_KINDS, BoqItem, Fact
from quantix.documents.evidence import check_quote, numbers_in
from quantix.office import records as office
from quantix.office.models import ENGINEER, Staff

ACTIVE = ("proposed", *APPROVED)


class ItemIn(BaseModel):
    """One BOQ line as the client wrote it."""

    section: str | None = Field(default=None, description="The BOQ section or bill heading, if any")
    item: str = Field(description="The item number exactly as printed, e.g. 3.1 or C-2.4")
    description: str = Field(description="The item description, shortened if long, in the document's language")
    unit: str = Field(description="The unit exactly as printed, e.g. m3, م3, nr")
    quantity: Decimal | None = Field(default=None, description="The BOQ quantity; leave empty if the BOQ has none")
    document_id: str
    page: int
    quote: str = Field(description="The line as read_page shows it, including the item number and the quantity")


def propose_items(session: Session, tender_id: str, by: Staff, items: list[ItemIn], autonomous: bool) -> str:
    """Save the items that check out; report the others so they can be corrected."""
    taken = {
        (i.section or "", i.item)
        for i in session.scalars(select(BoqItem).where(BoqItem.tender_id == tender_id, BoqItem.status.in_(ACTIVE)))
    }
    position = session.scalar(select(func.max(BoqItem.position)).where(BoqItem.tender_id == tender_id)) or 0
    saved, problems = 0, []
    for line in items:
        try:
            check_quote(session, tender_id, line.document_id, line.page, line.quote)
            if line.item.strip() not in line.quote:
                raise ValueError(f"the item number {line.item} is not in the quote")
            if line.quantity is not None and line.quantity not in numbers_in(line.quote):
                raise ValueError(f"the quantity {line.quantity} is not in the quote")
            if (line.section or "", line.item.strip()) in taken:
                raise ValueError("it is already in the BOQ")
        except ValueError as error:
            problems.append(f"item {line.item}: {error}")
            continue
        position += 1
        taken.add((line.section or "", line.item.strip()))
        session.add(
            BoqItem(
                tender_id=tender_id,
                section=line.section,
                item=line.item.strip(),
                description=line.description.strip(),
                unit=line.unit.strip(),
                quantity=line.quantity,
                document_id=line.document_id,
                page=line.page,
                quote=line.quote.strip(),
                position=position,
                proposed_by=by.id,
                status="office_approved" if autonomous else "proposed",
            )
        )
        saved += 1
    report = f"Saved {saved} BOQ items for the engineer's approval." if not autonomous else f"Saved {saved} items."
    return report + ("\nNot saved: " + "; ".join(problems) if problems else "")


def items(session: Session, tender_id: str) -> list[BoqItem]:
    query = select(BoqItem).where(BoqItem.tender_id == tender_id, BoqItem.status != "rejected")
    return list(session.scalars(query.order_by(BoqItem.position)))


def propose_fact(
    session: Session,
    tender_id: str,
    by: Staff,
    kind: str,
    value: str,
    document_id: str,
    page: int,
    quote: str,
    autonomous: bool,
) -> Fact:
    if kind not in FACT_KINDS:
        raise ValueError(f"Facts can be: {', '.join(FACT_KINDS)}.")
    check_quote(session, tender_id, document_id, page, quote)
    for older in session.scalars(select(Fact).where(Fact.tender_id == tender_id, Fact.kind == kind)):
        if older.status == "proposed" or (autonomous and older.status in APPROVED):
            older.status = "replaced"
    fact = Fact(
        tender_id=tender_id,
        kind=kind,
        value=value.strip(),
        document_id=document_id,
        page=page,
        quote=quote.strip(),
        proposed_by=by.id,
        status="office_approved" if autonomous else "proposed",
    )
    session.add(fact)
    session.flush()
    return fact


def facts(session: Session, tender_id: str) -> list[Fact]:
    query = select(Fact).where(Fact.tender_id == tender_id, Fact.status.in_(ACTIVE))
    return list(session.scalars(query.order_by(Fact.created_at)))


def decide(session: Session, record: BoqItem | Fact, approve: bool, reason: str | None = None) -> None:
    """The engineer's decision. A rejection goes back to whoever proposed it, so they can put it right."""
    record.status = "approved" if approve else "rejected"
    record.reason = reason
    record.decided_at = datetime.now(UTC)
    if isinstance(record, Fact) and approve:
        for older in session.scalars(select(Fact).where(Fact.tender_id == record.tender_id, Fact.kind == record.kind)):
            if older.id != record.id and older.status in ACTIVE:
                older.status = "replaced"
    if not approve:
        what = f"BOQ item {record.item}" if isinstance(record, BoqItem) else FACT_KINDS[record.kind].lower()
        office.post(
            session,
            record.tender_id,
            ENGINEER,
            record.proposed_by,
            f"I rejected {what}" + (f": {reason}" if reason else "."),
        )


def approve_all_items(session: Session, tender_id: str) -> int:
    waiting = session.scalars(select(BoqItem).where(BoqItem.tender_id == tender_id, BoqItem.status == "proposed")).all()
    for item in waiting:
        decide(session, item, approve=True)
    manager = office.manager(session, tender_id)
    if waiting and manager:
        office.post(session, tender_id, ENGINEER, manager.id, f"I approved {len(waiting)} BOQ items.")
    return len(waiting)


def waiting_counts(session: Session, tender_id: str) -> dict[str, int]:
    def count(model) -> int:
        return session.scalar(
            select(func.count()).select_from(model).where(model.tender_id == tender_id, model.status == "proposed")
        )

    return {"boq": count(BoqItem), "facts": count(Fact)}
