"""The tender audit before release: what the tender as a whole still lacks, and every open finding on the office's
work. A blocker keeps the tender from being ready; a warning needs the Tender Manager's reason. The Manager clears
the audit before he tells the engineer the tender is ready, and the engineer sees it before building the package."""

import hashlib
import re
from collections import Counter, defaultdict
from decimal import Decimal, InvalidOperation
from pathlib import Path

from pydantic import BaseModel, Field
from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix.boq import records as boq
from quantix.boq.models import FACT_KINDS, BoqItem
from quantix.core.review import APPROVED, LIVE, REVIEWED
from quantix.documents import library
from quantix.documents.models import Document
from quantix.estimate import records as estimate
from quantix.office import records as office
from quantix.office.models import Staff
from quantix.review import checks
from quantix.review import records as reviews
from quantix.review.checks import BLOCKER, WARNING, Finding, Ref
from quantix.review.models import Acceptance
from quantix.subcontract import records as subcontract
from quantix.submission import records as submission
from quantix.takeoff import records as takeoff

PRICE_FACTS = ("currency", "vat")  # the price can't be stated without them
AREAS = {
    "boq": "BOQ lines",
    "facts": "tender facts",
    "takeoff": "takeoff marks",
    "pricing": "prices",
    "subcontract": "quote choices",
    "submission": "drafts",
}
_CELL = re.compile(r"([A-Z]{1,3})(\d+)=(.*)")


def short(key: str) -> str:
    """A finding's short name, for the Manager to accept a warning by."""
    return hashlib.sha1(key.encode()).hexdigest()[:6]


def findings(session: Session, home: Path, tender_id: str) -> list[Finding]:
    """Everything the audit finds, blockers first, each once."""
    found = [
        *_waiting(session, tender_id),
        *_basics(session, tender_id),
        *_missing_rows(session, tender_id),
        *_documents(session, tender_id),
        *_records(session, home, tender_id),
    ]
    unique = list({f.key: f for f in found}.values())
    return sorted(unique, key=lambda f: f.severity != BLOCKER)


def open_findings(session: Session, home: Path, tender_id: str) -> list[Finding]:
    """The blockers, and the warnings the Manager hasn't accepted."""
    settled = reviews.accepted(session, tender_id)
    return [f for f in findings(session, home, tender_id) if f.severity == BLOCKER or f.key not in settled]


def _plural(n: int, one: str, many: str) -> str:
    return f"{n} {one if n == 1 else many}"


def _waiting(session: Session, tender_id: str) -> list[Finding]:
    found = []
    with_engineer = reviews.escalated(session, tender_id)
    queue = [p for p in reviews.pending(session, tender_id) if p.record.id not in with_engineer]
    if queue:
        found.append(
            Finding(
                "waiting:manager",
                BLOCKER,
                f"{_plural(len(queue), 'piece of work waits', 'pieces of work wait')} for the Tender Manager's review.",
            )
        )
    gates = {
        **boq.waiting_counts(session, tender_id),
        "takeoff": takeoff.waiting(session, tender_id),
        "pricing": estimate.waiting(session, tender_id),
        "subcontract": subcontract.waiting(session, tender_id),
        "submission": submission.waiting(session, tender_id),
    }
    if any(gates.values()):
        listed = ", ".join(f"{n} {AREAS[area]}" for area, n in gates.items() if n)
        found.append(Finding("waiting:engineer", BLOCKER, f"Waiting for the engineer's approval: {listed}."))
    questions = office.decisions(session, tender_id, waiting_only=True)
    if questions:
        titles = "; ".join(f"“{d.title}”" for d in questions[:5])
        found.append(
            Finding(
                "waiting:decisions",
                BLOCKER,
                f"{_plural(len(questions), 'decision waits', 'decisions wait')} for the engineer's answer: {titles}.",
            )
        )
    return found


def _basics(session: Session, tender_id: str) -> list[Finding]:
    """The price's facts and markups, a rate for every line with a quantity, and a checklist that is ready."""
    found = []
    recorded = {f.kind for f in boq.facts(session, tender_id)}
    for kind, name in FACT_KINDS.items():
        if kind not in recorded:
            severity = BLOCKER if kind in PRICE_FACTS else WARNING
            found.append(Finding(f"no-fact:{kind}", severity, f"{name} isn't recorded from the tender documents."))
    if estimate.current_markups(session, tender_id) is None:
        found.append(Finding("no-markups", BLOCKER, "There are no markups: no preliminaries, overheads or profit."))
    unpriced = [i for i in boq.items(session, tender_id) if i.quantity and estimate.current_rate(session, i.id) is None]
    if unpriced:
        found.append(
            Finding(
                "unpriced",
                BLOCKER,
                f"{_plural(len(unpriced), 'BOQ line with a quantity has', 'BOQ lines with a quantity have')} no rate: "
                + ", ".join(boq.reference(i) for i in unpriced[:15])
                + (" …" if len(unpriced) > 15 else ""),
            )
        )
    checklist = submission.requirements(session, tender_id)
    if not checklist:
        found.append(Finding("no-checklist", BLOCKER, "The submission checklist is empty."))
    missing = [r for r in checklist if submission.state(session, r) != "ready"]
    if missing:
        found.append(
            Finding(
                "checklist",
                BLOCKER,
                f"{_plural(len(missing), 'checklist item isn’t', 'checklist items aren’t')} ready: "
                + "; ".join(r.title for r in missing[:10]),
            )
        )
    return found


def _cells(line: str) -> tuple[int | None, dict[str, str]]:
    """A workbook row as the reader wrote it: "A12=3.1 | B12=Excavation | C12=m3 | D12=1240"."""
    row, cells = None, {}
    for part in line.split(" | "):
        match = _CELL.fullmatch(part.strip())
        if match:
            row, cells[match.group(1)] = int(match.group(2)), match.group(3).strip()
    return row, cells


def _number(text: str) -> Decimal | None:
    try:
        return Decimal(text.replace(",", ""))
    except InvalidOperation:
        return None


def _column(items: list[BoqItem], rows: dict[int, dict[str, str]], same) -> str | None:
    """The column that holds, on the rows already entered, the value `same` looks for."""
    votes: Counter[str] = Counter()
    for item in items:
        for column, value in rows.get(submission.row_of(item.quote) or 0, {}).items():
            if same(item, value):
                votes[column] += 1
    return votes.most_common(1)[0][0] if votes else None


def _missing_rows(session: Session, tender_id: str) -> list[Finding]:
    """Rows of the client's BOQ workbooks with a unit and a quantity that aren't in the BOQ, so they would go back
    unpriced. The quantity and unit columns are the ones the lines already entered use."""
    query = select(BoqItem).where(BoqItem.tender_id == tender_id, BoqItem.status.in_((*LIVE, "withdrawn")))
    sheets: dict[tuple[str, int], list[BoqItem]] = defaultdict(list)
    for item in session.scalars(query):
        if submission.row_of(item.quote) is not None:
            sheets[(item.document_id, item.page)].append(item)
    found = []
    for (document_id, page), items in sheets.items():
        document, text = session.get(Document, document_id), library.page(session, document_id, page)
        if document is None or document.kind != "spreadsheet" or text is None:
            continue
        rows = {}
        for line in text.text.splitlines():
            row, cells = _cells(line)
            if row is not None:
                rows[row] = cells
        quantity = _column(items, rows, lambda i, v: i.quantity is not None and _number(v) == i.quantity)
        unit = _column(items, rows, lambda i, v: v == i.unit.strip())
        if quantity is None or unit is None:
            continue
        entered = {submission.row_of(i.quote) for i in items}
        missing = []
        for number, cells in sorted(rows.items()):
            amount = _number(cells.get(quantity, ""))
            if number not in entered and amount and amount > 0 and cells.get(unit):
                words = " · ".join(v for c, v in cells.items() if c not in (quantity, unit))
                missing.append(f"row {number} ({words[:60]}: {cells[quantity]} {cells[unit]})")
        if missing:
            found.append(
                Finding(
                    f"missing-rows:{document_id}:{page}",
                    BLOCKER,
                    (
                        "A row of the client's BOQ has a quantity but isn't in the BOQ, so it would go back unpriced: "
                        if len(missing) == 1
                        else f"{len(missing)} rows of the client's BOQ have a quantity but aren't in the BOQ, so they "
                        "would go back unpriced: "
                    )
                    + "; ".join(missing[:8])
                    + (" …" if len(missing) > 8 else ""),
                    [Ref(f"{document.name}, page {page}", document_id, page)],
                )
            )
    return found


def _documents(session: Session, tender_id: str) -> list[Finding]:
    current = [d for d in library.documents(session, tender_id) if d.status != "replaced"]
    found = []
    reading = [d for d in current if d.status in ("waiting", "reading")]
    if reading:
        found.append(
            Finding(
                "reading",
                BLOCKER,
                f"{_plural(len(reading), 'document is', 'documents are')} still being read: "
                + ", ".join(d.name for d in reading[:10]),
            )
        )
    unread = [d for d in current if d.status in ("unreadable", "failed")]
    if unread:
        found.append(
            Finding(
                "unread:" + ",".join(sorted(d.id for d in unread)),
                WARNING,
                f"Quantix couldn't read {_plural(len(unread), 'document', 'documents')}, so the office hasn't seen "
                "what they say: " + "; ".join(d.name + (f" ({d.note})" if d.note else "") for d in unread[:10]),
            )
        )
    return found


def _records(session: Session, home: Path, tender_id: str) -> list[Finding]:
    """What the checks find on work already past the Manager's review. The engineer's own approval settles a
    warning; a blocker stands whoever approved the work."""
    found = []
    for kind, (model, module) in reviews.REVIEWED_KINDS.items():
        query = select(model).where(model.tender_id == tender_id, model.status.in_((REVIEWED, *APPROVED)))
        for record in session.scalars(query):
            for f in checks.for_record(session, home, kind, record, blockers_only=record.status == "approved"):
                label = module.label(session, record)
                found.append(Finding(f.key, f.severity, f"{label[0].upper()}{label[1:]}: {f.message}", f.refs))
    return found


class Accepted(BaseModel):
    """The Manager's reason for accepting one of the audit's warnings."""

    finding: str = Field(description="The warning's short name as audit_tender shows it, e.g. 3fa9c2")
    reason: str = Field(description="Why it needs no correction")


def accept(session: Session, home: Path, tender_id: str, manager: Staff, accepted: list[Accepted]) -> list[str]:
    """Keep the Manager's reasons for the warnings he accepts. Returns what couldn't be accepted."""
    warnings = {short(f.key): f for f in open_findings(session, home, tender_id) if f.severity == WARNING}
    listed = "; ".join(f"{name} ({f.message[:70]})" for name, f in warnings.items()) or "none"
    problems = []
    for a in accepted:
        finding = _named(a.finding, warnings)
        if finding is None:
            problems.append(f"{a.finding}: no open warning has that name. The open warnings are: {listed}")
        elif len(a.reason.split()) < 3:
            problems.append(f"{a.finding}: say why it needs no correction")
        else:
            session.add(
                Acceptance(tender_id=tender_id, key=finding.key, reason=a.reason.strip(), accepted_by=manager.id)
            )
    session.flush()
    return problems


def _named(text: str, warnings: dict[str, Finding]) -> Finding | None:
    """The warning a name points at: its short name anywhere in the text, or words only one warning contains."""
    token = re.search(r"\b[0-9a-f]{6}\b", text.lower())
    if token and token.group(0) in warnings:
        return warnings[token.group(0)]
    words = text.strip().lower()
    matches = [f for f in warnings.values() if words and words in f.message.lower()]
    return matches[0] if len(matches) == 1 else None


def report(found: list[Finding]) -> str:
    """The audit for the Manager."""
    if not found:
        return "The audit is clear: nothing blocks the release. Tell the engineer the tender is ready to build."
    blockers = sum(f.severity == BLOCKER for f in found)
    lines = []
    for f in found:
        where = "; ".join(r.label for r in f.refs)
        name = "BLOCKER" if f.severity == BLOCKER else f"WARNING, short name {short(f.key)}"
        lines.append(f"- {name}: {f.message}" + (f" ({where})" if where else ""))
    return (
        f"The audit found {_plural(blockers, 'blocker', 'blockers')} and "
        f"{_plural(len(found) - blockers, 'warning', 'warnings')}:\n"
        + "\n".join(lines)
        + "\nClear every blocker: send work back, assign it or escalate it. Accept a warning only with your reason, "
        'as accept_warnings=[{"finding": "<short name>", "reason": "why it needs no correction"}].'
    )
