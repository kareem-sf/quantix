"""Build the submission package on the engineer's computer. Nothing is ever sent to the client."""

import re
import shutil
from dataclasses import dataclass, field
from datetime import datetime
from decimal import Decimal
from pathlib import Path

import openpyxl
from sqlalchemy.orm import Session

from quantix import tenders
from quantix.boq import records as boq
from quantix.core.review import APPROVED
from quantix.documents import library
from quantix.documents.models import Document
from quantix.estimate import records as estimate
from quantix.submission import deck, package, pdf, records, word, workbooks


@dataclass
class Built:
    folder: str
    files: list[str] = field(default_factory=list)
    priced_total: Decimal = Decimal(0)
    summary_total: Decimal = Decimal(0)
    factor: Decimal = Decimal(1)
    not_ready: list[str] = field(default_factory=list)  # the tender audit's blockers, filled in by the caller


def exports_dir(home: Path) -> Path:
    return home / "exports"


def _safe(name: str, limit: int = 60) -> str:
    """A file or folder name Windows accepts, short enough that the package's paths stay under its 260 characters."""
    name = " ".join(re.sub(r'[<>:"/\\|?*\x00-\x1f]', " ", name).split())
    if len(name) > limit:
        name = name[:limit].rsplit(" ", 1)[0]  # at a word, not through one
    return name or "Tender"


def submitted_rates(session: Session, tender_id: str, spread: bool) -> tuple[dict[str, Decimal], Decimal]:
    """The rate to write for each priced item. Spreading lifts every rate by total ÷ net, so the markups sit in the
    rates and the BOQ adds up to the tender total, give or take the rounding of each rate to the cent."""
    summary = estimate.summary(session, tender_id)
    factor = summary.total / summary.net if spread and summary.net else Decimal(1)
    rates = {}
    for item in boq.items(session, tender_id):
        rate = estimate.current_rate(session, item.id)
        if rate is not None and item.quantity is not None:
            rates[item.id] = estimate.money(estimate.rate_of(rate) * factor)
    return rates, factor


def _client_format(home: Path, session: Session, tender_id: str, folder: Path, rates: dict[str, Decimal]):
    """Copies of the client's BOQ workbooks with rates and amounts written into their own columns. Returns the files
    and the items those sheets cover, priced or not."""
    covered: set[str] = set()
    files = []
    items = boq.items(session, tender_id)
    columns = records.pricing_columns(session, tender_id)
    for document_id in {c.document_id for c in columns}:
        document = session.get(Document, document_id)
        source = library.stored_file(home, document)
        workbook = openpyxl.load_workbook(source, keep_vba=source.suffix == ".xlsm")
        for sheet in (c for c in columns if c.document_id == document_id):
            worksheet = workbook.worksheets[sheet.sheet - 1]
            for item in items:
                row = records.row_of(item.quote)
                if item.document_id != document_id or item.page != sheet.sheet or row is None:
                    continue
                covered.add(item.id)
                if item.id not in rates:
                    continue
                rate = worksheet[f"{sheet.rate_column}{row}"]
                rate.value, rate.number_format = rates[item.id], "#,##0.00"
                amount = worksheet[f"{sheet.amount_column}{row}"]
                if not (isinstance(amount.value, str) and amount.value.startswith("=")):  # keep the client's formula
                    amount.value = estimate.money(item.quantity * rates[item.id])
                amount.number_format = "#,##0.00"
        name = f"Priced {document.name}"
        workbook.save(folder / name)
        files.append(name)
    return files, covered


def build(home: Path, session: Session, tender_id: str, spread: bool, now: datetime) -> Built:
    """The package: the combined submission PDF and the priced BOQ at the top, each document as Word and PDF under
    Documents, the client queries under Correspondence, and the checklist under Internal, which is never sent."""
    tender = tenders.get_tender(session, tender_id)
    folder = exports_dir(home) / f"{_safe(tender.name)} {now:%Y-%m-%d %H%M}"
    for part in ("Documents", "Correspondence", "Internal"):
        (folder / part).mkdir(parents=True, exist_ok=True)
    built = Built(folder=folder.name)
    head = package.letterhead(home, tender.name)
    opening = package.facts(head, now)

    items = boq.items(session, tender_id)
    rates, built.factor = submitted_rates(session, tender_id, spread)
    summary = estimate.summary(session, tender_id)
    built.summary_total = summary.total
    built.priced_total = sum((estimate.money(i.quantity * rates[i.id]) for i in items if i.id in rates), Decimal(0))
    client_files, covered = _client_format(home, session, tender_id, folder, rates)
    built.files += client_files
    if any(i.id not in covered for i in items):
        uncovered = [i for i in items if i.id not in covered]
        workbooks.priced(session, uncovered, rates, summary, head, opening, spread, folder / "Priced BOQ.xlsx")
        built.files.append("Priced BOQ.xlsx")
    priced = package.priced(session, items, rates, summary, opening, spread)
    pdf.write(priced, head, folder / "Priced BOQ.pdf")
    built.files.append("Priced BOQ.pdf")

    submitted = [priced]
    checklist: list[list[str | None]] = []
    for requirement in records.requirements(session, tender_id):
        state, file = records.state(session, requirement), None
        current = records.current_draft(session, requirement.id)
        part = "Correspondence" if requirement.section.strip().lower() == "correspondence" else "Documents"
        if requirement.file_name:
            file = f"{part}/{_safe(requirement.title)} - {requirement.file_name}"
            shutil.copyfile(records.attachments_dir(home, requirement) / requirement.file_name, folder / file)
            built.files.append(file)
        elif current is not None and current.status in APPROVED:
            document = package.draft(session, current, opening)
            file = f"{part}/{_safe(current.title)}"
            word.write(document, head, folder / f"{file}.docx")
            pdf.write(document, head, folder / f"{file}.pdf")
            built.files += [f"{file}.docx", f"{file}.pdf"]
            file = f"{file}.docx"
            if part == "Documents":
                submitted.append(document)
        source = session.get(Document, requirement.document_id) if requirement.document_id else None
        label = {
            "ready": "Ready",
            "review": "Draft waiting for your review",
            "manager": "Draft with the Tender Manager",
            "missing": "Missing",
        }[state]
        if current is not None and current.status == "office_approved" and not requirement.file_name:
            label = "Ready · approved by the office after the Tender Manager's review, not reviewed by you"
        checklist.append(
            [
                requirement.section,
                requirement.title,
                f"{source.name}, page {requirement.page}" if source else "Added by the engineer",
                label,
                file,
            ]
        )
    workbooks.checklist(checklist, head, opening, folder / "Internal" / "Checklist.xlsx")
    built.files.append("Internal/Checklist.xlsx")
    deck.write(session, home, tender_id, head, opening, folder / "Internal" / "Tender summary.pptx")
    built.files.append("Internal/Tender summary.pptx")

    combined = f"{_safe(tender.name)} - Submission.pdf"
    pdf.write_package(submitted, head, opening, folder / combined)
    built.files.insert(0, combined)
    return built
