"""Proposing, checking and deciding BOQ items and tender facts."""

import re
from datetime import UTC, datetime
from decimal import Decimal

from pydantic import BaseModel, Field
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from quantix.boq.models import FACT_KINDS, BoqItem, Fact
from quantix.core.review import LIVE, PROPOSED, REVIEWED, UNDECIDED
from quantix.documents import library
from quantix.documents.evidence import check_quote, numbers_in
from quantix.documents.models import Document
from quantix.office import records as office
from quantix.office.models import ENGINEER, Staff


class ItemIn(BaseModel):
    """One BOQ line as the client wrote it."""

    section: str | None = Field(
        default=None,
        description="The BOQ section or bill heading. When the package has several bills that reuse item numbers "
        '(one per building or substation), start it with the bill\'s name, e.g. "8485 · Asphalt"',
    )
    item: str = Field(description="The item number exactly as printed, e.g. 3.1 or C-2.4")
    description: str = Field(description="The item description, shortened if long, in the document's language")
    unit: str = Field(description="The unit exactly as printed, e.g. m3, م3, nr")
    quantity: Decimal | None = Field(default=None, description="The BOQ quantity; leave empty if the BOQ has none")
    document_id: str
    page: int
    quote: str = Field(description="The line as read_page shows it, including the item number and the quantity")


_CELL = re.compile(r"\b[A-Z]{1,3}(\d+)=")


def _source_line(document_id: str, page: int, quote: str) -> tuple[str, int, str]:
    """Where a BOQ line comes from: a workbook row, or the quoted line of a page."""
    row = _CELL.search(quote)
    return document_id, page, f"row {row.group(1)}" if row else " ".join(quote.lower().split())


def _revise(item: BoqItem, by: Staff, line: ItemIn, older_copy: str) -> None:
    """A line entered again from the newer copy of its document: the same line, so its rate and measurements stay
    with it, back to the Tender Manager's review with what the older copy said."""
    item.reason = f"Entered again from the newer copy of {older_copy}. The older copy said: “{item.quote}”"
    item.description, item.unit, item.quantity = line.description.strip(), line.unit.strip(), line.quantity
    item.document_id, item.page, item.quote = line.document_id, line.page, line.quote.strip()
    item.status, item.proposed_by, item.created_at = PROPOSED, by.id, datetime.now(UTC)
    item.reviewed_by = item.reviewed_at = item.review_note = item.decided_at = None


def propose_items(session: Session, tender_id: str, by: Staff, items: list[ItemIn]) -> str:
    """Save the items that check out, for the Tender Manager's review; report the others so they can be corrected.
    A line that rests on an older copy of its document is revised from the newer copy."""
    active = list(session.scalars(select(BoqItem).where(BoqItem.tender_id == tender_id, BoqItem.status.in_(LIVE))))
    older = {(i.section or "", i.item): i for i in active if library.superseded(session, i.document_id)}
    taken = {(i.section or "", i.item) for i in active} - older.keys()
    lines_in = {_source_line(i.document_id, i.page, i.quote) for i in active}  # the same client line, however filed
    position = session.scalar(select(func.max(BoqItem.position)).where(BoqItem.tender_id == tender_id)) or 0
    saved, revised, problems = 0, 0, []
    for line in items:
        try:
            check_quote(session, tender_id, line.document_id, line.page, line.quote)
            if line.item.strip() not in line.quote:
                raise ValueError(f"the item number {line.item} is not in the quote")
            if line.quantity is not None and line.quantity not in numbers_in(line.quote):
                raise ValueError(f"the quantity {line.quantity} is not in the quote")
            if _source_line(line.document_id, line.page, line.quote) in lines_in:
                raise ValueError("that line of the client's BOQ is already in, under another section")
            if (line.section or "", line.item.strip()) in taken:
                raise ValueError(
                    "it is already in the BOQ under this section; if it belongs to another bill (another building "
                    "or substation), start the section with that bill's name"
                )
        except ValueError as error:
            problems.append(f"item {line.item}: {error}")
            continue
        key = (line.section or "", line.item.strip())
        taken.add(key)
        lines_in.add(_source_line(line.document_id, line.page, line.quote))
        if key in older:
            stale = older.pop(key)
            _revise(stale, by, line, session.get(Document, stale.document_id).name)
            revised += 1
            continue
        position += 1
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
            )
        )
        saved += 1
    report = f"Saved {saved} BOQ items for the Tender Manager's review."
    if revised:
        report += f" Revised {revised} from the newer copy of their document, for his review."
    return report + ("\nNot saved: " + "; ".join(problems) if problems else "")


_ROW = re.compile(r"row (\d+)", re.IGNORECASE)


def _row(item: BoqItem) -> str | None:
    cell = _CELL.search(item.quote)
    return cell.group(1) if cell else None


def reference(item: BoqItem) -> str:
    """How to name an item so it can't be mistaken for the same number in another bill; find_item reads it back.
    A line the client left unnumbered is named by its row in the client's workbook."""
    number = item.item or (f"row {_row(item)}" if _row(item) else "")
    return f"{item.section} / {number}" if item.section else number


def find_item(session: Session, tender_id: str, name: str) -> BoqItem:
    """A BOQ item by its number, or by "<section> / <number>" when several bills share the number. An unnumbered
    line is "row <n>", its row in the client's workbook."""
    section, _, number = name.strip().rpartition(" / ")
    row = _ROW.fullmatch(number.strip())
    query = select(BoqItem).where(
        BoqItem.tender_id == tender_id, BoqItem.item == ("" if row else number.strip()), BoqItem.status.in_(LIVE)
    )
    numbered = [i for i in session.scalars(query) if not row or _row(i) == row.group(1)]
    found = [i for i in numbered if not section or (i.section or "").lower() == section.lower()]
    if not found:
        elsewhere = " or ".join(f"“{reference(i)}”" for i in numbered)
        raise ValueError(
            f"There is no BOQ item {name}."
            + (f" That number is {elsewhere}." if elsewhere else " Use list_boq to see the items.")
        )
    if len(found) > 1:
        sections = ", ".join(f"“{i.section or 'no section'}”" for i in found)
        raise ValueError(f"Item {number} is in more than one bill: {sections}. Give it as “<section> / {number}”.")
    return found[0]


def withdraw_items(session: Session, tender_id: str, by: Staff, references: list[str], reason: str) -> str:
    """Take back lines this person entered that the engineer hasn't decided yet, e.g. to re-enter them."""
    withdrawn, problems = 0, []
    for reference in references:
        try:
            item = find_item(session, tender_id, reference)
            if item.proposed_by != by.id or item.status != PROPOSED:
                raise ValueError("only lines you entered that haven't been reviewed yet can be withdrawn")
        except ValueError as error:
            problems.append(f"{reference}: {error}")
            continue
        item.status, item.reason = "withdrawn", reason.strip()
        withdrawn += 1
    return f"Withdrew {withdrawn} BOQ items." + ("\nNot withdrawn: " + "; ".join(problems) if problems else "")


def items(session: Session, tender_id: str) -> list[BoqItem]:
    query = select(BoqItem).where(BoqItem.tender_id == tender_id, BoqItem.status.not_in(("rejected", "withdrawn")))
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
) -> Fact:
    if kind not in FACT_KINDS:
        raise ValueError(f"Facts can be: {', '.join(FACT_KINDS)}.")
    check_quote(session, tender_id, document_id, page, quote)
    earlier = list(session.scalars(select(Fact).where(Fact.tender_id == tender_id, Fact.kind == kind)))
    settled = next((f for f in earlier if f.status == "approved"), None)
    if settled is not None and not library.superseded(session, settled.document_id):
        raise ValueError(
            f"The engineer approved the {FACT_KINDS[kind].lower()}: {settled.value}. If the documents say otherwise, "
            "say why with raise_concern."
        )
    for older in earlier:
        if older.status in UNDECIDED:  # the newest proposal of a kind is the one to review
            older.status = "replaced"
    fact = Fact(
        tender_id=tender_id,
        kind=kind,
        value=value.strip(),
        document_id=document_id,
        page=page,
        quote=quote.strip(),
        proposed_by=by.id,
    )
    session.add(fact)
    session.flush()
    return fact


def facts(session: Session, tender_id: str) -> list[Fact]:
    query = select(Fact).where(Fact.tender_id == tender_id, Fact.status.in_(LIVE))
    return list(session.scalars(query.order_by(Fact.created_at)))


def label(session: Session, record: BoqItem | Fact) -> str:
    """How a message names the record."""
    return f"BOQ item {reference(record)}" if isinstance(record, BoqItem) else f"the {FACT_KINDS[record.kind].lower()}"


def approve(session: Session, record: BoqItem | Fact, status: str = "approved") -> None:
    """Approved by the engineer, or by a fully autonomous office once the Tender Manager has reviewed it."""
    record.status, record.decided_at = status, datetime.now(UTC)
    if isinstance(record, Fact):  # one fact of each kind
        for older in session.scalars(select(Fact).where(Fact.tender_id == record.tender_id, Fact.kind == record.kind)):
            if older.id != record.id and older.status in LIVE:
                older.status = "replaced"


def decide(session: Session, record: BoqItem | Fact, approve_it: bool, reason: str | None = None) -> None:
    """The engineer's decision. A rejection goes back to whoever proposed it, so they can put it right."""
    if approve_it:
        approve(session, record)
    else:
        office.send_back(session, record.tender_id, record, label(session, record), reason, ENGINEER)


def approve_all_items(session: Session, tender_id: str) -> int:
    """Every BOQ item the Tender Manager has reviewed."""
    waiting = session.scalars(select(BoqItem).where(BoqItem.tender_id == tender_id, BoqItem.status == REVIEWED)).all()
    for item in waiting:
        approve(session, item)
    manager = office.manager(session, tender_id)
    if waiting and manager:
        office.post(session, tender_id, ENGINEER, manager.id, f"I approved {len(waiting)} BOQ items.")
    return len(waiting)


def waiting_counts(session: Session, tender_id: str) -> dict[str, int]:
    def count(model) -> int:
        return session.scalar(
            select(func.count()).select_from(model).where(model.tender_id == tender_id, model.status == REVIEWED)
        )

    return {"boq": count(BoqItem), "facts": count(Fact)}
