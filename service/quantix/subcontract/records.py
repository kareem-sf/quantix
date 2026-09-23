"""Packages, enquiries, quotes, levelling and the choice. Quantix does the levelling arithmetic."""

from dataclasses import dataclass, field
from datetime import UTC, datetime
from decimal import Decimal

from pydantic import BaseModel, Field
from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix.boq import records as boq
from quantix.boq.models import BoqItem
from quantix.documents.evidence import check_quote, numbers_in
from quantix.estimate import records as estimate
from quantix.estimate.models import Rate
from quantix.office import records as office
from quantix.office.models import ENGINEER
from quantix.subcontract.models import Company, Enquiry, Package, Quote
from quantix.tenders import LOCAL_OWNER


class QuoteLine(BaseModel):
    boq_item: str = Field(description="The BOQ item number the quoted rate is for")
    rate: Decimal = Field(description="The rate exactly as quoted")
    page: int
    quote: str = Field(description="The quote's line as read_page shows it, including the rate")


class Exclusion(BaseModel):
    description: str = Field(description="What the quote leaves out, e.g. scaffolding")
    amount: Decimal = Field(description="Your estimate of what it adds, to compare like with like")
    page: int
    quote: str = Field(description="Where the quote says it is excluded")


def directory(session: Session, words: str = "") -> list[Company]:
    rows = session.scalars(select(Company).where(Company.owner_id == LOCAL_OWNER).order_by(Company.name))
    terms = words.lower().split()
    return [c for c in rows if all(t in f"{c.name} {c.trades}".lower() for t in terms)]


def find_company(session: Session, name: str) -> Company:
    wanted = name.strip().lower()
    company = next((c for c in directory(session) if c.name.lower() == wanted), None)
    if company is None:
        raise ValueError(f"{name} is not in the directory. Add it with add_company first.")
    return company


def add_company(session: Session, by: str, name: str, kind: str, trades: str, email=None, phone=None) -> Company:
    if kind not in ("subcontractor", "supplier"):
        raise ValueError("A company is a subcontractor or a supplier.")
    if any(c.name.lower() == name.strip().lower() for c in directory(session)):
        raise ValueError(f"{name} is already in the directory.")
    company = Company(name=name.strip(), kind=kind, trades=trades.strip(), email=email, phone=phone, added_by=by)
    session.add(company)
    session.flush()
    return company


def packages(session: Session, tender_id: str) -> list[Package]:
    return list(session.scalars(select(Package).where(Package.tender_id == tender_id).order_by(Package.created_at)))


def find_package(session: Session, tender_id: str, name: str) -> Package:
    package = next((p for p in packages(session, tender_id) if p.name.lower() == name.strip().lower()), None)
    if package is None:
        raise ValueError(f"There is no package called {name}.")
    return package


def _item(session: Session, tender_id: str, number: str) -> BoqItem:
    item = session.scalars(
        select(BoqItem).where(
            BoqItem.tender_id == tender_id, BoqItem.item == number.strip(), BoqItem.status.in_(boq.ACTIVE)
        )
    ).first()
    if item is None:
        raise ValueError(f"There is no BOQ item {number}. Use list_boq to see the items.")
    return item


def create_package(session: Session, tender_id: str, by: str, name: str, kind: str, item_numbers: list[str]) -> Package:
    if kind not in ("subcontract", "supply"):
        raise ValueError("A package is a subcontract or a supply package.")
    if any(p.name.lower() == name.strip().lower() for p in packages(session, tender_id)):
        raise ValueError(f"There is already a package called {name}.")
    items = [_item(session, tender_id, n).id for n in item_numbers]
    if not items:
        raise ValueError("A package needs at least one BOQ item.")
    package = Package(tender_id=tender_id, name=name.strip(), kind=kind, items=items, created_by=by)
    session.add(package)
    session.flush()
    return package


def draft_enquiry(session: Session, package: Package, company: Company, by: str, subject: str, body: str) -> Enquiry:
    enquiry = Enquiry(
        package_id=package.id, company_id=company.id, subject=subject.strip(), body=body.strip(), created_by=by
    )
    session.add(enquiry)
    session.flush()
    return enquiry


def enquiries(session: Session, package_id: str) -> list[Enquiry]:
    return list(session.scalars(select(Enquiry).where(Enquiry.package_id == package_id).order_by(Enquiry.created_at)))


def record_quote(
    session: Session,
    package: Package,
    company: Company,
    by: str,
    document_id: str,
    lines: list[QuoteLine],
    exclusions: list[Exclusion],
) -> Quote:
    """Keep a company's quote. Every rate and exclusion must be on the quote's pages."""
    stored_lines = []
    for line in lines:
        item = _item(session, package.tender_id, line.boq_item)
        if item.id not in package.items:
            raise ValueError(f"Item {line.boq_item} is not in the {package.name} package.")
        check_quote(session, package.tender_id, document_id, line.page, line.quote)
        if line.rate not in numbers_in(line.quote):
            raise ValueError(f"The rate {line.rate} for item {line.boq_item} is not in the quoted line.")
        stored_lines.append({"boq_item_id": item.id, "rate": str(line.rate), "page": line.page, "quote": line.quote})
    for exclusion in exclusions:
        check_quote(session, package.tender_id, document_id, exclusion.page, exclusion.quote)
    for older in session.scalars(select(Quote).where(Quote.package_id == package.id, Quote.company_id == company.id)):
        session.delete(older)  # a revised quote from the same company replaces the earlier one
    quote = Quote(
        package_id=package.id,
        company_id=company.id,
        document_id=document_id,
        lines=stored_lines,
        exclusions=[e.model_dump(mode="json") for e in exclusions],
        proposed_by=by,
    )
    session.add(quote)
    session.flush()
    return quote


def quotes(session: Session, package_id: str) -> list[Quote]:
    return list(session.scalars(select(Quote).where(Quote.package_id == package_id).order_by(Quote.created_at)))


@dataclass
class Cell:
    rate: Decimal | None
    plugged: bool  # the quote left this item out, so our own rate stands in


@dataclass
class Column:
    quote_id: str
    company: str
    cells: dict[str, Cell]
    exclusions: Decimal
    quoted_total: Decimal
    levelled_total: Decimal | None  # None when a gap has no rate of ours to fill it
    rank: int | None = None


@dataclass
class Levelling:
    items: list[BoqItem]
    ours: dict[str, Decimal | None]
    columns: list[Column] = field(default_factory=list)


def _our_rate(session: Session, item_id: str, quoted_from: set[str]) -> Decimal | None:
    """Our own rate for the item: never one taken from this package's quotes, even after one was chosen."""
    current = estimate.current_rate(session, item_id)
    if current and not (current.basis == "quote" and current.document_id in quoted_from):
        return estimate.rate_of(current)
    older = select(Rate).where(Rate.boq_item_id == item_id, Rate.status != "rejected").order_by(Rate.created_at.desc())
    for rate in session.scalars(older):
        if not (rate.basis == "quote" and rate.document_id in quoted_from):
            return estimate.rate_of(rate)
    return None


def level(session: Session, package: Package) -> Levelling:
    items = [session.get(BoqItem, i) for i in package.items]
    received = quotes(session, package.id)
    quoted_from = {q.document_id for q in received}
    result = Levelling(items=items, ours={item.id: _our_rate(session, item.id, quoted_from) for item in items})
    ours = result.ours
    for quote in received:
        quoted = {line["boq_item_id"]: Decimal(line["rate"]) for line in quote.lines}
        cells, quoted_total, levelled = {}, Decimal(0), Decimal(0)
        for item in items:
            quantity = item.quantity or Decimal(0)
            if item.id in quoted:
                cells[item.id] = Cell(quoted[item.id], False)
                quoted_total += estimate.money(quantity * quoted[item.id])
                if levelled is not None:
                    levelled += estimate.money(quantity * quoted[item.id])
            else:
                cells[item.id] = Cell(ours[item.id], True)
                if levelled is None or ours[item.id] is None:
                    levelled = None
                else:
                    levelled += estimate.money(quantity * ours[item.id])
        exclusions = sum((Decimal(str(e["amount"])) for e in quote.exclusions), Decimal(0))
        result.columns.append(
            Column(
                quote_id=quote.id,
                company=session.get(Company, quote.company_id).name,
                cells=cells,
                exclusions=exclusions,
                quoted_total=quoted_total,
                levelled_total=levelled + exclusions if levelled is not None else None,
            )
        )
    ranked = sorted((c for c in result.columns if c.levelled_total is not None), key=lambda c: c.levelled_total)
    for position, column in enumerate(ranked, start=1):
        column.rank = position
    return result


def recommend(session: Session, package: Package, by: str, company: Company, reason: str) -> Quote:
    quote = next((q for q in quotes(session, package.id) if q.company_id == company.id), None)
    if quote is None:
        raise ValueError(f"There is no quote from {company.name} for {package.name}. Record it first.")
    package.recommended_quote_id, package.recommendation, package.recommended_by = quote.id, reason.strip(), by
    return quote


def select_quote(session: Session, package: Package, quote: Quote, status: str = "approved") -> int:
    """The choice: the quote's rates go into the estimate as quoted rates, each with its line as evidence."""
    company = session.get(Company, quote.company_id)
    for line in quote.lines:
        item = session.get(BoqItem, line["boq_item_id"])
        estimate.propose_rate(
            session,
            package.tender_id,
            quote.proposed_by,
            item.item,
            "quote",
            f"{'Subcontract' if package.kind == 'subcontract' else 'Supply'}: {company.name}",
            unit_rate=Decimal(line["rate"]),
            document_id=quote.document_id,
            page=line["page"],
            quote=line["quote"],
            status=status,
        )
    package.selected_quote_id, package.decided_at = quote.id, datetime.now(UTC)
    manager = office.manager(session, package.tender_id)
    if manager and status == "approved":
        text = f"I chose {company.name} for {package.name}. Their rates are now in the estimate."
        if quote.exclusions:
            text += " Their exclusions still need covering: " + "; ".join(e["description"] for e in quote.exclusions)
        office.post(session, package.tender_id, ENGINEER, manager.id, text)
    return len(quote.lines)


def waiting(session: Session, tender_id: str) -> int:
    """Packages with a recommendation the engineer hasn't acted on."""
    return sum(1 for p in packages(session, tender_id) if p.recommended_quote_id and not p.selected_quote_id)
