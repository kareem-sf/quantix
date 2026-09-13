"""Drive the unmodified Gemini CLI through its documented headless interface."""

import asyncio
import json
import os
import sys
from pathlib import Path
from uuid import uuid4

from .common import (
    RuntimeUnavailable,
    call_hook,
    child_environment,
    execution_limits,
    explicit_executable,
    reserve_runtime,
    runtime_usage,
    stop_process,
)
from .remote import SUBMIT_TOOL

GEMINI_VERSION = "0.58.0"


def gemini_command(home, connection) -> list[str]:
    entry = explicit_executable(connection)
    if entry is not None:
        if entry.suffix.lower() not in {".js", ".mjs", ".cjs"}:
            raise RuntimeUnavailable("Select the supported official Gemini npm JavaScript entry point, or let Quantix install its pinned package.")
        supported = False
        for directory in list(entry.parents)[:4]:
            manifest = directory / "package.json"
            if manifest.is_file():
                metadata = json.loads(manifest.read_text(encoding="utf-8"))
                if metadata.get("name") == "@google/gemini-cli":
                    published = metadata.get("bin", {}).get("gemini")
                    supported = (metadata.get("version") == GEMINI_VERSION and isinstance(published, str)
                                 and (directory / published).resolve() == entry.resolve())
                    break
        if not supported:
            raise RuntimeUnavailable("The selected Gemini entry point does not match the supported official package manifest. Use Install client.")
    if entry is None:
        from .runtime_setup import component_root
        package = component_root() / "client" / "node_modules" / "@google" / "gemini-cli"
        manifest = package / "package.json"
        if not manifest.is_file():
            raise RuntimeUnavailable("Install the official Gemini CLI for this connection first.")
        metadata = json.loads(manifest.read_text(encoding="utf-8"))
        if metadata.get("name") != "@google/gemini-cli" or metadata.get("version") != GEMINI_VERSION:
            raise RuntimeUnavailable("Install the Gemini CLI version supported by this Quantix adapter.")
        raw = metadata.get("bin", {}).get("gemini")
        if not isinstance(raw, str):
            raise RuntimeUnavailable("The Gemini CLI package has no published executable entry point.")
        entry = (package / raw).resolve()
        if not entry.is_relative_to(package.resolve()) or not entry.is_file():
            raise RuntimeUnavailable("The Gemini CLI executable is missing from its package.")
    if entry.suffix.lower() in {".js", ".mjs", ".cjs"}:
        from .runtime_setup import node_executable
        node = node_executable(home, connection)
        if node is None:
            raise RuntimeUnavailable("Use Install client to install the supported Node and Gemini runtimes.")
        return [str(node), str(entry)]
    return [str(entry)]


def _require_local_policy_authority():
    if os.name == "nt":
        root = Path(os.environ.get("PROGRAMDATA", "C:/ProgramData")) / "gemini-cli"
    elif sys.platform == "darwin":
        root = Path("/Library/Application Support/GeminiCli")
    else:
        root = Path("/etc/gemini-cli")
    if (root / "settings.json").exists() or any((root / "policies").glob("*.toml")):
        raise RuntimeUnavailable("Gemini CLI has centrally managed configuration. An administrator must provide an approved MCP-only integration; Quantix will not override it.")


async def execute_gemini(route, connection, credentials, context, instruction, output_type,
                         *, consult=None, before_request=None, on_response=None):
    if connection.get("billing") != "subscription" or connection["auth_type"] != "client_login":
        raise RuntimeUnavailable("Use a Gemini API connection for API-key or metered work. This adapter runs the original signed-in Gemini client only.")
    if route.get("web_search"):
        raise RuntimeUnavailable("This Gemini client exposes Tender tools only. Choose an approved API connection for web research.")
    if route.get("reasoning"):
        raise RuntimeUnavailable("This Gemini client adapter does not expose a checked reasoning-level mapping. Clear that setting or use the Gemini API connection.")
    _require_local_policy_authority()
    home = context.account_home
    command = gemini_command(home, connection)
    requests, _, timeout = execution_limits(connection, route)
    env = child_environment(home)
    env["GEMINI_CLI_HOME"] = str(home)
    env["GEMINI_CLI_SURFACE"] = "quantix"
    work = home / "work" / context.operation_id / uuid4().hex
    work.mkdir(parents=True, exist_ok=True)
    policy = work / "tender-tools.toml"
    policy.write_text('[[rule]]\ntoolName = "*"\ndecision = "deny"\npriority = 998\n\n[[rule]]\nmcpName = "quantix"\ndecision = "allow"\npriority = 999\n', encoding="utf-8")
    settings_file = work / "client-settings.json"
    bridge = context.bridge
    usage = runtime_usage(connection, route)
    reservation = None
    process = None
    stderr_task = None
    final = None
    output_bytes = 0

    async def discard_stderr(stream):
        # Original clients may emit paths and credentials in diagnostics. Never
        # persist or return their raw output from the application API.
        while await stream.read(8192):
            pass

    try:
        async with bridge.serve():
            env["QUANTIX_MCP_TOKEN"] = bridge.token
            env["GEMINI_CLI_SYSTEM_SETTINGS_PATH"] = str(settings_file)
            settings_file.write_text(json.dumps({
                "general": {"enableAutoUpdate": False, "enableAutoUpdateNotification": False},
                "model": {"name": route["model_id"], "maxSessionTurns": requests},
                "tools": {"core": []}, "telemetry": {"enabled": False, "logPrompts": False},
                "security": {"disableYoloMode": True, "auth": {"selectedType": "oauth-personal"}},
                "admin": {"secureModeEnabled": True, "extensions": {"enabled": False}, "skills": {"enabled": False}},
                "mcp": {"allowed": ["quantix"]},
                "mcpServers": {"quantix": {"httpUrl": bridge.url,
                    "headers": {"Authorization": "Bearer ${QUANTIX_MCP_TOKEN}"},
                    "includeTools": bridge.names}},
                "context": {"fileName": [], "includeDirectories": []},
                "hooksConfig": {"enabled": False}, "experimental": {"autoMemory": False},
            }), encoding="utf-8")
            reservation, _, _ = await reserve_runtime(connection, route, instruction, before_request)
            async with asyncio.timeout(timeout):
                process = await asyncio.create_subprocess_exec(
                    *command, "--model", route["model_id"], "--output-format", "stream-json",
                    "--admin-policy", str(policy), "--allowed-mcp-server-names", "quantix",
                    "--prompt", f"Carry out the supplied engineering instruction. Use only the quantix MCP tools and submit the complete proposal with {SUBMIT_TOOL}.",
                    cwd=work, env=env, stdin=asyncio.subprocess.PIPE,
                    stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE,
                    limit=64 * 1024 * 1024,
                    **({"creationflags": 0x08000000} if os.name == "nt" else {}),
                )
                stderr_task = asyncio.create_task(discard_stderr(process.stderr))
                process.stdin.write(instruction.encode("utf-8"))
                await process.stdin.drain()
                process.stdin.close()
                while line := await process.stdout.readline():
                    output_bytes += len(line)
                    if output_bytes > 256 * 1024 * 1024:
                        raise RuntimeUnavailable("Gemini CLI exceeded the bounded event output for this run.")
                    event = json.loads(line)
                    if not isinstance(event, dict) or not isinstance(event.get("type"), str):
                        raise RuntimeUnavailable("Gemini CLI emitted an invalid structured event.")
                    if event["type"] == "init":
                        actual = event.get("model")
                        usage["actual_model"] = actual
                        usage["session_id"] = event.get("session_id")
                        if actual and actual != route["model_id"]:
                            raise RuntimeUnavailable("Gemini CLI selected another model. The Tender result was withheld.")
                    if event["type"] == "result":
                        final = event
                await process.wait()
                if process.returncode != 0 or final is None or final.get("status") != "success":
                    raise RuntimeUnavailable("The original Gemini client did not complete this request. Review its sign-in, model access and limits.")
                reported_models = (final.get("stats") or {}).get("models") or {}
                if any(model != route["model_id"] for model in reported_models):
                    raise RuntimeUnavailable("Gemini CLI reported use of another model. The result was withheld because no fallback was approved for this client session.")
                # Keep unknown inference accounting visible. These stats remain
                # available for display without inventing a token-field mapping.
                usage["detail"] = "Gemini CLI completed; per-inference billing was not reconciled by this adapter."
                return {"output": await bridge.result(), "usage": usage, "web_sources": []}
    finally:
        bridge.closed = True
        await stop_process(process)
        if stderr_task:
            stderr_task.cancel()
            await asyncio.gather(stderr_task, return_exceptions=True)
        if reservation is not None:
            await call_hook(on_response, usage, reservation)
        policy.unlink(missing_ok=True)
        settings_file.unlink(missing_ok=True)
