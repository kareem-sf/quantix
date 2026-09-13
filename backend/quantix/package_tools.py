"""Read-only estimate and submission readiness checks for the Tender Manager.

They summarise what the estimate and the submission package still need, using
the same services the engineer's screens use. They never price, approve,
export or send anything.
"""

from __future__ import annotations

import json
import re
from collections import defaultdict

from .ai_tools import ToolArgumentError, ToolContext
from .office_tools import OfficeContext, safe_text, scoped_tool

_ESTIMATE_NOTE = (
    "BOQ rows include Excel candidates and source-based proposals. A complete check here does not prove the "
    "bill covers the whole scope; compare it with the specification and drawings."
)
_REHEARSAL_NOTE = (
    "This is a rehearsal only. It exports nothing, approves nothing and does not submit the Tender. "
    "Requirement approval, completion reviews and final release remain the engineer's decisions."
)


def _normalised(text) -> str:
    return re.sub(r"\s+", " ", str(text or "")).strip().casefold()


def package_tools():
    @scoped_tool
    async def check_estimate_coverage(
        ctx: ToolContext[OfficeContext], offset: int = 0, limit: int = 30
    ) -> str:
        """Check what the estimate still lacks: BOQ rows without a rate or usable quantity, unconfirmed rows, unknown VAT, possible duplicate rows, pending quantity and rate proposals, and whether totals are complete. Returns counts plus the rows that need attention."""
        office = ctx.context
        office.require_tool("check_estimate_coverage")
        office.ensure_scope_current()
        if office.is_staff:
            raise ValueError(
                "The estimate check covers the whole Tender and is available to the Tender Manager."
            )
        if offset < 0 or not 1 <= limit <= 50:
            raise ToolArgumentError("Choose a non-negative offset and a limit of 1–50.")
        from .estimates import EstimateService

        estimates = EstimateService(office.repo)
        view = estimates.view(office.tender_id)
        items = view["items"]
        pending_rates: dict[str, list[dict]] = defaultdict(list)
        for proposal in estimates.list_rate_proposals(office.tender_id):
            if proposal["status"] == "proposed":
                pending_rates[proposal["item_id"]].append(
                    {
                        "id": proposal["id"],
                        "is_current": proposal["is_current"],
                        "currency": proposal["payload"].get("currency"),
                    }
                )
        groups: dict[tuple[str, str], list[str]] = defaultdict(list)
        for item in items:
            if _normalised(item.get("description")):
                groups[
                    (_normalised(item.get("description")), _normalised(item.get("unit")))
                ].append(item["id"])
        duplicate_of = {
            identifier: [other for other in ids if other != identifier]
            for ids in groups.values()
            if len(ids) > 1
            for identifier in ids
        }
        attention = []
        for item in items:
            needs = []
            if item.get("unit_rate") is None:
                needs.append("no_rate")
            if item.get("effective_quantity") is None:
                needs.append("no_quantity")
            if not item.get("confirmed"):
                needs.append("unconfirmed")
            if item.get("vat_percent") is None or item.get("tax_basis") == "unknown":
                needs.append("unknown_vat")
            if not item.get("unit"):
                needs.append("no_unit")
            if item["id"] in duplicate_of:
                needs.append("possible_duplicate")
            pending_quantities = [
                proposal["id"]
                for proposal in item.get("quantity_proposals", [])
                if proposal.get("status") in {"proposed", "needs_review"}
            ]
            if not needs and not pending_quantities and not pending_rates.get(item["id"]):
                continue
            attention.append(
                {
                    "item_id": item["id"],
                    "description": safe_text(item.get("description"), 240),
                    "unit": item.get("unit"),
                    "effective_quantity": item.get("effective_quantity"),
                    "quantity_basis": item.get("quantity_basis"),
                    "unit_rate": item.get("unit_rate"),
                    "currency": item.get("currency"),
                    "needs": needs,
                    "issues": [safe_text(issue, 200) for issue in item.get("issues", [])][:5],
                    "possible_duplicate_of": duplicate_of.get(item["id"], [])[:5],
                    "pending_quantity_proposals": pending_quantities[:5],
                    "pending_rate_proposals": pending_rates.get(item["id"], [])[:5],
                }
            )
        summary = {
            "boq_rows": len(items),
            "rows_needing_attention": len(attention),
            "unpriced": view["unpriced_count"],
            "unconfirmed": view["unconfirmed_count"],
            "unknown_vat": view["unknown_vat_count"],
            "unresolved_quantity": view["unresolved_quantity_count"],
            "possible_duplicates": len(duplicate_of),
            "pending_rate_proposals": sum(len(rows) for rows in pending_rates.values()),
            "estimate_complete": view["complete"],
            "refresh_required": view["refresh_required"],
        }
        return json.dumps(
            {
                "summary": summary,
                "totals": view["totals"],
                "blocking_reasons": view["blocking_reasons"],
                "rows": attention[offset : offset + limit],
                "next_offset": offset + limit if offset + limit < len(attention) else None,
                "limitation": _ESTIMATE_NOTE,
            },
            ensure_ascii=False,
        )

    @scoped_tool
    async def rehearse_submission(
        ctx: ToolContext[OfficeContext], output_ids: list[str] | None = None
    ) -> str:
        """Rehearse the submission package without exporting it: check every registered submission requirement and the package documents (by default the generated documents linked to requirements) for missing approvals, completion reviews, linked documents, changed sources and stale drafts. Returns blockers with the record each concerns."""
        office = ctx.context
        office.require_tool("rehearse_submission")
        office.ensure_scope_current()
        if office.is_staff:
            raise ValueError(
                "The submission rehearsal covers the whole Tender and is available to the Tender Manager."
            )
        from .outputs import OutputService
        from .submissions import SubmissionService
        from .tender_requirements import RequirementService

        requirements_service = RequirementService(office.repo)
        active, cursor = [], 0
        while rows := requirements_service.list(office.tender_id, offset=cursor, limit=100):
            active.extend(rows)
            cursor += len(rows)
        available = {item["id"] for item in OutputService(office.repo).list(office.tender_id)}
        if output_ids is None:
            chosen = sorted(
                {
                    linked["output_id"]
                    for requirement in active
                    for linked in requirement.get("linked_outputs", [])
                    if linked.get("output_id") in available
                }
            )
        else:
            unknown = [identifier for identifier in output_ids if identifier not in available]
            if unknown:
                raise ToolArgumentError(
                    "These generated documents are not saved in this Tender: "
                    + ", ".join(unknown[:10])
                    + ". Use inspect_generated_documents for their IDs."
                )
            chosen = sorted(set(output_ids))
        if len(chosen) > 30:
            raise ToolArgumentError(
                "A submission package holds at most 30 generated documents. Choose the documents to rehearse."
            )
        if chosen:
            preview = SubmissionService(office.repo).preview(
                office.tender_id, {"output_ids": chosen, "requirement_ids": None}
            )
        else:
            basis = requirements_service.submission_basis(
                office.tender_id, [row["id"] for row in active], []
            )
            missing = "No generated documents are in the package yet."
            preview = {
                "outputs": [],
                "requirements": basis["requirements"],
                "blocking_reasons": [*basis["blocking_reasons"], missing],
                "blockers": [
                    *basis["blockers"],
                    {
                        "code": "no_documents",
                        "message": missing,
                        "target": {"kind": "package", "record_id": None, "output_ids": []},
                    },
                ],
                "warnings": basis["warnings"]
                + (
                    []
                    if active
                    else [
                        "No submission requirements are registered. Required Tender contents have not been established."
                    ]
                ),
            }
        requirements = preview["requirements"]
        result = {
            "ready": not preview["blocking_reasons"],
            "requirements_checked": len(requirements),
            "documents_checked": [
                {"output_id": item["id"], "filename": safe_text(item.get("filename"), 200)}
                for item in preview["outputs"]
            ],
            "blockers": [
                {
                    "code": item["code"],
                    "message": safe_text(item["message"], 400),
                    "target": item["target"],
                }
                for item in preview["blockers"]
            ][:60],
            "warnings": [safe_text(item, 400) for item in preview["warnings"]][:40],
            "requirement_states": [
                {
                    "id": item["id"],
                    "title": safe_text(item.get("title"), 200),
                    "status": item.get("status"),
                    "review_status": item.get("review_status"),
                    "source_current": item.get("is_current"),
                }
                for item in requirements
            ][:100],
            "limitation": _REHEARSAL_NOTE,
        }
        office.emit_event(
            "submission_rehearsed",
            "The submission package was rehearsed without exporting it.",
            {
                "ready": result["ready"],
                "blockers": len(preview["blockers"]),
                "requirements": len(requirements),
                "documents": len(chosen),
            },
        )
        return json.dumps(result, ensure_ascii=False)

    return [check_estimate_coverage, rehearse_submission]
