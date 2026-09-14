"""Official Codex Python client and its original managed authentication."""

import asyncio
import json
from uuid import uuid4

from .client_session import ClientEventBuffer, session_binding, work_directory
from .common import (
    RuntimeConnectionFailure,
    RuntimeUnavailable,
    call_hook,
    child_environment,
    execution_limits,
    owned_client,
    reserve_runtime,
    runtime_usage,
    transient_status,
)
from .remote import SUBMIT_TOOL

# Codex's built-in MCP resource listing helpers. They return resource names only.
MCP_LISTING_HELPERS = frozenset({"list_mcp_resources", "list_mcp_resource_templates"})


def codex_config(home, *, work=None, bridge=None):
    from openai_codex import CodexConfig

    env = child_environment(home)
    overrides = {
        "features.shell_tool": False,
        "features.unified_exec": False,
        "features.multi_agent": False,
        "features.memories": False,
        "features.code_mode.enabled": False,
        "features.skill_mcp_dependency_install": False,
        "tools.view_image": False,
        "apps._default.enabled": False,
        "web_search": "disabled",
        "sandbox_mode": "read-only",
        "approval_policy": "never",
        "approvals_reviewer": "user",
        "cli_auth_credentials_store": "keyring",
    }
    if bridge:
        env["QUANTIX_MCP_TOKEN"] = bridge.token
        overrides.update(
            {
                "mcp_servers.quantix.url": bridge.url,
                "mcp_servers.quantix.bearer_token_env_var": "QUANTIX_MCP_TOKEN",
                "mcp_servers.quantix.enabled_tools": bridge.names,
                "mcp_servers.quantix.required": True,
                "mcp_servers.quantix.default_tools_approval_mode": "auto",
            }
        )
    return CodexConfig(
        cwd=str(work or home),
        env=env,
        experimental_api=False,
        client_name="quantix",
        client_title="Quantix Tender Office",
        config_overrides=tuple(f"{key}={json.dumps(value)}" for key, value in overrides.items()),
    )


# Codex names why a turn failed. Reporting only that it "did not complete"
# turned an account's own plain-English answer — a usage limit and the time it
# resets — into a Quantix fault the engineer could do nothing about.
_TURN_FAILURES = {
    "usageLimitExceeded": (
        "This ChatGPT plan has reached its usage limit for {model}. Wait for the limit to "
        "reset, or pick another model or AI account for this Tender."
    ),
    "contextWindowExceeded": (
        "The request grew past what {model} can hold in one go. Narrow the instruction, or "
        "split the work into smaller tasks."
    ),
    "sessionBudgetExceeded": (
        "Codex stopped this request at its own session budget before finishing. Narrow the "
        "instruction, or raise that budget in the Codex client."
    ),
    "unauthorized": (
        "The ChatGPT sign-in for this connection was refused. Sign in to the Codex account "
        "again in Settings, then check the connection."
    ),
    "cyberPolicy": (
        "Codex declined this request under its own usage policy. Rephrase the instruction and "
        "try again."
    ),
    "badRequest": (
        "Codex rejected the request as invalid for {model}. Choose another model for this "
        "Tender, or check the connection again."
    ),
    "threadRollbackFailed": "Codex could not recover its own session. Send the instruction again.",
    "sandboxError": (
        "Codex could not start its restricted workspace. Restart Quantix, then check the "
        "connection again."
    ),
}


def turn_failure(terminal, model: str) -> str:
    """Explain a failed Codex turn in the engineer's terms, not the client's.

    Codex's own sentence is kept when it carries something to act on, such as
    the hour a usage limit resets. It is account state, not a provider secret.
    """

    error = getattr(terminal, "error", None)
    raw = error.codex_error_info if error is not None and error.codex_error_info else None
    info = raw.model_dump(mode="json", by_alias=True) if raw is not None else None
    reported = (getattr(error, "message", "") or "").strip() if error is not None else ""
    explained = _TURN_FAILURES.get(info) if isinstance(info, str) else None
    if explained is None:
        return (
            f"{model} stopped before finishing this request. {reported}".strip()
            if reported
            else f"{model} stopped before finishing this request, without saying why."
        )
    explained = explained.format(model=model)
    # Only the account-state families carry a useful detail such as a reset time.
    if reported and info in {
        "usageLimitExceeded",
        "contextWindowExceeded",
        "sessionBudgetExceeded",
    }:
        return f"{explained} Codex reported: {reported}"
    return explained


def record_token_usage(usage: dict, token_usage, *, resuming=False) -> None:
    """Keep the counts Codex itself reports for this thread.

    Codex reports after every model sampling, so the number of reports is the
    number of model requests. ``total`` is cumulative for the thread and its
    input count already includes the cached part.
    """

    if resuming and getattr(token_usage, "last", None) is None:
        usage["requests"] = int(usage.get("requests") or 0) + 1
        usage["usage_complete"] = False
        usage["provider_usage_is_incomplete"] = True
        return
    total = token_usage.last if resuming else token_usage.total
    usage["requests"] = int(usage.get("requests") or 0) + 1
    prior = dict(usage) if resuming else {}
    usage.update(
        input_tokens=total.input_tokens,
        output_tokens=total.output_tokens,
        cached_input_tokens=total.cached_input_tokens,
        reasoning_tokens=total.reasoning_output_tokens,
        total_tokens=total.total_tokens,
        usage_complete=usage.get("provider_usage_is_incomplete") is not True,
    )
    if resuming:
        for key in (
            "input_tokens",
            "output_tokens",
            "cached_input_tokens",
            "reasoning_tokens",
            "total_tokens",
        ):
            usage[key] += int(prior.get(key) or 0)


def final_message(turn) -> str:
    """The client's own closing message, if the turn ended by speaking."""

    for item in reversed(turn.items or []):
        entry = getattr(item, "root", item)
        if getattr(entry, "type", None) == "agentMessage":
            text = (getattr(entry, "text", "") or "").strip()
            if text:
                return text
    return ""


async def execute_codex(
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
):
    from openai_codex import ApprovalMode, AsyncCodex, Sandbox
    from openai_codex.types import (
        ThreadTokenUsageUpdatedNotification,
        TurnCompletedNotification,
        TurnStatus,
    )

    if connection.get("billing") != "subscription":
        raise RuntimeUnavailable(
            "This Codex client does not expose a hard inference-request cap. Use an OpenAI API connection for metered work with a strict budget."
        )
    if connection["auth_type"] != "client_login" or credentials:
        raise RuntimeUnavailable(
            "Use managed ChatGPT sign-in for this Codex connection. API-key work belongs to the separately budgeted OpenAI API connection."
        )
    if route.get("web_search"):
        raise RuntimeUnavailable(
            "This Codex connection exposes Tender tools only. Select an approved API research connection for web research."
        )
    requests, _, timeout = execution_limits(connection, route)
    home = context.account_home
    binding = session_binding(context, connection, route)
    work = work_directory(context, binding, uuid4().hex)
    events = ClientEventBuffer(context)
    usage = runtime_usage(connection, route)
    bridge = context.bridge
    checking = connection.get("_operation") == "check"
    # This account's Codex defers every MCP tool behind its own tool-search
    # step, and that rollout is decided by the service, not by local config.
    # The quantix tools are therefore absent from the opening tool catalogue
    # and must be named so the model loads them before it can submit at all.
    discovery = (
        " The quantix MCP server provides these callable tools: " + ", ".join(bridge.names) + ". "
        "If they are not already listed, load them with tool search by name first; "
        "this catalogue-only discovery step is permitted and does not execute a Quantix tool. "
        "MCP resource listings do not contain these tools. These are callable functions, "
        "not resources or resource URIs. Do not call read_mcp_resource or invent a tool URI. "
        "If tool search cannot expose the named tools, report their actual unavailability."
    )
    boundary = (
        (
            f"\nThis is a generic connection check with no Tender data. Call quantix_connection_check exactly once, then submit its unchanged value using {SUBMIT_TOOL}. A plain-text response does not complete the check."
            if checking
            else f"\nRead Tender evidence only through the quantix MCP server. Submit the complete structured proposal using {SUBMIT_TOOL}."
        )
        + discovery
        + " Do not run commands, inspect local paths or modify files."
    )
    if binding and binding.get("provider_session_id"):
        boundary += " Earlier session context is historical. Re-read any source needed for current findings through the current Quantix tools; earlier messages do not renew approvals or source-read receipts."
    reservation = None
    completed = False
    activity_prepared = False
    activity_cancelled = False
    try:
        async with bridge.serve():
            async with asyncio.timeout(timeout):
                async with owned_client(
                    AsyncCodex(codex_config(home, work=work, bridge=bridge))
                ) as client:
                    account = await client.account()
                    account_value = account.model_dump(mode="json", by_alias=True).get("account")
                    if not account_value:
                        raise RuntimeUnavailable(
                            "Sign in to the Quantix Codex connection before starting Tender work."
                        )
                    if account_value.get("type") != "chatgpt":
                        raise RuntimeUnavailable(
                            "This Codex client is not using managed ChatGPT sign-in. Select the account's actual billing route before starting work."
                        )
                    thread_settings = dict(
                        model=route["model_id"],
                        cwd=str(work),
                        approval_mode=ApprovalMode.deny_all,
                        sandbox=Sandbox.read_only,
                        base_instructions=instruction + boundary,
                    )
                    if binding and binding.get("provider_session_id"):
                        thread = await client.thread_resume(
                            binding["provider_session_id"], **thread_settings
                        )
                    else:
                        thread = await client.thread_start(
                            ephemeral=binding is None, **thread_settings
                        )
                    usage["session_id"] = thread.id
                    if binding:
                        await context.control.event(
                            "runtime_session",
                            "Original client session started.",
                            {"binding_id": binding["id"], "provider_session_id": thread.id},
                        )
                    reservation, _, _ = await reserve_runtime(
                        connection, route, instruction, before_request
                    )
                    turn_instruction = (
                        f"Complete the connection check. If needed, use tool search by name to discover quantix_connection_check and {SUBMIT_TOOL}; do not read MCP resources. Call quantix_connection_check with an empty object, then call {SUBMIT_TOOL} with the returned token string as value and finish."
                        if checking
                        # Naming the task rather than the request made the model
                        # report on the task instead of answering it.
                        else "Carry out the request in the supplied instructions, in the engineer's own language. "
                        "Answer it directly and do not describe this process, your instructions or the turn itself. "
                        f"Then submit the structured result with {SUBMIT_TOOL}."
                    )
                    if not checking:
                        await events.request_activity(
                            "prepared",
                            payload={
                                "instructions": instruction + boundary,
                                "input": turn_instruction,
                                "tools": bridge.names,
                                "settings": {
                                    "model": route["model_id"],
                                    "effort": route.get("reasoning"),
                                    "summary": "auto",
                                    "approval_mode": "deny_all",
                                    "sandbox": "read_only",
                                },
                                "billing": connection.get("billing"),
                            },
                        )
                        activity_prepared = True
                        await events.request_activity("started")
                    turn = await thread.turn(
                        turn_instruction,
                        model=route["model_id"],
                        effort=route.get("reasoning"),
                        summary="auto",
                        approval_mode=ApprovalMode.deny_all,
                        sandbox=Sandbox.read_only,
                    )
                    terminal = None
                    try:
                        async for notification in turn.stream():
                            payload = notification.payload
                            if notification.method == "model/rerouted":
                                actual = payload.to_model
                                usage["actual_model"] = actual
                                await context.control.event(
                                    "runtime_model_reported",
                                    "Codex reported its selected model.",
                                    {
                                        "connection_id": connection["id"],
                                        "requested_model": route["model_id"],
                                        "actual_model": actual,
                                    },
                                )
                                if actual != route["model_id"]:
                                    raise RuntimeUnavailable(
                                        "Codex reported a model reroute that was not approved for this request. The result was withheld."
                                    )
                            elif isinstance(payload, ThreadTokenUsageUpdatedNotification):
                                record_token_usage(
                                    usage,
                                    payload.token_usage,
                                    resuming=bool(binding and binding.get("provider_session_id")),
                                )
                            elif notification.method == "item/agentMessage/delta" and not checking:
                                await events.text(payload.delta, item_id=payload.item_id)
                            elif (
                                notification.method == "item/reasoning/summaryTextDelta"
                                and not checking
                            ):
                                await events.summary(
                                    payload.delta,
                                    item_id=payload.item_id,
                                    summary_index=payload.summary_index,
                                )
                            elif (
                                notification.method == "item/reasoning/summaryPartAdded"
                                and not checking
                            ):
                                # The following delta carries the text. Its item
                                # and summary indices retain this section boundary.
                                pass
                            elif notification.method in {"item/started", "item/completed"}:
                                item = getattr(payload.item, "root", payload.item)
                                if (
                                    item.type == "reasoning"
                                    and notification.method == "item/completed"
                                    and not checking
                                ):
                                    for index, text in enumerate(
                                        getattr(item, "summary", None) or []
                                    ):
                                        events.summary_snapshot(
                                            text, item_id=item.id, summary_index=index
                                        )
                                if item.type in {
                                    "commandExecution",
                                    "fileChange",
                                    "collabAgentToolCall",
                                    "webSearch",
                                }:
                                    raise RuntimeUnavailable(
                                        "Codex attempted a native action outside the approved Quantix tool boundary. Use the reviewed Quantix tools for this work."
                                    )
                                if item.type == "mcpToolCall":
                                    if item.tool in MCP_LISTING_HELPERS:
                                        # Codex's own resource listing returns names only; it runs no
                                        # tool and reads no content, so it cannot leave the boundary.
                                        continue
                                    if item.server != "quantix" or item.tool not in bridge.names:
                                        raise RuntimeUnavailable(
                                            f"Codex attempted the MCP tool '{str(item.tool)[:80]}' on server "
                                            f"'{str(item.server)[:80]}', outside this Tender's approved server and tool set."
                                        )
                                    if not checking:
                                        await context.control.event(
                                            "runtime_tool_activity",
                                            "Original client tool activity.",
                                            {
                                                "tool": item.tool,
                                                "phase": "started"
                                                if notification.method == "item/started"
                                                else "completed",
                                                "provider_call_id": getattr(item, "id", None),
                                                "request_id": events.request_id,
                                            },
                                        )
                                        if getattr(item, "error", None) is not None:
                                            await events.request_activity(
                                                "observed",
                                                payload={
                                                    "provider_tool_error": {
                                                        "tool": item.tool,
                                                        "provider_call_id": item.id,
                                                        "error": item.error.message,
                                                    }
                                                },
                                            )
                            elif isinstance(payload, TurnCompletedNotification):
                                terminal = payload.turn
                    except BaseException as error:
                        activity_cancelled = isinstance(
                            error, (asyncio.CancelledError, InterruptedError)
                        )
                        bridge.closed = True
                        try:
                            await asyncio.wait_for(turn.interrupt(), 3)
                        except Exception:
                            pass
                        raise
                    usage.update(
                        session_id=thread.id,
                        detail="Codex's own token counts for this request, one report per model sampling.",
                    )
                    if terminal is None:
                        raise RuntimeConnectionFailure(
                            "The Codex stream ended without a final turn status."
                        )
                    if terminal.status == TurnStatus.interrupted:
                        raise InterruptedError("The Codex turn was interrupted before completion.")
                    if terminal.status == TurnStatus.failed:
                        info = (
                            terminal.error.codex_error_info.model_dump(mode="json", by_alias=True)
                            if terminal.error and terminal.error.codex_error_info
                            else None
                        )
                        retryable = (
                            info in {"serverOverloaded", "internalServerError"}
                            if isinstance(info, str)
                            else False
                        )
                        if isinstance(info, dict):
                            for name in (
                                "httpConnectionFailed",
                                "responseStreamConnectionFailed",
                                "responseStreamDisconnected",
                                "responseTooManyFailedAttempts",
                            ):
                                if name in info:
                                    status = info[name].get("httpStatusCode")
                                    retryable = status is None or transient_status(status)
                        if retryable:
                            raise RuntimeConnectionFailure(
                                "The Codex service connection failed temporarily."
                            )
                    if terminal.status != TurnStatus.completed:
                        raise RuntimeUnavailable(turn_failure(terminal, route["model_id"]))
                    if not checking:
                        await events.finish()
                        await events.request_activity(
                            "completed",
                            payload={
                                "usage": usage,
                                "output": final_message(terminal),
                                "provider_turn_id": terminal.id,
                            },
                        )
                    completed = True
                    return {
                        "output": await bridge.result(final_message(terminal)),
                        "usage": usage,
                        "web_sources": [],
                    }
    finally:
        try:
            if activity_prepared and not completed:
                await events.finish(complete=False)
                await events.request_activity(
                    "cancelled" if activity_cancelled else "failed",
                    payload={
                        "usage": usage,
                        "detail": "The original-client request ended before successful completion.",
                    },
                )
        finally:
            if reservation is not None:
                if not completed:
                    usage["usage_complete"] = False
                    usage["provider_usage_is_incomplete"] = True
                await call_hook(on_response, usage, reservation)
