"""Standard MCP stdio operations for one immutable connection worker."""

import asyncio
import json
import logging
import os
import re
from pathlib import Path

import mcp.types as types
from mcp.server import Server
from mcp.server.stdio import stdio_server

from .common import RuntimeConnectionFailure, RuntimeUnavailable
from .grok_common import GROK_CHECK_MAX_ROUNDS

try:
    from .diagnostics import initialize as initialize_diagnostics
except ImportError:  # Source-only diagnostics probes before component preparation.
    from quantix.diagnostics import initialize as initialize_diagnostics


RUNTIMES = {"codex", "grok_build"}


def _nested_known_failure(error):
    """Recover only already-sanitized failures from an execution task group."""
    if isinstance(error, (RuntimeUnavailable, RuntimeConnectionFailure, InterruptedError)):
        return error
    if isinstance(error, BaseExceptionGroup):
        for child in error.exceptions:
            failure = _nested_known_failure(child)
            if failure is not None:
                return failure
    return None


class Worker:
    def __init__(self):
        self.connection = None
        self.account_home = None
        self.accounts = None
        self.diagnostics = initialize_diagnostics(
            os.environ.get("QUANTIX_DIAGNOSTICS_DIRECTORY"),
            component="worker",
            session_id=os.environ.get("QUANTIX_DIAGNOSTICS_SESSION_ID"),
            require_directory=True,
        )
        self.server = Server("quantix-ai-worker", version="1.0.0",
                             on_list_tools=self.list_tools, on_call_tool=self.call_tool)

    async def list_tools(self, _ctx, _params):
        schemas = {
            "initialize_connection": {"connection": {"type": "object"}, "account_home": {"type": "string"}},
            "catalog": {"credentials": {"type": "object"}},
            "runtime_status": {}, "login": {}, "login_device": {}, "logout": {}, "billing": {},
            "execute": {"credentials": {"type": "object"}, "execution": {"type": "object"}},
            "conversation": {"credentials": {"type": "object"}, "execution": {"type": "object"}},
            "check": {"credentials": {"type": "object"}, "execution": {"type": "object"}},
        }
        return types.ListToolsResult(tools=[types.Tool(
            name=name, description="Quantix host operation; never supplied to a model.",
            inputSchema={"type": "object", "properties": properties,
                         "required": list(properties), "additionalProperties": False},
        ) for name, properties in schemas.items()])

    async def operation(self, name, arguments):
        if name == "initialize_connection":
            if self.connection is not None:
                raise RuntimeUnavailable("This worker already has its immutable connection.")
            connection = arguments["connection"]
            identifier = connection.get("id", "")
            home = Path(arguments["account_home"])
            if (not re.fullmatch(r"[A-Za-z0-9_-]{1,100}", identifier)
                    or not home.is_absolute() or home.name != identifier or home.parent.name != "ai-runtimes"):
                raise RuntimeUnavailable("The worker received an invalid private account directory.")
            if connection.get("protocol") not in RUNTIMES:
                raise RuntimeUnavailable("This worker supports only the Codex and Grok subscription clients.")
            from .accounts import AccountService

            self.connection = connection
            self.account_home = home
            self.accounts = AccountService(connection, home)
            return {"initialized": True, "protocol": connection["protocol"]}
        if self.connection is None:
            raise RuntimeUnavailable("Initialize this worker's connection before starting an operation.")
        if name == "catalog":
            from .runtime_catalog import discover_runtime_models
            return await discover_runtime_models(self.account_home, self.connection, arguments["credentials"])
        if name in {"runtime_status", "login", "login_device", "logout"}:
            method = self.accounts.status if name == "runtime_status" else getattr(self.accounts, name)
            return await method()
        if name == "billing":
            if self.connection["protocol"] != "grok_build":
                raise RuntimeUnavailable("This connection does not expose subscription usage through its original client.")
            from .grok_auth import subscription_usage
            return await subscription_usage(self.account_home, self.connection)
        if name in {"execute", "check", "conversation"}:
            from .execution import execute_runtime
            from .remote import execution_context

            execution = arguments["execution"]
            if execution.get("account_home") != str(self.account_home):
                raise RuntimeUnavailable("The execution account directory does not match its initialized worker.")
            if not re.fullmatch(r"[A-Za-z0-9_-]{1,100}", execution.get("operation_id", "")):
                raise RuntimeUnavailable("The worker operation identifier is invalid.")
            connection = dict(self.connection)
            connection["_execution_limits"] = execution["limits"]
            connection["_model"] = execution["model"]
            connection["_operation"] = name
            if self.connection["protocol"] == "grok_build":
                connection["_grok_check_maximum_cost_usd"] = execution.get("grok_check_maximum_cost_usd")
            # Keep this worker boundary aligned with the core Grok check contract.
            check_requests = GROK_CHECK_MAX_ROUNDS if self.connection["protocol"] == "grok_build" else 3
            if name == "check" and (execution["limits"].get("max_requests", 0) > check_requests
                                     or execution["route"].get("max_output_tokens", 0) > 1024
                                     or execution["route"].get("web_search")):
                raise RuntimeUnavailable("The connection check exceeds its small approved request limits.")
            async with execution_context(execution) as context:
                return await execute_runtime(execution["route"], connection, arguments["credentials"], context,
                                     execution["instruction"], execution["output_schema"])
        raise RuntimeUnavailable("This worker operation is unsupported.")

    async def call_tool(self, _ctx, params):
        started = asyncio.get_running_loop().time()
        arguments = params.arguments or {}
        execution = arguments.get("execution") if isinstance(arguments, dict) else None
        operation_id = execution.get("operation_id") if isinstance(execution, dict) else None
        self.diagnostics.record("worker_operation", phase=params.name, outcome="started",
                                operation_id=operation_id,
                                protocol=(self.connection or {}).get("protocol"))
        try:
            result = await self.operation(params.name, params.arguments or {})
            body = {"ok": True, "result": result}
            self.diagnostics.record("worker_operation", phase=params.name, outcome="completed",
                                    operation_id=operation_id,
                                    protocol=(self.connection or {}).get("protocol"),
                                    duration_ms=int((asyncio.get_running_loop().time() - started) * 1000))
            return types.CallToolResult(content=[types.TextContent(type="text", text=json.dumps(body, ensure_ascii=False))], structuredContent=body)
        except asyncio.CancelledError:
            raise
        except Exception as error:
            # The execution context may report a sanitized provider/runtime
            # failure inside an AnyIO task group while its MCP transports close.
            # Preserve that known message; unknown exceptions remain generic.
            failure = _nested_known_failure(error)
            if failure is not None:
                error = failure
            kind, message = "unavailable", "The selected AI component could not complete this operation. Review its connection, model and limits."
            if isinstance(error, RuntimeUnavailable):
                message = str(error)
            elif isinstance(error, InterruptedError):
                kind, message = "interrupted", "The AI operation ended before completion. No result was published."
            elif isinstance(error, (RuntimeConnectionFailure, ConnectionError, TimeoutError)):
                kind, message = "connection", "The selected provider connection was interrupted or unavailable."
            self.diagnostics.record_exception(
                "worker_operation_failed", error, phase=params.name, operation_id=operation_id,
                protocol=(self.connection or {}).get("protocol"),
                duration_ms=int((asyncio.get_running_loop().time() - started) * 1000),
            )
            body = {"ok": False, "error": {"kind": kind, "message": message[:1200]}}
            return types.CallToolResult(isError=True, content=[types.TextContent(type="text", text=json.dumps(body))], structuredContent=body)

    async def run(self):
        self.diagnostics.record("worker_process", phase="startup", outcome="starting")
        try:
            async with stdio_server() as (reader, writer):
                await self.server.run(reader, writer, self.server.create_initialization_options())
        except asyncio.CancelledError:
            self.diagnostics.record("worker_process", phase="shutdown", outcome="cancelled")
            raise
        except Exception as error:
            self.diagnostics.record_exception("worker_process_failed", error, phase="runtime")
            raise
        finally:
            if self.accounts:
                await self.accounts.close()
            self.diagnostics.record("worker_process", phase="shutdown", outcome="completed")
            self.diagnostics.close()


def main():
    # SDK diagnostics can contain private prompts, URLs or credentials. The MCP
    # reply is sanitized explicitly; no raw SDK logs are relayed to the core.
    logging.disable(logging.CRITICAL)
    asyncio.run(Worker().run())
