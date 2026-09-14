"""Stage records for engineer review during a run; nothing is saved until the run finishes."""

from __future__ import annotations

import json
import types
import typing
from typing import Literal

from pydantic import TypeAdapter, ValidationError

from .ai_tools import ToolArgumentError, ToolContext, argument_problem, tool
from .office_tools import OfficeContext
from .office_types import OfficeOutput, OfficeProposals

ProposalKind = Literal[
    "plan",
    "boq_item_proposals",
    "quantity_proposals",
    "drawing_measurements",
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

RULES: dict[str, str] = {
    "plan": (
        "A work plan the engineer approves before it starts: up to 12 tasks, each with a role and the "
        "source IDs it rests on. After approval you carry it out with your team. Propose the plan and "
        "document needs first; drafts come after approval."
    ),
    "boq_item_proposals": (
        "Rows from a BOQ supplied as PDF or Word. Use the exact read source ID and excerpt, a stable unique "
        "row reference, the description, and the quantity and unit as written. Several rows may cite the same "
        "page. Inspect saved estimate rows first to avoid duplicates. When replacing a row affected by a "
        "revision, set replaces_item_id to the exact earlier row ID from the estimate's retired_source_rows; "
        "repeated labels on different pages are separate items. These create unconfirmed rows; they never "
        "confirm a quantity or install a rate, and a calculated quantity never replaces the supplied one."
    ),
    "quantity_proposals": (
        "Calculated quantities such as volumes, grouped items or dimensional build-ups, each linked to a BOQ "
        "item you inspected in this run. State dimensions, units, arithmetic, scope, deductions and assumptions "
        "in the calculation, with read source IDs. They never change the supplied BOQ quantity."
    ),
    "drawing_measurements": (
        "Takeoff measurements from drawings. Inspect every measured region with view_document_page, read the "
        "printed scale or a dimension to calibrate, and calculate with calculate_drawing_measurement. Return "
        "its geometry and the supporting source IDs. Never invent a scale or call a marked sample a complete "
        "takeoff."
    ),
    "unit_rate_proposals": (
        "Proposed installed unit rates for BOQ items you read with inspect_estimate in this run, with their "
        "provenance: read source IDs and any web search URLs from this run. Keep market prices separate from "
        "installed rates. They never install a rate."
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


def _item_type(kind: str):
    annotation = OfficeProposals.model_fields[kind].annotation
    if isinstance(annotation, types.UnionType) or typing.get_origin(annotation) is typing.Union:
        annotation = next(arg for arg in typing.get_args(annotation) if arg is not type(None))
    if typing.get_origin(annotation) is list:
        annotation = typing.get_args(annotation)[0]
    return annotation


def proposal_tools() -> list:
    @tool(read_only=False, idempotent=True)
    async def propose(ctx: ToolContext[OfficeContext], kind: ProposalKind, items: list[dict]) -> str:
        """Stage records for the engineer to review: a plan, BOQ rows, quantities, drawing measurements, unit rates, market prices, web findings, quote drafts, submission requirements, project map items, a programme or draft documents. Call proposal_format first for the fields and rules of a kind. Each call replaces what you staged for that kind, so pass the complete list (one item for plan and programme_proposal; an empty list withdraws). Staged records are checked now and saved only when your run finishes. Nothing is approved."""
        context = ctx.context
        if kind in SINGLE and len(items) > 1:
            raise ToolArgumentError(f"Pass exactly one {kind} item.")
        staged = dict(context.proposals)
        if kind in SINGLE:
            staged[kind] = items[0] if items else None
        else:
            staged[kind] = items
        try:
            proposals = OfficeProposals.model_validate(staged)
        except ValidationError as error:
            raise ToolArgumentError(argument_problem(error)) from None
        from .office import validate_proposals

        try:
            validate_proposals(OfficeOutput(summary="Staged proposals", **proposals.model_dump()), context)
        except (KeyError, ValueError) as error:
            raise ToolArgumentError(str(error.args[0] if error.args else error)) from None
        context.proposals = {key: value for key, value in staged.items() if value}
        return json.dumps({"kind": kind, "staged": len(items),
                           "detail": "Staged. It is saved for the engineer's review when the run finishes."})

    @tool(read_only=True, idempotent=True)
    async def proposal_format(ctx: ToolContext[OfficeContext], kind: ProposalKind) -> str:
        """Return the fields and rules for one kind of record before you call propose."""
        schema = TypeAdapter(_item_type(kind)).json_schema()
        return json.dumps({"kind": kind, "one_item": kind in SINGLE, "rules": RULES[kind], "item_schema": schema},
                          ensure_ascii=False)

    return [propose, proposal_format]
