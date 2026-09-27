"""The Tender Manager's review. Everything the staff propose comes to him before it reaches the engineer; he accepts
it or sends it back to whoever made it. In a fully autonomous office his acceptance approves it."""

from dataclasses import dataclass
from datetime import UTC, datetime
from decimal import Decimal
from typing import Any

from pydantic import BaseModel, Field
from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix.boq import records as boq
from quantix.boq.models import FACT_KINDS, BoqItem, Fact
from quantix.core.review import APPROVED, PROPOSED, REVIEWED
from quantix.documents.models import Document
from quantix.estimate import records as estimate
from quantix.estimate.models import Markups, Rate
from quantix.office import records as office
from quantix.office.models import ENGINEER, TEAM, Staff
from quantix.subcontract import records as subcontract
from quantix.subcontract.models import Company, Enquiry, Package, Quote
from quantix.submission import records as submission
from quantix.submission.models import Draft, Requirement
from quantix.takeoff import records as takeoff
from quantix.takeoff.models import Measurement, Scale

# Records with a status of their own, the module that approves them, and what the queue calls them
REVIEWED_KINDS: dict[str, Any] = {
    "boq": (BoqItem, boq),
    "fact": (Fact, boq),
    "scale": (Scale, takeoff),
    "measurement": (Measurement, takeoff),
    "rate": (Rate, estimate),
    "markups": (Markups, estimate),
    "draft": (Draft, submission),
}
KIND_NAMES = {"boq": "BOQ lines", "fact": "facts", "scale": "scales", "measurement": "measurements", "rate": "rates"}
KIND_NAMES |= {"markups": "markups", "draft": "drafts", "checklist": "checklist items", "enquiry": "enquiries"}
KIND_NAMES |= {"recommendation": "quote recommendations"}


@dataclass
class Pending:
    kind: str
    record: Any
    producer: str  # the staff member who made it
    since: datetime  # when it came to the Manager

    @property
    def ref(self) -> str:
        return f"{self.kind} {self.record.id[:8]}"


def pending(session: Session, tender_id: str) -> list[Pending]:
    """Everything the staff proposed that the Tender Manager hasn't reviewed yet, oldest first."""
    found: list[Pending] = []
    for kind, (model, _) in REVIEWED_KINDS.items():
        query = select(model).where(
            model.tender_id == tender_id, model.status == PROPOSED, model.proposed_by != ENGINEER
        )
        found += [Pending(kind, r, r.proposed_by, r.created_at) for r in session.scalars(query)]
    requirements = select(Requirement).where(
        Requirement.tender_id == tender_id, Requirement.added_by != ENGINEER, Requirement.reviewed_by.is_(None)
    )
    found += [Pending("checklist", r, r.added_by, r.created_at) for r in session.scalars(requirements)]
    enquiries = (
        select(Enquiry)
        .join(Package, Enquiry.package_id == Package.id)
        .where(Package.tender_id == tender_id, Enquiry.status == "draft", Enquiry.reviewed_by.is_(None))
        .where(Enquiry.created_by != ENGINEER)
    )
    found += [Pending("enquiry", e, e.created_by, e.created_at) for e in session.scalars(enquiries)]
    recommended = select(Package).where(
        Package.tender_id == tender_id,
        Package.recommended_quote_id.is_not(None),
        Package.reviewed_by.is_(None),
        Package.selected_quote_id.is_(None),
    )
    found += [
        Pending("recommendation", p, p.recommended_by or "", p.recommended_at or p.created_at)
        for p in session.scalars(recommended)
    ]
    return sorted(found, key=lambda p: p.since)


def has_new(session: Session, tender_id: str, since: datetime | None) -> bool:
    """Whether anything came to the Manager for review after he last looked at his queue."""
    return any(since is None or p.since > since for p in pending(session, tender_id))


def counts(session: Session, tender_id: str) -> str:
    """The queue in a line: who is waiting on the Manager, with what."""
    names = _names(session, tender_id)
    by_producer: dict[str, dict[str, int]] = {}
    for p in pending(session, tender_id):
        kinds = by_producer.setdefault(names.get(p.producer, "Someone"), {})
        kinds[p.kind] = kinds.get(p.kind, 0) + 1
    return "; ".join(
        f"{name}: " + ", ".join(f"{n} {KIND_NAMES[k] if n > 1 else k}" for k, n in kinds.items())
        for name, kinds in by_producer.items()
    )


def _names(session: Session, tender_id: str) -> dict[str, str]:
    return {m.id: m.first_name for m in office.team(session, tender_id, include_released=True)}


def _where(session: Session, document_id: str | None, page: int | None) -> str:
    document = session.get(Document, document_id) if document_id else None
    return f"{document.name}, page {page}" if document else ""


def _amount(value: Decimal | None) -> str:
    return "-" if value is None else f"{value:,.3f}".rstrip("0").rstrip(".")


def describe(session: Session, p: Pending, names: dict[str, str]) -> str:
    """One line for the queue."""
    r, by = p.record, names.get(p.producer, "Someone")
    if p.kind == "boq":
        line = f"{boq.reference(r)}: {r.description[:70]} · {_amount(r.quantity)} {r.unit}"
    elif p.kind == "fact":
        line = f"{FACT_KINDS[r.kind]}: {r.value}"
    elif p.kind == "scale":
        ratio = takeoff.drawing_ratio(r.metres_per_point)
        line = f"scale of {_where(session, r.document_id, r.page)}: about 1:{ratio:,}"
    elif p.kind == "measurement":
        item = session.get(BoqItem, r.boq_item_id) if r.boq_item_id else None
        line = f"“{r.label}”: {_amount(takeoff.quantity(session, r))} {r.unit}"
        line += f" for {boq.reference(item)}" if item else " (not linked to a BOQ line)"
    elif p.kind == "rate":
        item = session.get(BoqItem, r.boq_item_id)
        how = f"build-up of {len(r.lines)} lines" if r.lines else "unit rate"
        line = f"{boq.reference(item)}: {estimate.rate_of(r)} per {item.unit} ({r.basis}, {how})"
    elif p.kind == "markups":
        prelims = sum((estimate.preliminary_cost(i) for i in r.preliminary_items), Decimal(0))
        line = f"preliminaries {prelims:,} in {len(r.preliminary_items)} items, overheads {r.overheads:.1%}, profit"
        line += f" {r.profit:.1%}"
    elif p.kind == "draft":
        line = f"“{r.title}” for “{session.get(Requirement, r.requirement_id).title}”"
    elif p.kind == "checklist":
        line = f"“{r.title}” ({r.section}) from {_where(session, r.document_id, r.page)}"
    elif p.kind == "enquiry":
        company = session.get(Company, r.company_id)
        line = f"to {company.name} for {session.get(Package, r.package_id).name}: “{r.subject}”"
    else:
        quote = session.get(Quote, r.recommended_quote_id)
        line = f"{session.get(Company, quote.company_id).name} for {r.name}: {r.recommendation}"
    return f"{p.ref} · by {by} · {line}"


def details(session: Session, p: Pending) -> str:
    """Everything the Manager needs to check the record against its source."""
    r = p.record
    if p.kind in ("boq", "fact"):
        return f"From {_where(session, r.document_id, r.page)}: “{r.quote}”"
    if p.kind == "scale":
        return (
            f"Set from “{r.dimension}” as {r.length_m} m between {r.line} (page points) on "
            f"{_where(session, r.document_id, r.page)}. Check it against the scale printed in the title block."
        )
    if p.kind == "measurement":
        text = f"A {r.kind} of {len(r.points)} points on {_where(session, r.document_id, r.page)}: {r.points}."
        if r.multiplier is not None:
            text += f" Multiplied by {r.multiplier} m."
        item = session.get(BoqItem, r.boq_item_id) if r.boq_item_id else None
        if item is not None:
            text += f" The BOQ has {_amount(item.quantity)} {item.unit} for {boq.reference(item)}."
        return text
    if p.kind == "rate":
        item = session.get(BoqItem, r.boq_item_id)
        text = f"{boq.reference(item)}: {item.description} · {_amount(item.quantity)} {item.unit}. Basis: {r.basis}."
        if r.lines:
            text += "\n" + "\n".join(
                f"- {line['kind']}: {line['resource']}, {line['quantity']} {line['unit']} at {line['rate']}"
                f"{' + ' + str(line['wastage']) + ' wastage' if Decimal(str(line.get('wastage', 0))) else ''}"
                f" = {estimate.line_cost(line)}"
                for line in r.lines
            )
        if r.quote:
            text += f"\nQuoted on {_where(session, r.document_id, r.page)}: “{r.quote}”"
        return text + f"\nNote: {r.note}"
    if p.kind == "markups":
        s = estimate.summary(session, r.tender_id)
        prelims = sum((estimate.preliminary_cost(i) for i in r.preliminary_items), Decimal(0))
        overheads = estimate.money((s.net + prelims) * r.overheads)
        profit = estimate.money((s.net + prelims + overheads) * r.profit)
        items = "\n".join(
            f"- {i['item']}: {i['quantity']} {i['unit']} × {i['rate']} = {estimate.preliminary_cost(i)}"
            for i in r.preliminary_items
        )
        return (
            f"On a net cost of {s.net:,}:\n{items}\nPreliminaries {prelims:,} ({prelims / s.net:.1%} of net), "
            f"overheads {overheads:,}, profit {profit:,}, adjustment {r.adjustment:,}; "
            f"price {s.net + prelims + overheads + profit + r.adjustment:,}.\nNote: {r.note}"
            if s.net
            else f"{items}\nNote: {r.note}"
        )
    if p.kind == "draft":
        return r.body[:6000]
    if p.kind == "checklist":
        return f"Required by {_where(session, r.document_id, r.page)}: “{r.quote}”"
    if p.kind == "enquiry":
        return f"Subject: {r.subject}\n{r.body[:4000]}"
    result = subcontract.level(session, r)
    return (
        "\n".join(
            f"{c.rank or '-'}. {c.company}: quoted {c.quoted_total}, exclusions {c.exclusions}, levelled "
            f"{c.levelled_total if c.levelled_total is not None else 'incomplete'}"
            for c in sorted(result.columns, key=lambda c: c.rank or 999)
        )
        + f"\nRecommendation: {r.recommendation}"
    )


def find(session: Session, tender_id: str, ref: str) -> Pending:
    """A record in the queue by its reference, e.g. "rate 4690fa4c"."""
    kind, _, short = ref.strip().partition(" ")
    for p in pending(session, tender_id):
        if p.kind == kind.lower() and short.strip() and p.record.id.startswith(short.strip().lower()):
            return p
    raise ValueError("nothing with that reference is waiting for your review; use review_queue")


class Verdict(BaseModel):
    """The Manager's decision on one record in his queue."""

    record: str = Field(description='The record as review_queue shows it, e.g. "rate 4690fa4c"')
    accept: bool = Field(description="true to accept it, false to send it back to whoever made it")
    note: str = Field(description="Accepting: what you checked. Sending back: exactly what to correct and how")


def review(session: Session, tender_id: str, manager: Staff, verdicts: list[Verdict], autonomous: bool) -> str:
    """Apply the Manager's verdicts. One that can't be applied is reported, the others still count."""
    accepted, sent_back, problems = 0, 0, []
    for verdict in verdicts:
        try:
            if len(verdict.note.split()) < 3:
                raise ValueError("say in the note what you checked, or what to correct")
            p = find(session, tender_id, verdict.record)
            if verdict.accept:
                _accept(session, p, manager, verdict.note.strip(), autonomous)
                accepted += 1
            else:
                _send_back(session, tender_id, p, manager, verdict.note.strip())
                sent_back += 1
        except ValueError as error:
            problems.append(f"{verdict.record}: {error}")
    where = "approved by the office" if autonomous else "waiting for the engineer"
    report = f"Accepted {accepted} ({where}). Sent back {sent_back}."
    return report + ("\nNot done: " + "; ".join(problems) if problems else "")


def _accept(session: Session, p: Pending, manager: Staff, note: str, autonomous: bool) -> None:
    r = p.record
    r.reviewed_by, r.reviewed_at, r.review_note = manager.id, datetime.now(UTC), note
    if p.kind in REVIEWED_KINDS:
        module = REVIEWED_KINDS[p.kind][1]
        if autonomous:
            module.approve(session, r, "office_approved")
        else:
            r.status = REVIEWED
    elif p.kind == "recommendation" and autonomous:
        subcontract.select_quote(session, r, session.get(Quote, r.recommended_quote_id), status="office_approved")


def _send_back(session: Session, tender_id: str, p: Pending, manager: Staff, note: str) -> None:
    r = p.record
    if p.kind in REVIEWED_KINDS:
        module = REVIEWED_KINDS[p.kind][1]
        office.send_back(session, tender_id, r, module.label(session, r), note, manager.id)
        return
    person = session.get(Staff, p.producer)
    to = f"{person.first_name}, " if person else ""
    if p.kind == "checklist":
        what = f"the checklist item “{r.title}”; I removed it"
        session.delete(r)
    elif p.kind == "enquiry":
        what = f"the enquiry “{r.subject}”"
        r.status, r.reviewed_by, r.reviewed_at, r.review_note = "rejected", manager.id, datetime.now(UTC), note
    else:
        what = f"your recommendation for {r.name}"
        r.recommended_quote_id = r.recommendation = r.recommended_by = r.recommended_at = None
    office.post(session, tender_id, manager.id, TEAM, f"{to}I sent back {what}: {note}")


def reopen(session: Session, kind: str, record_id: str, reason: str) -> Any:
    """The engineer sends back something already approved, so the office does it again."""
    if kind not in REVIEWED_KINDS:
        raise ValueError("Only BOQ lines, facts, scales, measurements, rates, markups and drafts can be reopened.")
    model, module = REVIEWED_KINDS[kind]
    record = session.get(model, record_id)
    if record is None:
        raise LookupError("Not found.")
    if record.status not in APPROVED:
        raise ValueError("Only approved work can be reopened; send back what is still waiting instead.")
    office.send_back(session, record.tender_id, record, module.label(session, record), reason.strip(), ENGINEER)
    return record
