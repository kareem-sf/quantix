"""The bounded, no-tools conversational branch for the Tender Manager."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from pydantic import BaseModel, ConfigDict, Field

from .office_tools import OfficeContext, redact_prompt_data, redact_text, safe_text

if TYPE_CHECKING:
    from .repository import Repository


class ConversationOutput(BaseModel):
    """A reply shape that has no fields capable of publishing engineering records."""

    model_config = ConfigDict(extra="forbid")

    kind: Literal["conversation", "engineering"]
    reply: str = Field(default="", max_length=6000)
    next_action: str = Field(default="Review the Tender documents when you are ready.", max_length=500)


@dataclass(frozen=True)
class PreparedConversationResult:
    tender_id: str
    run_id: str
    output: ConversationOutput
    usage: dict


def classification_route(approved_route, connection, model):
    """A separate no-tool classifier within the engineering route's spend cap."""
    from .ai_thinking import light_level

    approved_limit = approved_route["max_output_tokens"]
    route = {**approved_route, "web_search": False, "max_search_calls": 0, "native_tools": [],
             "max_output_tokens": min(2048, approved_limit)}
    recorded = (model.get("capabilities") or {}).get("reasoning") or []
    candidate = light_level(connection, model)
    effort = route.get("reasoning")
    # The shared selector also knows documented direct-provider levels. Do not
    # discard those merely because this catalog omitted a reasoning list.
    # Explicit token budgets remain intact unless the model reports a lighter
    # named setting; preserve that separate budget/output-limit contract.
    fixed_budget = isinstance(effort, str) and effort.startswith("budget:")
    if effort not in {"none", "disabled"} and candidate is not None and (
        not fixed_budget or candidate in recorded
    ):
        route["reasoning"] = candidate
    effort = route.get("reasoning")
    if isinstance(effort, str) and effort.startswith("budget:"):
        try:
            budget = int(effort.split(":", 1)[1])
        except ValueError:
            raise ValueError("Review the approved thinking-token budget before classifying this request.") from None
        if not 1 <= budget < approved_limit:
            raise ValueError("The approved thinking budget must remain below its approved output limit.")
        route["max_output_tokens"] = max(route["max_output_tokens"], budget + 1)
    return route


def request_kind(
    content: str, *, action: Literal["review_documents"] | None = None
) -> Literal["conversation", "manager"]:
    # Only the explicit engineering action bypasses the bounded no-tools
    # routing pass.  All other freeform text is classified by the approved AI
    # route before any document tool or engineering publication is possible.
    return "manager" if action == "review_documents" else "conversation"


def _brief_status(repo: "Repository", tender_id: str) -> dict | None:
    """Progress headlines only; settled points and their sources stay with the engineering pass."""

    from .work_brief import WorkBriefService

    brief = WorkBriefService(repo).current(tender_id)
    if brief is None:
        return None
    return {
        "version": brief.version,
        "status": brief.status,
        "outcome": safe_text(brief.outcome, 600),
        "steps": [{"title": safe_text(step.title, 200), "state": step.state} for step in brief.steps],
        "open_questions": [
            {"text": safe_text(question.text, 400), "owner": question.owner}
            for question in brief.open_questions
        ],
        "next_step": safe_text(brief.next_step, 400),
        "progress_current": brief.progress_current,
        "later_work_saved": brief.latest_work_run_id is not None,
        "saved_at": brief.created_at,
    }


def _prompt(repo: "Repository", tender_id: str, instruction: str, manager_profile=None) -> str:
    messages = repo.messages(tender_id)[-8:]
    overview = repo.overview(tender_id)
    plan = overview.get("plan")
    documents = [
        safe_text(artifact["name"], 160) for artifact in repo.list_artifacts(tender_id)[:40]
    ]
    saved_status = {
        "tender_name": safe_text(overview["tender"].get("name"), 200),
        "document_names": documents,
        "tender_status": overview["tender"].get("status"),
        "document_count": overview.get("artifact_count", 0),
        "evidence_count": overview.get("evidence_count", 0),
        "coverage": overview.get("coverage", {}),
        "active_work": [
            {"kind": row.get("kind"), "status": row.get("status"), "detail": row.get("detail")}
            for row in overview.get("active_runs", [])[:8]
        ],
        "plan": {
            "status": plan.get("status"),
            "task_count": len(plan.get("tasks", [])),
            "completed_tasks": sum(task.get("status") == "completed" for task in plan.get("tasks", [])),
        }
        if plan
        else None,
        "manager_work_brief": _brief_status(repo, tender_id),
    }
    history = [
        {"role": row["role"], "content": safe_text(row["content"], 1200)}
        for row in messages
    ]
    return (
        "You are the Tender Manager's first bounded routing and conversation layer. Reply in "
        "the same language as the engineer, including Arabic when requested. Do not inspect "
        "additional material because of personality settings. Apply the supplied manager_profile "
        "to identity and communication within these routing and authority controls. Do not inspect "
        "Tender documents, use tools, search the web, or propose or publish findings, plans, "
        "tasks, quantities, prices, drafts or decisions. Use only the supplied dialogue history "
        "and compact saved status counters; do not treat either as source evidence. The "
        "manager_work_brief is the Manager's saved progress record: use it to answer questions "
        "about progress and next steps, and say when it has not been saved. Classify "
        "the engineer's freeform request as conversation for greetings, status questions and "
        "small talk, or engineering for any request that needs Tender evidence or engineering "
        "work. A question about the project, its documents, dates, scope or requirements that the "
        "saved status and history do not answer is engineering: never tell the engineer the "
        "information is unavailable, because the documents may contain it. "
        "For conversation, return a short helpful reply and exactly one clear next action. "
        "For engineering, leave reply empty and state that the source-grounded engineering pass "
        "will start next.\n\n"
        + redact_text(
            __import__("json").dumps(
                {"saved_status": saved_status, "history": history, "instruction": instruction,
                 "manager_profile": redact_prompt_data(manager_profile)},
                ensure_ascii=False,
            )
        )
    )


async def run_conversation(repo: "Repository", tender_id: str, run_id: str, instruction: str):
    """Execute one bounded model pass over the same approved Manager route, with no tools."""

    from .ai_connections import AIConnectionService
    from .ai_execution import execute_api
    from .ai_policy import AIPolicyService, BudgetMeter
    from .ai_readiness import require_ready
    from .manager_runtime import ManagerRunProfiles, prompt_profile

    repo.get_tender(tender_id)
    if repo.get_run(run_id)["tender_id"] != tender_id:
        raise ValueError("The run does not belong to this Tender.")
    # Job admission normally pins this version. Direct controller entry also
    # pins it idempotently; continuation cannot replace an existing snapshot.
    manager_profile = ManagerRunProfiles(repo).capture(tender_id, run_id)
    policy = AIPolicyService(repo)
    connections = AIConnectionService(repo)
    approved = next(
        (plan for plan in repo.list_plans(tender_id) if plan["status"] == "approved"), None
    )
    plan_id = approved["id"] if approved else None
    route = policy.routes_for(tender_id, plan_id=plan_id)[:1][0]
    context = OfficeContext(repo, tender_id, run_id)
    with connections.lease(route["connection_id"]) as connection:
        policy.routes_for(tender_id, plan_id=plan_id)
        # Routing a message and replying to small talk needs the lightest
        # thinking the model offers; the engineer's level applies to the work.
        model = next((m for m in connections.models(connection["id"]) if m["model_id"] == route["model_id"]), {})
        route = classification_route(route, connection, model)
        checked_component = require_ready(repo, connection, route["model_id"])
        meter = BudgetMeter(policy, tender_id, run_id, route)
        before_request = meter.before_request
        with repo.db.connect() as conn:
            _, _, used_requests = policy._totals(conn, tender_id, run_id)
        # A direct API returns the structured reply in a single request. A local
        # client (Codex, Grok) works in turns: it reserves one for its first
        # model turn and needs at least one more to call the submit tool, so a
        # two-request ceiling ends the run with no proposal at all. The tender's
        # own approved allowance still bounds both.
        local_client = connection["protocol"] in {"codex", "grok_build"}
        available = policy.get(tender_id)["max_requests"] - used_requests
        bounded = connection | {
            "_model": meter.model,
            "_checked_component_version": checked_component,
            "_execution_limits": {
                "max_requests": max(1, min(6 if local_client else 2, available)),
                "max_output_tokens": route["max_output_tokens"],
                "context_window": meter.model["capabilities"].get("context_window"),
            },
        }
        credentials = connections.credentials(connection["id"])
        try:
            response = await execute_api(
                route,
                bounded,
                credentials,
                context,
                _prompt(repo, tender_id, instruction, prompt_profile(manager_profile)),
                ConversationOutput,
                definitions=[],
                operation="conversation",
                system_instructions="",
                before_request=before_request,
                on_response=meter.on_response,
            )
            output = ConversationOutput.model_validate(response["output"])
            if output.kind == "conversation" and not output.reply.strip():
                raise ValueError("The conversational AI pass returned no reply.")
        except BaseException as error:
            meter.interrupted(error)
            raise
        connections.mark_used(connection["id"])
        return PreparedConversationResult(
            tender_id=tender_id,
            run_id=run_id,
            output=output,
            usage=response["usage"],
        )
