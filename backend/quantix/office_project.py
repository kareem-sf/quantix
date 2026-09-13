"""Manager access to structured project knowledge; no engineer decision tools."""

import json

from .ai_tools import ToolContext
from .office_tools import OfficeContext, redact_text, scoped_tool


def _clean(value):
    if isinstance(value, str):
        return redact_text(value)
    if isinstance(value, dict):
        return {key: _clean(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_clean(item) for item in value]
    return value


def _source_ids(value):
    found = []

    def collect(item):
        if isinstance(item, dict):
            for key, child in item.items():
                if key in {"source_ids", "source_ids_read", "supporting_source_ids"} and isinstance(child, list):
                    found.extend(value for value in child if isinstance(value, str))
                elif key == "source_id" and isinstance(child, str):
                    found.append(child)
                else:
                    collect(child)
        elif isinstance(item, list):
            for child in item:
                collect(child)

    collect(value)
    return list(dict.fromkeys(found))


def _artifact_ids(value):
    found = []

    def collect(item):
        if isinstance(item, dict):
            for key, child in item.items():
                if key == "artifact_id" and isinstance(child, str):
                    found.append(child)
                elif key in {"artifact_ids", "attachment_ids", "related_artifact_ids"} and isinstance(child, list):
                    found.extend(identifier for identifier in child if isinstance(identifier, str))
                else:
                    collect(child)
        elif isinstance(item, list):
            for child in item:
                collect(child)

    collect(value)
    return list(dict.fromkeys(found))


def _allowed(context, value):
    if not context.is_staff:
        return True
    references = _source_ids(value)
    artifacts = _artifact_ids(value)
    return bool(references or artifacts) and all(
        context.evidence_allowed(source_id) for source_id in references
    ) and all(_artifact_allowed(context, artifact_id) for artifact_id in artifacts)


def _artifact_allowed(context, artifact_id):
    try:
        context.ensure_artifact_allowed(artifact_id)
    except (KeyError, ValueError):
        return False
    return True


def _review_allowed(context, value):
    if not context.is_staff:
        return True
    artifact_id = value.get("artifact_id")
    if not isinstance(artifact_id, str):
        return False
    try:
        context.ensure_artifact_allowed(artifact_id)
    except (KeyError, ValueError):
        return False
    return True


def project_tools():
    @scoped_tool
    async def inspect_project_map(ctx: ToolContext[OfficeContext], offset: int, limit: int) -> str:
        """Read source-linked project structure and explicit review scopes. Read the cited originals before using project facts; approval never implies all Tender documents were reviewed."""
        ctx.context.require_tool("inspect_project_map")
        ctx.context.ensure_scope_current()
        from .project_map import ProjectMapService

        if offset < 0 or not 1 <= limit <= 30:
            raise ValueError("Read 1 to 30 project-map items from a nonnegative offset.")
        view = ProjectMapService(ctx.context.repo).view(ctx.context.tender_id)
        nodes = [node for node in view["nodes"] if _allowed(ctx.context, node)]
        reviews = [
            review
            for review in view["review_scopes"]
            if _review_allowed(ctx.context, review)
        ]
        if ctx.context.is_staff:
            result = {
                "nodes": nodes[offset:offset + limit],
                "review_scopes": reviews[offset:offset + limit],
                "coverage": {"current_review_scopes": sum(item.get("is_current", False) for item in reviews)},
                "scope_filter_applied": True,
                "next_offset": offset + limit if max(len(nodes), len(reviews)) > offset + limit else None,
            }
        else:
            result = {
                "nodes": nodes[offset:offset + limit], "total_nodes": len(nodes),
                "review_scopes": reviews[offset:offset + limit], "total_review_scopes": len(reviews),
                "coverage": view["coverage"], "directory_areas": view["directory_areas"],
                "next_offset": offset + limit if max(len(nodes), len(reviews)) > offset + limit else None,
            }
        return json.dumps(_clean(result), ensure_ascii=False)

    @scoped_tool
    async def inspect_submission_requirements(ctx: ToolContext[OfficeContext], offset: int, limit: int) -> str:
        """Inspect submission requirements, linked documents, pending reviews and exceptions. Requirement approval and final release belong to the engineer."""
        ctx.context.require_tool("inspect_submission_requirements")
        ctx.context.ensure_scope_current()
        from .tender_requirements import RequirementService

        if offset < 0 or not 1 <= limit <= 20:
            raise ValueError("Read 1 to 20 submission requirements from a nonnegative offset.")
        service = RequirementService(ctx.context.repo)
        if ctx.context.is_staff:
            with ctx.context.repo.db.connect() as conn:
                initial_total = conn.execute(
                    """
                    SELECT COUNT(*) FROM submission_requirements r
                    WHERE r.tender_id=? AND NOT EXISTS(
                        SELECT 1 FROM requirement_events e
                        WHERE e.requirement_id=r.id AND e.action='withdraw'
                    )
                    """,
                    (ctx.context.tender_id,),
                ).fetchone()[0]
            rows, raw_offset = [], 0
            batch_size = 100
            while raw_offset < initial_total and len(rows) < offset + limit + 1:
                batch = service.list(
                    ctx.context.tender_id, offset=raw_offset, limit=min(batch_size, 100)
                )
                raw_offset += len(batch)
                if not batch:
                    break
                rows.extend(row for row in batch if _allowed(ctx.context, row))
            visible_rows = rows[offset : offset + limit]
            has_more = offset + limit < len(rows)
        else:
            rows = service.list(ctx.context.tender_id, offset=offset, limit=limit)
            visible_rows, has_more = rows, len(rows) == limit
        result = {"requirements": visible_rows, "next_offset": offset + limit if has_more else None}
        if ctx.context.is_staff:
            result["scope_filter_applied"] = True
        return json.dumps(_clean(result), ensure_ascii=False)

    @scoped_tool
    async def inspect_generated_documents(ctx: ToolContext[OfficeContext], offset: int, limit: int) -> str:
        """List saved draft outputs and their source references. Draft generation is not final release."""
        ctx.context.require_tool("inspect_generated_documents")
        ctx.context.ensure_scope_current()
        from .outputs import OutputService

        if offset < 0 or not 1 <= limit <= 20:
            raise ValueError("Read 1 to 20 generated documents from a nonnegative offset.")
        rows = OutputService(ctx.context.repo).list(ctx.context.tender_id)
        rows = [row for row in rows if _allowed(ctx.context, row)]
        result = {"outputs": rows[offset:offset + limit], "next_offset": offset + limit if offset + limit < len(rows) else None}
        if ctx.context.is_staff:
            result["scope_filter_applied"] = True
        else:
            result["total"] = len(rows)
        return json.dumps(_clean(result), ensure_ascii=False)

    return [inspect_project_map, inspect_submission_requirements, inspect_generated_documents]


def publish_project(output, context):
    from .project_map import ProjectMapService
    from .tender_requirements import RequirementService

    nodes, requirements, measurements = [], [], []
    if output.project_map_nodes:
        service = ProjectMapService(context.repo)
        nodes = [service.propose(context.tender_id, node.model_dump(), origin="agent", run_id=context.run_id) for node in output.project_map_nodes]
    if output.submission_requirements:
        service = RequirementService(context.repo)
        requirements = [service.propose(context.tender_id, requirement.model_dump(mode="json"), origin="manager", run_id=context.run_id) for requirement in output.submission_requirements]
    if output.drawing_measurements:
        from .measurements import MeasurementService
        service = MeasurementService(context.repo)
        measurements = [service.propose_agent(context.tender_id, measurement.model_dump(), context.run_id) for measurement in output.drawing_measurements]
    return {
        "project_map_nodes": nodes, "submission_requirements": requirements,
        "drawing_measurements": measurements,
        "programme_proposal": output.programme_proposal.model_dump(mode="json") if output.programme_proposal else None,
    }
