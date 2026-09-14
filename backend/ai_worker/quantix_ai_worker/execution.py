"""Original-client execution inside the isolated worker process."""

from .common import RuntimeUnavailable


async def execute_runtime(route, connection, credentials, context, instruction, output_schema):
    from .common import RuntimeConnectionFailure

    protocol = connection["protocol"]
    if protocol == "codex":
        from .codex import execute_codex as execute
    elif protocol == "grok_build":
        from .grok import execute_grok as execute
    else:
        raise RuntimeUnavailable("This connection has no supported original client for Tender work.")
    try:
        return await execute(route, connection, credentials, context, instruction, output_schema,
                             before_request=context.control.before_request, on_response=context.control.on_response)
    except (RuntimeConnectionFailure, ValueError, InterruptedError):
        raise
    except (TimeoutError, ConnectionError):
        raise RuntimeConnectionFailure("The original AI client's connection timed out or was interrupted.") from None
    except Exception as error:
        if protocol == "codex":
            from openai_codex.errors import TransportClosedError, is_retryable_error
            if isinstance(error, TransportClosedError) or is_retryable_error(error):
                raise RuntimeConnectionFailure("The Codex connection closed or its server was temporarily unavailable.") from None
        raise RuntimeUnavailable("The original AI client could not complete this request. Review its setup, model and limits.") from None
