"""GitHub's official SDK with an explicit MCP-only tool set."""

import asyncio
from uuid import uuid4

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


async def execute_copilot(route, connection, credentials, context, instruction, output_type,
                          *, consult=None, before_request=None, on_response=None):
    from copilot import CopilotClient, RuntimeConnection
    from copilot.generated.rpc import PermissionDecisionApproveOnce, PermissionDecisionReject
    from copilot.session import ModelCapabilitiesOverride, ModelLimitsOverride

    if connection.get("billing") != "subscription":
        raise RuntimeUnavailable("The Copilot agent loop cannot enforce Quantix's per-inference metered budget. Use a direct API connection for that billing route.")
    if route.get("web_search"):
        raise RuntimeUnavailable("This Copilot connection exposes Tender tools only. Select an approved API research connection for web research.")
    if connection["auth_type"] not in {"client_login", "api_key", "environment"}:
        raise RuntimeUnavailable("Use the original Copilot sign-in or an explicitly supplied GitHub token.")

    requests, max_output, timeout = execution_limits(connection, route)
    home = context.account_home
    from .accounts import copilot_runtime
    executable = copilot_runtime(home)
    env = child_environment(home)
    work = home / "work" / context.operation_id / uuid4().hex
    work.mkdir(parents=True, exist_ok=True)
    bridge = context.bridge
    usage = runtime_usage(connection, route)
    reservation = None
    failure = None
    seen_calls = set()
    stop = asyncio.Event()

    def permission(request, _invocation):
        if (request.kind == "mcp" and request.server_name == "quantix"
                and request.tool_name in bridge.names
                and not getattr(request, "managed_approval_required", False)):
            return PermissionDecisionApproveOnce()
        return PermissionDecisionReject(feedback="Only this run's scoped Tender tools are permitted.")

    def event_received(event):
        nonlocal failure
        kind = event.type.value if hasattr(event.type, "value") else event.type
        data = event.data
        if kind == "assistant.usage":
            identifier = data.api_call_id or data.provider_call_id or data.service_request_id
            if identifier and identifier in seen_calls:
                return
            if identifier:
                seen_calls.add(identifier)
            usage["requests"] += 1
            usage["input_tokens"] += int(data.input_tokens or 0)
            usage["output_tokens"] += int(data.output_tokens or 0)
            usage["actual_model"] = data.model
            if data.input_tokens is None or data.output_tokens is None:
                usage["missing_usage"] = True
            if data.model != route["model_id"]:
                failure = RuntimeUnavailable("Copilot reported a different model. The result was withheld; select an exact approved model before retrying.")
            if usage["requests"] > requests:
                failure = RuntimeUnavailable("Copilot reached the approved request limit.")
            if failure:
                bridge.closed = True
                stop.set()
        elif kind == "session.error":
            failure_type = RuntimeConnectionFailure if transient_status(data.status_code) else RuntimeUnavailable
            failure = failure_type("The Copilot session reported a service error. No Tender result was published.")
            stop.set()

    token = credentials.get("api_key") if connection["auth_type"] != "client_login" else None
    if connection["auth_type"] != "client_login" and not token:
        raise RuntimeUnavailable("Add the GitHub token for this Copilot connection.")
    try:
        async with bridge.serve():
            async with asyncio.timeout(timeout):
                async with owned_client(CopilotClient(
                    connection=RuntimeConnection.for_stdio(path=str(executable)),
                    working_directory=str(work), base_directory=str(home / "copilot"),
                    env=env, github_token=token,
                    use_logged_in_user=connection["auth_type"] == "client_login",
                    enable_remote_sessions=False, log_level="none",
                ), close_method="stop", force_method="force_stop") as client:
                    auth = await client.get_auth_status()
                    if not auth.isAuthenticated:
                        raise RuntimeUnavailable("Complete the original Copilot sign-in before starting Tender work.")
                    session = await client.create_session(
                        model=route["model_id"], session_id=str(uuid4()), client_name="quantix",
                        reasoning_effort=route.get("reasoning"),
                        system_message={"mode": "replace", "content": instruction + f"\nOnly quantix MCP tools may be used. Call {SUBMIT_TOOL} with the complete structured proposal to finish. Do not use any other tools."},
                        available_tools=["mcp:*"], excluded_tools=["builtin:*", "custom:*"],
                        on_permission_request=permission,
                        mcp_servers={"quantix": {"type": "http", "url": bridge.url,
                            "headers": {"Authorization": f"Bearer {bridge.token}"},
                            "tools": bridge.names, "timeout": 120000}},
                        disabled_mcp_servers=["github"],
                        config_directory=str(work), enable_config_discovery=False,
                        skip_custom_instructions=True, enable_on_demand_instruction_discovery=False,
                        enable_file_hooks=False, enable_host_git_operations=False,
                        enable_session_telemetry=False, enable_file_change_tracking=False,
                        enable_session_store=False, enable_skills=False,
                        plugin_directories=[], skill_directories=[], instruction_directories=[],
                        memory={"enabled": False}, infinite_sessions={"enabled": False},
                        large_output={"enabled": False}, tool_search={"enabled": False},
                        model_capabilities=ModelCapabilitiesOverride(limits=ModelLimitsOverride(max_output_tokens=max_output)),
                        streaming=True, on_event=event_received,
                    )
                    async with session:
                        reservation, _, _ = await reserve_runtime(connection, route, instruction, before_request)
                        generation = asyncio.create_task(session.send_and_wait(
                            "Carry out the engineering instruction and submit the structured Tender proposal.",
                            timeout=timeout,
                        ))
                        stopping = asyncio.create_task(stop.wait())
                        try:
                            done, _ = await asyncio.wait({generation, stopping}, return_when=asyncio.FIRST_COMPLETED)
                            if failure:
                                await session.abort()
                                raise failure
                            await generation
                        except BaseException:
                            bridge.closed = True
                            try:
                                await asyncio.wait_for(session.abort(), 3)
                            except Exception:
                                pass
                            raise
                        finally:
                            stopping.cancel()
                            if not generation.done():
                                generation.cancel()
                            await asyncio.gather(generation, stopping, return_exceptions=True)
                    if failure:
                        raise failure
                    usage["usage_complete"] = usage["requests"] > 0 and not usage.get("missing_usage", False)
                    return {"output": await bridge.result(), "usage": usage, "web_sources": []}
    finally:
        if reservation is not None:
            await call_hook(on_response, usage, reservation)
