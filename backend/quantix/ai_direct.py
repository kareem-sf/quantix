"""Public facade for Quantix's bundled direct API runtime."""

from __future__ import annotations

import asyncio
import hashlib
import importlib
import json
import secrets
import time
from importlib.metadata import PackageNotFoundError, version

from pydantic import BaseModel, ConfigDict

from .ai_api_catalog import discover_models
from .ai_api_engine import CHECK_DEADLINE_SECONDS, run_model
from .ai_api_errors import (
    DirectAPIError,
    DirectDependencyError,
    DirectProviderError,
    dependency_detail,
    preserve_control_failure,
    provider_failure,
)
from .diagnostics import record, record_exception

DIRECT_ADAPTER_REVISION = "direct-api-v1"
EXPECTED_DEPENDENCIES = {
    "pydantic-ai-slim": ("pydantic_ai", "2.40.0"),
    "openai": ("openai", "3.8.0"),
    "anthropic": ("anthropic", "1.4.0"),
    "google-genai": ("google.genai", "2.22.0"),
    "httpx2": ("httpx2", "2.12.0"),
}
DIRECT_PROFILES = {
    "openai": {"openai_responses", "openai_chat"},
    "anthropic": {"anthropic"},
    "google": {"google"},
    "xai": {"openai_responses", "openai_chat"},
    "custom": {"openai_chat", "openai_responses"},
}


def supports_direct(connection: dict) -> bool:
    """Return whether a saved connection is one of the five direct profiles."""

    provider = connection.get("provider_id")
    protocol = connection.get("protocol")
    if provider not in DIRECT_PROFILES or protocol not in DIRECT_PROFILES[provider]:
        return False
    if connection.get("auth_type") not in {"api_key", "environment"}:
        return False
    billing = connection.get("billing")
    if provider == "custom":
        return billing in {"unknown", "metered"}
    return billing == "metered"


def _dependency_state_for(connection: dict | None) -> tuple[dict[str, str | None], list[str]]:
    if connection is None:
        names = list(EXPECTED_DEPENDENCIES)
    elif connection.get("provider_id") in {"openai", "xai", "custom"}:
        names = ["pydantic-ai-slim", "openai", "httpx2"]
    elif connection.get("provider_id") == "anthropic":
        names = ["pydantic-ai-slim", "anthropic", "httpx2"]
    elif connection.get("provider_id") == "google":
        names = ["pydantic-ai-slim", "google-genai", "httpx2"]
    else:
        names = []
    versions: dict[str, str | None] = {}
    missing: list[str] = []
    for distribution in names:
        module, expected = EXPECTED_DEPENDENCIES[distribution]
        try:
            installed = version(distribution)
            importlib.import_module(module)
        except (PackageNotFoundError, ImportError, ModuleNotFoundError):
            versions[distribution] = None
            missing.append(distribution)
            continue
        versions[distribution] = installed
        if installed != expected:
            missing.append(f"{distribution} {expected}")
    return versions, missing


def direct_runtime_status(connection: dict) -> dict:
    """Describe the installed bundled adapter, independently of client workers."""

    versions, missing = _dependency_state_for(connection)
    supported = supports_direct(connection)
    if not supported:
        state = "attention"
        detail = "This connection is not eligible for the bundled direct API runtime."
    elif missing:
        state = "missing"
        detail = dependency_detail(missing)
    else:
        state = "ready"
        detail = "The bundled direct API adapter is installed and ready for an explicit connection check."
    material = json.dumps(
        {"adapter": DIRECT_ADAPTER_REVISION, "dependencies": versions},
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")
    revision = f"{DIRECT_ADAPTER_REVISION}+{hashlib.sha256(material).hexdigest()[:16]}"
    return {
        "component_id": "direct-api",
        "state": state,
        "progress": None,
        "detail": detail,
        "version": revision,
    }


def _ensure_connection(connection: dict) -> None:
    if not supports_direct(connection):
        raise DirectAPIError(
            "Choose an API-key direct connection with metered billing, or a custom connection with unknown or metered billing."
        )
    if not connection.get("enabled", True):
        raise DirectAPIError("This AI connection is disabled.")
    status = direct_runtime_status(connection)
    if status["state"] != "ready":
        raise DirectDependencyError(status["detail"])


class _ConnectionCheckOutput(BaseModel):
    model_config = ConfigDict(extra="forbid")
    value: str


class _ConnectionCheckTool:
    name = "quantix_connection_check"
    description = "Read the short check value and return it in the structured reply. This tool has no Tender data."
    parameters = {"type": "object", "properties": {}, "additionalProperties": False}
    read_only = True
    idempotent = False
    requires_invocation_id = False

    def __init__(self):
        self.value = secrets.token_urlsafe(18)
        self.calls = 0

    async def invoke(self, _context, arguments, *, invocation_id=None):
        if arguments:
            raise ValueError("The connection check tool accepts no arguments.")
        self.calls += 1
        if self.calls > 1:
            raise ValueError("The connection check tool may be called exactly once.")
        return {"value": self.value}


def _check_reasoning(route: dict, connection: dict) -> str | None:
    """Choose only a capability-recorded low check effort; never invent one."""

    if route.get("reasoning") is not None:
        return route.get("reasoning")
    options = (connection.get("_model") or {}).get("capabilities", {}).get("reasoning")
    if not isinstance(options, list):
        return None
    order = {
        "openai_chat": ("none", "minimal", "low"),
        "openai_responses": ("none", "minimal", "low"),
        "google": ("minimal", "low"),
        "anthropic": ("disabled", "low"),
    }.get(connection.get("protocol"), ("none", "minimal", "low"))
    return next((value for value in order if value in options), None)


class DirectAPIService:
    """Request-scoped direct SDK service used by setup and Office dispatch."""

    def __init__(self, repo):
        self.repo = repo
        self.closed = False
        self._active: set[asyncio.Task] = set()

    async def _run(self, operation):
        if self.closed:
            raise DirectAPIError("The bundled direct API service is closing.")
        task = asyncio.current_task()
        if task is not None:
            self._active.add(task)
        try:
            return await operation()
        finally:
            if task is not None:
                self._active.discard(task)

    async def catalog(self, connection, credentials):
        _ensure_connection(connection)
        started = time.monotonic()
        fields = {
            "phase": "catalog",
            "outcome": "started",
            "protocol": connection.get("protocol"),
            "connection_id": connection.get("id"),
        }
        record("direct_api_operation", **fields)
        try:
            result = await self._run(lambda: discover_models(connection, credentials))
        except asyncio.CancelledError:
            record(
                "direct_api_operation",
                phase="catalog",
                outcome="cancelled",
                protocol=connection.get("protocol"),
                connection_id=connection.get("id"),
                duration_ms=int((time.monotonic() - started) * 1000),
            )
            raise
        except BaseException as error:
            record_exception(
                "direct_api_operation_failed",
                error,
                phase="catalog",
                protocol=connection.get("protocol"),
                connection_id=connection.get("id"),
                duration_ms=int((time.monotonic() - started) * 1000),
            )
            raise
        record(
            "direct_api_operation",
            phase="catalog",
            outcome="completed",
            protocol=connection.get("protocol"),
            connection_id=connection.get("id"),
            duration_ms=int((time.monotonic() - started) * 1000),
            requests=1,
        )
        return result

    async def check(self, route, connection, credentials, *, before_request=None, on_response=None):
        _ensure_connection(connection)
        selected = {
            **route,
            "max_output_tokens": min(1024, int(route.get("max_output_tokens", 1024))),
            "web_search": False,
            "max_search_calls": 3,
            "output_mode": "auto",
        }
        bounded = dict(connection)
        bounded["_execution_limits"] = {
            "max_requests": 2,
            "max_output_tokens": 1024,
            "context_window": (connection.get("_model") or {}).get("capabilities", {}).get("context_window"),
        }
        bounded["_model"] = connection.get("_model") or {"model_id": selected.get("model_id"), "capabilities": {}}
        selected["reasoning"] = _check_reasoning(selected, bounded)
        check_tool = _ConnectionCheckTool()
        instruction = (
            "Perform this connection check only. Call quantix_connection_check exactly once, then return its value unchanged "
            "in a structured object with the single property value. Do not use any other tools, Tender data or web access."
        )

        async def operation():
            try:
                async with asyncio.timeout(CHECK_DEADLINE_SECONDS):
                    result = await run_model(
                        selected,
                        bounded,
                        credentials,
                        None,
                        instruction,
                        _ConnectionCheckOutput,
                        before_request=before_request,
                        on_response=on_response,
                        definitions=[check_tool],
                        operation="check",
                    )
            except asyncio.CancelledError:
                raise
            except (DirectAPIError, DirectProviderError, InterruptedError):
                raise
            except Exception as error:
                preserved = preserve_control_failure(error)
                if preserved is not None:
                    raise preserved
                raise provider_failure(error) from None
            value = result["output"].value if hasattr(result["output"], "value") else None
            return {
                "usage": result["usage"],
                "actual_model": result["usage"].get("actual_model"),
                "tools_supported": check_tool.calls == 1,
                "output_supported": check_tool.calls == 1 and value == check_tool.value,
                "checked_count": check_tool.calls,
            }

        return await self._run(operation)

    async def execute(self, route, connection, credentials, context, instruction, output_type,
                      *, consult=None, before_request=None, on_response=None,
                      definitions=None, operation_name="execute", validate_output=None):
        _ensure_connection(connection)
        if not isinstance(instruction, str) or not instruction.strip():
            raise DirectAPIError("The direct API instruction is empty.")
        bounded = dict(connection)
        bounded.setdefault("_execution_limits", {
            "max_requests": int(route.get("max_requests", 12)),
            "max_output_tokens": int(route.get("max_output_tokens", 8192)),
        })
        bounded.setdefault("_model", {"model_id": route.get("model_id"), "capabilities": {}})

        async def operation():
            return await run_model(
                route,
                bounded,
                credentials,
                context,
                instruction,
                output_type,
                consult=consult,
                before_request=before_request,
                on_response=on_response,
                definitions=definitions,
                operation=operation_name,
                validate_output=validate_output,
            )

        return await self._run(operation)

    async def close(self):
        self.closed = True
        current = asyncio.current_task()
        pending = [task for task in self._active if task is not current]
        for task in pending:
            task.cancel()
        if pending:
            await asyncio.gather(*pending, return_exceptions=True)
