"""The submission checklist, drafts for review and the client BOQ's pricing columns."""

import math
import re
from dataclasses import dataclass
from datetime import UTC, datetime
from decimal import Decimal
from pathlib import Path

from pydantic import BaseModel, Field
from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix.boq import records as boq
from quantix.core.review import APPROVED, LIVE, PROPOSED, REVIEWED
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


class ActivityIn(BaseModel):
    """One line of a work schedule."""

    boq_item: str = Field(description='The BOQ line as list_boq shows it, e.g. "8486 · Earthwork / C.1.2"')
    output: Decimal = Field(gt=0, description="What one crew does in a day, in the line's unit")
    crews: int = Field(ge=1, le=50)


@dataclass
class Duration:
    item_id: str
    reference: str
    description: str
    quantity: Decimal
    unit: str
    output: Decimal
    crews: int
    days: int


def durations(session: Session, tender_id: str, activities: list[ActivityIn]) -> list[Duration]:
    """Days per line of a work schedule, from the BOQ quantity and the assumed output and crews. Every line that
    can't be found, or comes twice, is reported at once, so one correction fixes them all."""
    rows, problems = [], []
    for activity in activities:
        try:
            item = boq.find_item(session, tender_id, activity.boq_item)
        except ValueError as error:
            problems.append(str(error))
            continue
        if any(r.item_id == item.id for r in rows):
            others = [
                boq.reference(i) for i in boq.items(session, tender_id) if i.item == item.item and i.id != item.id
            ]
            elsewhere = " or ".join(f"“{o}”" for o in others)
            problems.append(
                f"{boq.reference(item)} is listed twice."
                + (f" The same number in another bill is {elsewhere}." if others else "")
            )
            continue
        quantity = item.quantity or Decimal(0)
        days = math.ceil(quantity / (activity.output * activity.crews)) if quantity > 0 else 0
        rows.append(
            Duration(
                item.id,
                boq.reference(item),
                item.description,
                quantity,
                item.unit,
                activity.output,
                activity.crews,
                days,
            )
        )
    if problems:
        raise ValueError("Correct these lines and send the whole schedule again: " + " ".join(problems))
    return rows


def _amount(value: Decimal) -> str:
    return f"{value:,.3f}".rstrip("0").rstrip(".")


def schedule_record(rows: list[Duration], overall_days: int) -> dict:
    """What the checks need from a work schedule: the lines it covers and the overall duration stated for it."""
    return {
        "lines": [{"item_id": r.item_id, "reference": r.reference, "days": r.days} for r in rows],
        "overall_days": overall_days,
    }


def schedule_text(rows: list[Duration], sequence: str, overall_days: int | None = None) -> str:
    """A work schedule: each line's duration as Quantix worked it out, then the planned sequence and overlaps."""
    lines = [
        f"- {r.reference}, {r.description[:80]}: {_amount(r.quantity)} {r.unit} at {_amount(r.output)} {r.unit} a "
        f"day × {r.crews} crew{'s' if r.crews > 1 else ''} = {r.days} day{'s' if r.days != 1 else ''}"
        for r in rows
    ]
    overall = [f"Overall duration: {overall_days} working days."] if overall_days is not None else []
    return "\n".join(
        ["Durations, from the BOQ quantities and the assumed outputs:", *lines, "", sequence.strip(), *overall]
    )


def requirements(session: Session, tender_id: str) -> list[Requirement]:
    query = select(Requirement).where(Requirement.tender_id == tender_id)
    return list(session.scalars(query.order_by(Requirement.created_at)))


def find_requirement(session: Session, tender_id: str, title: str) -> Requirement:
    """By its title, or as list_requirements shows it: "<section> · <title>"."""
    wanted = title.strip().lower()
    checklist = requirements(session, tender_id)
    found = next((r for r in checklist if wanted in (r.title.lower(), f"{r.section} · {r.title}".lower())), None)
    if found is None:
        raise ValueError(
            f"There is no requirement called {title}. Give one of these titles exactly: "
            + "; ".join(f"“{r.title}”" for r in checklist[:40])
            + ". For a letter to the client, such as a clarification query, first add it with add_requirements in the "
            "section Correspondence, citing the page it is about."
        )
    return found


def add_requirements(session: Session, tender_id: str, by: str, items: list[RequirementIn]) -> str:
    """Save the requirements whose clause checks out; report the others so they can be corrected. A requirement that
    rests on an older copy of its document takes its clause from the newer copy, and goes back to the Manager."""
    checklist = requirements(session, tender_id)
    taken = [r.title for r in checklist]
    saved, problems = 0, []
    for item in items:
        same = next((t for t in taken if office.same_subject(item.title, t)), None)
        earlier = next((r for r in checklist if r.title == same), None)
        stale = earlier is not None and library.superseded(session, earlier.document_id)
        if same and not stale:
            problems.append(f"{item.title}: the checklist already has “{same}”")
            continue
        try:
            check_quote(session, tender_id, item.document_id, item.page, item.quote)
        except ValueError as error:
            problems.append(f"{item.title}: {error}")
            continue
        if stale:
            earlier.document_id, earlier.page, earlier.quote = item.document_id, item.page, item.quote
            earlier.added_by, earlier.created_at = by, datetime.now(UTC)
            earlier.reviewed_by = earlier.reviewed_at = earlier.review_note = None
            saved += 1
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
        taken.append(item.title.strip())
        saved += 1
    session.flush()
    return "\n".join([f"{saved} requirements added to the checklist.", *problems])


def current_draft(session: Session, requirement_id: str) -> Draft | None:
    query = select(Draft).where(Draft.requirement_id == requirement_id, Draft.status.in_(LIVE))
    return session.scalars(query.order_by(Draft.created_at.desc())).first()


def draft(
    session: Session,
    requirement: Requirement,
    by: str,
    title: str,
    body: str,
    status: str = PROPOSED,
    schedule: dict | None = None,
) -> Draft:
    if not body.strip():
        raise ValueError("The draft is empty.")
    current = current_draft(session, requirement.id)
    if current is not None and current.status == "approved" and by != ENGINEER:
        raise ValueError(
            f"The engineer approved “{current.title}” for this requirement; a new draft doesn't replace it. "
            "If you think it needs changing, say why with raise_concern."
        )
    for older in session.scalars(select(Draft).where(Draft.requirement_id == requirement.id, Draft.status.in_(LIVE))):
        older.status = "replaced"
    new = Draft(
        tender_id=requirement.tender_id,
        requirement_id=requirement.id,
        title=title.strip(),
        body=body.strip(),
        proposed_by=by,
        status=status,
        schedule=schedule,
    )
    session.add(new)
    session.flush()
    return new


def label(session: Session, record: Draft) -> str:
    """How a message names the record."""
    return f"the draft “{record.title}”"


def approve(session: Session, record: Draft, status: str = "approved") -> None:
    """Approved by the engineer, or by a fully autonomous office once the Tender Manager has reviewed it."""
    record.status, record.decided_at = status, datetime.now(UTC)


def decide(session: Session, record: Draft, approve_it: bool, reason: str | None = None) -> None:
    if approve_it:
        approve(session, record)
    else:
        office.send_back(session, record.tender_id, record, label(session, record), reason, ENGINEER)


def state(session: Session, requirement: Requirement) -> str:
    """ready, review (a draft the Tender Manager reviewed waits for the engineer), manager (a draft is with the
    Manager for review) or missing."""
    if requirement.ready_note is not None or requirement.file_name:
        return "ready"
    current = current_draft(session, requirement.id)
    if current is None:
        return "missing"
    if current.status in APPROVED:
        return "ready"
    return "review" if current.status == REVIEWED else "manager"


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
    """Drafts the Tender Manager reviewed, for the engineer to decide."""
    return len(session.scalars(select(Draft.id).where(Draft.tender_id == tender_id, Draft.status == REVIEWED)).all())


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
    every_copy = [d.id for d in library.copies(session, document)]  # a newer copy's columns replace an older one's
    for older in session.scalars(
        select(PricingColumns).where(PricingColumns.document_id.in_(every_copy), PricingColumns.sheet == sheet)
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
