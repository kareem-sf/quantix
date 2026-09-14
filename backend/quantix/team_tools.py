"""The Tender Manager's tools for building its team and delegating work."""

from __future__ import annotations

import json

from pydantic import ValidationError

from .ai_tools import ToolArgumentError, ToolContext, argument_problem, tool
from .office_tools import OfficeContext, redact_prompt_data
from .team import TeamService
from .team_models import StaffDraft


def team_tools(repo, tender_id: str, run_id: str) -> list:
    team = TeamService(repo)

    def manager(ctx) -> OfficeContext:
        context = ctx.context
        if not isinstance(context, OfficeContext) or context.tender_id != tender_id or context.assignment_id:
            raise ValueError("Only the Tender Manager can manage the team.")
        return context

    def encoded(value) -> str:
        return json.dumps(redact_prompt_data(value), ensure_ascii=False)

    def models():
        from .ai_policy import AIPolicyService

        return [{"model": f"{account['connection_id']}/{model['model_id']}", "account": account["name"],
                 "billing": account["billing"], "images": model["capabilities"].get("images"),
                 "context_window": model["capabilities"].get("context_window")}
                for account in AIPolicyService(repo).context_catalog(tender_id) for model in account["models"]]

    @tool(read_only=False)
    async def hire_staff(ctx: ToolContext[OfficeContext], name: str, role: str, specialisms: list[str],
                         background: str, working_style: str) -> str:
        """Add a colleague to this tender's team, shaped to the work the tender actually needs.
        Give a realistic name, a specific construction or commercial role, 1-8 specialisms, a short professional
        background and how they work. Check list_team first: hire only when no existing colleague fits."""
        manager(ctx)
        try:
            draft = StaffDraft(name=name, role=role, specialisms=specialisms, background=background,
                               working_style=working_style)
        except ValidationError as error:
            raise ToolArgumentError(argument_problem(error)) from None
        member = team.hire(tender_id, run_id, draft)
        repo.event(run_id, "staff_hired", f"The Tender Manager added {member.name} ({member.role}) to the team.",
                   {"staff_id": member.id})
        return encoded({"staff_id": member.id, "name": member.name, "role": member.role})

    @tool(read_only=False)
    async def assign_work(ctx: ToolContext[OfficeContext], staff_id: str, title: str, brief: str,
                          expected_result: str, source_ids: list[str] | None = None, model: str | None = None) -> str:
        """Give one colleague a piece of work. It starts after your turn ends; their result or question
        comes back to you next turn. Write the brief so a professional can act on it alone: scope, the documents
        to start from (source_ids you have read, optional), and what a finished result contains.
        Choose `model` from list_team's available_models only when the work needs it, for example image input
        for drawings; otherwise the tender's specialist model is used. Split large work into several assignments."""
        manager(ctx)
        from .ai_policy import AIPolicyService

        policies = AIPolicyService(repo)
        route = policies.routes_for(tender_id, role="specialist")[0]
        if model:
            connection_id, _, model_id = model.partition("/")
            if not connection_id or not model_id:
                raise ToolArgumentError("Choose model as 'account/model' from list_team's available_models.")
            try:
                route = policies.validate_route(route | {"connection_id": connection_id, "model_id": model_id},
                                                policies.get(tender_id)["allowed_connection_ids"])
            except (KeyError, ValueError) as error:
                raise ToolArgumentError(f"{error} Choose one of list_team's available_models.") from None
        if not title.strip() or not brief.strip() or not expected_result.strip():
            raise ToolArgumentError("Give the assignment a title, a brief and the expected result.")
        try:
            assignment = team.assign(tender_id, run_id, staff_id, title=title.strip()[:200], brief=brief.strip()[:6000],
                                     expected_result=expected_result.strip()[:2000], source_ids=list(source_ids or [])[:50],
                                     route=route)
        except KeyError as error:
            raise ToolArgumentError(str(error.args[0] if error.args else error)) from None
        member = team.get_staff(tender_id, staff_id)
        repo.event(run_id, "work_assigned", f"The Tender Manager assigned {member.name}: {assignment.title}",
                   {"assignment_id": assignment.id, "staff_id": member.id})
        return encoded({"assignment_id": assignment.id, "status": assignment.status,
                        "detail": "Queued. It starts after this turn ends."})

    @tool
    async def list_team(ctx: ToolContext[OfficeContext]) -> str:
        """List this tender's team, their recent assignments and the AI models you may choose for assignments."""
        manager(ctx)
        staff = [member.model_dump(include={"id", "name", "role", "specialisms", "status"})
                 for member in team.list_staff(tender_id)]
        assignments = [assignment.model_dump(include={"id", "staff_id", "title", "status", "question"})
                       for assignment in team.list(tender_id, limit=40)]
        return encoded({"staff": staff, "recent_assignments": assignments, "available_models": models()})

    @tool
    async def read_assignment(ctx: ToolContext[OfficeContext], assignment_id: str) -> str:
        """Read one assignment's brief and its result, question or failure. A colleague's citations are not your
        own reading: read the cited sources yourself before stating those facts."""
        manager(ctx)
        try:
            assignment = team.get(tender_id, assignment_id)
        except KeyError as error:
            raise ToolArgumentError(str(error.args[0])) from None
        return encoded(assignment.model_dump(exclude={"usage"}))

    @tool(read_only=False)
    async def answer_staff(ctx: ToolContext[OfficeContext], assignment_id: str, answer: str) -> str:
        """Answer a colleague's waiting question. Their assignment continues after your turn ends."""
        manager(ctx)
        if not answer.strip():
            raise ToolArgumentError("Write the answer for your colleague.")
        try:
            assignment = team.answer(tender_id, assignment_id, answer.strip()[:4000])
        except (KeyError, ValueError) as error:
            raise ToolArgumentError(str(error.args[0] if error.args else error)) from None
        return encoded({"assignment_id": assignment.id, "status": assignment.status})

    return [hire_staff, assign_work, list_team, read_assignment, answer_staff]
