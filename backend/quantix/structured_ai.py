"""One bounded, no-tools AI request that returns a validated structured result.

Used by the automatic package analysis (document briefs, package map, project
identity). It runs on the Tender's approved Manager route with the lightest
thinking level, inside the Tender's request and spending allowance.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, TypeVar

from pydantic import BaseModel

from .office_tools import OfficeContext

if TYPE_CHECKING:
    from .repository import Repository

Output = TypeVar("Output", bound=BaseModel)
USAGE_COUNTERS = ("requests", "input_tokens", "output_tokens", "cached_input_tokens", "reasoning_tokens")


async def ask_structured(
    repo: "Repository", tender_id: str, run_id: str, prompt: str, output_type: type[Output], *,
    operation: str,
) -> tuple[Output, dict]:
    from .ai_connections import AIConnectionService
    from .ai_execution import execute_api
    from .ai_policy import AIPolicyService, BudgetMeter
    from .ai_readiness import require_ready
    from .benchmark_adoption import BenchmarkAdoptionService
    from .conversation import classification_route

    if repo.get_run(run_id)["tender_id"] != tender_id:
        raise ValueError("The run does not belong to this Tender.")
    policy = AIPolicyService(repo)
    connections = AIConnectionService(repo)
    route = policy.routes_for(tender_id)[:1][0]
    adoption_route = dict(route)
    context = OfficeContext(repo, tender_id, run_id)
    with connections.lease(route["connection_id"]) as connection:
        policy.routes_for(tender_id)
        model = next((m for m in connections.models(connection["id"]) if m["model_id"] == route["model_id"]), {})
        approved_output = route["max_output_tokens"]
        # The lightest thinking level, but not the routing pass's short reply cap:
        # a batch of document briefs needs the Tender's full approved output limit.
        route = classification_route(route, connection, model) | {"max_output_tokens": approved_output}
        checked_component = require_ready(repo, connection, route["model_id"])
        meter = BudgetMeter(policy, tender_id, run_id, route)
        before_request = BenchmarkAdoptionService(repo).guard(tender_id, run_id, adoption_route, meter.before_request)
        with repo.db.connect() as conn:
            _, _, used_requests = policy._totals(conn, tender_id, run_id)
        local_client = connection["protocol"] in {"codex", "grok_build"}
        available = policy.get(tender_id)["max_requests"] - used_requests
        if available < 1:
            raise ValueError("The Tender AI request allowance is used up for this analysis.")
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
        # The prepared AI worker accepts only execute, check and conversation. A
        # no-tools structured request is an execute with the submit tool alone;
        # the analysis step name stays in the run's own events.
        from .ai_connections import is_subscription_profile

        worker_operation = "execute" if is_subscription_profile(connection) else operation
        try:
            response = await execute_api(
                route, bounded, credentials, context, prompt, output_type,
                definitions=[], operation=worker_operation, system_instructions="",
                before_request=before_request, on_response=meter.on_response,
            )
            output = output_type.model_validate(response["output"])
        except BaseException as error:
            meter.interrupted(error)
            raise
        connections.mark_used(connection["id"])
    return output, response["usage"]


def add_usage(total: dict, part: dict) -> dict:
    merged = dict(total)
    for counter in USAGE_COUNTERS:
        merged[counter] = (merged.get(counter, 0) or 0) + (part.get(counter, 0) or 0)
    merged["total_tokens"] = merged.get("input_tokens", 0) + merged.get("output_tokens", 0)
    merged["usage_complete"] = bool(total.get("usage_complete", True) and part.get("usage_complete", True))
    merged["request_details"] = [*total.get("request_details", []), *part.get("request_details", [])]
    return merged
