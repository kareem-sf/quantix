"""Public native-tool stream parts, without invoking or authorizing any tool."""

import json

from pydantic_ai.messages import NativeToolCallPart, NativeToolReturnPart


def observe_provider_tool(recorder, request_id, part, operations, seen):
    if not request_id or not isinstance(part, (NativeToolCallPart, NativeToolReturnPart)):
        return
    returned = isinstance(part, NativeToolReturnPart)
    outcome = getattr(part, "outcome", None) if returned else None
    phase = {
        "success": "completed",
        "failed": "failed",
        "denied": "blocked",
        "interrupted": "interrupted",
    }.get(outcome, "observed")
    payload = (
        {"outputs": part.content, "provider_outcome": outcome}
        if returned
        else {"inputs": part.args}
    )
    fingerprint = (part.tool_call_id, returned, json.dumps(payload, sort_keys=True, default=str))
    if fingerprint in seen:
        return
    seen.add(fingerprint)
    operation = operations.get(part.tool_call_id)
    message = (
        f"Provider reported {part.tool_name}: {outcome or 'outcome not supplied'}."
        if returned
        else f"Provider reported {part.tool_name} inputs."
    )
    if operation:
        recorder.record(
            operation,
            "provider_tool",
            phase if returned else "observed",
            message,
            payload,
            capture_status="partial",
            unavailable_fields=["unreported provider-internal steps"],
        )
    else:
        operations[part.tool_call_id] = recorder.start(
            "provider_tool",
            message,
            payload,
            phase=phase if returned else "started",
            tool=part.tool_name,
            parent_operation_id=request_id,
            provider_call_id=part.tool_call_id,
            capture_status="partial",
            unavailable_fields=["provider tool inputs/start"]
            if returned
            else ["unreported provider-internal steps"],
        )
