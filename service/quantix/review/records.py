"""The Tender Manager's review. Everything the staff propose comes to him before it reaches the engineer; he accepts
it or sends it back to whoever made it. In a fully autonomous office his acceptance approves it. Quantix checks each
record first: he can't accept one with a blocker, and accepts a warning only with his reason. What the office can't
settle, he escalates to the engineer with where it shows and his suggested corrections."""

from dataclasses import dataclass
from datetime import UTC, datetime
from decimal import Decimal
from pathlib import Path
from typing import Any

from pydantic import BaseModel, Field
from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix.boq import records as boq
from quantix.boq.models import FACT_KINDS, BoqItem, Fact
from quantix.core.review import APPROVED, PROPOSED, REVIEWED
from quantix.documents import library
from quantix.documents.models import Document
from quantix.estimate import records as estimate
from quantix.estimate.models import Markups, Rate
from quantix.office import records as office
from quantix.office.models import ENGINEER, TEAM, Decision, Staff
from quantix.review import checks
from quantix.review.models import Acceptance
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
CHECKED = {kind: model for kind, (model, _) in REVIEWED_KINDS.items()} | {"recommendation": Package}


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


def escalation(session: Session, record_id: str) -> Decision | None:
    """The engineer's open decision on a record the Manager escalated."""
    query = select(Decision).where(Decision.subject_id == record_id, Decision.status == "waiting")
    return session.scalars(query).first()


def escalated(session: Session, tender_id: str) -> set[str]:
    query = select(Decision.subject_id).where(Decision.tender_id == tender_id, Decision.status == "waiting")
    return {i for i in session.scalars(query) if i}


def has_new(session: Session, tender_id: str, since: datetime | None) -> bool:
    """Whether anything came to the Manager for review after he last looked at his queue."""
    return any(since is None or p.since > since for p in pending(session, tender_id))


def counts(session: Session, tender_id: str) -> str:
    """The queue in a line: who is waiting on the Manager, with what."""
    names, with_engineer = _names(session, tender_id), escalated(session, tender_id)
    by_producer: dict[str, dict[str, int]] = {}
    for p in pending(session, tender_id):
        if p.record.id in with_engineer:
            continue  # escalated: waiting for the engineer's answer
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
    if escalation(session, r.id):
        line += " · escalated: waiting for the engineer's answer"
    return f"{p.ref} · by {by} · {line}"


def accepted(session: Session, tender_id: str) -> dict[str, Acceptance]:
    """The warnings the Manager accepted, by their key."""
    return {a.key: a for a in session.scalars(select(Acceptance).where(Acceptance.tender_id == tender_id))}


def findings(session: Session, home: Path, tender_id: str, p: Pending) -> list[checks.Finding]:
    """What Quantix's checks find in a record in the queue, less the warnings the Manager already accepted."""
    settled = accepted(session, tender_id)
    return [f for f in checks.for_record(session, home, p.kind, p.record) if f.key not in settled]


def flags(found: list[checks.Finding]) -> str:
    """The findings in a few words, for the queue."""
    blockers = sum(f.severity == checks.BLOCKER for f in found)
    warnings = len(found) - blockers
    parts = [f"{blockers} blocker{'s' if blockers > 1 else ''}"] if blockers else []
    parts += [f"{warnings} warning{'s' if warnings > 1 else ''}"] if warnings else []
    return f" · Quantix found {' and '.join(parts)}" if parts else ""


def findings_text(found: list[checks.Finding]) -> str:
    """The findings, for review_details."""
    if not found:
        return ""
    lines = []
    for f in found:
        where = "; ".join(r.label for r in f.refs)
        lines.append(f"- {f.severity.upper()}: {f.message}" + (f" ({where})" if where else ""))
    return "\nQuantix's checks:\n" + "\n".join(lines)


def record_findings(session: Session, home: Path, kind: str, record_id: str) -> list[tuple[Any, Any]]:
    """What the checks find in a record now, each with the Manager's acceptance if he accepted it."""
    model = CHECKED.get(kind)
    record = session.get(model, record_id) if model else None
    if record is None:
        raise LookupError("Not found.")
    settled = accepted(session, record.tender_id)
    return [(f, settled.get(f.key)) for f in checks.for_record(session, home, kind, record)]


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
    warnings_reason: str | None = Field(
        default=None,
        description="Accepting a record Quantix warned about: why each warning doesn't need correcting",
    )


def review(
    session: Session, home: Path, tender_id: str, manager: Staff, verdicts: list[Verdict], autonomous: bool
) -> str:
    """Apply the Manager's verdicts. One that can't be applied is reported, the others still count."""
    accepted_n, sent_back, problems = 0, 0, []
    for verdict in verdicts:
        try:
            if len(verdict.note.split()) < 3:
                raise ValueError("say in the note what you checked, or what to correct")
            p = find(session, tender_id, verdict.record)
            if escalation(session, p.record.id):
                raise ValueError("you escalated it to the engineer; apply their answer when it comes")
            if verdict.accept:
                _accept(session, home, tender_id, p, manager, verdict, autonomous)
                accepted_n += 1
            else:
                _send_back(session, tender_id, p, manager, verdict.note.strip())
                sent_back += 1
        except ValueError as error:
            problems.append(f"{verdict.record}: {error}")
    where = "approved by the office" if autonomous else "waiting for the engineer"
    report = f"Accepted {accepted_n} ({where}). Sent back {sent_back}."
    return report + ("\nNot done:\n" + "\n".join(problems) if problems else "")


def _accept(
    session: Session, home: Path, tender_id: str, p: Pending, manager: Staff, verdict: Verdict, autonomous: bool
) -> None:
    found = findings(session, home, tender_id, p)
    blockers = [f.message for f in found if f.severity == checks.BLOCKER]
    if blockers:
        raise ValueError(
            "Quantix's checks stop it: " + " ".join(blockers) + " Send it back saying exactly what to correct."
        )
    reason = (verdict.warnings_reason or "").strip()
    if found and len(reason.split()) < 3:
        raise ValueError(
            "Quantix warns: " + " ".join(f.message for f in found) + " Accept it with warnings_reason saying why "
            "each is acceptable, or send it back with the correction."
        )
    for f in found:
        session.add(Acceptance(tender_id=tender_id, key=f.key, reason=reason, accepted_by=manager.id))
    r = p.record
    r.reviewed_by, r.reviewed_at, r.review_note = manager.id, datetime.now(UTC), verdict.note.strip()
    if p.kind in REVIEWED_KINDS:
        module = REVIEWED_KINDS[p.kind][1]
        if autonomous:
            module.approve(session, r, "office_approved")
        else:
            r.status = REVIEWED
    elif p.kind == "recommendation" and autonomous:
        subcontract.select_quote(session, r, session.get(Quote, r.recommended_quote_id), status="office_approved")


def _same_work(kind: str, record: Any) -> Any:
    """What finds earlier versions of the same work: the same line, sheet, requirement or fact."""
    if kind == "rate":
        return Rate.boq_item_id == record.boq_item_id
    if kind == "measurement":
        where = (Measurement.document_id == record.document_id) & (Measurement.page == record.page)
        if record.boq_item_id:
            return where & (Measurement.boq_item_id == record.boq_item_id)
        return where & (Measurement.label == record.label)
    if kind == "draft":
        return Draft.requirement_id == record.requirement_id
    if kind == "fact":
        return Fact.kind == record.kind
    if kind == "boq":
        return (BoqItem.section == record.section) & (BoqItem.item == record.item)
    if kind == "scale":
        return (Scale.document_id == record.document_id) & (Scale.page == record.page)
    return Markups.id.is_not(None)  # the markups: one set per tender


def _sent_back_before(session: Session, tender_id: str, p: Pending) -> list[str]:
    """Why the same work was sent back before, oldest first."""
    if p.kind not in REVIEWED_KINDS:
        return []
    model = REVIEWED_KINDS[p.kind][0]
    query = select(model).where(
        model.tender_id == tender_id, model.status == "rejected", model.id != p.record.id, _same_work(p.kind, p.record)
    )
    return [r.reason or "" for r in session.scalars(query.order_by(model.created_at))]


def _send_back(session: Session, tender_id: str, p: Pending, manager: Staff, note: str) -> None:
    r = p.record
    earlier = _sent_back_before(session, tender_id, p)
    answered = select(Decision.id).where(Decision.subject_id == r.id, Decision.status == "answered")
    if len(earlier) >= 2 and session.scalars(answered).first() is None:
        raise ValueError(
            f"it has been sent back {len(earlier)} times already and still isn't right (last: {earlier[-1]}). "
            "Escalate it to the engineer with escalate: the problem, where it shows and your suggested corrections"
        )
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


class Source(BaseModel):
    """Where a problem shows, for the engineer to open: a page of a document, or a BOQ line."""

    document_id: str | None = Field(default=None, description="A document, with its page")
    page: int | None = None
    boq_item: str | None = Field(default=None, description='Or a BOQ line as list_boq shows it, e.g. "8486 · C.1.2"')
    what: str = Field(description="What the engineer will find there, in a few words")


def _source(session: Session, tender_id: str, source: Source) -> dict[str, Any]:
    what = source.what.strip()
    if source.document_id:
        document = session.get(Document, source.document_id)
        if document is None or document.tender_id != tender_id or not source.page:
            raise ValueError(f"{what}: no document of this tender has that id; use list_documents")
        if library.page(session, document.id, source.page) is None:
            raise ValueError(f"{what}: {document.name} has no page {source.page}")
        label = f"{document.name}, page {source.page}: {what}"
        return {"label": label, "document_id": document.id, "page": source.page}
    if source.boq_item:
        item = boq.find_item(session, tender_id, source.boq_item)
        return {"label": f"BOQ line {boq.reference(item)}: {what}", "boq_item_id": item.id}
    raise ValueError(f"{what}: give a document and its page, or a BOQ line")


def _own_sources(session: Session, p: Pending) -> list[dict[str, Any]]:
    """Where the record itself comes from: its BOQ line and its page."""
    r, found = p.record, []
    item_id = r.id if p.kind == "boq" else getattr(r, "boq_item_id", None)
    item = session.get(BoqItem, item_id) if item_id else None
    if item is not None:
        found.append({"label": f"BOQ line {boq.reference(item)}", "boq_item_id": item.id})
    where = session.get(Requirement, r.requirement_id) if p.kind == "draft" else r
    document_id, page = getattr(where, "document_id", None), getattr(where, "page", None)
    document = session.get(Document, document_id) if document_id and page else None
    if document is not None:
        found.append({"label": f"{document.name}, page {page}", "document_id": document.id, "page": page})
    return found


def _title(session: Session, p: Pending) -> str:
    r = p.record
    if p.kind in REVIEWED_KINDS:
        text = REVIEWED_KINDS[p.kind][1].label(session, r)
    elif p.kind == "checklist":
        text = f"the checklist item “{r.title}”"
    elif p.kind == "enquiry":
        text = f"the enquiry “{r.subject}”"
    else:
        text = f"the recommendation for {r.name}"
    return text[0].upper() + text[1:]


def escalate(
    session: Session,
    tender_id: str,
    manager: Staff,
    ref: str,
    problem: str,
    sources: list[Source],
    suggestions: list[str],
) -> Decision:
    """A decision for the engineer on a record the office couldn't settle. The record stays in the Manager's queue
    until the engineer answers; the answer goes to his chat for him to apply."""
    p = find(session, tender_id, ref)
    if escalation(session, p.record.id):
        raise ValueError("You already escalated it. The engineer's answer will come to your chat.")
    if len(problem.split()) < 5:
        raise ValueError("Say what is wrong and why the office can't settle it.")
    if not sources:
        raise ValueError("Show the engineer where the problem is: at least one document page or BOQ line.")
    suggestions = [s.strip() for s in suggestions if s.strip()]
    if not 1 <= len(suggestions) <= 4:
        raise ValueError("Suggest 1 to 4 corrections for the engineer to choose from, each complete enough to act on.")
    shown = _own_sources(session, p) + [_source(session, tender_id, s) for s in sources]
    decision = Decision(
        tender_id=tender_id,
        raised_by=manager.id,
        title=_title(session, p),
        text=problem.strip(),
        options=suggestions,
        subject_kind=p.kind,
        subject_id=p.record.id,
        sources=shown,
    )
    session.add(decision)
    session.flush()
    return decision
