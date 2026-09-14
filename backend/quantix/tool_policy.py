"""Single invoke fence for direct SDK, original-client MCP and nested calls."""

from __future__ import annotations

import asyncio
import json
from typing import Any, Literal

from .capability_models import ExecutionReceipt, validate_result_value
from .db import new_id
from .execution_context import OfficeExecutionIdentity, identity_from_office_context

Entrypoint = Literal["direct", "mcp", "nested"]
MAX_RESULT_BYTES = 1024 * 1024
# Identical read-only calls allowed in one run before further repeats are refused.
# Any saved change resets the count, because a read after a write can differ.
REPEAT_READ_LIMIT = 2
_SCOPE_REFUSAL = "The source is outside the reviewed source scope."
_IDENTITY_REFUSAL = "Tender tool calls require a server-resolved Tender identity."


class ToolFenceError(ValueError):
    """A capability was refused or its result was rejected before publication.

    `recoverable` marks a tool's own complaint about the call it was given, which
    the caller may hand back to the model to correct. Refused authority is never
    recoverable.
    """

    def __init__(self, receipt: ExecutionReceipt, *, recoverable: bool = False):
        self.receipt = receipt
        self.recoverable = recoverable
        detail = receipt.limitations[0] if receipt.limitations else "This tool call was blocked."
        super().__init__(detail)


def _blocked(identity: OfficeExecutionIdentity, capability_id: str, request_id: str, detail: str) -> ExecutionReceipt:
    return ExecutionReceipt(
        status="blocked",
        capability_id=capability_id,
        capability_version=1,
        request_id=request_id,
        limitations=[detail],
    )


def _uncertain(identity: OfficeExecutionIdentity, capability_id: str, request_id: str, detail: str) -> ExecutionReceipt:
    return ExecutionReceipt(
        status="uncertain_external_outcome",
        capability_id=capability_id,
        capability_version=1,
        request_id=request_id,
        limitations=[detail],
    )


def _validate_result(result: Any) -> Any:
    """Validate result structure; returned record fields never grant authority."""

    return validate_result_value(result, max_bytes=MAX_RESULT_BYTES)


def _ensure_active(context) -> None:
    """Stop dispatch before a saved run can invoke another tool result."""

    repo = getattr(context, "repo", None)
    run_id = getattr(context, "run_id", None)
    if repo is None or run_id is None:
        return
    try:
        run = repo.get_run(run_id)
    except KeyError as error:
        raise InterruptedError("This Tender run is no longer active.") from error
    if run.get("status") not in {"queued", "running"}:
        raise InterruptedError("This Tender run is no longer active.")


async def invoke(
    identity: OfficeExecutionIdentity,
    capability_id: str,
    payload: dict,
    *,
    context,
    definition,
    timeout: float | None = None,
) -> ExecutionReceipt:
    """Execute one registered capability. Caller payload cannot set destination or identity."""

    request_id = identity.trusted_invocation_id or new_id()
    if identity.tender_id is None:
        raise ToolFenceError(_blocked(identity, capability_id, request_id, _IDENTITY_REFUSAL))
    if not isinstance(payload, dict):
        raise ToolFenceError(
            _blocked(identity, capability_id, request_id, "Tool arguments must be an object.")
        )
    from .run_activity import ActivityRecorder, current_operation

    if current_operation():
        ActivityRecorder(context).record(current_operation(), "tool", "started", f"Using {definition.name}.")
    try:
        from .office_tools import commit_deferred_read, defer_read_commit

        with defer_read_commit(context):
            call = definition.invoke(context, payload, invocation_id=request_id)
            if timeout is not None:
                result = await asyncio.wait_for(call, timeout=timeout)
            else:
                result = await call
            result = _validate_result(result)
            commit_deferred_read(context)
    except ToolFenceError:
        raise
    except asyncio.CancelledError:
        raise
    except (TimeoutError, asyncio.TimeoutError):
        raise ToolFenceError(
            _uncertain(
                identity,
                capability_id,
                request_id,
                "The tool call timed out before a confirmed result. The request identity is retained.",
            )
        ) from None
    except (KeyError, ValueError) as error:
        from .ai_tools import ToolArgumentError

        detail = str(error) or _SCOPE_REFUSAL
        raise ToolFenceError(
            _blocked(identity, capability_id, request_id, detail),
            recoverable=isinstance(error, ToolArgumentError),
        ) from error
    return ExecutionReceipt(
        status="completed",
        capability_id=capability_id,
        capability_version=1,
        request_id=request_id,
        result=result,
    )


async def _dispatch(
    entrypoint: Entrypoint,
    definition,
    context,
    arguments: dict | None,
    *,
    invocation_id: str | None = None,
    timeout: float | None = None,
):
    """Shared entry for direct, MCP and nested callers. Returns the raw tool result."""

    del entrypoint
    payload = arguments or {}
    if context is None or not hasattr(context, "tender_id"):
        return await definition.invoke(context, payload, invocation_id=invocation_id)
    _ensure_active(context)
    identity = identity_from_office_context(context, invocation_id=invocation_id)
    repeats = context.__dict__.setdefault("_repeated_reads", {})
    repeat_key = None
    if definition.read_only:
        repeat_key = json.dumps([definition.name, payload], sort_keys=True, default=str)
        if repeats.get(repeat_key, 0) >= REPEAT_READ_LIMIT:
            raise ToolFenceError(
                _blocked(
                    identity,
                    definition.name,
                    identity.trusted_invocation_id or new_id(),
                    f"This exact {definition.name} call already ran {REPEAT_READ_LIMIT} times in this "
                    "run and nothing has been saved since, so it would return the same result. Use "
                    "the earlier result, change the request, or report what you have found.",
                ),
                recoverable=True,
            )
    receipt = await invoke(
        identity,
        definition.name,
        payload,
        context=context,
        definition=definition,
        timeout=timeout,
    )
    if repeat_key is None:
        repeats.clear()
    else:
        repeats[repeat_key] = repeats.get(repeat_key, 0) + 1
    return receipt.result


async def dispatch(entrypoint: Entrypoint, definition, context, arguments: dict | None, *, invocation_id: str | None = None, timeout: float | None = None):
    """Record one actual invocation, including nested calls and refused attempts."""
    from .run_activity import ActivityRecorder, ActivityRecordingError, activity_scope

    recorder = ActivityRecorder(context)
    operation = recorder.start(
        "tool", f"Preparing {definition.name}.",
        {"inputs": arguments or {}, "entrypoint": entrypoint, "read_only": definition.read_only},
        phase="prepared", tool=definition.name, provider_call_id=invocation_id,
    )
    missing = object()
    result = missing
    with activity_scope(operation):
        try:
            result = await _dispatch(entrypoint, definition, context, arguments, invocation_id=invocation_id, timeout=timeout)
            _ensure_active(context)
        except ActivityRecordingError:
            raise
        except (asyncio.CancelledError, InterruptedError):
            returned = result is not missing
            recorder.record(operation, "tool", "interrupted",
                            f"{definition.name} returned after Stop; its result was not delivered to the model." if returned else f"{definition.name} stopped before a confirmed result.",
                            {"outputs": result, "outcome": "A tool result was returned. Final run publication is separate and was not confirmed here."} if returned else {"outcome": "No completed result was confirmed."})
            raise
        except ToolFenceError as error:
            phase = "uncertain" if error.receipt.status == "uncertain_external_outcome" else "blocked"
            recorder.record(operation, "tool", phase, f"{definition.name} could not complete.",
                            {"error": str(error), "receipt": error.receipt.model_dump(mode="json"), "recoverable": error.recoverable})
            raise
        except Exception as error:
            recorder.record(operation, "tool", "failed", f"{definition.name} failed.", {"error": str(error), "error_type": type(error).__name__})
            raise
        recorder.record(operation, "tool", "completed", f"Finished {definition.name}.", {"outputs": result})
        return result
