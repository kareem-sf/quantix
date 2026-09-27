from datetime import UTC, datetime
from decimal import Decimal
from typing import Literal

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel, Field
from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix import tenders
from quantix.api.tenders import DB
from quantix.documents.models import Document
from quantix.subcontract import records
from quantix.subcontract.models import Company, Enquiry, Package, Quote

router = APIRouter(tags=["subcontract"])


class CompanyFields(BaseModel):
    name: str = Field(min_length=1)
    kind: Literal["subcontractor", "supplier"]
    trades: str = Field(min_length=1)
    email: str | None = None
    phone: str | None = None
    website: str | None = None


class CompanyIn(CompanyFields):
    different_from: list[str] = Field(default=[], description="Firms in the directory the engineer says it isn't")


class CompanyOut(CompanyFields):
    id: str
    aliases: list[str]
    added_by: str


class Merge(BaseModel):
    into: str = Field(description="The firm the duplicate is the same as")


class EnquiryOut(BaseModel):
    id: str
    company: str
    email: str | None
    subject: str
    body: str
    status: str
    created_by: str
    reviewed_by: str | None  # the Tender Manager, once he has reviewed it
    review_note: str | None


class QuotedCell(BaseModel):
    rate: Decimal | None
    amount: Decimal | None
    plugged: bool
    page: int | None
    quote: str | None


class ExclusionOut(BaseModel):
    description: str
    amount: Decimal
    page: int
    quote: str


class QuoteOut(BaseModel):
    id: str
    company: str
    document_id: str
    document: str
    cells: dict[str, QuotedCell]  # by BOQ item id
    exclusions: list[ExclusionOut]
    quoted_total: Decimal
    exclusions_total: Decimal
    levelled_total: Decimal | None
    rank: int | None
    proposed_by: str


class PackageItem(BaseModel):
    id: str
    section: str | None
    item: str
    description: str
    unit: str
    quantity: Decimal | None
    our_rate: Decimal | None


class PackageOut(BaseModel):
    id: str
    name: str
    kind: str
    items: list[PackageItem]
    enquiries: list[EnquiryOut]
    quotes: list[QuoteOut]
    recommended_quote_id: str | None
    recommendation: str | None
    recommended_by: str | None
    reviewed_by: str | None  # the Tender Manager's review of the recommendation
    review_note: str | None
    selected_quote_id: str | None
    created_by: str


class Choice(BaseModel):
    quote_id: str


def _tender(session: Session, tender_id: str) -> None:
    if tenders.get_tender(session, tender_id) is None:
        raise HTTPException(status_code=404, detail="Tender not found.")


def _package(session: Session, package: Package) -> PackageOut:
    levelling = records.level(session, package)
    evidence = {q.id: {line["boq_item_id"]: line for line in q.lines} for q in records.quotes(session, package.id)}
    quotes = []
    for column in levelling.columns:
        quote = session.get(Quote, column.quote_id)
        cells = {}
        for item in levelling.items:
            cell, line = column.cells[item.id], evidence[quote.id].get(item.id)
            amount = None
            if cell.rate is not None and item.quantity is not None:
                amount = records.estimate.money(item.quantity * cell.rate)
            cells[item.id] = QuotedCell(
                rate=cell.rate,
                amount=amount,
                plugged=cell.plugged,
                page=line["page"] if line else None,
                quote=line["quote"] if line else None,
            )
        quotes.append(
            QuoteOut(
                id=quote.id,
                company=column.company,
                document_id=quote.document_id,
                document=session.get(Document, quote.document_id).name,
                cells=cells,
                exclusions=[ExclusionOut(**e) for e in quote.exclusions],
                quoted_total=column.quoted_total,
                exclusions_total=column.exclusions,
                levelled_total=column.levelled_total,
                rank=column.rank,
                proposed_by=quote.proposed_by,
            )
        )
    enquiries = []
    for enquiry in records.enquiries(session, package.id):
        company = session.get(Company, enquiry.company_id)
        enquiries.append(
            EnquiryOut(
                id=enquiry.id,
                company=company.name,
                email=company.email,
                subject=enquiry.subject,
                body=enquiry.body,
                status=enquiry.status,
                created_by=enquiry.created_by,
                reviewed_by=enquiry.reviewed_by,
                review_note=enquiry.review_note,
            )
        )
    return PackageOut(
        id=package.id,
        name=package.name,
        kind=package.kind,
        items=[
            PackageItem(
                id=i.id,
                section=i.section,
                item=i.item,
                description=i.description,
                unit=i.unit,
                quantity=i.quantity,
                our_rate=levelling.ours[i.id],
            )
            for i in levelling.items
        ],
        enquiries=enquiries,
        quotes=quotes,
        recommended_quote_id=package.recommended_quote_id,
        recommendation=package.recommendation,
        recommended_by=package.recommended_by,
        reviewed_by=package.reviewed_by,
        review_note=package.review_note,
        selected_quote_id=package.selected_quote_id,
        created_by=package.created_by,
    )


@router.get("/tenders/{tender_id}/packages")
def get_packages(tender_id: str, session: DB) -> list[PackageOut]:
    _tender(session, tender_id)
    return [_package(session, p) for p in records.packages(session, tender_id)]


@router.post("/enquiries/{enquiry_id}/sent")
def mark_sent(enquiry_id: str, session: DB) -> None:
    enquiry = session.get(Enquiry, enquiry_id)
    if enquiry is None:
        raise HTTPException(status_code=404, detail="Not found.")
    enquiry.status, enquiry.sent_at = "sent", datetime.now(UTC)
    session.commit()


@router.post("/packages/{package_id}/choice")
def choose(package_id: str, body: Choice, session: DB, request: Request) -> None:
    package = session.get(Package, package_id)
    if package is None:
        raise HTTPException(status_code=404, detail="Not found.")
    if package.selected_quote_id:
        raise HTTPException(status_code=400, detail="A quote has already been chosen for this package.")
    quote = session.get(Quote, body.quote_id)
    if quote is None or quote.package_id != package.id:
        raise HTTPException(status_code=404, detail="That quote is not in this package.")
    try:
        records.select_quote(session, package, quote)
    except ValueError as error:
        raise HTTPException(status_code=400, detail=str(error)) from error
    session.commit()
    request.app.state.office.engineer_spoke(package.tender_id)


@router.get("/directory")
def get_directory(session: DB, q: str = "") -> list[CompanyOut]:
    rows = records.directory(session, q)
    session.commit()  # keeps the meaning of firms searched for the first time
    return [CompanyOut.model_validate(c, from_attributes=True) for c in rows]


@router.post("/directory", status_code=201, responses={409: {"description": "It may be a firm already there"}})
def add_company(body: CompanyIn, session: DB) -> CompanyOut:
    """Add a firm. 409 when it may be a firm already in the directory: the detail names them, and adding it again
    with them in `different_from` confirms it is another firm."""
    try:
        company = records.add_company(session, "engineer", **body.model_dump())
    except records.NearDuplicate as maybe:
        raise HTTPException(status_code=409, detail={"message": str(maybe), "firms": maybe.firms}) from maybe
    except ValueError as error:
        raise HTTPException(status_code=400, detail=str(error)) from error
    session.commit()
    return CompanyOut.model_validate(company, from_attributes=True)


@router.post("/directory/{company_id}/merge", status_code=204)
def merge_company(company_id: str, body: Merge, session: DB) -> None:
    """The engineer says a firm is the same as another: its enquiries and quotes move there, and its name stays as
    one of the other firm's names."""
    duplicate, into = session.get(Company, company_id), session.get(Company, body.into)
    if duplicate is None or into is None:
        raise HTTPException(status_code=404, detail="Not found.")
    try:
        records.merge(session, duplicate, into)
    except ValueError as error:
        raise HTTPException(status_code=400, detail=str(error)) from error
    session.commit()


@router.delete("/directory/{company_id}", status_code=204)
def remove_company(company_id: str, session: DB) -> None:
    company = session.get(Company, company_id)
    if company is None:
        raise HTTPException(status_code=404, detail="Not found.")
    used = (
        session.scalars(select(Quote.id).where(Quote.company_id == company_id)).first()
        or session.scalars(select(Enquiry.id).where(Enquiry.company_id == company_id)).first()
    )
    if used:
        raise HTTPException(status_code=400, detail="A tender's enquiry or quote is from this company, so it is kept.")
    session.delete(company)
    session.commit()
