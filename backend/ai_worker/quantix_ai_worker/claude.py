"""Anthropic Agent SDK with BYOK, no built-in tools or account-token reuse."""

import asyncio
from uuid import uuid4

from .common import (
    RuntimeConnectionFailure,
    RuntimeUnavailable,
    call_hook,
    child_environment,
    execution_limits,
    reserve_runtime,
    runtime_usage,
    transient_status,
)
from .remote import SUBMIT_TOOL


async def execute_claude(route, connection, credentials, context, instruction, output_type,
                         *, consult=None, before_request=None, on_response=None):
    from claude_agent_sdk import (
        AssistantMessage,
        ClaudeAgentOptions,
        ClaudeSDKClient,
        HookMatcher,
        PermissionResultAllow,
        PermissionResultDeny,
        ResultMessage,
    )

    if connection["auth_type"] not in {"api_key", "environment"}:
        raise RuntimeUnavailable("Claude Agent requires an API key. Claude subscription sign-in belongs to the original attended Claude Code client.")
    key = credentials.get("api_key")
    if not key:
        raise RuntimeUnavailable("Add the API key for this Claude Agent connection.")
    if route.get("web_search"):
        raise RuntimeUnavailable("This Claude Agent connection exposes Tender tools only. Select an approved API research connection for web research.")

    requests, max_output, timeout = execution_limits(connection, route)
    home = context.account_home
    env = child_environment(home)
    env.update({"ANTHROPIC_API_KEY": key, "CLAUDE_CODE_MAX_OUTPUT_TOKENS": str(max_output),
                "CLAUDE_CODE_DISABLE_AUTO_MEMORY": "1"})
    if connection.get("base_url"):
        env["ANTHROPIC_BASE_URL"] = connection["base_url"]
    work = home / "work" / context.operation_id / uuid4().hex
    work.mkdir(parents=True, exist_ok=True)
    # Passing a file avoids operating-system command-line limits for a full
    # engineering instruction. It is private runtime data outside the repository.
    system_path = work / "instruction.txt"
    system_path.write_text(instruction + f"\nUse {SUBMIT_TOOL} to submit the complete proposal. All other tools are scoped reads. No files or commercial decisions may be changed.", encoding="utf-8")
    bridge = context.bridge
    usage = runtime_usage(connection, route)
    reservation = None
    terminal = None
    spoken = ""
    models = set()

    async def permission(name, _args, _ctx):
        if name in {f"mcp__quantix__{tool}" for tool in bridge.names}:
            return PermissionResultAllow()
        return PermissionResultDeny(message="Only this run's Tender tools are permitted.")

    async def before_tool(data, _tool_use_id, _ctx):
        name = data.get("tool_name", "")
        allowed = name in {f"mcp__quantix__{tool}" for tool in bridge.names}
        return {"hookSpecificOutput": {"hookEventName": "PreToolUse",
                "permissionDecision": "allow" if allowed else "deny",
                "permissionDecisionReason": "The local client can use only scoped Tender tools."}}

    try:
        async with bridge.serve():
            options = ClaudeAgentOptions(
                tools=[], allowed_tools=[], permission_mode="default", can_use_tool=permission,
                hooks={"PreToolUse": [HookMatcher(hooks=[before_tool])]},
                system_prompt={"type": "file", "path": str(system_path)},
                mcp_servers={"quantix": {"type": "http", "url": bridge.url,
                    "headers": {"Authorization": f"Bearer {bridge.token}"}}},
                strict_mcp_config=True, setting_sources=[], skills=[], plugins=[], agents={},
                model=route["model_id"], effort=route.get("reasoning"),
                max_turns=requests, cwd=str(work), env=env,
                session_id=str(uuid4()), enable_file_checkpointing=False,
            )
            reservation, _, _ = await reserve_runtime(connection, route, instruction, before_request)
            async with asyncio.timeout(timeout):
                async with ClaudeSDKClient(options=options) as client:
                    try:
                        await client.query("Carry out the engineering instruction and submit the structured result through the Tender proposal tool.")
                        async for message in client.receive_response():
                            if isinstance(message, AssistantMessage):
                                models.add(message.model)
                                usage["requests"] += 1
                                # Kept only for the bounded conversation pass,
                                # where the control service may accept it when
                                # the client answered without submitting.
                                said = "".join(
                                    getattr(block, "text", "") or ""
                                    for block in (message.content or [])
                                ).strip()
                                if said:
                                    spoken = said
                                if message.model != route["model_id"]:
                                    raise RuntimeUnavailable("Claude Agent reported a different model. The result was withheld; choose its exact model identifier before retrying.")
                                if usage["requests"] > requests:
                                    raise RuntimeUnavailable("Claude Agent reached the approved request limit.")
                            if isinstance(message, ResultMessage):
                                terminal = message
                    except BaseException:
                        bridge.closed = True
                        try:
                            await asyncio.wait_for(client.interrupt(), 3)
                        except Exception:
                            pass
                        raise
            if terminal is None:
                raise RuntimeUnavailable("Claude Agent did not report a final run result.")
            reported = terminal.usage if isinstance(terminal.usage, dict) else {}
            required = ("input_tokens", "output_tokens")
            optional = ("cache_creation_input_tokens", "cache_read_input_tokens")
            def valid_count(value):
                return type(value) is int and value >= 0

            def count(key):
                return reported[key] if key in reported and valid_count(reported[key]) else 0

            complete_usage = all(key in reported and valid_count(reported[key]) for key in required)
            complete_usage = complete_usage and all(key not in reported or valid_count(reported[key]) for key in optional)
            usage.update(
                requests=terminal.num_turns,
                input_tokens=count("input_tokens") + count("cache_creation_input_tokens") + count("cache_read_input_tokens"),
                output_tokens=count("output_tokens"),
                actual_model=next(iter(models)) if len(models) == 1 else None,
                usage_complete=complete_usage,
                provider_reported_cost_usd=terminal.total_cost_usd,
                session_id=terminal.session_id,
            )
            if terminal.terminal_reason in {"aborted_streaming", "aborted_tools"}:
                raise InterruptedError("The Claude Agent turn was interrupted before completion.")
            if terminal.is_error or terminal.subtype != "success":
                if transient_status(terminal.api_error_status):
                    raise RuntimeConnectionFailure("The Claude service was temporarily unavailable.")
                raise RuntimeUnavailable("Claude Agent ended without a successful result. Review its account or request limits before retrying.")
            return {"output": await bridge.result(spoken), "usage": usage, "web_sources": []}
    finally:
        if reservation is not None:
            await call_hook(on_response, usage, reservation)
        system_path.unlink(missing_ok=True)
