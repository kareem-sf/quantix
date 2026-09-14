"""The Tender Manager's tool for keeping its working brief between turns."""

from __future__ import annotations

import hashlib
import json

from pydantic import ValidationError

from .ai_tools import ToolArgumentError, ToolContext, argument_problem, tool
from .office_tools import OfficeContext
from .work_brief import WorkBriefService
from .work_brief_models import BriefPoint, BriefQuestion, BriefStatus, BriefStep, WorkBriefDraft


def work_brief_tools(repo, tender_id: str, run_id: str, manager_id: str) -> list:
    @tool(read_only=False, idempotent=True, requires_invocation_id=True)
    async def save_work_brief(
        ctx: ToolContext[OfficeContext],
        outcome: str,
        status: BriefStatus,
        expected_version: int,
        next_step: str = "",
        done_when: list[str] | None = None,
        steps: list[BriefStep] | None = None,
        settled: list[BriefPoint] | None = None,
        open_questions: list[BriefQuestion] | None = None,
        work_product_ids: list[str] | None = None,
    ) -> str:
        """Save your working brief for multi-step work so the next turn can continue it: the requested outcome, checks that mean it is done, steps with their state, settled points with the source IDs you read, open questions with their owner, saved work-product IDs and the one next step. Each save replaces the whole brief; pass expected_version from manager_work_brief (0 when none is saved). It records progress only and approves nothing."""
        context = ctx.context
        if not isinstance(context, OfficeContext) or context.tender_id != tender_id or context.assignment_id:
            raise ValueError("Only the Tender Manager keeps the working brief.")
        try:
            draft = WorkBriefDraft(
                outcome=outcome, status=status, next_step=next_step, done_when=done_when or [],
                steps=steps or [], settled=settled or [], open_questions=open_questions or [],
                work_product_ids=work_product_ids or [],
            )
        except ValidationError as error:
            raise ToolArgumentError(argument_problem(error)) from None
        key = hashlib.sha256(f"work_brief\0{ctx.invocation_id}".encode()).hexdigest()
        try:
            saved = WorkBriefService(repo).save(
                tender_id, run_id, manager_id, draft, expected_version=expected_version,
                inspected_source_ids=set(context.seen_sources), idempotency_key=key,
            )
        except (KeyError, ValueError) as error:
            # Content, version and source complaints are the Manager's to correct.
            raise ToolArgumentError(str(error.args[0] if error.args else error)) from None
        context.emit_event("work_brief_saved", "The Tender Manager saved its working brief.",
                           {"version": saved.version, "status": saved.status})
        return json.dumps({"version": saved.version, "status": saved.status,
                           "detail": "Working brief saved. Use this version as expected_version next time."})

    return [save_work_brief]
