"""Looking up the office's own work: any record by its reference, with what it rests on, who made and decided it and
the versions before it; the office's records by words; and the priced BOQ. Quantix computes every figure shown."""

import re
from decimal import Decimal
from pathlib import Path
from typing import Any

from sqlalchemy import select
from sqlalchemy.orm import Session

from quantix.boq import records as boq
from quantix.boq.models import FACT_KINDS, BoqItem, Fact
from quantix.core.review import APPROVED, UNDECIDED
from quantix.documents import library
from quantix.documents.arabic import searchable
from quantix.documents.models import Document, WebPage
from quantix.estimate import records as estimate
from quantix.estimate.models import Markups, Rate
from quantix.office import records as office
from quantix.office.models import ENGINEER, Opened, Task
from quantix.review import queries
from quantix.review import records as reviews
from quantix.review.models import TenderQuery
from quantix.subcontract import records as subcontract
from quantix.subcontract.models import Company, Enquiry, Package, Quote
from quantix.submission import records as submission
from quantix.submission.models import Draft, Requirement
from quantix.takeoff import drawings
from quantix.takeoff import records as takeoff
from quantix.takeoff.models import LayerMap, Measurement, Scale

MODELS: dict[str, Any] = {
    "boq": BoqItem,
    "fact": Fact,
    "scale": Scale,
    "measurement": Measurement,
    "layers": LayerMap,
    "query": TenderQuery,
    "rate": Rate,
    "markups": Markups,
    "draft": Draft,
    "checklist": Requirement,
    "package": Package,
    "enquiry": Enquiry,
    "quote": Quote,
}
STATES = {
    "proposed": "with the Tender Manager for review",
    "reviewed": "accepted by the Tender Manager, waiting for the engineer",
    "approved": "approved by the engineer",
    "office_approved": "approved by the office, not reviewed by the engineer",
    "rejected": "sent back",
    "withdrawn": "withdrawn",
    "replaced": "replaced by a newer version",
}
SHORT = {
    "proposed": "with the Manager",
    "reviewed": "waiting for the engineer",
    "approved": "approved",
    "office_approved": "approved by the office",
}
PAGE = 60  # priced BOQ lines at a time
# Summaries Quantix computes, which an answer may rest on once the person has called them
SUMMARIES = {
    "estimate_summary": "The estimate as Quantix computes it",
    "priced_boq": "The priced BOQ",
    "list_boq": "The BOQ",
    "takeoff_summary": "The takeoff against the BOQ",
    "find_problems": "What Quantix's checks find in the drawings and the BOQ",
    "list_requirements": "The submission checklist",
    "audit_tender": "The audit of the tender",
    "what_changed": "What changed in the office",
    "coverage": "How much of the package the office has read",
    "list_packages": "The subcontract and supply packages",
    "price_breakdown": "Where the money is, as Quantix computes it",
    "what_if": "The price with the changes asked about, as Quantix computes it",
}
_PAGE_REF = re.compile(r"^(?P<name>.+?),?\s+page\s+(?P<page>\d+)$", re.IGNORECASE)
_ID = re.compile(r"[0-9a-f]{4,32}")


def _amount(value: Decimal | None) -> str:
    return "-" if value is None else f"{value:,.3f}".rstrip("0").rstrip(".")


def _names(session: Session, tender_id: str) -> dict[str, str]:
    names = {m.id: m.first_name for m in office.team(session, tender_id, include_released=True)}
    names[ENGINEER] = "the engineer"
    return names


def _tender_of(session: Session, record: Any) -> str:
    if isinstance(record, Enquiry | Quote):
        return session.get(Package, record.package_id).tender_id
    return record.tender_id


def find(session: Session, tender_id: str, ref: str) -> tuple[str, Any]:
    """A record by its reference ("rate 42a9fb15", "markups") or a BOQ line by its number ("Earthwork / C.1.2"). A
    whole line from find_records is taken by the reference it starts with."""
    text = ref.split(" · ")[0].strip()
    kind, _, short = text.partition(" ")
    kind = {"recommendation": "package", "requirement": "checklist"}.get(kind.lower(), kind.lower())
    short = short.strip().lower()
    if kind == "markups" and not short:
        markups = estimate.current_markups(session, tender_id) or returned_markups(session, tender_id)
        if markups is None:
            raise ValueError("No markups have been proposed yet.")
        return kind, markups
    if kind in MODELS and _ID.fullmatch(short):
        model = MODELS[kind]
        for record in session.scalars(select(model).where(model.id.startswith(short))):
            if _tender_of(session, record) == tender_id:
                return kind, record
        raise ValueError(f"There is no {kind} {short} on this tender. Use find_records to look it up by words.")
    try:
        return "boq", boq.find_item(session, tender_id, text)
    except ValueError as error:
        raise ValueError(
            f"“{text}” is neither a record like “rate 42a9fb15” nor a BOQ line like “C.1.2”: {error}"
        ) from error


def returned_markups(session: Session, tender_id: str) -> Markups | None:
    """The markups sent back last, when nothing has replaced them: what to correct. Sent back last, not made last:
    reopening approved markups sends back an older set than a duplicate turned down the day before."""
    if estimate.current_markups(session, tender_id) is not None:
        return None
    query = select(Markups).where(Markups.tender_id == tender_id, Markups.status == "rejected")
    return session.scalars(query.order_by(Markups.decided_at.desc())).first()


def returned_draft(session: Session, requirement: Requirement) -> Draft | None:
    """The checklist item's newest draft when it was sent back and nothing has replaced it: the text to correct."""
    if submission.current_draft(session, requirement.id) is not None:
        return None
    query = select(Draft).where(Draft.requirement_id == requirement.id, Draft.status == "rejected")
    return session.scalars(query.order_by(Draft.created_at.desc())).first()


def state(session: Session, kind: str, r: Any) -> str:
    if kind == "checklist":
        returned = returned_draft(session, r)
        if returned is not None and submission.state(session, r) == "missing":
            return f"its draft was sent back: open draft {returned.id[:8]} for the text to correct"
        return {
            "ready": "ready",
            "review": "its draft is waiting for the engineer",
            "manager": "its draft is with the Tender Manager",
            "missing": "nothing drafted or attached yet",
        }[submission.state(session, r)]
    if kind == "enquiry":
        return {"draft": "a draft", "sent": "sent", "rejected": "sent back"}.get(r.status, r.status)
    if kind == "package":
        if r.selected_quote_id:
            return "the engineer chose a quote"
        if r.recommended_quote_id:
            return "recommendation waiting for the engineer" if r.reviewed_by else "recommendation with the Manager"
        return "no quote recommended yet"
    if kind == "quote":
        return "recorded"
    return STATES.get(r.status, r.status)


def _pending(kind: str, r: Any) -> reviews.Pending:
    producer = getattr(r, "proposed_by", None) or getattr(r, "added_by", None) or getattr(r, "created_by", "")
    return reviews.Pending("recommendation" if kind == "package" else kind, r, producer, r.created_at)


def _made(kind: str, r: Any, names: dict[str, str]) -> list[str]:
    """Who made it, what the Tender Manager said, and what the engineer decided."""
    maker = getattr(r, "proposed_by", None) or getattr(r, "added_by", None) or getattr(r, "created_by", None)
    lines = [f"Made by {names.get(maker, 'someone')} on {r.created_at:%d %b %Y}."] if maker else []
    if getattr(r, "reviewed_by", None):
        who = names.get(r.reviewed_by, "The Tender Manager")
        who = who[0].upper() + who[1:]
        sent_back = getattr(r, "status", None) == "rejected" and not getattr(r, "decided_at", None)
        note = f": {r.review_note}" if r.review_note else "."
        lines.append(f"{who} {'sent it back' if sent_back else 'accepted it'}{note}")
    decided = getattr(r, "decided_at", None)
    if decided and kind != "package":
        if r.status == "office_approved":
            lines.append(f"Approved by the office on {decided:%d %b %Y}, without the engineer's review.")
        elif r.status in APPROVED:
            lines.append(f"The engineer approved it on {decided:%d %b %Y}.")
        elif r.status == "rejected":
            lines.append(f"The engineer sent it back on {decided:%d %b %Y}: {r.reason}")
    return lines


def _brief(session: Session, kind: str, r: Any) -> str:
    """A version in a few words, for the history."""
    if kind == "rate":
        return f"{estimate.rate_of(r)} per unit ({r.basis})"
    if kind == "boq":
        return f"{_amount(r.quantity)} {r.unit}"
    if kind == "measurement":
        return f"{_amount(takeoff.quantity(session, r))} {r.unit}"
    if kind == "fact":
        return r.value
    if kind == "draft":
        return f"“{r.title}”"
    if kind == "scale":
        document = session.get(Document, r.document_id)
        if document is not None and document.kind == "cad":
            return drawings.unit_name(r.metres_per_point)
        return f"about 1:{takeoff.drawing_ratio(r.metres_per_point):,}"
    if kind == "layers":
        return f"{len(r.layers)} layers, {len(r.blocks)} blocks"
    if kind == "query":
        return f"“{r.title}”"
    return f"overheads {r.overheads:.1%}, profit {r.profit:.1%}"


def _history(session: Session, tender_id: str, kind: str, r: Any) -> list[str]:
    if kind not in reviews.REVIEWED_KINDS:
        return []
    model = reviews.REVIEWED_KINDS[kind][0]
    query = select(model).where(model.tender_id == tender_id, reviews.same_work(kind, r), model.id != r.id)
    others = list(session.scalars(query.order_by(model.created_at.desc())))
    if not others:
        return []
    lines = ["Other versions of the same work, newest first:"]
    for o in others[:8]:
        why = f": {o.reason}" if o.status == "rejected" and o.reason else ""
        lines.append(
            f"- {kind} {o.id[:8]} · {o.created_at:%d %b %H:%M} · {_brief(session, kind, o)} · "
            f"{STATES.get(o.status, o.status)}{why}"
        )
    if len(others) > 8:
        lines.append(f"… and {len(others) - 8} older.")
    return lines


def _rate_line(session: Session, item: BoqItem) -> str:
    rate = estimate.current_rate(session, item.id)
    if rate is None:
        return "Not priced yet."
    value = estimate.rate_of(rate)
    line = f"Rate: {value} per {item.unit} ({rate.basis})"
    if item.quantity is not None:
        line += f"; {_amount(item.quantity)} {item.unit} is {estimate.money(item.quantity * value):,}"
    return f"{line} · rate {rate.id[:8]}, {SHORT.get(rate.status, '')}"


def _body(session: Session, kind: str, r: Any) -> str:
    """What the record says and rests on."""
    if kind == "boq":
        lines = [f"{boq.reference(r)}: {r.description} · {_amount(r.quantity)} {r.unit}"]
        lines.append(reviews.details(session, _pending(kind, r)))
        lines.append(_rate_line(session, r))
        for m in takeoff.measurements(session, r.tender_id):
            if m.boq_item_id == r.id:
                lines.append(
                    f"Measured: “{m.label}” {_amount(takeoff.quantity(session, m))} {m.unit} · "
                    f"measurement {m.id[:8]}, {SHORT.get(m.status, m.status)}"
                )
        lines += [f"In the {p.name} package." for p in subcontract.packages(session, r.tender_id) if r.id in p.items]
        return "\n".join(lines)
    if kind == "rate":
        item = session.get(BoqItem, r.boq_item_id)
        text = reviews.details(session, _pending(kind, r))
        value = estimate.rate_of(r)
        text += f"\nQuantix: {value} per {item.unit}"
        if item.quantity is not None:
            text += f"; {_amount(item.quantity)} {item.unit} at this rate is {estimate.money(item.quantity * value):,}"
        return text + "."
    if kind == "checklist":
        text = f"{r.section} · {r.title}\n{reviews.details(session, _pending(kind, r))}"
        draft = submission.current_draft(session, r.id)
        if draft is not None:
            return text + f"\nCurrent draft: draft {draft.id[:8]}, “{draft.title}”"
        returned = returned_draft(session, r)
        if returned is not None:
            return text + (
                f"\nSent back: draft {returned.id[:8]}, “{returned.title}”: {returned.reason}\nOpen it for its text, "
                "correct it and draft it again for this item."
            )
        return text
    if kind == "enquiry":
        package, firm = session.get(Package, r.package_id), session.get(Company, r.company_id)
        return f"To {firm.name} for the {package.name} package.\n{reviews.details(session, _pending(kind, r))}"
    if kind == "package":
        items = [session.get(BoqItem, i) for i in r.items]
        lines = [f"{r.name} ({r.kind}): " + ", ".join(boq.reference(i) for i in items if i)]
        for e in subcontract.enquiries(session, r.id):
            lines.append(
                f"Enquiry to {session.get(Company, e.company_id).name}: {state(session, 'enquiry', e)} · "
                f"enquiry {e.id[:8]}"
            )
        for q in subcontract.quotes(session, r.id):
            lines.append(f"Quote from {session.get(Company, q.company_id).name} · quote {q.id[:8]}")
        levelled = subcontract.level(session, r)
        if levelled.columns:
            lines.append("Levelled by Quantix:")
            lines += [
                f"{c.rank or '-'}. {c.company}: quoted {c.quoted_total}, exclusions {c.exclusions}, levelled "
                f"{c.levelled_total if c.levelled_total is not None else 'incomplete'}"
                for c in sorted(levelled.columns, key=lambda c: c.rank or 999)
            ]
        if r.recommended_quote_id:
            firm = session.get(Company, session.get(Quote, r.recommended_quote_id).company_id)
            lines.append(f"Recommended: {firm.name}. {r.recommendation}")
        if r.selected_quote_id:
            chosen = session.get(Quote, r.selected_quote_id)
            lines.append(f"Chosen: {session.get(Company, chosen.company_id).name}.")
        return "\n".join(lines)
    if kind == "quote":
        package, firm = session.get(Package, r.package_id), session.get(Company, r.company_id)
        document = session.get(Document, r.document_id)
        lines = [f"{firm.name}'s quote for the {package.name} package, from {document.name}:"]
        for line in r.lines:
            item = session.get(BoqItem, line["boq_item_id"])
            lines.append(
                f"- {boq.reference(item)}: {line['rate']} per {item.unit} (page {line['page']}: “{line['quote']}”)"
            )
        for e in r.exclusions:
            lines.append(f"- Excludes {e['description']}, which we put at {e['amount']} (page {e['page']})")
        return "\n".join(lines)
    return reviews.details(session, _pending(kind, r))


def related(session: Session, kind: str, r: Any) -> list[tuple[str, str]]:
    """The records opening this one also shows: a BOQ line's rate, and a rate's or measurement's BOQ line."""
    if kind == "boq":
        rate = estimate.current_rate(session, r.id)
        return [("rate", rate.id)] if rate else []
    if kind in ("rate", "measurement") and r.boq_item_id:
        return [("boq", r.boq_item_id)]
    return []


def explain(session: Session, home: Path, tender_id: str, kind: str, r: Any) -> str:
    """Everything about a record: what it says, what it rests on, who made and decided it, what Quantix's checks
    find while it is undecided, and the versions before it."""
    names = _names(session, tender_id)
    parts = [f"{kind} {r.id[:8]} · {state(session, kind, r)}", _body(session, kind, r), *_made(kind, r, names)]
    recommending = kind == "package" and r.recommended_quote_id and not r.selected_quote_id
    if (kind in reviews.CHECKED and r.status in UNDECIDED) or recommending:
        parts.append(reviews.findings_text(reviews.findings(session, home, tender_id, _pending(kind, r))).strip())
    parts += _history(session, tender_id, kind, r)
    return "\n".join(p for p in parts if p)


def _boq_line(session: Session, item: BoqItem) -> str:
    line = f"boq {item.id[:8]} · {boq.reference(item)}: {item.description[:90]} · {_amount(item.quantity)} {item.unit}"
    line += f" · {SHORT.get(item.status, item.status)}"
    rate = estimate.current_rate(session, item.id)
    if rate is not None:
        line += f" · rate {rate.id[:8]}: {estimate.rate_of(rate)} per {item.unit}, {SHORT.get(rate.status, '')}"
    return line


FINDABLE = ("boq", "fact", "checklist", "draft", "measurement", "query", "package", "quote", "markups")


def search(session: Session, tender_id: str, words: str, kind: str | None = None) -> list[str]:
    """The office's records whose words contain every word asked for, as lines that open with their reference."""
    if kind and kind not in FINDABLE:
        raise ValueError(f"kind is one of {', '.join(FINDABLE)}, or leave it out to look through them all.")
    wanted = searchable(words).split()
    found: list[str] = []

    def add(text: str, line: str) -> None:
        if all(w in searchable(text) for w in wanted):
            found.append(line)

    if kind in (None, "boq"):
        for item in boq.items(session, tender_id):
            add(f"{boq.reference(item)} {item.description} {item.section or ''}", _boq_line(session, item))
    if kind in (None, "fact"):
        for f in boq.facts(session, tender_id):
            add(
                f"{FACT_KINDS[f.kind]} {f.value}",
                f"fact {f.id[:8]} · {FACT_KINDS[f.kind]}: {f.value} · {SHORT.get(f.status, f.status)}",
            )
    if kind in (None, "checklist"):
        for r in submission.requirements(session, tender_id):
            add(
                f"{r.section} {r.title}",
                f"checklist {r.id[:8]} · {r.section} · {r.title} · {state(session, 'checklist', r)}",
            )
    if kind in (None, "draft"):  # each checklist item's draft, by its own title too, which can differ from the item's
        for r in submission.requirements(session, tender_id):
            d = submission.current_draft(session, r.id) or returned_draft(session, r)
            if d is not None:
                add(
                    f"{d.title} {r.section} {r.title}",
                    f"draft {d.id[:8]} · “{d.title}” for checklist {r.id[:8]} ({r.section} · {r.title}) · "
                    f"{STATES.get(d.status, d.status)}",
                )
    if kind in (None, "measurement"):
        for m in takeoff.measurements(session, tender_id):
            item = session.get(BoqItem, m.boq_item_id) if m.boq_item_id else None
            add(
                f"{m.label} {boq.reference(item) if item else ''}",
                f"measurement {m.id[:8]} · “{m.label}” {_amount(takeoff.quantity(session, m))} {m.unit}"
                + (f" for {boq.reference(item)}" if item else "")
                + f" · {SHORT.get(m.status, m.status)}",
            )
    if kind in (None, "query"):
        for q in queries.queries(session, tender_id):
            add(
                f"{q.title} {q.detail} {queries.KINDS[q.kind]}",
                f"query {q.id[:8]} · {queries.KINDS[q.kind]}: {q.title} · {SHORT.get(q.status, q.status)}",
            )
    markups = None
    if kind in (None, "markups"):
        markups = estimate.current_markups(session, tender_id) or returned_markups(session, tender_id)
    if markups is not None:  # one record, found by what it holds: its preliminaries by name, and its note
        heads = ", ".join(i["item"] for i in markups.preliminary_items)
        add(
            f"markups markup preliminaries overheads profit adjustment {heads} {markups.note}",
            f"markups · preliminaries priced item by item ({len(markups.preliminary_items)} items: {heads or 'none'}), "
            f"overheads {markups.overheads:.1%} and profit {markups.profit:.1%} of cost, adjustment "
            f"{markups.adjustment:.2f} as a lump sum · {STATES.get(markups.status, markups.status)}",
        )
    if kind in (None, "package", "quote"):
        for p in subcontract.packages(session, tender_id):
            if kind in (None, "package"):
                add(
                    f"{p.name} {p.kind}",
                    f"package {p.id[:8]} · {p.name} ({p.kind}), {len(p.items)} items · {state(session, 'package', p)}",
                )
            if kind in (None, "quote"):
                for q in subcontract.quotes(session, p.id):
                    firm = session.get(Company, q.company_id).name
                    add(f"{firm} {p.name}", f"quote {q.id[:8]} · {firm} for {p.name}, {len(q.lines)} rates")
    return found


def priced(session: Session, tender_id: str, section: str | None = None, start: int = 1) -> str:
    """The priced BOQ, a page of lines at a time, with the total of the lines asked for."""
    items = boq.items(session, tender_id)
    if section:
        items = [i for i in items if searchable(section) in searchable(i.section or "")]
        if not items:
            sections = sorted({i.section or "(no section)" for i in boq.items(session, tender_id)})
            raise ValueError(f"No BOQ section matches “{section}”. The sections are: {', '.join(sections)}.")
    if not items:
        return "The BOQ is empty."
    rows, total, counted = [], Decimal(0), 0
    for item in items:
        rate = estimate.current_rate(session, item.id)
        if rate is None:
            rows.append(
                f"{boq.reference(item)} | {item.description[:60]} | {_amount(item.quantity)} {item.unit} | not priced"
            )
            continue
        value = estimate.rate_of(rate)
        amount = estimate.money(item.quantity * value) if item.quantity is not None else None
        if amount is not None:
            total += amount
            counted += 1
        rows.append(
            f"{boq.reference(item)} | {item.description[:60]} | {_amount(item.quantity)} {item.unit} | {value} | "
            f"{f'{amount:,}' if amount is not None else '-'} | {rate.basis} | {SHORT.get(rate.status, '')} | "
            f"rate {rate.id[:8]}"
        )
    first = max(start, 1)
    shown = rows[first - 1 : first - 1 + PAGE]
    if not shown:
        raise ValueError(f"There are {len(rows)} lines; start from 1 to {len(rows)}.")
    currency = estimate.summary(session, tender_id).currency
    where = f" in “{section}”" if section else ""
    head = (
        f"{len(items)} lines{where}, {counted} priced, together {total:,} {currency} (Quantix's figures). "
        "item | description | quantity | rate | amount | basis | state | reference"
    )
    last = first + len(shown) - 1
    more = f" Call again with start={last + 1} for the rest." if last < len(rows) else ""
    return "\n".join([head, *shown, f"Lines {first} to {last} of {len(rows)}.{more}"])


def _document_named(session: Session, tender_id: str, name: str) -> Document | None:
    wanted = name.strip().strip("“”\"'").lower()
    for document in library.documents(session, tender_id):
        if document.status != "replaced" and wanted in (
            document.name.lower(),
            document.path.lower(),
            document.name.rsplit(".", 1)[0].lower(),
        ):
            return document
    return None


def _label(session: Session, kind: str, r: Any) -> str:
    if kind in reviews.REVIEWED_KINDS:
        text = reviews.REVIEWED_KINDS[kind][1].label(session, r)
    elif kind == "checklist":
        text = f"the checklist item “{r.title}”"
    elif kind == "package":
        text = f"the {r.name} package"
    elif kind == "quote":
        text = f"{session.get(Company, r.company_id).name}'s quote for {session.get(Package, r.package_id).name}"
    else:
        text = f"the enquiry “{r.subject}”"
    return text[0].upper() + text[1:]


def _link(session: Session, kind: str, r: Any) -> dict[str, Any]:
    """The source as the engineer's chat shows it: a label, opening the BOQ line or the page it rests on."""
    source: dict[str, Any] = {"label": _label(session, kind, r)}
    item_id = r.id if kind == "boq" else getattr(r, "boq_item_id", None)
    where = session.get(Requirement, r.requirement_id) if kind == "draft" else r
    if item_id:
        source["boq_item_id"] = item_id
    elif getattr(where, "document_id", None) and getattr(where, "page", None):
        source |= {"document_id": where.document_id, "page": where.page}
    return source


def cited(session: Session, tender_id: str, staff_id: str, text: str) -> dict[str, Any]:
    """A source someone gives for what they tell the engineer. It must be something they opened."""
    wanted = text.strip()
    key = wanted.lower()
    if key in SUMMARIES:
        if office.has_opened(session, staff_id, "summary", key):
            return {"label": SUMMARIES[key]}
        raise ValueError(f"You haven't looked at {key} yet: call it first, then give it as a source.")
    if wanted.startswith(("https://", "http://")):
        query = select(WebPage).where(WebPage.url == wanted).order_by(WebPage.read_at.desc())
        read = next((p for p in session.scalars(query) if office.has_opened(session, staff_id, "web", p.id)), None)
        if read is None:
            raise ValueError(f"You haven't read {wanted}. Read it with read_web_page first.")
        return {"label": f"{read.title}, read {read.read_at:%d %b %Y}", "url": read.url}
    page = _PAGE_REF.match(wanted)
    if page:
        document = _document_named(session, tender_id, page["name"])
        if document is None:
            raise ValueError(f"No document is called “{page['name']}”. Name it as list_documents shows it.")
        number = int(page["page"])
        if not office.has_opened(session, staff_id, "page", f"{document.id}:{number}"):
            raise ValueError(f"You haven't read {document.name}, page {number}. Read it with read_page first.")
        return {"label": f"{document.name}, page {number}", "document_id": document.id, "page": number}
    kind, record = find(session, tender_id, wanted)
    if not office.has_opened(session, staff_id, kind, record.id):
        raise ValueError(f"You haven't opened {wanted.split(' · ')[0]}. Open it with open_record first.")
    return _link(session, kind, record)


def citable(session: Session, tender_id: str, staff_id: str, limit: int = 8) -> list[str]:
    """What the person opened since the engineer last wrote to them, newest first, each written as a source is
    given: so a refused answer can say exactly what it may rest on."""
    asked = [m for m in office.messages(session, tender_id, staff_id, limit=20) if m.sender == ENGINEER]
    query = select(Opened).where(Opened.tender_id == tender_id, Opened.staff_id == staff_id)
    if asked:
        query = query.where(Opened.at >= asked[-1].created_at)
    found: list[str] = []
    for opened in session.scalars(query.order_by(Opened.at.desc()).limit(limit * 3)):
        if opened.kind == "summary":
            source = opened.ref if opened.ref in SUMMARIES else None
        elif opened.kind == "page":
            document_id, _, page = opened.ref.partition(":")
            document = session.get(Document, document_id)
            source = f"{document.name}, page {page}" if document else None
        elif opened.kind == "web":
            web_page = session.get(WebPage, opened.ref)
            source = web_page.url if web_page else None
        else:
            source = f"{opened.kind} {opened.ref[:8]}" if opened.kind in MODELS else None
        if source and source not in found:
            found.append(source)
        if len(found) == limit:
            break
    return found


def answering(session: Session, tender_id: str, staff_id: str) -> bool:
    """Whether a message to the engineer now answers them: they wrote last, or the person is replacing the
    unanswered reply without sources they sent after the engineer's message. An answer with sources stays."""
    last = office.messages(session, tender_id, staff_id, limit=2)
    if not last:
        return False
    if last[-1].sender == ENGINEER:
        return True
    return len(last) == 2 and last[-1].sender == staff_id and not last[-1].sources and last[0].sender == ENGINEER


def promised(session: Session, tender_id: str, staff_id: str) -> bool:
    """Whether the person already set next steps for the engineer's latest message to them: one set per question."""
    asked = [m for m in office.messages(session, tender_id, staff_id, limit=20) if m.sender == ENGINEER]
    if not asked:
        return False
    query = select(Task.id).where(
        Task.staff_id == staff_id, Task.title.startswith("Follow up: "), Task.created_at > asked[-1].created_at
    )
    return session.scalars(query.limit(1)).first() is not None


def _measured(session: Session, m: Measurement) -> str:
    document = session.get(Document, m.document_id)
    if m.entities is not None:
        metres = drawings.metres_per_unit(session, m.document_id)
        units = f"in {drawings.unit_name(metres)}" if metres else "with no units set"
        times = f" × {m.multiplier} m" if m.multiplier is not None else ""
        return (
            f"measurement {m.id[:8]} · “{m.label}” on {document.name}, page {m.page} {units}: a {m.kind} of "
            f"{len(m.entities)} drawing objects by the rule {m.rule}{times} = {_amount(takeoff.quantity(session, m))} "
            f"{m.unit} · {SHORT.get(m.status, '')}"
        )
    scale = takeoff.scale_for(session, m.document_id, m.page)
    ratio = f"at about 1:{takeoff.drawing_ratio(scale.metres_per_point):,}" if scale else "with no scale set"
    times = f" × {m.multiplier} m" if m.multiplier is not None else ""
    return (
        f"measurement {m.id[:8]} · “{m.label}” on {document.name}, page {m.page} {ratio}: a {m.kind} of "
        f"{len(m.points)} points{times} = {_amount(takeoff.quantity(session, m))} {m.unit} · {SHORT.get(m.status, '')}"
    )


def takeoff_view(session: Session, tender_id: str, reference: str | None = None) -> str:
    """One BOQ line's measurements against its quantity; or, for the whole BOQ, the lines measured and those with a
    quantity nobody has measured yet."""
    if reference:
        item = boq.find_item(session, tender_id, reference)
        found = [m for m in takeoff.measurements(session, tender_id) if m.boq_item_id == item.id]
        lines = [f"{boq.reference(item)} {item.description[:80]}: BOQ {_amount(item.quantity)} {item.unit}."]
        if not found:
            return lines[0] + " Nothing is measured for it yet."
        lines += [f"- {_measured(session, m)}" for m in found]
        row = next((c for c in takeoff.compare(session, tender_id) if c.boq_item_id == item.id), None)
        if row and row.takeoff is not None:
            differs = f", {row.difference:+.1%} against the BOQ" if row.difference is not None else ""
            lines.append(f"Takeoff {_amount(row.takeoff)} {row.unit}{differs} ({row.result.replace('_', ' ')}).")
        return "\n".join(lines)
    rows = takeoff.compare(session, tender_id)
    measured = {r.boq_item_id for r in rows}
    lines = [
        f"{r.item or '(no BOQ line)'} {r.description[:60]}: takeoff {_amount(r.takeoff)} {r.unit}, BOQ "
        f"{_amount(r.boq_quantity)} {r.boq_unit or ''}"
        + (f" ({r.difference:+.1%})" if r.difference is not None else "")
        + f" → {r.result.replace('_', ' ')}"
        for r in rows
    ]
    missing = [boq.reference(i) for i in boq.items(session, tender_id) if i.quantity and i.id not in measured]
    if missing:
        lines.append(f"Not measured yet, {len(missing)} lines with a quantity: " + ", ".join(missing[:60]))
    return "\n".join(lines) or "Nothing is measured yet, and the BOQ has no quantities."


def packages_view(session: Session, tender_id: str) -> str:
    """Each package: its lines, its enquiries and quotes, and where the choice stands."""
    rows = []
    for p in subcontract.packages(session, tender_id):
        enquiries = subcontract.enquiries(session, p.id)
        sent = sum(e.status == "sent" for e in enquiries)
        firms = [session.get(Company, q.company_id).name for q in subcontract.quotes(session, p.id)]
        rows.append(
            f"package {p.id[:8]} · {p.name} ({p.kind}), {len(p.items)} lines · {len(enquiries)} enquiries, {sent} "
            f"sent · quotes: {', '.join(firms) or 'none yet'} · {state(session, 'package', p)}"
        )
    return "\n".join(rows) or "No packages yet. Group lines to price from outside with create_package."
