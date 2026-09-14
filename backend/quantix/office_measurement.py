"""Read-only drawing calculations from inspected regions and current read evidence."""

import asyncio
import json
import math
from typing import Literal

from .ai_tools import ToolContext
from .measurement_models import AgentMeasurement, MeasurementInput
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
    """The measured and calibration points must lie in drawing regions viewed in this run."""
    proposal = AgentMeasurement.model_validate(values)
    context.ensure_artifact_allowed(proposal.artifact_id)
    context.validate_sources(proposal.source_ids)
    viewed_regions = []
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
        raise ValueError("Inspect the measured drawing page with view_document_page before measuring it.")
    for px, py in [*proposal.points, *(proposal.calibration_points or [])]:
        if not any(x <= px <= x + width and y <= py <= y + height for x, y, width, height in viewed_regions):
            raise ValueError("Inspect every measured and calibration point in the drawing before measuring it.")
    return proposal


async def calculate_agent_measurement(context, values):
    """Calculate with the original PDF dimensions; nothing is stored."""
    proposal = validate_agent_measurement(context, values)
    calculation = await asyncio.to_thread(
        MeasurementService(context.repo).calculate,
        context.tender_id,
        proposal.model_dump(include=set(MeasurementInput.model_fields)),
    )
    context.emit_event("drawing_measurement_calculated", "A drawing quantity was calculated.", {
        "artifact_id": proposal.artifact_id, "page": proposal.page,
        "source_ids": proposal.source_ids, "mode": proposal.mode,
    })
    return {
        **calculation,
        "scope_label": proposal.scope_label,
        "supporting_source_ids": proposal.source_ids,
        "instruction": "A calculation from the marked points and the stated calibration, not a checked quantity. Put the quantity and this calculation in the takeoff line's working, citing the drawing page.",
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
        """Scale a length, area or count from a drawing when no dimension is printed. View the regions first with view_document_page; points are top-left page fractions and calibration is a printed dimension or scale bar. Count requires null calibration. Nothing is saved."""
        result = await calculate_agent_measurement(ctx.context, {
            "artifact_id": artifact_id, "page": page, "mode": mode, "points": points,
            "calibration_points": calibration_points, "calibration_metres": calibration_metres,
            "scope_label": scope_label, "source_ids": source_ids,
        })
        return json.dumps(_clean(result), ensure_ascii=False)

    return [calculate_drawing_measurement]
