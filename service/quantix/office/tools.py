"""The tools office agents work with. Every tool records what the person is doing, from the real call."""

import threading
from collections.abc import Callable
from contextlib import contextmanager
from dataclasses import dataclass
from decimal import Decimal
from pathlib import Path

from pydantic_ai import BinaryContent, ModelRetry, RunContext, ToolReturn
from sqlalchemy.orm import Session, sessionmaker

from quantix.boq import records as boq
from quantix.boq.models import FACT_KINDS
from quantix.documents import library, readers
from quantix.documents.models import Document
from quantix.estimate import records as estimate
from quantix.office import records
from quantix.office.models import TEAM, Staff
from quantix.subcontract import records as subcontract
from quantix.submission import records as submission
from quantix.takeoff import records as takeoff


class Stopped(Exception):
    """The engineer pressed Stop."""


@dataclass
class Turn:
    home: Path
    sessions: sessionmaker[Session]
    tender_id: str
    staff_id: str
    autonomous: bool
    stop: threading.Event


@contextmanager
def _working(ctx: RunContext[Turn], doing: str | None = None):
    """A session for one tool call, with the person's current activity set and errors returned to the model."""
    if ctx.deps.stop.is_set():
        raise Stopped()
    with ctx.deps.sessions() as session:
        me = session.get(Staff, ctx.deps.staff_id)
        if doing:
            me.now = doing[:300]
        try:
            yield session, me
        except ValueError as error:
            session.rollback()
            raise ModelRetry(str(error)) from error
        session.commit()


def _document(session: Session, tender_id: str, document_id: str) -> Document:
    document = session.get(Document, document_id)
    if document is None or document.tender_id != tender_id:
        raise ValueError("No document has that id. Use list_documents or search_documents to find its id.")
    return document


def list_documents(ctx: RunContext[Turn]) -> str:
    """List the tender's documents with their ids, page counts and any reading problem."""
    with _working(ctx, "Looking through the document list") as (session, _):
        rows = [
            f"{d.id} · {d.path} · {d.page_count or 0} pages" + (f" · {d.note}" if d.note else "")
            for d in library.documents(session, ctx.deps.tender_id)
            if d.status != "replaced"
        ]
    return "\n".join(rows) or "No documents have been added yet."


def search_documents(ctx: RunContext[Turn], query: str) -> str:
    """Search every page of the tender's documents for these words. Returns document ids, pages and snippets."""
    with _working(ctx, f"Searching the documents for “{query}”") as (session, _):
        hits = library.search(session, ctx.deps.tender_id, query, limit=15)
    if not hits:
        return "Nothing found. Try other words, or the Arabic or English term."
    return "\n".join(f"{h['document_id']} · {h['name']} · page {h['page']}: {h['snippet']}" for h in hits)


def read_page(ctx: RunContext[Turn], document_id: str, page: int) -> str:
    """Read one page's text. Cite what you use as "<document name>, page <n>"."""
    with _working(ctx) as (session, me):
        document = _document(session, ctx.deps.tender_id, document_id)
        me.now = f"Reading {document.name}, page {page}"
        found = library.page(session, document_id, page)
        if found is None:
            raise ValueError(f"{document.name} has pages 1 to {document.page_count or 0}.")
        if not found.has_text:
            return f"{document.name}, page {page} is a scan with no text. Use view_page to look at it."
        return f"{document.name}, page {page}:\n{found.text}"


def view_page(ctx: RunContext[Turn], document_id: str, page: int) -> ToolReturn:
    """Look at a page as an image: drawings, scans, tables and stamps. Cite it as "<document name>, page <n>"."""
    with _working(ctx) as (session, me):
        document = _document(session, ctx.deps.tender_id, document_id)
        if document.kind != "pdf" or not 1 <= page <= (document.page_count or 0):
            raise ValueError(f"Only PDF pages can be viewed; {document.name} has {document.page_count or 0} pages.")
        me.now = f"Looking at {document.name}, page {page}"
        path = library.stored_file(ctx.deps.home, document)
    image = readers.render_page(path, page, width=1600)
    return ToolReturn(
        return_value=f"The image of {document.name}, page {page} follows.",
        content=[BinaryContent(data=image, media_type="image/png")],
    )


def post_to_team(ctx: RunContext[Turn], text: str) -> str:
    """Say something in the team room, where the whole office and the engineer can read it."""
    with _working(ctx) as (session, me):
        records.post(session, ctx.deps.tender_id, me.id, TEAM, text)
    return "Posted."


def message_engineer(ctx: RunContext[Turn], text: str) -> str:
    """Write to the engineer in your own chat with them."""
    with _working(ctx) as (session, me):
        records.post(session, ctx.deps.tender_id, me.id, me.id, text)
    return "Sent."


def raise_concern(ctx: RunContext[Turn], text: str) -> str:
    """Disagree or warn, plainly: a risk, a doubtful figure, a decision you think is wrong. Shown in the team room."""
    with _working(ctx) as (session, me):
        records.post(session, ctx.deps.tender_id, me.id, TEAM, text, kind="concern")
    return "Your concern is in the team room."


def ask_engineer(ctx: RunContext[Turn], title: str, question: str, options: list[str]) -> str:
    """Ask the engineer to decide something that is theirs to decide. Give 2 to 4 clear options.
    The answer comes back to you in your chat with the engineer."""
    if ctx.deps.autonomous:
        return (
            "The office is working fully autonomously: decide this yourself, then explain the decision and your "
            "reasons in the team room so the engineer can review it later."
        )
    if not 2 <= len(options) <= 4:
        raise ModelRetry("Give between 2 and 4 options.")
    with _working(ctx) as (session, me):
        records.ask(session, ctx.deps.tender_id, me, title, question, options)
    return "The question is waiting for the engineer."


def complete_task(ctx: RunContext[Turn], task_id: str, result: str) -> str:
    """Finish one of your tasks with a clear result the Manager can use, citing documents and pages."""
    with _working(ctx) as (session, me):
        task = records.complete(session, me, task_id, result)
        me.now = f"Finished: {task.title}"
    return "Done. The Manager has your result."


def hire(
    ctx: RunContext[Turn],
    name: str,
    role: str,
    discipline: str,
    experience_years: int,
    background: str,
    working_style: str,
    opinions: str,
    voice: str,
) -> str:
    """Hire someone for work this tender needs. Make them a real person: a full name that suits the region,
    their own background, working style, professional opinions and way of speaking."""
    profile = {
        "discipline": discipline,
        "experience_years": experience_years,
        "background": background,
        "working_style": working_style,
        "opinions": opinions,
        "voice": voice,
    }
    with _working(ctx, f"Hiring a {role}") as (session, me):
        member = records.hire(session, ctx.deps.tender_id, name, role, profile)
        member.now = "Just joined the team"
        records.post(session, ctx.deps.tender_id, me.id, TEAM, f"{member.name} joins the team as {role}.", "task")
    return f"{name} has joined. Give them a task with assign_task."


def assign_task(ctx: RunContext[Turn], staff_name: str, title: str, brief: str) -> str:
    """Give a team member a task: a short title that reads after "asked <name> to", and a clear brief with
    the documents to start from and the result you expect."""
    with _working(ctx) as (session, me):
        member = records.find_staff(session, ctx.deps.tender_id, staff_name)
        if member is None or member.is_manager:
            raise ValueError("No one on the team has that name. Hire them first, or check the team list.")
        task = records.assign(session, ctx.deps.tender_id, me, member, title, brief)
        me.now = f"Briefing {member.first_name}"
    return f"Task {task.id} is with {member.first_name}."


def release(ctx: RunContext[Turn], staff_name: str, reason: str) -> str:
    """Release a team member whose work on this tender is finished."""
    with _working(ctx) as (session, me):
        member = records.find_staff(session, ctx.deps.tender_id, staff_name)
        if member is None or member.is_manager:
            raise ValueError("No one on the team has that name.")
        member.status, member.now = "released", None
        records.post(session, ctx.deps.tender_id, me.id, TEAM, f"{member.name} has left the team: {reason}", "task")
    return f"{member.first_name} has been released."


def propose_boq_items(ctx: RunContext[Turn], items: list[boq.ItemIn]) -> str:
    """Add BOQ lines exactly as the client's BOQ states them, up to 40 at a time, each with the page it is on and a
    quote that includes the item number and the quantity. Lines that don't check out come back with the reason."""
    with _working(ctx, f"Entering {len(items)} BOQ items") as (session, me):
        return boq.propose_items(session, ctx.deps.tender_id, me, items[:40], ctx.deps.autonomous)


def list_boq(ctx: RunContext[Turn]) -> str:
    """The BOQ as the office has it so far: item, description, unit, quantity and approval."""
    with _working(ctx, "Checking the BOQ") as (session, _):
        rows = boq.items(session, ctx.deps.tender_id)
        lines = [
            f"{r.section + ' / ' if r.section else ''}{r.item} | {r.description[:80]} | {r.unit} | "
            f"{r.quantity if r.quantity is not None else '-'} | {r.status}"
            for r in rows
        ]
        known = [f"{FACT_KINDS[f.kind]}: {f.value} ({f.status})" for f in boq.facts(session, ctx.deps.tender_id)]
    return "\n".join(known + [f"{len(rows)} BOQ items:"] + lines[:300]) if rows or known else "The BOQ is empty."


def propose_fact(ctx: RunContext[Turn], kind: str, value: str, document_id: str, page: int, quote: str) -> str:
    """Record a tender fact pricing depends on, for the engineer's approval: kind is method_of_measurement,
    currency or vat. The quote must be on the page."""
    with _working(ctx, f"Recording the {FACT_KINDS.get(kind, kind).lower()}") as (session, me):
        boq.propose_fact(session, ctx.deps.tender_id, me, kind, value, document_id, page, quote, ctx.deps.autonomous)
    return "Recorded." if ctx.deps.autonomous else "Recorded for the engineer's approval."


VIEW_WIDTH = 1600  # view_page images are this many pixels wide; takeoff tools use the same pixels


def _points(session: Session, tender_id: str, document_id: str, page: int) -> float:
    """Page points per view_page pixel."""
    found = library.page(session, document_id, page)
    document = session.get(Document, document_id)
    if not found or not found.width or document is None or document.tender_id != tender_id:
        raise ValueError("Takeoff works on PDF pages of this tender; check the document id and page.")
    return found.width / VIEW_WIDTH


def _snapped(ctx: RunContext[Turn], session: Session, document_id: str, page: int, pixels, factor: float):
    """view_page pixels to page points, snapped onto the drawing's own corners and line ends."""
    path = library.stored_file(ctx.deps.home, session.get(Document, document_id))
    return takeoff.snap([[x * factor, y * factor] for x, y in pixels], readers.vector_points(path, page))


def find_on_page(ctx: RunContext[Turn], document_id: str, page: int, text: str) -> str:
    """Find where text is printed on a drawing (a dimension, grid label, room name or note), exactly, from the PDF
    itself. Positions are in view_page pixels: left, top, right, bottom."""
    with _working(ctx, f"Finding “{text}” on a drawing") as (session, _):
        factor = _points(session, ctx.deps.tender_id, document_id, page)
        path = library.stored_file(ctx.deps.home, session.get(Document, document_id))
    boxes = readers.find_text(path, page, text)
    if not boxes:
        return f"“{text}” is not printed on that page as text. Look at the page with view_page instead."
    return "\n".join(
        f"“{text}” at left {b[0] / factor:.0f}, top {b[1] / factor:.0f}, "
        f"right {b[2] / factor:.0f}, bottom {b[3] / factor:.0f}"
        for b in boxes
    )


def set_scale(
    ctx: RunContext[Turn],
    document_id: str,
    page: int,
    from_xy: list[float],
    to_xy: list[float],
    length_m: float,
    dimension_text: str,
) -> str:
    """Set a drawing's scale from a dimension printed on it: the two ends of the dimension line in view_page pixels,
    its real length in metres, and the dimension text as printed (e.g. "40.00"). Use find_on_page to locate it."""
    with _working(ctx, "Setting the scale of a drawing") as (session, me):
        factor = _points(session, ctx.deps.tender_id, document_id, page)
        line = _snapped(ctx, session, document_id, page, [from_xy, to_xy], factor)
        status = "office_approved" if ctx.deps.autonomous else "proposed"
        scale = takeoff.set_scale(
            session, ctx.deps.tender_id, me.id, document_id, page, line, length_m, dimension_text, status
        )
        return f"Scale set: 1 metre is {1 / scale.metres_per_point / factor:.1f} view_page pixels."


def measure(
    ctx: RunContext[Turn],
    document_id: str,
    page: int,
    kind: str,
    label: str,
    points: list[list[float]],
    unit: str,
    multiplier_m: float | None = None,
    boq_item: str | None = None,
) -> str:
    """Measure on a drawing whose scale is set. kind is length (a polyline), area (a closed outline) or count (one
    point per thing counted). Points are in view_page pixels. unit: length m, or m2 with a height as multiplier_m;
    area m2, or m3 with a thickness; count nr. Link the BOQ item number it belongs to when there is one.
    Quantix computes the quantity from your points."""
    with _working(ctx, f"Measuring {label}") as (session, me):
        factor = _points(session, ctx.deps.tender_id, document_id, page)
        status = "office_approved" if ctx.deps.autonomous else "proposed"
        m = takeoff.measure(
            session,
            ctx.deps.tender_id,
            me.id,
            document_id,
            page,
            kind,
            label,
            _snapped(ctx, session, document_id, page, points, factor),
            unit,
            Decimal(str(multiplier_m)) if multiplier_m is not None else None,
            boq_item,
            status,
        )
        q = takeoff.quantity(session, m)
    return (
        f"Measured {label}: {q} {unit}." if q is not None else "Saved, but the sheet has no scale yet: set_scale first."
    )


def takeoff_summary(ctx: RunContext[Turn]) -> str:
    """The takeoff so far against the BOQ: each measured item with its takeoff and BOQ quantities and the result."""
    with _working(ctx, "Comparing the takeoff with the BOQ") as (session, _):
        rows = takeoff.compare(session, ctx.deps.tender_id)
    if not rows:
        return "Nothing has been measured yet."
    return "\n".join(
        f"{r.item or '(no BOQ item)'} {r.description[:60]}: "
        f"takeoff {r.takeoff if r.takeoff is not None else '-'} {r.unit}, "
        f"BOQ {r.boq_quantity if r.boq_quantity is not None else '-'} {r.boq_unit or ''} "
        f"→ {r.result.replace('_', ' ')}"
        for r in rows
    )


def search_library(ctx: RunContext[Turn], words: str) -> str:
    """Search the firm's rate library (labour, plant, material, subcontract and unit rates from earlier tenders).
    Check each entry's date before relying on it."""
    with _working(ctx, f"Checking the rate library for “{words}”") as (session, _):
        rows = estimate.library(session, words)[:25]
    if not rows:
        return "Nothing in the library matches. Price it from a quote, or estimate it and say how."
    return "\n".join(
        f"{r.id} · {r.kind} · {r.name} · {r.rate} {r.currency} per {r.unit} · dated {r.dated} · {r.source}"
        for r in rows
    )


def propose_rate(
    ctx: RunContext[Turn],
    boq_item: str,
    basis: str,
    note: str,
    unit_rate: Decimal | None = None,
    lines: list[estimate.LineIn] | None = None,
    document_id: str | None = None,
    page: int | None = None,
    quote: str | None = None,
    library_id: str | None = None,
) -> str:
    """Price one BOQ item, for the engineer's approval: either a unit_rate, or a build-up of lines (labour, plant,
    material, subcontract per one unit of the item, with wastage). basis is how you know the price: "quote" (give
    the document, page and the quoted line), "library" (give the library_id) or "estimate" (your own judgement:
    put the outputs, prices and assumptions in the note). Quantix computes the rate and the amount."""
    with _working(ctx, f"Pricing item {boq_item}") as (session, me):
        status = "office_approved" if ctx.deps.autonomous else "proposed"
        rate = estimate.propose_rate(
            session,
            ctx.deps.tender_id,
            me.id,
            boq_item,
            basis,
            note,
            unit_rate,
            lines,
            document_id,
            page,
            quote,
            library_id,
            status,
        )
        return f"Item {boq_item} priced at {estimate.rate_of(rate)} per unit."


def propose_markups(
    ctx: RunContext[Turn], preliminaries: Decimal, overheads: Decimal, profit: Decimal, adjustment: Decimal, note: str
) -> str:
    """Propose the tender's markups as fractions (0.08 is 8%): preliminaries on the net cost, overheads, profit, and
    a lump-sum adjustment. Explain them in the note. Quantix computes the totals."""
    with _working(ctx, "Proposing the markups") as (session, me):
        status = "office_approved" if ctx.deps.autonomous else "proposed"
        estimate.propose_markups(
            session, ctx.deps.tender_id, me.id, preliminaries, overheads, profit, adjustment, note, status
        )
        total = estimate.summary(session, ctx.deps.tender_id).total
    return f"Proposed. The price with these markups is {total}."


def estimate_summary(ctx: RunContext[Turn]) -> str:
    """The price so far, as Quantix computes it: net cost, markups, total, VAT, and the items still unpriced."""
    with _working(ctx, "Checking the estimate") as (session, _):
        s = estimate.summary(session, ctx.deps.tender_id)
    lines = [
        f"{s.priced} of {s.items} items priced ({s.waiting} waiting for the engineer).",
        f"Net {s.net} {s.currency}; preliminaries {s.preliminaries}; overheads {s.overheads}; profit {s.profit}; "
        f"adjustment {s.adjustment}; total {s.total}.",
    ]
    if s.vat is not None:
        lines.append(f"VAT {s.vat}; total with VAT {s.total_with_vat}.")
    if s.unpriced:
        lines.append("Not priced yet: " + ", ".join(s.unpriced[:60]))
    return "\n".join(lines)


def search_directory(ctx: RunContext[Turn], words: str = "") -> str:
    """Search the firm's directory of subcontractors and suppliers by name or trade."""
    with _working(ctx, "Checking the directory") as (session, _):
        rows = subcontract.directory(session, words)[:40]
    if not rows:
        return "Nobody in the directory matches. Add a company with add_company."
    return "\n".join(f"{c.name} · {c.kind} · {c.trades}" + (f" · {c.email}" if c.email else "") for c in rows)


def add_company(
    ctx: RunContext[Turn], name: str, kind: str, trades: str, email: str | None = None, phone: str | None = None
) -> str:
    """Add a subcontractor or supplier to the firm's directory, e.g. from the tender's approved vendor list.
    kind is subcontractor or supplier."""
    with _working(ctx, f"Adding {name} to the directory") as (session, me):
        subcontract.add_company(session, me.id, name, kind, trades, email, phone)
    return f"{name} is in the directory."


def create_package(ctx: RunContext[Turn], name: str, kind: str, boq_items: list[str]) -> str:
    """Group BOQ items into a package to price from outside: kind "subcontract" (a trade let to a subcontractor)
    or "supply" (materials bought from a supplier)."""
    with _working(ctx, f"Setting up the {name} package") as (session, me):
        subcontract.create_package(session, ctx.deps.tender_id, me.id, name, kind, boq_items)
    return f"The {name} package has {len(boq_items)} items. Draft enquiries with draft_enquiry."


def draft_enquiry(ctx: RunContext[Turn], package: str, company: str, subject: str, body: str) -> str:
    """Draft an enquiry email to a company in the directory. The engineer sends it from their own mail program.
    Include the items, quantities, units, the scope, the programme and the date quotes are due."""
    with _working(ctx, f"Drafting an enquiry to {company}") as (session, me):
        found = subcontract.find_company(session, company)
        subcontract.draft_enquiry(
            session, subcontract.find_package(session, ctx.deps.tender_id, package), found, me.id, subject, body
        )
    return f"The enquiry to {company} is ready for the engineer to send."


def record_quote(
    ctx: RunContext[Turn],
    package: str,
    company: str,
    document_id: str,
    lines: list[subcontract.QuoteLine],
    exclusions: list[subcontract.Exclusion] | None = None,
) -> str:
    """Record a company's quote from its document: each quoted rate with the page and line it is on, and anything
    the quote excludes with your estimate of what it adds. Quantix levels the quotes."""
    with _working(ctx, f"Recording {company}'s quote") as (session, me):
        found_package = subcontract.find_package(session, ctx.deps.tender_id, package)
        subcontract.record_quote(
            session,
            found_package,
            subcontract.find_company(session, company),
            me.id,
            document_id,
            lines,
            exclusions or [],
        )
    return f"{company}'s quote is recorded. Use levelling to compare the quotes."


def levelling(ctx: RunContext[Turn], package: str) -> str:
    """The package's quotes side by side, as Quantix levels them: gaps filled with our own rate, exclusions added."""
    with _working(ctx, f"Levelling the {package} quotes") as (session, _):
        result = subcontract.level(session, subcontract.find_package(session, ctx.deps.tender_id, package))
    if not result.columns:
        return "No quotes recorded yet."
    lines = []
    for column in sorted(result.columns, key=lambda c: c.rank or 999):
        gaps = [i.item for i in result.items if column.cells[i.id].plugged]
        lines.append(
            f"{column.rank or '-'}. {column.company}: quoted {column.quoted_total}, exclusions {column.exclusions}, "
            f"levelled {column.levelled_total if column.levelled_total is not None else 'incomplete'}"
            + (f" (our rate used for {', '.join(gaps)})" if gaps else "")
        )
    return "\n".join(lines)


def recommend_quote(ctx: RunContext[Turn], package: str, company: str, reason: str) -> str:
    """Recommend which quote to take, and why: price after levelling, gaps, exclusions and your view of the company.
    The engineer chooses."""
    with _working(ctx, f"Recommending a quote for {package}") as (session, me):
        found = subcontract.find_package(session, ctx.deps.tender_id, package)
        quote = subcontract.recommend(session, found, me.id, subcontract.find_company(session, company), reason)
        if ctx.deps.autonomous:
            subcontract.select_quote(session, found, quote, status="office_approved")
            return f"The office has taken {company}'s quote; its rates are in the estimate."
    return "Your recommendation is waiting for the engineer."


def add_requirements(ctx: RunContext[Turn], requirements: list[submission.RequirementIn]) -> str:
    """Add what the tender requires the bidder to submit to the checklist: forms, bonds, certificates, schedules,
    method statements, the priced BOQ. Each with the clause that requires it."""
    with _working(ctx, "Building the submission checklist") as (session, me):
        return submission.add_requirements(session, ctx.deps.tender_id, me.id, requirements)


def list_requirements(ctx: RunContext[Turn]) -> str:
    """The submission checklist and where each requirement stands."""
    with _working(ctx, "Checking the submission checklist") as (session, _):
        rows = [
            f"- {r.section} · {r.title}: {submission.state(session, r)}"
            for r in submission.requirements(session, ctx.deps.tender_id)
        ]
    return "\n".join(rows) or "The checklist is empty. Add requirements with add_requirements."


def draft_document(ctx: RunContext[Turn], requirement: str, title: str, text: str) -> str:
    """Draft a submission document for a checklist requirement, such as a method statement, a covering letter or a
    schedule. Base it on the tender documents and the approved figures; leave signatures and anything only the
    engineer can provide as clear blanks. A new draft replaces your earlier one."""
    with _working(ctx, f"Drafting {title}") as (session, me):
        found = submission.find_requirement(session, ctx.deps.tender_id, requirement)
        status = "office_approved" if ctx.deps.autonomous else "proposed"
        submission.draft(session, found, me.id, title, text, status)
    return "The draft is in the checklist." if ctx.deps.autonomous else "The draft is waiting for the engineer."


def set_pricing_columns(
    ctx: RunContext[Turn], document_id: str, sheet: int, rate_column: str, amount_column: str, header_quote: str
) -> str:
    """Say which columns of the client's BOQ workbook take the rate and the amount, so the priced BOQ is returned in
    the client's own format. sheet is the page number read_page uses; header_quote is the header row as shown."""
    with _working(ctx, "Reading the client's BOQ layout") as (session, me):
        submission.set_pricing_columns(
            session, ctx.deps.tender_id, me.id, document_id, sheet, rate_column, amount_column, header_quote
        )
    return f"Rates will go in column {rate_column.upper()} and amounts in column {amount_column.upper()}."


COMMON: list[Callable] = [
    list_documents,
    search_documents,
    read_page,
    view_page,
    post_to_team,
    message_engineer,
    raise_concern,
    ask_engineer,
    propose_boq_items,
    list_boq,
    propose_fact,
    find_on_page,
    set_scale,
    measure,
    takeoff_summary,
    search_library,
    propose_rate,
    propose_markups,
    estimate_summary,
    search_directory,
    add_company,
    create_package,
    draft_enquiry,
    record_quote,
    levelling,
    recommend_quote,
    add_requirements,
    list_requirements,
    draft_document,
    set_pricing_columns,
]
STAFF: list[Callable] = [*COMMON, complete_task]
MANAGER: list[Callable] = [*COMMON, hire, assign_task, release]
