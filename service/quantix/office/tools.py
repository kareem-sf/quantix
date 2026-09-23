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
from quantix.office import records
from quantix.office.models import TEAM, Staff
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
]
STAFF: list[Callable] = [*COMMON, complete_task]
MANAGER: list[Callable] = [*COMMON, hire, assign_task, release]
