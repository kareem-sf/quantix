"""The submission checklist, drafts for review and the client BOQ's pricing columns."""

import re
from datetime import UTC, datetime
from pathlib import Path

from pydantic import BaseModel, Field
from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix.boq.models import APPROVED
from quantix.documents import library
from quantix.documents.evidence import check_quote
from quantix.office import records as office
from quantix.office.models import ENGINEER
from quantix.submission.models import Draft, PricingColumns, Requirement


class RequirementIn(BaseModel):
    section: str = Field(description="Commercial, Technical, or the tender's own grouping")
    title: str = Field(description="What must be submitted, e.g. 'Bid bond, 1% of the tender price'")
    document_id: str
    page: int
    quote: str = Field(description="The clause that requires it, as read_page shows it")


def requirements(session: Session, tender_id: str) -> list[Requirement]:
    query = select(Requirement).where(Requirement.tender_id == tender_id)
    return list(session.scalars(query.order_by(Requirement.created_at)))


def find_requirement(session: Session, tender_id: str, title: str) -> Requirement:
    wanted = title.strip().lower()
    found = next((r for r in requirements(session, tender_id) if r.title.lower() == wanted), None)
    if found is None:
        raise ValueError(f"There is no requirement called {title}. Use list_requirements to see them.")
    return found


def add_requirements(session: Session, tender_id: str, by: str, items: list[RequirementIn]) -> str:
    """Save the requirements whose clause checks out; report the others so they can be corrected."""
    taken = {r.title.lower() for r in requirements(session, tender_id)}
    saved, problems = 0, []
    for item in items:
        if item.title.strip().lower() in taken:
            continue
        try:
            check_quote(session, tender_id, item.document_id, item.page, item.quote)
        except ValueError as error:
            problems.append(f"{item.title}: {error}")
            continue
        session.add(
            Requirement(
                tender_id=tender_id,
                section=item.section.strip(),
                title=item.title.strip(),
                document_id=item.document_id,
                page=item.page,
                quote=item.quote,
                added_by=by,
            )
        )
        taken.add(item.title.strip().lower())
        saved += 1
    session.flush()
    return "\n".join([f"{saved} requirements added to the checklist.", *problems])


def current_draft(session: Session, requirement_id: str) -> Draft | None:
    query = select(Draft).where(Draft.requirement_id == requirement_id, Draft.status.in_(("proposed", *APPROVED)))
    return session.scalars(query.order_by(Draft.created_at.desc())).first()


def draft(session: Session, requirement: Requirement, by: str, title: str, body: str, status="proposed") -> Draft:
    if not body.strip():
        raise ValueError("The draft is empty.")
    for older in session.scalars(
        select(Draft).where(Draft.requirement_id == requirement.id, Draft.status.in_(("proposed", *APPROVED)))
    ):
        older.status = "replaced"
    new = Draft(
        tender_id=requirement.tender_id,
        requirement_id=requirement.id,
        title=title.strip(),
        body=body.strip(),
        proposed_by=by,
        status=status,
    )
    session.add(new)
    session.flush()
    return new


def decide(session: Session, record: Draft, approve: bool, reason: str | None = None) -> None:
    record.status = "approved" if approve else "rejected"
    record.reason = reason
    record.decided_at = datetime.now(UTC)
    if not approve and record.proposed_by != ENGINEER:
        text = f"I sent back the draft “{record.title}”" + (f": {reason}" if reason else ".")
        office.post(session, record.tender_id, ENGINEER, record.proposed_by, text)


def state(session: Session, requirement: Requirement) -> str:
    """ready, review (a draft waits for the engineer) or missing."""
    if requirement.ready_note is not None or requirement.file_name:
        return "ready"
    current = current_draft(session, requirement.id)
    if current is None:
        return "missing"
    return "review" if current.status == "proposed" else "ready"


def attachments_dir(home: Path, requirement: Requirement) -> Path:
    return home / "tenders" / requirement.tender_id / "submission" / requirement.id


def attach(home: Path, requirement: Requirement, name: str, content: bytes) -> None:
    """Keep a file the engineer provides, such as a signed form or a certificate. It replaces an earlier one."""
    name = library.clean_path(name).split("/")[-1]
    if not name:
        raise ValueError("The file needs a name.")
    folder = attachments_dir(home, requirement)
    folder.mkdir(parents=True, exist_ok=True)
    for old in folder.iterdir():
        old.unlink()
    (folder / name).write_bytes(content)
    requirement.file_name = name


def waiting(session: Session, tender_id: str) -> int:
    """Drafts for the engineer to review."""
    return len(session.scalars(select(Draft.id).where(Draft.tender_id == tender_id, Draft.status == "proposed")).all())


_CELL = r"\b{}(\d+)="


def set_pricing_columns(
    session: Session, tender_id: str, by: str, document_id: str, sheet: int, rate: str, amount: str, quote: str
) -> PricingColumns:
    """Record where the client's workbook takes rates and amounts, from its header row."""
    document = check_quote(session, tender_id, document_id, sheet, quote)
    if document.kind != "spreadsheet":
        raise ValueError(
            "Pricing columns are for the client's BOQ workbook; other BOQs are priced in Quantix's layout."
        )
    rate, amount = rate.strip().upper(), amount.strip().upper()
    for label, column in (("rate", rate), ("amount", amount)):
        if not re.fullmatch(r"[A-Z]{1,3}", column):
            raise ValueError(f"Give the {label} column as a letter, e.g. F.")
        if not re.search(_CELL.format(column), quote):
            raise ValueError(f"The header you quoted has no cell in column {column}.")
    for older in session.scalars(
        select(PricingColumns).where(PricingColumns.document_id == document_id, PricingColumns.sheet == sheet)
    ):
        session.delete(older)
    columns = PricingColumns(
        tender_id=tender_id,
        document_id=document_id,
        sheet=sheet,
        rate_column=rate,
        amount_column=amount,
        quote=quote,
        proposed_by=by,
    )
    session.add(columns)
    session.flush()
    return columns


def pricing_columns(session: Session, tender_id: str) -> list[PricingColumns]:
    return list(session.scalars(select(PricingColumns).where(PricingColumns.tender_id == tender_id)))


def row_of(quote: str) -> int | None:
    """The workbook row a BOQ line was read from: its quote starts with cells like "A12=3.1"."""
    match = re.search(r"\b[A-Z]{1,3}(\d+)=", quote)
    return int(match.group(1)) if match else None
