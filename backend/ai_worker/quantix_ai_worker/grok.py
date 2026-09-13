"""Run the unmodified pinned Grok Build with scoped Tender MCP tools only."""

import asyncio
import json
import time
from decimal import Decimal, InvalidOperation
from uuid import uuid4

from .client_session import ClientEventBuffer, session_binding, work_directory
from .common import (
    RuntimeConnectionFailure,
    RuntimeUnavailable,
    call_hook,
    execution_limits,
    reserve_runtime,
    runtime_usage,
)
from .grok_auth import acp_session, billing_in_session, check_billing_permission, models_in_session
from .grok_common import (
    GROK_CHECK_MAX_ROUNDS,
    account_lock,
    close_process,
    configure,
    discard,
    environment,
    grok_command,
    spawn,
    write_private,
)
from .remote import SUBMIT_TOOL

try:
    from .diagnostics import record, record_exception
except ImportError:  # Source-only probes run before a managed writer copy exists.
    from quantix.diagnostics import record, record_exception


# The pinned Grok Build client accepts the public `grok-4.5` model id but its
# headless usage ledger reports the provider's Build identity `grok-4.5-build`.
# Keep this explicit per-route alias map; do not infer arbitrary suffixes.
_MODEL_RESPONSE_ALIASES = {
    "grok-4.5": frozenset({"grok-4.5-build"}),
}

_KNOWN_STOP_REASONS = frozenset({"end_turn", "cancelled", "max_turns", "max_tokens", "max_turn_requests", "refusal", "error", "missing", "other"})

_ERROR_CATEGORY_ALIASES = {
    "authentication_failed": "authentication_failed",
    "authentication": "authentication_failed",
    "authentication_error": "authentication_failed",
    "auth": "authentication_failed",
    "invalid_request": "invalid_request",
    "invalid_request_error": "invalid_request",
    "rate_limit": "rate_limit",
    "rate_limited": "rate_limit",
    "rate_limit_exceeded": "rate_limit",
    "capacity": "rate_limit",
    "server_error": "server_error",
    "server": "server_error",
    "overloaded": "server_error",
    "overloaded_error": "server_error",
    "max_output_tokens": "max_output_tokens",
    "unknown": "unknown",
}

_ERROR_CATEGORY_MESSAGES = {
    "authentication_failed": "Grok could not authenticate the account. Review Grok sign-in and retry the operation.",
    "access_denied": "Grok denied access for this account or model. Review subscription and model permissions, then retry.",
    "invalid_request": "Grok rejected the request. Review the selected model and limits, then retry.",
    "rate_limit": "Grok reported a rate limit. Review provider limits and reset time, then retry the operation.",
    "server_error": "Grok provider service is unavailable. Check its status and retry.",
    "max_output_tokens": "Grok reached its output limit. Shorten the request and retry the operation.",
}


def classify_provider_error(event):
    """Extract only allowlisted error metadata present on a native event."""
    if not isinstance(event, dict):
        return {"category": "unknown", "status_code": None}
    category = None
    for key in ("category", "error_category", "error_type", "subtype"):
        value = event.get(key)
        if isinstance(value, str):
            category = _ERROR_CATEGORY_ALIASES.get(value.lower())
            if category:
                break
    status = event.get("status_code")
    if type(status) is not int or not 100 <= status <= 599:
        status = None
    if category is None and status is not None:
        category = {
            400: "invalid_request",
            401: "authentication_failed",
            403: "access_denied",
            429: "rate_limit",
        }.get(status, "server_error" if 500 <= status <= 599 else None)
    return {"category": category or "unknown", "status_code": status}


def _safe_failure_message(reason, *, operation):
    if operation == "check":
        if reason == "max_turns":
            return (f"Grok reached the {GROK_CHECK_MAX_ROUNDS}-round connection check limit before completing "
                    "the structured tool exchange. Retry the access check.")
        if reason in {"provider_error", "unknown"}:
            return "Grok reported a provider error during the connection check. Retry the access check."
        if reason in _ERROR_CATEGORY_MESSAGES:
            return _ERROR_CATEGORY_MESSAGES[reason].replace("the operation", "the access check")
        if reason == "missing_terminal":
            return "Grok ended without a terminal connection-check result. Retry the access check."
        return "Grok did not complete the connection check. Retry the access check."
    if reason == "max_turns":
        return "Grok reached its approved model-turn limit before completing the engineering proposal. Retry this operation."
    if reason in {"provider_error", "unknown"}:
        return "Grok reported a provider error before completing the engineering proposal. Retry this operation."
    if reason in _ERROR_CATEGORY_MESSAGES:
        return _ERROR_CATEGORY_MESSAGES[reason]
    return "Grok did not complete the engineering proposal. Retry this operation."


def model_identity_matches(requested, reported):
    if not isinstance(requested, str) or not isinstance(reported, str):
        return False
    return reported == requested or reported in _MODEL_RESPONSE_ALIASES.get(requested, ())


def _count(value):
    return value if type(value) is int and 0 <= value <= 10**12 else None


def _cost(event):
    ticks = event.get("total_cost_usd_ticks")
    if type(ticks) is int and 0 <= ticks <= 10**19:
        return format(Decimal(ticks) / Decimal(10**10), "f")
    raw = event.get("total_cost_usd")
    if type(raw) not in {str, int, float}:
        return None
    try:
        value = Decimal(str(raw))
        return format(value, "f") if value.is_finite() and 0 <= value <= 10**9 else None
    except InvalidOperation:
        return None


def update_usage(usage, event, expected_model, request_limit):
    """Native stream input is uncached; Quantix input is the full prompt sum."""
    aggregate = event.get("usage")
    models = event.get("modelUsage")
    turns = _count(event.get("num_turns"))
    incomplete = event.get("usage_is_incomplete")
    partial_cost = event.get("cost_is_partial")
    wrong_model = isinstance(models, dict) and any(
        not model_identity_matches(expected_model, value) for value in models
    )
    exceeded_turns = turns is not None and turns > request_limit
    usage.update(provider_usage_is_incomplete=incomplete if type(incomplete) is bool else None,
                 provider_cost_is_partial=partial_cost if type(partial_cost) is bool else None,
                 provider_reported_cost_usd=_cost(event) if partial_cost is not True else None)
    if isinstance(event.get("sessionId"), str):
        usage["session_id"] = event["sessionId"][:200]
    if isinstance(models, dict) and models:
        actual = list(models)
        usage["actual_model"] = actual[0] if len(actual) == 1 else None
        usage["reported_models"] = [str(value)[:300] for value in actual[:30]]
    if turns is not None:
        usage["requests"] = turns
    if not isinstance(aggregate, dict):
        usage["usage_complete"] = False
        if wrong_model or exceeded_turns:
            raise RuntimeUnavailable("Grok reported work outside the approved model or turn allowance. Its result was withheld.")
        return
    names = ("input_tokens", "output_tokens", "cache_read_input_tokens", "cache_creation_input_tokens")
    incoming, outgoing, cached, created = (_count(aggregate.get(name)) for name in names)
    complete = all(value is not None for value in (incoming, outgoing, cached, created))
    if incoming is not None:
        usage["input_tokens"] = incoming + (cached or 0) + (created or 0)
    if outgoing is not None:
        usage["output_tokens"] = outgoing
    if cached is not None:
        usage["cached_input_tokens"] = cached
    if created is not None:
        usage["cache_creation_input_tokens"] = created
    reasoning = _count(aggregate.get("reasoning_tokens"))
    if reasoning is not None:
        usage["reasoning_tokens"] = reasoning
    usage["total_tokens"] = usage["input_tokens"] + usage["output_tokens"]
    one_approved_model = (isinstance(models, dict) and len(models) == 1
                          and all(model_identity_matches(expected_model, value) for value in models))
    usage["usage_complete"] = bool(complete and turns is not None and turns > 0 and incomplete is not True
                                    and one_approved_model and not exceeded_turns)
    if wrong_model:
        raise RuntimeUnavailable("Grok reported a different or additional model. The result was withheld because that model was not approved for this request.")
    if exceeded_turns:
        raise RuntimeUnavailable("Grok exceeded this run's approved model-turn allowance. Its result was withheld.")


def _tool_name(value):
    return value if isinstance(value, str) else value.get("name") if isinstance(value, dict) else None


def accumulate_boundary(usage, event):
    """Retain observed response tokens if cancellation prevents a final ledger."""
    values = event.get("usage")
    if not isinstance(values, dict):
        return
    incoming = _count(values.get("input_tokens"))
    outgoing = _count(values.get("output_tokens"))
    cached = _count(values.get("cache_read_input_tokens"))
    created = _count(values.get("cache_creation_input_tokens"))
    usage["requests"] += 1
    usage["input_tokens"] += (incoming or 0) + (cached or 0) + (created or 0)
    usage["output_tokens"] += outgoing or 0
    if cached is not None:
        usage["cached_input_tokens"] = usage.get("cached_input_tokens", 0) + cached
    usage["usage_complete"] = False


def headless_command(binary, route, requests, profile, prompt, *, no_tools=False, resume_session_id=None):
    """Build the pinned Grok top-level headless invocation.

    Headless runs use the top-level options, where the profile path is supplied
    by `--agent`. The pinned CLI also accepts the explicit no-leader and
    no-auto-update guards used by this isolated worker.
    """
    command = [*binary, "--no-leader", "--no-auto-update", "--model", route["model_id"],
               "--output-format", "streaming-json", "--max-turns", str(requests)]
    if not no_tools:
        command.extend(["--tools", "search_tool,use_tool"])
    command.extend(["--disallowed-tools", "Agent", "--allow", "MCPTool(quantix__*)",
                    "--agent", str(profile), "--prompt-file", str(prompt)])
    if route.get("reasoning"):
        command.extend(["--reasoning-effort", route["reasoning"]])
    if resume_session_id is not None:
        from uuid import UUID
        try:
            UUID(resume_session_id)
        except (ValueError, TypeError):
            raise RuntimeUnavailable("Resume Grok only by its exact native UUID.") from None
        command.extend(["--resume", resume_session_id])
    return command


async def execute_grok(route, connection, credentials, context, instruction, output_type,
                       *, consult=None, before_request=None, on_response=None):
    if connection.get("auth_type") != "client_login" or connection.get("billing") != "subscription" or credentials:
        raise RuntimeUnavailable("Use the original Grok Build sign-in for this subscription connection. API-key billing requires its separate xAI API connection.")
    if route.get("web_search"):
        raise RuntimeUnavailable("This Grok Build connection exposes Tender tools only. Use an approved research connection for online sources.")
    requests, output_limit, timeout = execution_limits(connection, route)
    home, bridge = context.account_home, context.bridge
    binding = session_binding(context, connection, route)
    public_events = ClientEventBuffer(context)
    activity_prepared = False
    usage = runtime_usage(connection, route, estimated_cost_usd=None, provider_reported_cost_usd=None,
                          provider_cost_is_partial=None, provider_usage_is_incomplete=None,
                          detail="Grok's reported tokens and nominal provider cost are separate from included allowance or extra account charges.")
    reservation = None
    process = None
    diagnostics = None
    prompt = None
    profile = None
    started = time.monotonic()
    submission_status = "unknown"
    final_outcome = "failed"
    terminal = None
    operation = connection.get("_operation", "execute")
    record("grok_phase", phase="start", outcome="started", protocol="grok_build",
           model=route.get("model_id"), operation_id=context.operation_id)

    async def waiting():
        record("grok_phase", phase="queue", outcome="waiting", protocol="grok_build",
               model=route.get("model_id"), operation_id=context.operation_id)
        await context.control.event("runtime_waiting", "Waiting for this Grok account's current operation to finish.",
                                    {"connection_id": connection["id"]})

    async def release():
        bridge.closed = True
        await close_process(process)

    try:
        async with account_lock(home, on_wait=waiting, before_release=release):
            # A fresh, authenticated preflight belongs to this operation. No
            # previously displayed allowance snapshot authorizes paid exposure.
            configure(home, model=route["model_id"], output_limit=output_limit)
            async with acp_session(home, connection) as (client, initialized):
                snapshot = await billing_in_session(client, initialized)
                check_billing_permission(snapshot, connection)
                models = await models_in_session(client)
                selected = next((model for model in models if model["model_id"] == route["model_id"]), None)
                if selected is None:
                    raise RuntimeUnavailable("The selected model is no longer available to this Grok account. Refresh its model list before retrying.")
                if route.get("reasoning") and route["reasoning"] not in selected["capabilities"]["reasoning"]:
                    raise RuntimeUnavailable("Grok did not advertise the approved reasoning setting for this model. Review the model settings.")
                record("grok_phase", phase="preflight", outcome="passed", protocol="grok_build",
                       model=route.get("model_id"), operation_id=context.operation_id)
            usage["subscription_usage"] = snapshot
            work = work_directory(context, binding, uuid4().hex)
            prompt, profile = work / "instruction.txt", work / "agent.md"
            # A strict profile and CLI allowlist compose. Meta-tools may only
            # reach the one configured Quantix server; no shell is exposed.
            builtin_tools = ["search_tool", "use_tool"]
            profile_description = (
                "Classify a bounded Tender conversation without inspecting documents."
                if operation == "conversation"
                else "Inspect scoped Tender evidence and submit an engineering proposal."
            )
            profile_content = ("---\nname: quantix-tender\ndescription: " + profile_description + "\n"
                          f"tools: {builtin_tools}\ndisallowedTools: [Agent]\n"
                          f"maxTurns: {requests}\n---\n"
                          f"Use only the quantix MCP tools. Submit the complete proposal with {SUBMIT_TOOL}. "
                          "Do not inspect local paths, run commands, use other services, or delegate to built-in agents.\n")
            prompt_content = instruction + f"\n\nUse only the quantix MCP tools and submit the complete proposal with {SUBMIT_TOOL}.\n"
            write_private(profile, profile_content)
            write_private(prompt, prompt_content)
            async with bridge.serve():
                configure(home, model=route["model_id"], output_limit=output_limit, bridge=bridge)
                command = headless_command(
                    grok_command(connection), route, requests, profile, prompt,
                    no_tools=operation == "conversation",
                    resume_session_id=binding.get("provider_session_id") if binding else None,
                )
                reservation, _, _ = await reserve_runtime(connection, route, instruction, before_request)
                if operation != "check":
                    await public_events.request_activity("prepared", payload={"instructions": profile_content, "input": prompt_content,
                        "tools": bridge.names, "settings": {"model": route["model_id"], "reasoning": route.get("reasoning"),
                            "max_output_tokens": output_limit, "max_requests": requests}, "billing": connection.get("billing")})
                    activity_prepared = True
                    await public_events.request_activity("started")
                async with asyncio.timeout(timeout):
                    process = await spawn(command, work, environment(home, bridge=bridge), limit=16 * 1024 * 1024)
                    record("grok_process", phase="process", outcome="started", protocol="grok_build",
                           model=route.get("model_id"), operation_id=context.operation_id)
                    # Keep the native process owner's parent pipe open. EOF is
                    # its explicit shutdown signal, even for a headless child.
                    diagnostics = asyncio.create_task(discard(process.stderr))
                    terminal = None
                    failed = False
                    failure_reason = None
                    response_ids = set()
                    total = 0
                    while line := await process.stdout.readline():
                        total += len(line)
                        if total > 64 * 1024 * 1024:
                            raise RuntimeUnavailable("Grok exceeded the bounded output stream for this Tender run.")
                        try:
                            event = json.loads(line)
                        except (ValueError, UnicodeDecodeError):
                            raise RuntimeUnavailable("Grok returned an unsupported structured stream. Its result was withheld.") from None
                        if not isinstance(event, dict) or not isinstance(event.get("type"), str):
                            raise RuntimeUnavailable("Grok returned an invalid structured event.")
                        if terminal is not None:
                            raise RuntimeUnavailable("Grok emitted content after its terminal result. Its result was withheld.")
                        kind = event["type"]
                        if kind == "text" and operation != "check":
                            await public_events.text(event.get("data"), item_id=f"response-{usage['requests'] + 1}")
                        if kind == "usage":
                            identity = event.get("messageId")
                            if identity is not None and (not isinstance(identity, str) or len(identity) > 500):
                                raise RuntimeUnavailable("Grok returned an invalid usage boundary.")
                            if identity is None or identity not in response_ids:
                                accumulate_boundary(usage, event)
                                if identity is not None:
                                    response_ids.add(identity)
                                record("grok_round", phase="inference", outcome="observed",
                                       protocol="grok_build", model=route.get("model_id"),
                                       operation_id=context.operation_id, round=usage.get("requests"),
                                       requests=usage.get("requests"), input_tokens=usage.get("input_tokens"),
                                       output_tokens=usage.get("output_tokens"), actual_model=usage.get("actual_model"))
                                await context.control.event("model_response", "Model response received.",
                                                            {"round": usage.get("requests"), "request_id": public_events.request_id, "provider_response_id": identity,
                                                             "usage": {name: usage.get(name) for name in ("requests", "input_tokens", "output_tokens", "cached_input_tokens", "reasoning_tokens")}})
                            if usage["requests"] > requests:
                                raise RuntimeUnavailable("Grok exceeded the approved model-turn allowance. Its result was withheld.")
                        if kind == "available_commands":
                            tools = event.get("tools")
                            if isinstance(tools, list):
                                builtin_tools = {"search_tool", "use_tool"}
                                allowed = {*builtin_tools, *bridge.names,
                                           *("quantix__" + name for name in bridge.names)}
                                tool_names = [_tool_name(tool) for tool in tools]
                                safe_tools = [name for name in tool_names if name in allowed]
                                record("grok_tools", phase="inference", outcome="advertised",
                                       protocol="grok_build", model=route.get("model_id"),
                                       operation_id=context.operation_id, tool_names=safe_tools,
                                       tool_in_allowlist=all(name in allowed for name in tool_names))
                                if any(_tool_name(tool) not in allowed for tool in tools):
                                    raise RuntimeUnavailable("Grok advertised tools outside this Tender's approved tool set.")
                        if kind == "tool_call":
                            name = event.get("toolName")
                            builtin_tools = {"search_tool", "use_tool"}
                            if name in {*builtin_tools, *bridge.names,
                                         *("quantix__" + item for item in bridge.names)}:
                                record("grok_tool_call", phase="inference", outcome="observed",
                                       protocol="grok_build", model=route.get("model_id"),
                                       operation_id=context.operation_id, tool_name=name,
                                       submission_status=submission_status)
                            if name not in {*builtin_tools, *bridge.names,
                                            *("quantix__" + item for item in bridge.names)}:
                                raise RuntimeUnavailable("Grok attempted a tool outside the approved Tender tool set.")
                            if operation != "check":
                                await context.control.event("runtime_tool_activity", "Original client tool activity.", {"tool": name, "phase": "started",
                                    "provider_call_id": event.get("toolCallId"), "request_id": public_events.request_id})
                        if kind.startswith("auto_compact"):
                            record("grok_terminal", phase="inference", outcome="failed",
                                   protocol="grok_build", model=route.get("model_id"),
                                   operation_id=context.operation_id, stop_reason="other",
                                   submission_status=submission_status)
                            usage["provider_usage_is_incomplete"] = True
                            raise RuntimeUnavailable("Grok reached a context-compaction boundary. Split the engineering work before retrying; auxiliary inference is not approved for this run.")
                        if kind in {"error", "max_turns_reached"}:
                            error_details = classify_provider_error(event) if kind == "error" else {"category": "max_turns", "status_code": None}
                            record("grok_terminal", phase="inference", outcome="failed",
                                   protocol="grok_build", model=route.get("model_id"),
                                   operation_id=context.operation_id,
                                    stop_reason="max_turns" if kind == "max_turns_reached" else "error",
                                    submission_status=submission_status,
                                    error_type=error_details["category"],
                                    status_code=error_details["status_code"])
                            update_usage(usage, event, route["model_id"], requests)
                            # Grok emits its max-turn marker before the final
                            # aggregate. Keep reading that terminal accounting;
                            # a subsequent end_turn cannot erase this failure.
                            failed = True
                            failure_reason = error_details["category"]
                        if kind == "end":
                            terminal = event
                            update_usage(usage, event, route["model_id"], requests)
                            reason = terminal.get("stopReason")
                            record("grok_terminal", phase="inference", outcome="observed",
                                   protocol="grok_build", model=route.get("model_id"),
                                    operation_id=context.operation_id,
                                    stop_reason=reason if reason in _KNOWN_STOP_REASONS else "other",
                                    submission_status=submission_status,
                                   rounds=usage.get("requests"), actual_model=usage.get("actual_model"))
                    await process.wait()
                    record("grok_process", phase="process", outcome="completed" if process.returncode == 0 else "failed",
                           protocol="grok_build", model=route.get("model_id"), operation_id=context.operation_id,
                           exit_code=process.returncode, rounds=usage.get("requests"),
                           submission_status=submission_status)
                    if failed:
                        raise RuntimeUnavailable(_safe_failure_message(failure_reason or "provider_error", operation=operation))
                    if terminal is None:
                        record("grok_terminal", phase="inference", outcome="failed",
                               protocol="grok_build", model=route.get("model_id"),
                               operation_id=context.operation_id, stop_reason="missing",
                               submission_status=submission_status, rounds=usage.get("requests"))
                        raise RuntimeConnectionFailure(_safe_failure_message("missing_terminal", operation=operation) + " Any unreported usage remains uncertain.")
                    if process.returncode != 0:
                        raise RuntimeConnectionFailure("Grok's provider process ended unexpectedly. Retry the operation; any unreported usage remains uncertain.")
                    if terminal.get("stopReason") == "cancelled":
                        raise InterruptedError("Grok cancelled the request before completion.")
                    if terminal.get("stopReason") != "end_turn":
                        raise RuntimeUnavailable("Grok stopped before the complete engineering proposal was ready.")
                    if not model_identity_matches(route["model_id"], usage.get("actual_model")):
                        raise RuntimeUnavailable("Grok did not establish the exact approved model identity. The result was withheld.")
                    try:
                        result = await bridge.result()
                    except RuntimeUnavailable:
                        if operation == "check":
                            raise RuntimeUnavailable("Grok completed without a validated structured connection-check submission. Retry the access check.") from None
                        raise RuntimeUnavailable("Grok completed without a validated structured engineering submission. Retry this operation.") from None
                    submission_status = "received"
                    if operation != "check":
                        await public_events.finish()
                    if binding and usage.get("session_id"):
                        await context.control.event("runtime_session", "Original client session observed.",
                            {"binding_id": binding["id"], "provider_session_id": usage["session_id"]})
                    final_outcome = "completed"
                    record("grok_phase", phase="submission", outcome="received", protocol="grok_build",
                           model=route.get("model_id"), operation_id=context.operation_id,
                           submission_status="received", rounds=usage.get("requests"))
                    await context.control.event("runtime_model_reported", "The original client reported its selected model.",
                                                {"connection_id": connection["id"], "requested_model": route["model_id"],
                                                 "actual_model": usage["actual_model"]})
                    if operation != "check":
                        await public_events.request_activity("completed", payload={"usage": usage, "output": result})
                    return {"output": result, "usage": usage, "web_sources": []}
    except BaseException as error:
        if activity_prepared:
            await public_events.finish(complete=False)
            await public_events.request_activity("cancelled" if isinstance(error, (asyncio.CancelledError, InterruptedError)) else "failed",
                payload={"usage": usage, "detail": "The original-client request ended before successful completion."})
        if not isinstance(error, asyncio.CancelledError):
            record_exception("grok_failed", error, phase="inference", protocol="grok_build",
                             model=route.get("model_id"), operation_id=context.operation_id,
                             rounds=usage.get("requests"),
                             submission_status=submission_status)
        raise
    finally:
        record("grok_phase", phase="stop", outcome=final_outcome, protocol="grok_build",
               model=route.get("model_id"), operation_id=context.operation_id,
               duration_ms=int((time.monotonic() - started) * 1000), rounds=usage.get("requests"),
               submission_status=submission_status)
        bridge.closed = True
        try:
            await close_process(process)
        finally:
            if diagnostics:
                diagnostics.cancel()
                await asyncio.gather(diagnostics, return_exceptions=True)
            try:
                if reservation is not None:
                    await call_hook(on_response, usage, reservation)
            finally:
                if prompt:
                    prompt.unlink(missing_ok=True)
                if profile:
                    profile.unlink(missing_ok=True)
