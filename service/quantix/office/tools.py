"""The tools office agents work with. Every tool records what the person is doing, from the real call."""

import re
import threading
from collections.abc import Callable
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import UTC, datetime
from decimal import Decimal
from pathlib import Path

from pydantic import BaseModel, Field, ValidationError, field_validator
from pydantic_ai import BinaryContent, ModelRetry, RunContext, ToolReturn
from sqlalchemy.orm import Session, sessionmaker

from quantix import company
from quantix.boq import records as boq
from quantix.boq.models import FACT_KINDS, BoqItem
from quantix.documents import library, readers
from quantix.documents.models import Document
from quantix.estimate import records as estimate
from quantix.office import records
from quantix.office.models import TEAM, Staff
from quantix.review import audit
from quantix.review import records as reviews
from quantix.subcontract import records as subcontract
from quantix.submission import records as submission
from quantix.takeoff import records as takeoff

TEAM_LIMIT = 5  # staff under the Manager; a tender's work is shared among a few, not spread across many
FOREIGN_SCRIPT = re.compile("[぀-ヿ㐀-鿿가-힯]")  # Chinese, Japanese, Korean


class Persona(BaseModel):
    """A generated person for the office. Checked, because a weak model sometimes writes placeholders."""

    name: str = Field(description="Full name that suits the region of the tender")
    discipline: str
    experience_years: int = Field(ge=2, le=45)
    background: str = Field(description="Two sentences about their career")
    working_style: str = Field(description="How they work, in one or two sentences")
    opinions: str = Field(description="Professional views they hold and will voice")
    voice: str = Field(
        description="How they speak and write: a distinct manner that stays clear and easy to read, "
        "with no catchphrases, forms of address or slang"
    )

    @field_validator("name")
    @classmethod
    def _full_name(cls, name: str) -> str:
        if len(name.split()) < 2:
            raise ValueError("give a full name, first and last")
        return name.strip()

    @field_validator("discipline", "background", "working_style", "opinions", "voice")
    @classmethod
    def _plain_english(cls, text: str) -> str:
        if FOREIGN_SCRIPT.search(text):
            raise ValueError("write it in plain English only")
        return text.strip()

    @field_validator("background", "working_style", "opinions", "voice")
    @classmethod
    def _real_words(cls, text: str) -> str:
        if len(text.split()) < 2 or re.search(r"\w_\w", text):
            raise ValueError("write real words, not a label or placeholder")
        return text


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
    sees_images: bool = True  # False when the office's AI failed the image check


BLIND = (
    "The office's AI can't read images, so it can't look at drawings or scans. Work from the text with read_page "
    "and find_on_page, and tell the engineer what you couldn't check: they can measure on the Takeoff screen or "
    "choose an AI that reads images in Settings."
)


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


def view_page(
    ctx: RunContext[Turn], document_id: str, page: int, region: list[float] | None = None
) -> ToolReturn | str:
    """Look at a page as an image: drawings, scans, tables and stamps. Cite it as "<document name>, page <n>".
    To read small detail such as a scale bar, a dimension's ticks or a corner, zoom in: region is the part to
    enlarge as [left, top, right, bottom] in view_page pixels of the whole page."""
    if not ctx.deps.sees_images:
        return BLIND
    with _working(ctx) as (session, me):
        document = _document(session, ctx.deps.tender_id, document_id)
        if document.kind != "pdf" or not 1 <= page <= (document.page_count or 0):
            raise ValueError(f"Only PDF pages can be viewed; {document.name} has {document.page_count or 0} pages.")
        me.now = f"Looking at {document.name}, page {page}"
        path = library.stored_file(ctx.deps.home, document)
        factor = _points(session, ctx.deps.tender_id, document_id, page)
    if region is None:
        return ToolReturn(
            return_value=f"The image of {document.name}, page {page} follows.",
            content=[BinaryContent(data=readers.render_page(path, page, width=VIEW_WIDTH), media_type="image/png")],
        )
    if len(region) != 4 or not (region[0] < region[2] and region[1] < region[3]):
        raise ModelRetry("Give the region as [left, top, right, bottom] with left < right and top < bottom.")
    left, top, right, bottom = region
    image = readers.render_page(path, page, width=VIEW_WIDTH, region=tuple(v * factor for v in region))
    k = (right - left) / VIEW_WIDTH
    return ToolReturn(
        return_value=(
            f"A close-up of {document.name}, page {page}, from ({left:.0f}, {top:.0f}) to ({right:.0f}, "
            f"{bottom:.0f}) follows, enlarged {1 / k:.1f} times. A point at (u, v) in it is at "
            f"({left:.0f} + u × {k:.4f}, {top:.0f} + v × {k:.4f}) in view_page pixels: give those to the takeoff "
            "tools."
        ),
        content=[BinaryContent(data=image, media_type="image/png")],
    )


def post_to_team(ctx: RunContext[Turn], text: str) -> str:
    """Say something in the team room, where the whole office and the engineer can read it."""
    with _working(ctx) as (session, me):
        records.post(session, ctx.deps.tender_id, me.id, TEAM, text)
    return "Posted."


def message_engineer(ctx: RunContext[Turn], text: str) -> str:
    """Write to the engineer in your own chat with them. While they haven't answered your last message, this one
    replaces it, so they read one current update: include anything from it that still matters."""
    with _working(ctx) as (session, me):
        last = records.messages(session, ctx.deps.tender_id, me.id, limit=1)
        replaced = bool(last) and last[0].sender == me.id
        if replaced:
            session.delete(last[0])
        records.post(session, ctx.deps.tender_id, me.id, me.id, text)
    return "Sent. It replaces your last message, which the engineer hadn't answered." if replaced else "Sent."


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
        try:
            records.ask(session, ctx.deps.tender_id, me, title, question, options)
        except ValueError as already:
            return str(already)  # not an error: carry on with other work until the engineer answers
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
    try:
        Persona(name=name, **profile)
    except ValidationError as error:
        problems = "; ".join(f"{e['loc'][0]}: {e['msg']}" for e in error.errors())
        raise ModelRetry(f"Make them a real person. {problems}.") from error
    with _working(ctx, f"Hiring a {role}") as (session, me):
        staff = [m for m in records.team(session, ctx.deps.tender_id) if not m.is_manager]
        if len(staff) >= TEAM_LIMIT:
            raise ValueError(
                f"The team already has {len(staff)} people: {', '.join(f'{m.name} ({m.role})' for m in staff)}. "
                "Give this work to one of them, or release someone whose work is done first."
            )
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
        return boq.propose_items(session, ctx.deps.tender_id, me, items[:40])


def withdraw_boq_items(ctx: RunContext[Turn], items: list[str], reason: str) -> str:
    """Withdraw BOQ lines you entered that the engineer hasn't decided yet, for example to re-enter them under the
    right section. Refer to each as "<section> / <item>" when an item number is in more than one bill."""
    with _working(ctx, f"Withdrawing {len(items)} BOQ items") as (session, me):
        return boq.withdraw_items(session, ctx.deps.tender_id, me, items, reason)


def list_boq(ctx: RunContext[Turn]) -> str:
    """The BOQ as the office has it so far: item, description, unit, quantity and approval."""
    with _working(ctx, "Checking the BOQ") as (session, _):
        rows = boq.items(session, ctx.deps.tender_id)
        lines = [
            f"{boq.reference(r)} | {r.description[:80]} | {r.unit} | "
            f"{r.quantity if r.quantity is not None else '-'} | {r.status}"
            for r in rows
        ]
        known = [f"{FACT_KINDS[f.kind]}: {f.value} ({f.status})" for f in boq.facts(session, ctx.deps.tender_id)]
    return "\n".join(known + [f"{len(rows)} BOQ items:"] + lines[:300]) if rows or known else "The BOQ is empty."


def propose_fact(ctx: RunContext[Turn], kind: str, value: str, document_id: str, page: int, quote: str) -> str:
    """Record a tender fact pricing depends on, for the engineer's approval: kind is method_of_measurement,
    currency or vat. The quote must be on the page."""
    with _working(ctx, f"Recording the {FACT_KINDS.get(kind, kind).lower()}") as (session, me):
        boq.propose_fact(session, ctx.deps.tender_id, me, kind, value, document_id, page, quote)
    return "Recorded for the Tender Manager's review."


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
    its real length in metres, and the dimension text as printed (e.g. "40.00"). Use find_on_page to locate it.
    The points go on the line's end ticks, never on its text. A graphic scale bar works too: its 0 and end ticks,
    with the end label (e.g. "25") as the dimension text."""
    if not ctx.deps.sees_images:
        return BLIND
    with _working(ctx, "Setting the scale of a drawing") as (session, me):
        factor = _points(session, ctx.deps.tender_id, document_id, page)
        line = _snapped(ctx, session, document_id, page, [from_xy, to_xy], factor)
        scale = takeoff.set_scale(session, ctx.deps.tender_id, me.id, document_id, page, line, length_m, dimension_text)
        return (
            f"Scale set: 1 metre is {1 / scale.metres_per_point / factor:.1f} view_page pixels, about "
            f"1:{takeoff.drawing_ratio(scale.metres_per_point):,} at the sheet's printed size. Check that against the "
            "scale in the title block, and measure a second known dimension before you rely on it."
        )


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
    Quantix computes the quantity from your points. Take the points from what you see: look at the sheet with
    view_page, then zoom in on each corner with its region before you place a point there."""
    if not ctx.deps.sees_images:
        return BLIND
    with _working(ctx, f"Measuring {label}") as (session, me):
        factor = _points(session, ctx.deps.tender_id, document_id, page)
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
        )
        q = takeoff.quantity(session, m)
        item = session.get(BoqItem, m.boq_item_id) if m.boq_item_id else None
    if q is None:
        return "Saved, but the sheet has no scale yet: set_scale first."
    report = f"Measured {label}: {q} {unit}."
    if item and item.quantity and takeoff.plain_unit(item.unit) == takeoff.plain_unit(unit):
        times = q / item.quantity
        if times > 3 or times < Decimal("0.33"):
            boq_quantity = f"{item.quantity:,.3f}".rstrip("0").rstrip(".")
            report += (
                f" That is {times:,.2f} times the BOQ quantity of {boq_quantity} {item.unit}: check the scale and "
                "your points before going on, and raise a concern if the BOQ looks wrong."
            )
    return report


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
        )
        return f"Item {boq_item} priced at {estimate.rate_of(rate)} per unit, for the Tender Manager's review."


def propose_markups(
    ctx: RunContext[Turn],
    preliminaries: list[estimate.PreliminaryIn],
    overheads: Decimal,
    profit: Decimal,
    adjustment: Decimal,
    note: str,
) -> str:
    """Propose the tender's markups: preliminaries as the site's own costs priced item by item (site staff for the
    programme's months, plant mobilisation, site facilities, insurances the tender requires, testing), overheads
    and profit as fractions (0.06 is 6%), and a lump-sum adjustment. Explain your assumptions in the note.
    Quantix totals the preliminaries and computes the price."""
    with _working(ctx, "Proposing the markups") as (session, me):
        estimate.propose_markups(session, ctx.deps.tender_id, me.id, preliminaries, overheads, profit, adjustment, note)
        total = estimate.summary(session, ctx.deps.tender_id).total
    return f"Proposed for the Tender Manager's review. The price with these markups is {total}."


def estimate_summary(ctx: RunContext[Turn]) -> str:
    """The price so far, as Quantix computes it: net cost, markups, total, VAT, and the items still unpriced."""
    with _working(ctx, "Checking the estimate") as (session, _):
        s = estimate.summary(session, ctx.deps.tender_id)
    lines = [
        f"{s.priced} of {s.items} items priced ({s.reviewing} with the Tender Manager, {s.waiting} waiting for the "
        "engineer).",
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
    The Tender Manager reviews it; the engineer chooses."""
    with _working(ctx, f"Recommending a quote for {package}") as (session, me):
        found = subcontract.find_package(session, ctx.deps.tender_id, package)
        subcontract.recommend(session, found, me.id, subcontract.find_company(session, company), reason)
    return "Your recommendation is with the Tender Manager for review."


def search_past_tenders(ctx: RunContext[Turn], words: str) -> str:
    """Rates the firm approved on its earlier tenders for similar items, with each tender's outcome and the date.
    Use them as benchmarks; check that a rate is still current before relying on it."""
    with _working(ctx, f"Looking up past tenders for “{words}”") as (session, _):
        found = company.past_rates(session, ctx.deps.tender_id, words)
    if not found:
        return "No earlier tender has an approved rate for items like that."
    return "\n".join(
        f"{p.tender} ({p.outcome}, {p.dated:%d %b %Y}) · {p.item} {p.description} · {p.rate} per {p.unit} ({p.basis})"
        for p in found
    )


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


def draft_work_schedule(
    ctx: RunContext[Turn],
    requirement: str,
    title: str,
    activities: list[submission.ActivityIn],
    sequence: str,
    overall_days: int,
) -> str:
    """Draft the work schedule for its checklist requirement. activities: every BOQ line with a quantity, with the
    output you assume for one crew in a day and the number of crews; Quantix takes the quantity from the BOQ, works
    out the days and writes them into the draft. sequence: the order of the work and the overlaps you plan, with
    your assumptions. overall_days: the whole programme in working days, from the lines and your overlaps."""
    with _working(ctx, f"Drafting {title}") as (session, me):
        found = submission.find_requirement(session, ctx.deps.tender_id, requirement)
        rows = submission.durations(session, ctx.deps.tender_id, activities)
        text = submission.schedule_text(rows, sequence, overall_days)
        schedule = submission.schedule_record(rows, overall_days)
        submission.draft(session, found, me.id, title, text, schedule=schedule)
    total = sum(r.days for r in rows)
    return f"{text}\n\nIt is with the Tender Manager for review. {total} days if every line ran one after another."


def draft_document(ctx: RunContext[Turn], requirement: str, title: str, text: str) -> str:
    """Draft a submission document for a checklist requirement, such as a method statement, a covering letter or a
    schedule. Base it on the tender documents and the approved figures; leave signatures and anything only the
    engineer can provide as clear blanks. A new draft replaces your earlier one."""
    with _working(ctx, f"Drafting {title}") as (session, me):
        found = submission.find_requirement(session, ctx.deps.tender_id, requirement)
        submission.draft(session, found, me.id, title, text)
    return "The draft is with the Tender Manager for review."


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


def review_queue(ctx: RunContext[Turn]) -> str:
    """Everything your staff proposed that you haven't reviewed yet, oldest first, one line each, with what
    Quantix's checks found in it. Look at the detail with review_details, then decide with review."""
    tender_id = ctx.deps.tender_id
    with _working(ctx, "Going through the review queue") as (session, me):
        waiting = reviews.pending(session, tender_id)
        me.reviewed_up_to = datetime.now(UTC)
        names = {m.id: m.first_name for m in records.team(session, tender_id, include_released=True)}
        lines = [
            reviews.describe(session, p, names) + reviews.flags(reviews.findings(session, ctx.deps.home, tender_id, p))
            for p in waiting[:60]
        ]
    if not lines:
        return "Nothing is waiting for your review."
    more = f"\n… and {len(waiting) - 60} more after these." if len(waiting) > 60 else ""
    return f"{len(waiting)} waiting for your review:\n" + "\n".join(lines) + more


def review_details(ctx: RunContext[Turn], records_to_check: list[str]) -> str:
    """The full detail of up to 10 records in your review queue, e.g. ["rate 4690fa4c", "draft 1cb82413"]: their
    source, build-up, measurement, text or levelling, and what Quantix's checks found. Check each against its pages
    before you decide."""
    with _working(ctx, "Checking the team's work") as (session, _):
        parts = []
        for ref in records_to_check[:10]:
            try:
                found = reviews.find(session, ctx.deps.tender_id, ref)
            except ValueError as error:
                parts.append(f"{ref}: {error}")
                continue
            checked = reviews.findings(session, ctx.deps.home, ctx.deps.tender_id, found)
            parts.append(f"{found.ref}:\n{reviews.details(session, found)}{reviews.findings_text(checked)}")
    return "\n\n".join(parts)


def review(ctx: RunContext[Turn], verdicts: list[reviews.Verdict]) -> str:
    """Decide on records in your review queue, as many as you like at once. Accept only what you would defend to
    the engineer, saying what you checked. You can't accept a record while Quantix finds a blocker in it; accept a
    warning only with warnings_reason saying why it needs no correction. Send back anything wrong, saying exactly
    what to correct: it goes to whoever made it, in the team room."""
    with _working(ctx, "Reviewing the team's work") as (session, me):
        return reviews.review(session, ctx.deps.home, ctx.deps.tender_id, me, verdicts, ctx.deps.autonomous)


def audit_tender(ctx: RunContext[Turn], accept_warnings: list[audit.Accepted] | None = None) -> str:
    """Audit the whole tender before you tell the engineer it is ready: work still waiting, missing facts, markups
    or rates, client BOQ rows left out, unread documents, the checklist, and what Quantix's checks find in the
    office's work. accept_warnings: warnings you accept, each by its short name with your reason. Clear every
    blocker, then run it again until nothing blocks the release."""
    with _working(ctx, "Auditing the tender") as (session, me):
        problems = audit.accept(session, ctx.deps.home, ctx.deps.tender_id, me, accept_warnings or [])
        text = audit.report(audit.open_findings(session, ctx.deps.home, ctx.deps.tender_id))
    return text + ("\nNot accepted: " + "; ".join(problems) if problems else "")


def escalate(
    ctx: RunContext[Turn], record: str, problem: str, sources: list[reviews.Source], suggestions: list[str]
) -> str:
    """Bring the engineer a problem the office can't settle: a record in your review queue that keeps coming back
    wrong, or one only the engineer can decide. problem: what is wrong and why the office can't settle it. sources:
    where it shows, each a document page or a BOQ line, with what the engineer will find there. suggestions: 1 to 4
    corrections you would make, each complete enough to act on. The record waits in your queue; the engineer's
    answer comes to your chat, and you apply it with review."""
    with _working(ctx, "Bringing a problem to the engineer") as (session, me):
        found = reviews.escalate(session, ctx.deps.tender_id, me, record, problem, sources, suggestions)
        title = found.title
    return f"Escalated to the engineer as “{title}”. Carry on with other work until they answer."


READ: list[Callable] = [
    list_documents,
    search_documents,
    read_page,
    view_page,
    find_on_page,
    post_to_team,
    message_engineer,
    raise_concern,
    list_boq,
    takeoff_summary,
    search_library,
    estimate_summary,
    search_directory,
    levelling,
    search_past_tenders,
    list_requirements,
]
PRODUCE: list[Callable] = [
    propose_boq_items,
    withdraw_boq_items,
    propose_fact,
    set_scale,
    measure,
    propose_rate,
    propose_markups,
    add_company,
    create_package,
    draft_enquiry,
    record_quote,
    recommend_quote,
    add_requirements,
    draft_work_schedule,
    draft_document,
    set_pricing_columns,
]
STAFF: list[Callable] = [*READ, *PRODUCE, complete_task]
# The Manager leads and reviews; he never produces records himself, so every record has a second pair of eyes
MANAGER: list[Callable] = [
    *READ,
    ask_engineer,
    hire,
    assign_task,
    release,
    review_queue,
    review_details,
    review,
    escalate,
    audit_tender,
]
