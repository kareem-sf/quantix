"""Tender execution dispatch for direct APIs and approved original clients."""

# Static instructions come first and never change between turns, so providers can
# cache them; everything after this marker is the per-turn request and context.
TURN_CONTEXT_MARKER = "\n\n---- Current request and context ----\n"


async def execute_api(
    route,
    connection,
    credentials,
    context,
    instruction,
    output_type,
    *,
    consult=None,
    before_request=None,
    on_response=None,
    definitions=None,
    operation="execute",
    system_instructions=None,
    validate_output=None,
):
    from .ai_connections import is_subscription_profile
    from .ai_direct import DirectAPIService, supports_direct
    from .office import INSTRUCTIONS

    if system_instructions is None:
        bounded_instruction = (
            f"{INSTRUCTIONS}{TURN_CONTEXT_MARKER}Engineering work request:\n{instruction}"
        )
    else:
        bounded_instruction = (
            f"{system_instructions}{TURN_CONTEXT_MARKER}{instruction}"
            if system_instructions
            else instruction
        )
    if is_subscription_profile(connection):
        from .ai_worker_client import AIWorkerClient

        return await AIWorkerClient(context.repo).execute(
            route,
            connection,
            credentials,
            context,
            bounded_instruction,
            output_type,
            consult=consult,
            before_request=before_request,
            on_response=on_response,
            definitions=definitions,
            operation=operation,
            validate_output=validate_output,
        )
    if not supports_direct(connection):
        raise ValueError(
            "This saved AI account is retired. Choose one of the five supported provider routes."
        )
    return await DirectAPIService(context.repo).execute(
        route,
        connection,
        credentials,
        context,
        bounded_instruction,
        output_type,
        consult=consult,
        before_request=before_request,
        on_response=on_response,
        definitions=definitions,
        operation_name=operation,
        validate_output=validate_output,
    )
