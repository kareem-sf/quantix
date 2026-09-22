"""Stage records for engineer review during a run or assignment; nothing is saved until the work finishes."""

from __future__ import annotations

import json
import re
import types
import typing
from typing import Literal

from pydantic import TypeAdapter, ValidationError

from .ai_tools import ToolArgumentError, ToolContext, argument_problem, tool
from .office_tools import OfficeContext
from .office_types import OfficeOutput, OfficeProposals

ProposalKind = Literal[
    "plan",
    "takeoff",
    "boq_item_proposals",
    "quantity_proposals",
    "unit_rate_proposals",
    "price_proposals",
    "web_findings",
    "quote_drafts",
    "submission_requirements",
    "project_map_nodes",
    "programme_proposal",
    "draft_documents",
]

SINGLE = {"plan", "programme_proposal"}
# Staff stage records inside their own assignment; plans, commercial and web records stay with the Manager.
STAFF_KINDS = {
    "takeoff",
    "boq_item_proposals",
    "quantity_proposals",
    "submission_requirements",
    "project_map_nodes",
}

RULES: dict[str, str] = {
    "takeoff": (
        "One line per BOQ item, or per item of work the BOQ does not include, with its location (building, "
        "floor, grid or zone). Take quantities from printed dimensions and schedules first. Scale from the "
        "drawing with calculate_drawing_measurement only where nothing is dimensioned, and count symbols by "
        "viewing each region with view_document_page. Do the arithmetic with calculate_engineering and write it "
        "in working with deductions, laps, waste and assumptions. Cite the drawing pages you viewed and any "
        "schedule or specification you read. Set boq_item_id to the BOQ item you read with inspect_estimate and "
        "give the quantity in that item's unit; Quantix compares the two. Leave boq_item_id empty for work shown "
        "on the drawings that has no BOQ item: these are the missing items. For a BOQ item whose work you "
        "cannot find on the drawings, give its boq_item_id with no quantity and say what you checked."
    ),
    "plan": (
        "A work plan the engineer approves before it starts: up to 12 tasks, each with a role and the "
        "source IDs it rests on. After approval you carry it out with your team. Propose the plan and "
        "document needs first; drafts come after approval."
    ),
    "boq_item_proposals": (
        "Rows from a BOQ supplied as PDF or Word. Stage them as you go, about 25 rows per propose call "
        "after each page or two you read; never hold a whole BOQ for one call or write the rows out in your "
        "thinking first. Use the exact read source ID and excerpt, a stable unique "
        "row reference, and the quantity and unit as written. A long row's excerpt may be shortened with ... "
        "between exact pieces, ending with the piece that holds its unit and quantity; the words between are "
        "taken from the passage. Leave description out when the excerpt reads "
        "clearly: the row then uses the excerpt without its item number, unit and quantity. Write a "
        "description only to make a garbled excerpt readable. Several rows may cite the same "
        "page. Inspect saved estimate rows first to avoid duplicates. After a file is read again, propose its "
        "rows again from the new reading with the same row references: each replaces the earlier row the "
        "engineer has not decided yet, so no row IDs are needed. When replacing a row affected by a new file "
        "revision, set replaces_item_id to the exact earlier row ID from the estimate's retired_source_rows; "
        "repeated labels on different pages are separate items. These create unconfirmed rows; they never "
        "confirm a quantity or install a rate, and a calculated quantity never replaces the supplied one."
    ),
    "quantity_proposals": (
        "Calculated quantities such as volumes, grouped items or dimensional build-ups, each linked to a BOQ "
        "item you inspected in this run. State dimensions, units, arithmetic, scope, deductions and assumptions "
        "in the calculation, with read source IDs. They never change the supplied BOQ quantity."
    ),
    "unit_rate_proposals": (
        "Proposed installed unit rates for BOQ items you read with inspect_estimate in this run, with their "
        "provenance: read source IDs and any web search URLs from this run. Each has either unit_rate or "
        "components, never both; keep component names short. Keep market prices separate from installed "
        "rates. They never install a rate."
    ),
    "price_proposals": (
        "Market prices from native web search. Label observed quotations separately from estimates and give "
        "unit, location, currency, tax basis, observation date, validity and conditions. Cite only URLs the "
        "search returned; a search result is not a binding quotation. Never use a publication date as the "
        "observation date."
    ),
    "web_findings": (
        "Facts found with native web search, citing the exact URLs the search returned. Keep source_ids for "
        "tender evidence only."
    ),
    "quote_drafts": (
        "Complete supplier quotation requests the engineer sends from their own mail program. Recipients must "
        "appear in read tender evidence, the engineer's request or this run's web findings. Attach only current "
        "originals you inspected."
    ),
    "submission_requirements": (
        "What the submission must contain, from tender evidence. Copy each complete clause into source_quote, "
        "state applicability as unconditional or conditional, and copy conditions and exceptions verbatim; keep "
        "words such as if, unless and where applicable. Never infer applicability from a document title. "
        "Inspect existing requirements first to avoid duplicates."
    ),
    "project_map_nodes": (
        "Source-backed buildings, areas, disciplines, work items and requirements. Inspect the project map "
        "first; parent IDs must be existing map items."
    ),
    "programme_proposal": (
        "A construction programme when one is requested: an explicit working calendar, realistic dependencies "
        "and durations, and source IDs or clear assumptions for every activity. Do not derive activities from "
        "the plan's task list."
    ),
    "draft_documents": (
        "Routine review documents generated from saved work after the run, only inside an approved work plan. "
        "They are drafts, never releases. Client-format BOQ mappings stay an engineer action."
    ),
}


# Field names models commonly use for these records instead of the real ones.
# Fields a model adds that the record does not keep; dropping them is safe because
# the same wording stays in detail or the quote.
_IGNORED: dict[str, set[str]] = {"submission_requirements": {"language", "format", "copies"}}

_ALIASES: dict[str, dict[str, str]] = {
    "boq_item_proposals": {
        "item_number": "row_reference",
        "item_no": "row_reference",
        "item": "row_reference",
        "item_ref": "row_reference",
        "reference": "row_reference",
        "ref": "row_reference",
        "row": "row_reference",
        "row_ref": "row_reference",
        "excerpt": "source_excerpt",
        "quote": "source_excerpt",
        "source_quote": "source_excerpt",
        "source_text": "source_excerpt",
        "qty": "quantity",
        "source": "source_id",
        "evidence_id": "source_id",
    },
}


def _decimal_text(value) -> str:
    from decimal import Decimal

    text = format(Decimal(repr(value)) if isinstance(value, float) else Decimal(value), "f")
    return text.rstrip("0").rstrip(".") if "." in text else text


_DATE = re.compile(r"\d{4}-\d{2}-\d{2}")


def _list_fields(kind: str) -> set[str]:
    schema = TypeAdapter(_item_type(kind)).json_schema()
    fields = set()
    for name, spec in schema.get("properties", {}).items():
        options = spec.get("anyOf", [spec])
        if any(option.get("type") == "array" for option in options):
            fields.add(name)
    return fields


def _string_fields(kind: str) -> set[str]:
    schema = TypeAdapter(_item_type(kind)).json_schema()
    fields = set()
    for name, spec in schema.get("properties", {}).items():
        options = spec.get("anyOf", [spec])
        if any(option.get("type") == "string" for option in options):
            fields.add(name)
    return fields


def normalize_items(kind: str, items: list) -> list:
    """Accept obvious near-misses before validation: known alias names, a single
    source ID sent as a list, and numbers sent where the record keeps text."""

    aliases = _ALIASES.get(kind, {})
    try:
        strings = _string_fields(kind)
        names = set(TypeAdapter(_item_type(kind)).json_schema().get("properties", {}))
    except Exception:  # noqa: BLE001 - validation below reports the real problem
        return items
    normalized = []
    ignored = _IGNORED.get(kind, set())
    lists = _list_fields(kind)
    for item in items:
        if not isinstance(item, dict):
            normalized.append(item)
            continue
        item = {key: value for key, value in item.items() if key not in ignored}
        fixed: dict = {}
        for key, value in item.items():
            name = key
            if key not in names:
                if key == "source_ids" and "source_id" in names and "source_ids" not in names:
                    if isinstance(value, list) and len(value) == 1:
                        name, value = "source_id", value[0]
                else:
                    name = aliases.get(key, key)
            if name in fixed and name != key:
                continue
            if name in strings and isinstance(value, (int, float)) and not isinstance(value, bool):
                value = _decimal_text(value)
            if name in lists and isinstance(value, str):
                # One item written as text where the record keeps a list.
                value = [value] if value.strip() else []
            if name == "due_date" and isinstance(value, str) and not _DATE.fullmatch(value.strip()):
                # A year or "with the offer" is not a date; the wording stays in detail.
                continue
            fixed[name] = value
        normalized.append(fixed)
    return normalized


# Records that name a saved BOQ row by its id.
_ROW_KINDS = frozenset({"unit_rate_proposals", "quantity_proposals"})


def resolve_item_numbers(kind: str, items: list, context) -> list:
    """Rates and quantities name a BOQ row by its id. Models often send the item
    number ("1.1") they see instead; accept it when it names exactly one row
    read in this job, so the check does not send them back to read it again."""

    read = list(getattr(context, "item_bases", {}) or {})
    if kind not in _ROW_KINDS or not read:
        return items
    with context.repo.db.connect() as conn:
        numbers = {
            identifier: str(reference or "").strip().casefold()
            for identifier, reference in conn.execute(
                "SELECT id, json_extract(data_json,'$.row_reference') FROM boq_items "
                f"WHERE id IN ({','.join('?' * len(read))})",
                read,
            )
        }
    resolved = []
    for item in items:
        named = item.get("item_id") if isinstance(item, dict) else None
        if isinstance(named, str) and named not in numbers:
            matches = [
                identifier
                for identifier, number in numbers.items()
                if number and number == named.strip().casefold()
            ]
            if len(matches) == 1:
                item = {**item, "item_id": matches[0]}
        resolved.append(item)
    return resolved


def field_guide(kind: str) -> str:
    """The item fields for a kind, so one correction can fix every field at once."""

    schema = TypeAdapter(_item_type(kind)).json_schema()
    required = set(schema.get("required", []))
    parts = []
    for name, spec in schema.get("properties", {}).items():
        options = spec.get("anyOf", [spec])
        kinds = sorted({option.get("type", "object") for option in options} - {"null"})
        label = "/".join(kinds) or "value"
        if any("pattern" in option for option in options):
            label = 'number written as text, e.g. "12.5"'
        parts.append(f"{name} ({label}{', required' if name in required else ''})")
    return f"Each {kind} item has exactly these fields: " + "; ".join(parts) + "."


def _item_type(kind: str):
    annotation = OfficeProposals.model_fields[kind].annotation
    if isinstance(annotation, types.UnionType) or typing.get_origin(annotation) is typing.Union:
        annotation = next(arg for arg in typing.get_args(annotation) if arg is not type(None))
    if typing.get_origin(annotation) is list:
        annotation = typing.get_args(annotation)[0]
    return annotation


def proposal_tools() -> list:
    @tool(read_only=False)
    async def propose(
        ctx: ToolContext[OfficeContext],
        kind: ProposalKind,
        items: list[dict],
        replace: bool = False,
    ) -> str:
        """Stage records for the engineer to review: a plan, takeoff lines, BOQ rows, quantities, unit rates, market prices, web findings, quote drafts, submission requirements, project map items, a programme or draft documents. Call proposal_format first for the fields and rules of a kind. Items are added to what you already staged for that kind; pass replace=true to replace that kind's list, or replace=true with no items to withdraw it. A plan or programme is always replaced. Everything staged is checked now and saved for review when your work finishes. Nothing is approved."""
        context = ctx.context
        if context.is_staff and kind not in STAFF_KINDS:
            raise ToolArgumentError(
                f"Staff stage {', '.join(sorted(STAFF_KINDS))}. Report anything else to the Tender Manager in your findings."
            )
        if kind in SINGLE and len(items) > 1:
            raise ToolArgumentError(f"Pass exactly one {kind} item.")
        items = resolve_item_numbers(kind, normalize_items(kind, items), context)
        staged = dict(context.proposals)
        if kind in SINGLE:
            staged[kind] = items[0] if items else None
        else:
            staged[kind] = list(items) if replace else [*staged.get(kind, []), *items]
        try:
            proposals = OfficeProposals.model_validate(staged)
        except ValidationError as error:
            raise ToolArgumentError(f"{argument_problem(error)} {field_guide(kind)}") from None
        from .office import validate_proposals

        try:
            validate_proposals(
                OfficeOutput(summary="Staged proposals", **proposals.model_dump()), context
            )
        except (KeyError, ValueError) as error:
            raise ToolArgumentError(str(error.args[0] if error.args else error)) from None
        context.proposals = {key: value for key, value in staged.items() if value}
        current = staged[kind]
        return json.dumps(
            {
                "kind": kind,
                "staged_total": len(current)
                if isinstance(current, list)
                else int(current is not None),
                "detail": "Staged. It is saved for the engineer's review when your work finishes.",
            }
        )

    @tool(read_only=True, idempotent=True)
    async def proposal_format(ctx: ToolContext[OfficeContext], kind: ProposalKind) -> str:
        """Return the fields and rules for one kind of record before you call propose."""
        schema = TypeAdapter(_item_type(kind)).json_schema()
        return json.dumps(
            {"kind": kind, "one_item": kind in SINGLE, "rules": RULES[kind], "item_schema": schema},
            ensure_ascii=False,
        )

    return [propose, proposal_format]
