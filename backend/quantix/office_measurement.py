"""Read-only drawing calculations from inspected regions and current read evidence."""

import asyncio
import json
import math
from typing import Literal

from .ai_tools import ToolContext
from .measurement_models import AgentMeasurementProposal, MeasurementInput
from .measurements import MeasurementService
from .office_tools import OfficeContext, redact_text, scoped_tool


def _clean(value):
    if isinstance(value, str):
        return redact_text(value)
    if isinstance(value, dict):
        return {key: _clean(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_clean(item) for item in value]
    return value


def validate_agent_measurement(context, values):
    """Root calls this before accepting/publishing an agent drawing proposal."""
    if context.is_staff:
        context.require_tool("calculate_drawing_measurement")
    proposal = AgentMeasurementProposal.model_validate(values)
    context.ensure_scope_current()
    context.ensure_artifact_allowed(proposal.artifact_id)
    context.validate_sources(proposal.source_ids)
    viewed_regions = []
    if context.is_staff:
        from .staff_context import StaffContextService

        for receipt in StaffContextService(context.repo).list_visual_receipts(
            context.tender_id,
            actor_id=context.actor_id,
            profile_version=context.staff_version,
            assignment_id=context.assignment_id,
            route_binding_id=context.route_binding_id,
            root_run_id=context.run_id,
            artifact_id=proposal.artifact_id,
            page=proposal.page,
        ):
            if (
                receipt.method != "visual"
                or not context.has_seen_source(receipt.source_id)
                or receipt.actor_id != context.actor_id
                or receipt.profile_id != context.actor_id
                or receipt.profile_version != context.staff_version
                or receipt.assignment_id != context.assignment_id
                or receipt.route_binding_id != context.route_binding_id
                or receipt.root_run_id != context.run_id
                or receipt.artifact_id != proposal.artifact_id
                or receipt.page != proposal.page
            ):
                continue
            basis = context.reviewed_artifacts.get(receipt.artifact_id)
            expected_hash = (
                basis.content_hash if hasattr(basis, "content_hash") else basis["content_hash"]
            ) if basis is not None else None
            if receipt.content_hash != expected_hash:
                continue
            region = receipt.region
            if region is None or any(
                isinstance(value, bool)
                or not isinstance(value, (int, float))
                or not math.isfinite(value)
                for value in region
            ):
                continue
            x, y, width, height = region
            if min(x, y) < 0 or min(width, height) <= 0 or x + width > 1 or y + height > 1:
                continue
            viewed_regions.append(region)
    else:
        for event in context.repo.run_events(context.run_id):
            data = event.get("data") or {}
            if (
                event["kind"] != "visual_source_viewed"
                or data.get("artifact_id") != proposal.artifact_id
                or data.get("page") != proposal.page
                or not context.has_seen_source(data.get("source_id"))
            ):
                continue
            region = data.get("region")
            if not isinstance(region, list) or len(region) != 4:
                continue
            if any(
                isinstance(value, bool)
                or not isinstance(value, (int, float))
                or not math.isfinite(value)
                for value in region
            ):
                continue
            x, y, width, height = region
            if min(x, y) < 0 or min(width, height) <= 0 or x + width > 1 or y + height > 1:
                continue
            viewed_regions.append(region)
    if not viewed_regions:
        raise ValueError("Inspect the measured drawing page with view_document_page before proposing a measurement.")
    for px, py in [*proposal.points, *(proposal.calibration_points or [])]:
        if not any(x <= px <= x + width and y <= py <= y + height for x, y, width, height in viewed_regions):
            raise ValueError("Inspect every measured and calibration point in the drawing before proposing its quantity.")
    MeasurementService(context.repo).supporting_sources(context.tender_id, proposal)
    return proposal


async def calculate_agent_measurement(context, values):
    """Calculate with the original PDF dimensions; persist no measurement or review."""
    proposal = validate_agent_measurement(context, values)
    service = MeasurementService(context.repo)
    calculation = await asyncio.to_thread(
        service.calculate,
        context.tender_id,
        proposal.model_dump(include=set(MeasurementInput.model_fields)),
    )
    context.validate_sources(proposal.source_ids)
    supporting = service.supporting_sources(context.tender_id, proposal)
    context.emit_event("drawing_measurement_calculated", "A drawing quantity was calculated for an agent proposal, without engineer approval.", {
        "artifact_id": proposal.artifact_id, "page": proposal.page,
        "source_ids": proposal.source_ids, "mode": proposal.mode,
        "origin": "agent", "status": "proposed",
    })
    return {
        **calculation,
        "scope_label": proposal.scope_label,
        "supporting_source_ids": proposal.source_ids,
        "supporting_sources": supporting,
        "origin": "agent",
        "status": "proposed",
        "reviewed_at": None,
        "review_rationale": None,
        "instruction": "This is an agent calculation from interpreted marks and stated calibration evidence. It is not an engineer-reviewed quantity or a complete drawing takeoff. Return the geometry and source IDs in drawing_measurements to propose it. The engineer must inspect the drawing, calibration and scope before linking and separately approving a BOQ quantity.",
    }


def measurement_tools():
    @scoped_tool
    async def calculate_drawing_measurement(
        ctx: ToolContext[OfficeContext],
        artifact_id: str,
        page: int,
        mode: Literal["length", "area", "count"],
        points: list[list[float]],
        calibration_points: list[list[float]] | None,
        calibration_metres: str | None,
        scope_label: str,
        source_ids: list[str],
    ) -> str:
        """Calculate a proposed length, area or count after viewing the drawing regions and reading supporting dimension sources. Points are top-left page fractions. Count requires null calibration. This neither saves nor approves a measurement."""
        ctx.context.require_tool("calculate_drawing_measurement")
        result = await calculate_agent_measurement(ctx.context, {
            "artifact_id": artifact_id, "page": page, "mode": mode, "points": points,
            "calibration_points": calibration_points, "calibration_metres": calibration_metres,
            "scope_label": scope_label, "source_ids": source_ids,
        })
        return json.dumps(_clean(result), ensure_ascii=False)

    return [calculate_drawing_measurement]
