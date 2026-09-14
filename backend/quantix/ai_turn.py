"""One agent turn on an approved tender AI route, metered against its run's allowance."""

from __future__ import annotations

from typing import Any


async def run_turn(
    repo,
    tender_id: str,
    run_id: str,
    route: dict,
    context,
    prompt: str,
    output_type,
    *,
    system_instructions: str,
    definitions: list | None,
    validate_output=None,
    role: str = "Tender Manager",
    metadata: dict[str, Any] | None = None,
) -> dict:
    """Lease the route's account, run the agent loop and return the provider response.

    Every model request passes through the tender's budget meter. A failure
    releases the request's budget hold and never leaks the account credential.
    """

    from .ai_connections import AIConnectionService, is_supported_profile
    from .ai_execution import execute_api
    from .ai_policy import AIPolicyService, BudgetMeter
    from .ai_readiness import require_ready

    policies = AIPolicyService(repo)
    connections = AIConnectionService(repo)
    policy = policies.get(tender_id)
    with connections.lease(route["connection_id"]) as connection:
        if not is_supported_profile(connection):
            raise ValueError(
                "This saved AI account is retired. Choose a supported AI account for this tender."
            )
        meter = BudgetMeter(policies, tender_id, run_id, route, metadata=metadata)
        checked_component = require_ready(repo, connection, route["model_id"])
        with repo.db.connect() as conn:
            _, _, used_requests = policies._totals(conn, tender_id, run_id)
        remaining = policy["max_requests"] - used_requests
        if remaining < 1:
            raise ValueError(
                "The tender's AI request allowance for this work is used up. Review the saved progress before continuing."
            )
        leased = connection | {
            "_model": meter.model,
            "_checked_component_version": checked_component,
            "_execution_limits": {
                "max_requests": remaining,
                "max_output_tokens": route["max_output_tokens"],
                "context_window": meter.model["capabilities"].get("context_window"),
            },
        }
        repo.event(
            run_id,
            "ai_route_selected",
            "Using an approved AI connection.",
            {
                "connection_id": connection["id"],
                "provider": connection["provider_id"],
                "model": route["model_id"],
                "billing": connection["billing"],
                "role": role,
                **(metadata or {}),
            },
        )
        credentials = connections.credentials(connection["id"])
        try:
            response = await execute_api(
                route,
                leased,
                credentials,
                context,
                prompt,
                output_type,
                system_instructions=system_instructions,
                definitions=definitions,
                before_request=meter.before_request,
                on_response=meter.on_response,
                validate_output=validate_output,
            )
        except Exception as error:
            meter.interrupted(error)
            connections.mark_error(connection["id"], str(error))
            message = str(error)
            for secret in credentials.values():
                if secret:
                    message = message.replace(secret, "[private credential]")
            raise ValueError(message) from None
        except BaseException:
            meter.interrupted()
            raise
        connections.mark_used(connection["id"])
    return response
