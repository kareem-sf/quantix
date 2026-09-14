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


def project_tools():
    @scoped_tool
    async def inspect_project_map(ctx: ToolContext[OfficeContext], offset: int, limit: int) -> str:
        """Read source-linked project structure and explicit review scopes. Read the cited originals before using project facts; approval never implies all Tender documents were reviewed."""
        from .project_map import ProjectMapService

        if offset < 0 or not 1 <= limit <= 30:
            raise ValueError("Read 1 to 30 project-map items from a nonnegative offset.")
        view = ProjectMapService(ctx.context.repo).view(ctx.context.tender_id)
        nodes, reviews = view["nodes"], view["review_scopes"]
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
        from .tender_requirements import RequirementService

        if offset < 0 or not 1 <= limit <= 20:
            raise ValueError("Read 1 to 20 submission requirements from a nonnegative offset.")
        rows = RequirementService(ctx.context.repo).list(ctx.context.tender_id, offset=offset, limit=limit)
        result = {"requirements": rows, "next_offset": offset + limit if len(rows) == limit else None}
        return json.dumps(_clean(result), ensure_ascii=False)

    @scoped_tool
    async def inspect_generated_documents(ctx: ToolContext[OfficeContext], offset: int, limit: int) -> str:
        """List saved draft outputs and their source references. Draft generation is not final release."""
        from .outputs import OutputService

        if offset < 0 or not 1 <= limit <= 20:
            raise ValueError("Read 1 to 20 generated documents from a nonnegative offset.")
        rows = OutputService(ctx.context.repo).list(ctx.context.tender_id)
        result = {"outputs": rows[offset:offset + limit], "total": len(rows),
                  "next_offset": offset + limit if offset + limit < len(rows) else None}
        return json.dumps(_clean(result), ensure_ascii=False)

    return [inspect_project_map, inspect_submission_requirements, inspect_generated_documents]


def publish_project(output, context):
    from .project_map import ProjectMapService
    from .tender_requirements import RequirementService

    nodes, requirements = [], []
    if output.project_map_nodes:
        service = ProjectMapService(context.repo)
        nodes = [service.propose(context.tender_id, node.model_dump(), origin="agent", run_id=context.run_id) for node in output.project_map_nodes]
    if output.submission_requirements:
        service = RequirementService(context.repo)
        requirements = [service.propose(context.tender_id, requirement.model_dump(mode="json"), origin="manager", run_id=context.run_id) for requirement in output.submission_requirements]
    return {
        "project_map_nodes": nodes, "submission_requirements": requirements,
        "programme_proposal": output.programme_proposal.model_dump(mode="json") if output.programme_proposal else None,
    }
