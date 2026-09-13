"""Manager delegation queues work; provider execution remains in the controller."""

from __future__ import annotations

import hashlib
import json

from pydantic import ValidationError

from .ai_tools import ToolArgumentError, ToolContext, argument_problem, tool
from .manager_runtime import ManagerRunProfiles
from .office_message_models import OfficeRecipientTarget, OfficeReferenceRequest
from .office_messages import OfficeMessageService
from .office_tools import OfficeContext, redact_prompt_data
from .staff_assignments import StaffAssignmentService
from .staff_models import ManagerCreationContext
from .staff_routing import StaffRoutingService
from .staff_store import StaffStore
from .work_brief import WorkBriefService
from .work_brief_models import BriefPoint, BriefQuestion, BriefStatus, BriefStep, WorkBriefDraft


def manager_office_tools(repo, tender_id: str, run_id: str, plan_id: str | None):
    """Close over server-owned identities; never accept them from the model."""
    manager = ManagerRunProfiles(repo).get(tender_id, run_id)
    routing = StaffRoutingService(repo)
    assignments = StaffAssignmentService(repo)
    messages = OfficeMessageService(repo)

    def checked(ctx, *, require_grant=False):
        context = ctx.context
        if (not isinstance(context, OfficeContext) or context.repo is not repo
                or context.tender_id != tender_id or context.run_id != run_id
                or context.is_staff or context.actor_id != manager.id):
            raise ValueError("This office action requires its active Tender Manager context.")
        if repo.get_run(run_id)["status"] not in {"queued", "running"}:
            raise ValueError("The Tender Manager's work has already stopped.")
        if require_grant:
            if plan_id is None:
                raise ValueError("Review and approve the work scope before assigning staff.")
            routing.validate_root(tender_id, run_id, plan_id)
        return context

    def key(ctx, action):
        if not ctx.invocation_id:
            raise ValueError("The office action has no trusted invocation identity.")
        return hashlib.sha256(f"{action}\0{ctx.invocation_id}".encode()).hexdigest()

    def encoded(value):
        if hasattr(value, "model_dump"):
            value = value.model_dump(mode="json")
        return json.dumps(redact_prompt_data(value), ensure_ascii=False)

    @tool(read_only=False, idempotent=True, requires_invocation_id=True)
    async def execute_staff(ctx: ToolContext[OfficeContext], staff_id: str, work_order_id: str, route_option_id: str) -> str:
        """Queue a saved colleague's work under an exact reviewed route. The controller starts it after this Manager turn ends."""
        with routing.policy.connections.authority_guard(), repo.atomic():
            context = checked(ctx, require_grant=True)
            from .office_checkpoints import OfficeCheckpointService

            operation = key(ctx, "execute")
            checkpoints = OfficeCheckpointService(repo)
            request = {"staff_id": staff_id, "work_order_id": work_order_id, "route_option_id": route_option_id}
            previous = checkpoints.invocation(run_id, operation, request)
            for prior in checkpoints.verified_staff_for_root(tender_id, run_id, plan_id):
                saved = prior["assignment"]
                if saved["staff_id"] != staff_id or saved["work_order_id"] != work_order_id:
                    continue
                with repo.db.connect() as conn:
                    prior_route = conn.execute(
                        "SELECT route_option_id FROM office_route_bindings WHERE tender_id=? AND id=?",
                        (tender_id, saved["route_binding_id"]),
                    ).fetchone()
                if prior_route and prior_route[0] == route_option_id:
                    checkpoints.invocation(run_id, operation, request, checkpoint_id=prior["checkpoint_id"])
                    return encoded({**prior, "detail": "This completed step was verified and reused from the resumed work. No new assignment or provider request was started."})
            if previous:
                raise ValueError("The saved step basis changed. Verify it again under a new staff invocation.")
            authority = ManagerCreationContext(tender_id, run_id, manager.version, plan_id)
            binding = routing.bind(authority, staff_id, work_order_id, route_option_id, operation)
            assignment = assignments.queue(authority, binding.id, operation)
            order = StaffStore(repo).get_work_order(tender_id, work_order_id)
            messages.post_manager(
                context, [OfficeRecipientTarget(staff_id=staff_id, assignment_id=assignment.id)],
                "instruction", order.work_order.brief, idempotency_key=operation,
            )
            return encoded({"assignment": assignment.model_dump(mode="json"), "detail": "The assignment is saved. The controller will start queued work after this Manager turn ends." if assignment.status == "queued" else assignment.detail})

    @tool
    async def read_staff_result(ctx: ToolContext[OfficeContext], result_id: str) -> str:
        """Read a saved draft and its actual source bases. This does not inspect its cited Tender sources or approve its conclusions."""
        checked(ctx)
        return encoded(assignments.get_result(tender_id, result_id))

    @tool
    async def read_office_messages(ctx: ToolContext[OfficeContext], cursor: str | None = None, limit: int = 20) -> str:
        """Read actual office exchanges; follow next_cursor for older retained messages."""
        checked(ctx)
        return encoded(messages.page(tender_id, cursor=cursor, limit=limit))

    @tool(read_only=False, idempotent=True, requires_invocation_id=True)
    async def send_staff_message(ctx: ToolContext[OfficeContext], staff_id: str, text: str, kind: str = "note", assignment_id: str | None = None, reply_to: str | None = None, references: list[OfficeReferenceRequest] | None = None) -> str:
        """Send a real Manager instruction, note, or handoff to a colleague. References do not establish source inspection."""
        context = checked(ctx)
        if kind == "reply" and assignment_id:
            raise ValueError("Use answer_staff_question to answer and continue a waiting assignment.")
        return encoded(messages.post_manager(
            context, [OfficeRecipientTarget(staff_id=staff_id, assignment_id=assignment_id)],
            kind, text, references or (), reply_to, key(ctx, "message"),
        ))

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
        context = checked(ctx)
        try:
            draft = WorkBriefDraft(
                outcome=outcome,
                status=status,
                next_step=next_step,
                done_when=done_when or [],
                steps=steps or [],
                settled=settled or [],
                open_questions=open_questions or [],
                work_product_ids=work_product_ids or [],
            )
        except ValidationError as error:
            raise ToolArgumentError(argument_problem(error)) from None
        with routing.policy.connections.authority_guard(), repo.atomic():
            checked(ctx)
            try:
                saved = WorkBriefService(repo).save(
                    tender_id,
                    run_id,
                    manager.id,
                    draft,
                    expected_version=expected_version,
                    inspected_source_ids=set(context.seen_sources),
                    idempotency_key=key(ctx, "work_brief"),
                )
            except (KeyError, ValueError) as error:
                # Content, version and source complaints are the Manager's to correct.
                raise ToolArgumentError(str(error.args[0] if error.args else error)) from None
        context.emit_event(
            "work_brief_saved",
            "The Tender Manager saved its working brief.",
            {"version": saved.version, "status": saved.status},
        )
        return encoded({"version": saved.version, "status": saved.status,
                        "detail": "Working brief saved. Use this version as expected_version next time."})

    @tool(read_only=False, idempotent=True, requires_invocation_id=True)
    async def answer_staff_question(ctx: ToolContext[OfficeContext], assignment_id: str, question_message_id: str, text: str) -> str:
        """Answer a saved staff question and queue continuation under the same root and allowance."""
        with routing.policy.connections.authority_guard(), repo.atomic():
            context = checked(ctx, require_grant=True)
            assignment = assignments.get(tender_id, assignment_id)
            reply = messages.post_manager(
                context, [OfficeRecipientTarget(staff_id=assignment.staff_id, assignment_id=assignment_id)],
                "reply", text, reply_to=question_message_id, idempotency_key=key(ctx, "answer"),
            )
            resumed = assignments.resume_after_reply(tender_id, assignment_id, reply.id)
            return encoded({"reply": reply.model_dump(mode="json"), "assignment": resumed.model_dump(mode="json")})

    return [execute_staff, read_staff_result, read_office_messages, send_staff_message,
            answer_staff_question, save_work_brief]
