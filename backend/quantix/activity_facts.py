"""Plain-English facts about what each Tender tool actually did.

A fact is built only from a tool's real arguments and its real result, never
from a fixed per-tool label, so the engineer can follow the work as they would
follow a colleague's. Formatting must never break a run: any unexpected shape
falls back to a short generic line.
"""

from __future__ import annotations

import json
import re
from collections.abc import Callable
from typing import Any

# Tools that are internal bookkeeping and not engineering work.
HIDDEN_TOOLS = frozenset({"proposal_format", "quantix_submit_result", "search_tools"})

_MAX_FOUND = 5
_MAX_LINE = 220

Lookup = Callable[[str, str], str | None]

_PROPOSAL_WORDS = {
    "plan": "a work plan",
    "findings": "findings",
    "decisions": "decisions",
    "tasks": "tasks",
    "project_map_nodes": "project breakdown items",
    "submission_requirements": "submission requirements",
    "quantities": "quantities",
    "rates": "rates",
    "boq_items": "BOQ items",
    "document_needs": "document requests",
    # The kinds the propose tool stages.
    "takeoff": "takeoff lines",
    "boq_item_proposals": "BOQ rows",
    "quantity_proposals": "quantities",
    "unit_rate_proposals": "rates",
    "price_proposals": "market prices",
    "web_findings": "web findings",
    "quote_drafts": "supplier quote requests",
    "programme_proposal": "a programme",
    "draft_documents": "draft documents",
}


def _load(value: Any) -> Any:
    if isinstance(value, str):
        try:
            return json.loads(value)
        except ValueError:
            return value
    if isinstance(value, list) and value and all(isinstance(item, dict) for item in value):
        # Visual tools return [{"type": "text", "text": "{json}"}, {"type": "image"}].
        texts = [item.get("text") for item in value if item.get("type") == "text"]
        if texts and len(texts) == len([item for item in value if item.get("type") != "image"]):
            loaded = _load(texts[0])
            if isinstance(loaded, dict):
                return loaded
    return value


def _readable(text: str) -> bool:
    letters = sum(character.isalpha() for character in text)
    return len(text) >= 12 and letters / max(len(text), 1) > 0.5


def _lines(text: Any, limit: int = 3) -> list[str]:
    """The first few readable lines of extracted text, trimmed."""

    if not isinstance(text, str):
        return []
    found = []
    for raw in re.split(r"[\r\n]+", text):
        line = " ".join(raw.split())
        if _readable(line):
            found.append(line if len(line) <= _MAX_LINE else f"{line[: _MAX_LINE - 1]}…")
        if len(found) >= limit:
            break
    return found


def _count(value: int, one: str, many: str | None = None) -> str:
    return f"{value} {one if value == 1 else (many or one + 's')}"


def _pages(pages: list[int]) -> str | None:
    pages = sorted({page for page in pages if isinstance(page, int)})
    if not pages:
        return None
    if len(pages) == 1:
        return f"page {pages[0]}"
    return f"pages {pages[0]}–{pages[-1]}"


def _name(lookup: Lookup | None, kind: str, identifier: Any) -> str | None:
    if not lookup or not isinstance(identifier, str):
        return None
    try:
        return lookup(kind, identifier)
    except Exception:
        return None


def _fact(kind: str, line: str, **values: Any) -> dict[str, Any]:
    fact = {"kind": kind, "line": line}
    for key, value in values.items():
        if value not in (None, "", [], {}):
            fact[key] = value
    return fact


def _running(tool: str, inputs: dict, lookup: Lookup | None) -> dict[str, Any]:
    document = _name(lookup, "artifact", inputs.get("artifact_id"))
    if tool in {"read_whole_document", "read_document"}:
        return _fact("read", "Reading", subject=document or "a document")
    if tool == "read_source":
        return _fact(
            "read", "Reading a passage", subject=_name(lookup, "source", inputs.get("source_id"))
        )
    if tool == "search_sources":
        return _fact("search", "Searching the documents")
    if tool == "view_document_page":
        page = inputs.get("page")
        return _fact(
            "view",
            "Looking at",
            subject=document or "a drawing",
            result=f"page {page}" if isinstance(page, int) else None,
        )
    if tool == "hire_staff":
        return _fact("hire", "Hiring", subject=inputs.get("name"), result=inputs.get("role"))
    if tool == "assign_work":
        return _fact(
            "assign",
            "Handing work to",
            subject=_name(lookup, "staff", inputs.get("staff_id")) or "a staff member",
            result=inputs.get("title"),
        )
    if tool.startswith("calculate") or tool.startswith("check_engineering"):
        return _fact("calculate", "Calculating")
    if tool.startswith("save"):
        return _fact("save", "Saving", subject=inputs.get("title"))
    if tool == "propose":
        return _fact("propose", "Preparing records for your review")
    return _fact("check", _generic_line(tool, running=True))


def _generic_line(tool: str, *, running: bool = False) -> str:
    words = tool.replace("_", " ").strip()
    verbs = {
        "inspect": ("Checking", "Checked"),
        "list": ("Checking", "Checked"),
        "read": ("Reading", "Read"),
        "check": ("Checking", "Checked"),
        "compare": ("Comparing", "Compared"),
        "trace": ("Tracing", "Traced"),
        "rehearse": ("Checking", "Checked"),
        "answer": ("Answering", "Answered"),
    }
    first, _, rest = words.partition(" ")
    if first in verbs:
        return f"{verbs[first][0 if running else 1]} {rest}".strip()
    return words[:1].upper() + words[1:]


def _completed(tool: str, inputs: dict, outputs: Any, lookup: Lookup | None) -> dict[str, Any]:
    out = _load(outputs)
    data = out if isinstance(out, dict) else {}

    if tool in {"read_whole_document", "read_document"}:
        passages = (
            data.get("passages")
            if isinstance(data.get("passages"), list)
            else (out if isinstance(out, list) else [])
        )
        passages = [item for item in passages if isinstance(item, dict)]
        name = next(
            (item.get("artifact_name") for item in passages if item.get("artifact_name")), None
        )
        name = name or _name(lookup, "artifact", inputs.get("artifact_id"))
        found = []
        for passage in passages:
            found.extend(_lines(passage.get("text"), 2))
            if len(found) >= 3:
                break
        pages = [item.get("page") for item in passages]
        first = next((item for item in passages if isinstance(item.get("page"), int)), None)
        return _fact(
            "read",
            "Read",
            subject=name or "a document",
            result=_pages(pages) or ("no readable text" if not passages else None),
            found=found[:3],
            details=["More pages remain to be read."] if data.get("next_offset") else None,
            open={"artifact_id": inputs.get("artifact_id"), "page": first.get("page")}
            if first and inputs.get("artifact_id")
            else None,
        )

    if tool == "read_source":
        passage = data
        page = passage.get("page")
        name = passage.get("artifact_name") or _name(lookup, "source", inputs.get("source_id"))
        if passage.get("already_returned"):
            return _fact("read", "Already had a passage from", subject=name or "a document")
        return _fact(
            "read",
            "Read a passage in",
            subject=name or "a document",
            result=f"page {page}" if isinstance(page, int) else passage.get("sheet"),
            found=_lines(passage.get("text"), 3),
            open={"source_id": passage.get("id"), "page": page} if passage.get("id") else None,
        )

    if tool == "search_sources":
        hits = [
            item
            for item in (out if isinstance(out, list) else data.get("results") or [])
            if isinstance(item, dict)
        ]
        files: dict[str, list[int]] = {}
        for hit in hits:
            files.setdefault(str(hit.get("artifact_name") or "a document"), []).append(
                hit.get("page")
            )
        found = [
            f"{name} · {_pages(pages) or 'found'}"
            for name, pages in list(files.items())[:_MAX_FOUND]
        ]
        if len(files) > _MAX_FOUND:
            found.append(f"…and {_count(len(files) - _MAX_FOUND, 'more file')}")
        query = inputs.get("query")
        return _fact(
            "search",
            "Searched the documents",
            result=f"{_count(len(hits), 'match', 'matches')} in {_count(len(files), 'file')}"
            if hits
            else "nothing found",
            found=found,
            details=[f"Search terms: “{query}”" + (" (exact words)" if inputs.get("exact") else "")]
            if query
            else None,
        )

    if tool == "view_document_page":
        page = data.get("page") or inputs.get("page")
        return _fact(
            "view",
            "Looked at",
            subject=data.get("artifact_name")
            or _name(lookup, "artifact", inputs.get("artifact_id"))
            or "a drawing",
            result=f"page {page}" if isinstance(page, int) else None,
            open={"artifact_id": inputs.get("artifact_id"), "page": page}
            if inputs.get("artifact_id")
            else None,
        )

    if tool == "read_package_map":
        if not data.get("available"):
            return _fact("check", "Checked the package overview", result="not prepared yet")
        identity = data.get("identity") or {}
        found = [
            f"{label}: {value}"
            for label, value in (
                ("Project", identity.get("name")),
                ("Client", identity.get("client")),
                ("Location", identity.get("location")),
                ("Reference", identity.get("reference")),
            )
            if isinstance(value, str) and value
        ]
        return _fact(
            "check",
            "Checked the package overview",
            result=None if data.get("current", True) else "out of date",
            found=[
                line if len(line) <= _MAX_LINE else f"{line[: _MAX_LINE - 1]}…" for line in found
            ],
        )

    if tool == "list_documents":
        documents = [item for item in data.get("documents") or [] if isinstance(item, dict)]
        total = data.get("total", len(documents))
        names = [str(item.get("name")) for item in documents[:_MAX_FOUND] if item.get("name")]
        if total > len(names):
            names.append(f"…and {_count(total - len(names), 'more')}")
        return _fact(
            "check", "Checked the document list", result=_count(total, "document"), found=names
        )

    if tool == "inspect_extraction_coverage":
        totals = data.get("totals") or {}
        documents = totals.get("documents")
        extracted = totals.get("extracted")
        attention = totals.get("needs_attention") or 0
        problems = []
        for item in data.get("documents") or []:
            if not isinstance(item, dict) or item.get("status") == "extracted":
                continue
            warnings = item.get("other_warnings") or []
            reason = (
                warnings[0].get("message") if warnings and isinstance(warnings[0], dict) else None
            )
            problems.append(
                f"{item.get('name')}: {reason or item.get('status', '').replace('_', ' ')}"
            )
        result = (
            f"{extracted} of {_count(documents, 'document')} read"
            if isinstance(documents, int) and isinstance(extracted, int)
            else None
        )
        if result and attention:
            result += f", {attention} need a second look"
        return _fact(
            "check",
            "Checked which documents could be read",
            result=result,
            found=problems[:_MAX_FOUND],
        )

    if tool in {"inspect_estimate", "check_estimate_coverage"}:
        summary = data.get("summary") or {}
        rows = data.get("total_items", summary.get("boq_rows"))
        return _fact(
            "check",
            "Checked the estimate",
            result=_count(rows, "BOQ row")
            if isinstance(rows, int) and rows
            else "no BOQ rows saved yet",
            found=[str(reason) for reason in (data.get("blocking_reasons") or [])[:3]],
        )

    if tool == "inspect_submission_requirements":
        requirements = [item for item in data.get("requirements") or [] if isinstance(item, dict)]
        return _fact(
            "check",
            "Checked submission requirements",
            result=_count(len(requirements), "saved requirement")
            if requirements
            else "none saved yet",
            found=[
                str(item.get("title")) for item in requirements[:_MAX_FOUND] if item.get("title")
            ],
        )

    if tool == "inspect_project_map":
        total = data.get("total_nodes")
        return _fact(
            "check",
            "Checked the project breakdown",
            result=_count(total, "item")
            if isinstance(total, int) and total
            else "nothing mapped yet",
        )

    if tool == "inspect_tender_records":
        kind = str(data.get("record_type") or inputs.get("record_type") or "records").replace(
            "_", " "
        )
        records = data.get("records") or []
        return _fact(
            "check",
            f"Checked earlier {kind}",
            result=_count(len(records), "record") if records else "none yet",
        )

    if tool == "list_work_products":
        items = [item for item in data.get("items") or [] if isinstance(item, dict)]
        return _fact(
            "check",
            "Checked earlier drafts",
            result=_count(len(items), "draft") if items else "none yet",
            found=[str(item.get("title")) for item in items[:_MAX_FOUND] if item.get("title")],
        )

    if tool in {"read_work_product", "save_work_product"}:
        version = data.get("version")
        return _fact(
            "read" if tool.startswith("read") else "save",
            "Read the draft" if tool.startswith("read") else "Saved the draft",
            subject=data.get("title") or inputs.get("title"),
            result=f"version {version}" if isinstance(version, int) else None,
            found=_lines(data.get("content"), 3),
        )

    if tool == "save_work_brief":
        steps = [item for item in inputs.get("steps") or [] if isinstance(item, dict)]
        words = {
            "to_do": "to do",
            "in_progress": "in progress",
            "done": "done",
            "blocked": "blocked",
        }
        return _fact(
            "save",
            "Updated the plan",
            result=_count(len(steps), "step") if steps else None,
            found=[
                f"{item.get('title')} · {words.get(item.get('state'), item.get('state'))}"
                for item in steps[:8]
            ],
        )

    if tool == "list_reusable_notes":
        notes = [item for item in data.get("notes") or [] if isinstance(item, dict)]
        return _fact(
            "check",
            "Checked company guidance",
            result=_count(len(notes), "note") if notes else "none saved yet",
            found=[str(item.get("title")) for item in notes[:_MAX_FOUND] if item.get("title")],
        )

    if tool == "read_reusable_note":
        return _fact(
            "read",
            "Read company guidance",
            subject=data.get("title"),
            found=_lines(data.get("content"), 3),
        )

    if tool == "list_team":
        staff = [item for item in data.get("staff") or [] if isinstance(item, dict)]
        names = [
            f"{item.get('name')} · {item.get('role')}"
            if item.get("role")
            else str(item.get("name"))
            for item in staff
            if item.get("name")
        ]
        return _fact(
            "check",
            "Checked the team",
            result=_count(len(staff), "staff member") if staff else "no staff hired yet",
            found=names[:_MAX_FOUND],
        )

    if tool == "hire_staff":
        return _fact(
            "hire",
            "Hired",
            subject=data.get("name") or inputs.get("name"),
            result=data.get("role") or inputs.get("role"),
        )

    if tool == "assign_work":
        return _fact(
            "assign",
            "Handed work to",
            subject=_name(lookup, "staff", inputs.get("staff_id")) or "a staff member",
            result=inputs.get("title"),
            found=_lines(inputs.get("brief"), 2),
            assignment_id=data.get("assignment_id"),
        )

    if tool == "propose":
        kind = str(data.get("kind") or inputs.get("kind") or "")
        total = data.get("staged_total")
        if not isinstance(total, int):
            total = len(inputs.get("items") or [])
        words = _PROPOSAL_WORDS.get(kind, kind.replace("_", " ") or "records")
        titles = [
            str(item.get("title"))
            for item in (inputs.get("items") or [])
            if isinstance(item, dict) and item.get("title")
        ]
        return _fact(
            "propose",
            "Prepared for your review",
            result=words if kind in {"plan", "programme_proposal"} else f"{total} {words}",
            found=titles[:_MAX_FOUND],
        )

    if tool.startswith("calculate") or tool.startswith("check_engineering"):
        return _fact(
            "calculate", "Calculated", subject=inputs.get("title") or inputs.get("description")
        )

    return _fact("check", _generic_line(tool))


def fact_for(
    tool: str,
    phase: str,
    inputs: Any = None,
    outputs: Any = None,
    *,
    error: str | None = None,
    recoverable: bool = False,
    lookup: Lookup | None = None,
) -> dict[str, Any] | None:
    """Describe one tool call for the engineer, or None for internal tools."""

    if tool in HIDDEN_TOOLS:
        return None
    arguments = inputs if isinstance(inputs, dict) else {}
    try:
        if phase in {"prepared", "started"}:
            fact = _running(tool, arguments, lookup)
            fact["state"] = "running"
        elif phase == "completed":
            fact = _completed(tool, arguments, outputs, lookup)
            fact["state"] = "done"
        else:
            fact = _running(tool, arguments, lookup)
            verb = fact["line"]
            fact["line"] = f"Couldn't finish: {verb[:1].lower()}{verb[1:]}"
            fact["state"] = "failed"
            if error and recoverable:
                # The AI's own mistake in a call, which it is told to correct: the
                # exact complaint is there for anyone who wants it, not the headline.
                fact["result"] = "needed a correction"
                fact["details"] = [error if len(error) <= 600 else f"{error[:599]}…"]
            elif error:
                fact["result"] = error if len(error) <= _MAX_LINE else f"{error[: _MAX_LINE - 1]}…"
            if recoverable:
                fact["recoverable"] = True
    except Exception:
        fact = {
            "kind": "check",
            "line": _generic_line(tool, running=phase in {"prepared", "started"}),
            "state": "running"
            if phase in {"prepared", "started"}
            else "done"
            if phase == "completed"
            else "failed",
        }
    return fact
