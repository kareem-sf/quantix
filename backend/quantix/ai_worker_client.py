"""SDK-free host client for managed, isolated provider workers over MCP stdio."""

import asyncio
import json
import os
import secrets
import time
from contextlib import AsyncExitStack, asynccontextmanager
from pathlib import Path
from uuid import uuid4

from mcp.client import Client
from mcp.client.stdio import StdioServerParameters, stdio_client
from pydantic import BaseModel, ConfigDict

from .ai_runtime_common import (
    RuntimeConnectionFailure,
    RuntimeUnavailable,
    child_environment,
    runtime_home,
)
from .ai_runtime_mcp import RuntimeToolBridge, WorkerControlBridge, project_run_activity
from .diagnostics import initialize, record, record_exception
from .storage import desktop_browser_context, logs_dir

CHECK_DEADLINE_SECONDS = 180
# Mirrored by the setup preview and managed Grok worker check validation.
GROK_CHECK_MAX_ROUNDS = 5
# Codex exposes one SDK turn for the check. The SDK's internal inference and
# output accounting is not a public request/token limit, so this is an
# internal worker turn bound only; callers must not present it as a provider
# allowance or an API-style 2-request/1024-token check.
CODEX_CHECK_MAX_TURNS = 1


class ModelWorkerFailure(ConnectionError):
    """Sanitized provider/transport failure eligible only for an approved fallback."""


def _nested_known_failure(error):
    """Recover a sanitized worker failure hidden inside an MCP task group."""
    if isinstance(error, (RuntimeUnavailable, ModelWorkerFailure, InterruptedError)):
        return error
    if isinstance(error, RuntimeConnectionFailure):
        return error
    if isinstance(error, BaseExceptionGroup):
        for child in error.exceptions:
            failure = _nested_known_failure(child)
            if failure is not None:
                return failure
    return None


def _result(reply):
    body = reply.structured_content
    if not isinstance(body, dict) or type(body.get("ok")) is not bool:
        raise ModelWorkerFailure("The AI component returned an invalid control response.")
    if not body["ok"]:
        error = body.get("error") or {}
        message = str(error.get("message") or "The AI component could not complete the operation.")[
            :1200
        ]
        kind = error.get("kind")
        if kind == "interrupted":
            raise InterruptedError(message)
        if kind == "connection":
            raise ModelWorkerFailure(message)
        raise RuntimeUnavailable(message)
    if reply.is_error:
        raise ModelWorkerFailure("The AI component returned a contradictory control response.")
    return body["result"]


class _ConnectionCheckOutput(BaseModel):
    model_config = ConfigDict(extra="forbid")
    value: str


class _ConnectionCheckTool:
    name = "quantix_connection_check"
    description = "Callable connection-check function. Call with an empty argument object to receive a short token in the value property. Return that token as instructed by this connection's check workflow. No Tender data is available."
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
        return {"value": self.value}


def returned_check_value(reported, expected) -> bool:
    """Accept the check token itself, or the tool's own object around it.

    The check tool answers with ``{"value": "<token>"}``. A local client that
    has to discover its tools first sometimes forwards that whole object as the
    string. The one-time token still made the round trip intact, which is what
    this check asks; failing the account over the wrapper reported a broken
    connection when nothing was broken.
    """

    if not isinstance(reported, str) or not isinstance(expected, str):
        return False
    if reported == expected:
        return True
    try:
        decoded = json.loads(reported)
    except ValueError:
        return False
    return isinstance(decoded, dict) and decoded.get("value") == expected


def connection_check_instruction(protocol):
    """Keep each provider's generic check contract explicit and bounded."""
    if protocol == "grok_build":
        return (
            "Perform this connection check only. Discover both Quantix tools together, "
            "then call quantix_connection_check exactly once. Submit its returned value "
            "unchanged as the structured object with the single property value using "
            "quantix_submit_result, then finish without further tools. "
            "Do not use any other account capabilities or web access."
        )
    if protocol == "codex":
        # "Its returned value unchanged" read as the whole returned object, and
        # a client passed {"value": "…"} through as the string, so a working
        # account failed its own check. Name the two shapes apart instead.
        return (
            "Perform this connection check only. Call quantix_connection_check exactly once. "
            "It answers with an object holding one property, value, whose content is a short "
            "text token. Then call quantix_submit_result exactly once, passing that token "
            "itself as its value property: the token string alone, not the object around it "
            "and not a JSON rendering of it. Both tools are provided by the quantix MCP "
            "server as callable functions, not MCP resources. If their schemas are deferred, "
            "Codex's catalogue-only tool search by name is permitted to discover both tools first. "
            "Only these two Quantix tools may be executed. Never call read_mcp_resource or invent "
            "resource URIs for tools. Finish after submission; do not inspect Tender data, local "
            "files or other account capabilities, and do not browse the web."
        )
    return (
        "Perform this connection check only. Call quantix_connection_check exactly once, "
        "then return its value unchanged in a structured object with the single property value. "
        "Do not use any other account capabilities, tools or web access."
    )


class _AccountWorker:
    """An owner task keeps MCP/AnyIO entry and cleanup in the same task."""

    def __init__(self, owner, connection):
        self.owner = owner
        self.connection = connection
        self.queue = asyncio.Queue()
        self.task = asyncio.create_task(self.run())

    async def run(self):
        current = None
        failure = None
        try:
            async with self.owner._session(self.connection) as client:
                while True:
                    name, future = await self.queue.get()
                    current = future
                    if future.cancelled():
                        continue
                    try:
                        value = _result(await client.call_tool(name, {}))
                        if not future.done():
                            future.set_result(value)
                    except asyncio.CancelledError:
                        raise
                    except Exception as error:
                        if not future.done():
                            future.set_exception(error)
                    current = None
        except asyncio.CancelledError:
            if current is not None and not current.done():
                current.cancel()
            raise
        except Exception as error:
            failure = error
            if current is not None and not current.done():
                current.set_exception(error)
        finally:
            while not self.queue.empty():
                _, future = self.queue.get_nowait()
                if not future.done():
                    future.set_exception(
                        failure or ModelWorkerFailure("The account component is no longer running.")
                    )

    async def call(self, name):
        if self.task.done():
            raise ModelWorkerFailure(
                "The account component is no longer running. Prepare the connection and retry."
            )
        future = asyncio.get_running_loop().create_future()
        self.queue.put_nowait((name, future))
        try:
            return await future
        except asyncio.CancelledError:
            await self.close()
            raise

    async def close(self):
        self.task.cancel()
        await asyncio.gather(self.task, return_exceptions=True)


class AIWorkerClient:
    """One repository-owned coordinator; no constructor starts a process."""

    def __new__(cls, repo):
        existing = getattr(repo, "_ai_worker_client", None)
        if existing is not None and not existing.closed:
            return existing
        instance = super().__new__(cls)
        repo._ai_worker_client = instance
        return instance

    def __init__(self, repo):
        if getattr(self, "_initialized", False):
            return
        self._initialized = True
        self.repo = repo
        self.accounts = {}
        self.active = set()
        self.executions = set()
        self.runtime_locks = {}
        self.closed = False

    def _environment(self, connection, command):
        from .ai_components import AIComponentService

        home = runtime_home(self.repo, connection)
        env = child_environment(home)
        env.update(AIComponentService(self.repo).environment(connection, command))
        diagnostics = initialize(logs_dir(self.repo.home))
        # The worker's isolated HOME is deliberately separate from this
        # explicit diagnostic destination.
        env["QUANTIX_DIAGNOSTICS_DIRECTORY"] = str(logs_dir(self.repo.home))
        env["QUANTIX_DIAGNOSTICS_SESSION_ID"] = diagnostics.session_id
        env["QUANTIX_AI_HOST_BINARY"] = command[0]
        # Original clients sometimes open the browser themselves. Carry only
        # desktop profile locations, separately from the isolated worker HOME.
        # Workers may apply these only to an explicit sign-in subprocess.
        browser_context = desktop_browser_context()
        browser_context["HOME"] = browser_context["HOME"] or str(Path.home())
        env["QUANTIX_DESKTOP_BROWSER_CONTEXT"] = json.dumps(browser_context)
        # Native account windows require the current desktop session. These are
        # OS display coordinates, never provider keys or model routing options.
        for key in (
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "DBUS_SESSION_BUS_ADDRESS",
            "XDG_RUNTIME_DIR",
            "XAUTHORITY",
            "USER",
            "LOGNAME",
        ):
            if os.environ.get(key):
                env[key] = os.environ[key]
        # stdio_client supplies a constrained default environment and merges
        # this mapping over it. Do not pass the deliberately blanked values
        # from child_environment: SDKs that merge their own config with
        # os.environ would otherwise see internal optional flags as present
        # but empty (Codex then rejects its initialize metadata). Omitting a
        # value keeps it absent while retaining the explicit private HOME and
        # connection settings above.
        return {key: value for key, value in env.items() if value != ""}

    @asynccontextmanager
    async def _session(self, connection):
        from .ai_components import AIComponentService, ComponentUnavailable

        if self.closed:
            raise RuntimeUnavailable("The AI worker service is closing.")
        task = asyncio.current_task()
        started = time.monotonic()
        record(
            "ai_worker_session",
            phase="worker_session",
            outcome="starting",
            protocol=connection.get("protocol"),
            connection_id=connection.get("id"),
        )
        self.active.add(task)
        try:
            async with AIComponentService(self.repo).async_lease(connection) as command:
                expected = connection.get("_checked_component_version")
                if expected and Path(command[-1]).parent.parent.name != expected:
                    raise RuntimeUnavailable(
                        "This AI component changed after the connection check. Check the connection again before starting Tender work."
                    )
                home = runtime_home(self.repo, connection)
                env = self._environment(connection, command)
                params = StdioServerParameters(
                    command=command[0], args=command[1:], env=env, cwd=home
                )
                # No provider diagnostic streams reach the application log.
                with open(os.devnull, "w", encoding="utf-8") as diagnostics:
                    async with AsyncExitStack() as stack:
                        async with asyncio.timeout(45):
                            client = await stack.enter_async_context(
                                Client(
                                    stdio_client(params, errlog=diagnostics),
                                    cache=None,
                                    read_timeout_seconds=1900,
                                )
                            )
                            _result(
                                await client.call_tool(
                                    "initialize_connection",
                                    {
                                        "connection": {
                                            key: value
                                            for key, value in connection.items()
                                            if not key.startswith("_")
                                        },
                                        "account_home": str(home),
                                    },
                                )
                            )
                        yield client
        except ComponentUnavailable as error:
            record_exception(
                "ai_worker_session_failed",
                error,
                phase="worker_session",
                protocol=connection.get("protocol"),
                connection_id=connection.get("id"),
            )
            raise RuntimeUnavailable(str(error)) from None
        except (RuntimeUnavailable, ModelWorkerFailure, InterruptedError) as error:
            record_exception(
                "ai_worker_session_failed",
                error,
                phase="worker_session",
                protocol=connection.get("protocol"),
                connection_id=connection.get("id"),
            )
            raise
        except asyncio.CancelledError:
            record(
                "ai_worker_session",
                phase="worker_session",
                outcome="cancelled",
                protocol=connection.get("protocol"),
                connection_id=connection.get("id"),
                duration_ms=int((time.monotonic() - started) * 1000),
            )
            raise
        except Exception as error:
            # AnyIO may wrap a worker's structured failure in one or more
            # ExceptionGroups while the stdio session is closing. Preserve the
            # worker's already-sanitized domain message instead of turning it
            # into the unhelpful generic connection error.
            failure = _nested_known_failure(error)
            if failure is not None:
                record_exception(
                    "ai_worker_session_failed",
                    failure,
                    phase="worker_session",
                    protocol=connection.get("protocol"),
                    connection_id=connection.get("id"),
                )
                raise failure from None
            record_exception(
                "ai_worker_session_failed",
                error,
                phase="worker_session",
                protocol=connection.get("protocol"),
                connection_id=connection.get("id"),
            )
            raise ModelWorkerFailure(
                "The managed AI component could not start or its connection closed unexpectedly."
            ) from None
        finally:
            self.active.discard(task)
            record(
                "ai_worker_session",
                phase="worker_session",
                outcome="closed",
                protocol=connection.get("protocol"),
                connection_id=connection.get("id"),
                duration_ms=int((time.monotonic() - started) * 1000),
            )

    async def catalog(self, connection, credentials):
        async with self._session(connection) as client:
            async with asyncio.timeout(90):
                result = _result(await client.call_tool("catalog", {"credentials": credentials}))
        if not isinstance(result, list) or len(result) > 2000:
            raise RuntimeUnavailable("The AI component returned an invalid model catalog.")
        return result

    async def _account(self, connection, operation):
        worker = self.accounts.get(connection["id"])
        if worker and (
            worker.connection["revision"] != connection["revision"] or worker.task.done()
        ):
            await worker.close()
            worker = None
        if worker is None:
            worker = _AccountWorker(self, dict(connection))
            self.accounts[connection["id"]] = worker
        return await worker.call(operation)

    async def runtime_status(self, connection):
        return await self._account(connection, "runtime_status")

    async def login(self, connection, *, sign_in_method=None):
        if sign_in_method == "device_code":
            if connection["protocol"] not in {"codex", "grok_build"}:
                raise ValueError("This account does not expose this sign-in code option.")
            return await self._account(connection, "login_device")
        return await self._account(connection, "login")

    async def logout(self, connection):
        return await self._account(connection, "logout")

    async def subscription_usage(self, connection):
        from .ai_subscription import subscription_snapshot, unknown_subscription

        if connection["protocol"] != "grok_build":
            raise ValueError("This account does not provide Grok subscription usage.")
        try:
            async with self._session(connection) as client:
                async with asyncio.timeout(90):
                    snapshot = subscription_snapshot(_result(await client.call_tool("billing", {})))
        except (RuntimeUnavailable, ModelWorkerFailure, ValueError, TimeoutError):
            snapshot = unknown_subscription()
        from .ai_connections import AIConnectionService
        from .ai_setup_store import SetupStore

        connections = AIConnectionService(self.repo)
        with connections.authority_guard(), self.repo.atomic():
            # A stale account operation never replaces a newer revision's display.
            if connections.get(connection["id"])["revision"] == connection["revision"]:
                SetupStore(self.repo).update(connection["id"], subscription_usage=snapshot)
        return snapshot

    async def cancel_account(self, connection_id):
        worker = self.accounts.pop(connection_id, None)
        if worker is not None:
            await worker.close()

    def runtime_busy(self, connection_id):
        lock = self.runtime_locks.get(connection_id)
        return bool(lock and lock.locked())

    async def _execute(
        self,
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
        definitions=None,
        operation="execute",
        validate_output=None,
    ):
        task = asyncio.current_task()
        started = time.monotonic()
        record(
            "ai_worker_operation",
            phase="queue",
            outcome="started",
            operation=operation,
            protocol=connection.get("protocol"),
            model=route.get("model_id"),
            run_id=context.run_id if context is not None else None,
        )
        self.executions.add(task)
        try:
            if connection["protocol"] == "grok_build":
                lock = self.runtime_locks.setdefault(connection["id"], asyncio.Lock())
                if lock.locked() and context is not None:
                    self.repo.event(
                        context.run_id,
                        "runtime_waiting",
                        "Waiting for this Grok account's current work to finish.",
                    )
                    project_run_activity(self.repo, context.run_id, "runtime_waiting")
                # Queue before billing metadata as well as inference. Otherwise
                # a long preceding task could exhaust the short metadata timeout.
                async with lock:
                    result = await self._execute_in_slot(
                        route,
                        connection,
                        credentials,
                        context,
                        instruction,
                        output_type,
                        consult=consult,
                        before_request=before_request,
                        on_response=on_response,
                        definitions=definitions,
                        operation=operation,
                        validate_output=validate_output,
                    )
            else:
                result = await self._execute_in_slot(
                    route,
                    connection,
                    credentials,
                    context,
                    instruction,
                    output_type,
                    consult=consult,
                    before_request=before_request,
                    on_response=on_response,
                    definitions=definitions,
                    operation=operation,
                    validate_output=validate_output,
                )
            record(
                "ai_worker_operation",
                phase="queue",
                outcome="completed",
                operation=operation,
                protocol=connection.get("protocol"),
                model=route.get("model_id"),
                run_id=context.run_id if context is not None else None,
                duration_ms=int((time.monotonic() - started) * 1000),
            )
            return result
        except asyncio.CancelledError:
            record(
                "ai_worker_operation",
                phase="queue",
                outcome="cancelled",
                operation=operation,
                protocol=connection.get("protocol"),
                model=route.get("model_id"),
                run_id=context.run_id if context is not None else None,
                duration_ms=int((time.monotonic() - started) * 1000),
            )
            raise
        except Exception as error:
            record_exception(
                "ai_worker_operation_failed",
                error,
                phase="queue",
                operation=operation,
                protocol=connection.get("protocol"),
                model=route.get("model_id"),
                run_id=context.run_id if context is not None else None,
                duration_ms=int((time.monotonic() - started) * 1000),
            )
            raise
        finally:
            self.executions.discard(task)

    async def _execute_in_slot(
        self,
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
        definitions=None,
        operation="execute",
        validate_output=None,
    ):
        if connection["protocol"] == "grok_build":
            from .ai_subscription import require_subscription_access

            snapshot = await self.subscription_usage(connection)
            require_subscription_access(connection, snapshot, checking=operation == "check")
        requests = int(connection.get("_execution_limits", {}).get("max_requests", 12))
        operation_id = context.run_id if context is not None else uuid4().hex
        source = RuntimeToolBridge(
            context,
            output_type,
            consult,
            max_calls=min(1000, requests * 10),
            image_support=connection.get("_model", {}).get("capabilities", {}).get("images")
            is True,
            definitions=definitions,
            operation=operation,
            validate_output=validate_output,
        )
        native_session = None
        native_service = None
        if (
            context is not None
            and operation != "check"
            and connection["protocol"] in {"codex", "grok_build"}
        ):
            from .native_execution import NativeExecutionService

            native_service = NativeExecutionService(self.repo)
            tool_contracts = [
                {
                    "name": tool.name,
                    "parameters": tool.parameters,
                    "read_only": tool.read_only,
                    "idempotent": tool.idempotent,
                }
                for tool in sorted(source.tools.values(), key=lambda tool: tool.name)
            ]
            tool_contracts.append(
                {"name": "quantix_submit_result", "parameters": output_type.model_json_schema()}
            )
            native_session = native_service.prepare(
                context,
                connection,
                route,
                operation=operation,
                tools=tool_contracts,
                resume_session_id=connection.get("_native_resume_session_id"),
            )
            connection = {**connection, "_native_session_binding": native_session.model_dump()}
        control = WorkerControlBridge(
            source, connection, route, before_request=before_request, on_response=on_response
        )
        try:
            async with source.serve(), control.serve():
                async with self._session(connection) as client:
                    result = _result(
                        await client.call_tool(
                            operation,
                            {
                                "credentials": credentials,
                                "execution": {
                                    "route": route,
                                    "limits": connection.get(
                                        "_execution_limits", {"max_requests": requests}
                                    ),
                                    "model": connection.get("_model", {}),
                                    "instruction": instruction,
                                    "operation_id": operation_id,
                                    "session_binding": native_session.model_dump()
                                    if native_session
                                    else None,
                                    "account_home": str(runtime_home(self.repo, connection)),
                                    "output_schema": output_type.model_json_schema(),
                                    "source": {
                                        "url": source.url,
                                        "token": source.token,
                                        "names": source.names,
                                    },
                                    "control": {"url": control.url, "token": control.token},
                                },
                            },
                        )
                    )
                    if source.failure:
                        raise source.failure
                    if (
                        not isinstance(result, dict)
                        or not isinstance(result.get("usage"), dict)
                        or not isinstance(result.get("web_sources"), list)
                    ):
                        raise RuntimeUnavailable(
                            "The AI component returned an invalid execution result."
                        )
                    try:
                        result["output"] = output_type.model_validate(result["output"])
                    except ValueError:
                        raise RuntimeUnavailable(
                            "The AI response did not match the required structured proposal. Its result was not published."
                        ) from None
                    if native_session is not None:
                        native_service.finish(
                            context,
                            native_session.id,
                            provider_session_id=result["usage"].get("session_id"),
                            state="completed",
                        )
                    return result
        except BaseException as error:
            if native_session is not None:
                native_service.finish(
                    context,
                    native_session.id,
                    state="interrupted"
                    if isinstance(error, (asyncio.CancelledError, InterruptedError))
                    else "failed",
                )
            raise

    async def execute(
        self,
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
        definitions=None,
        operation="execute",
        validate_output=None,
    ):
        return await self._execute(
            route,
            connection,
            credentials,
            context,
            instruction,
            output_type,
            consult=consult,
            before_request=before_request,
            on_response=on_response,
            definitions=definitions,
            operation=operation,
            validate_output=validate_output,
        )

    async def check(self, route, connection, credentials, *, before_request=None, on_response=None):
        check_tool = _ConnectionCheckTool()
        bounded = dict(connection)
        # Keep this host boundary aligned with the setup preview and Grok worker.
        subscription_check = connection.get("billing") == "subscription"
        if connection["protocol"] == "grok_build":
            requests = GROK_CHECK_MAX_ROUNDS
        elif connection["protocol"] == "codex" and subscription_check:
            requests = CODEX_CHECK_MAX_TURNS
        else:
            requests = 2
        bounded["_execution_limits"] = {"max_requests": requests, "max_output_tokens": 1024}
        if subscription_check:
            bounded["_execution_limits"]["subscription_check"] = True
        chosen = {
            **route,
            "max_output_tokens": min(1024, int(route.get("max_output_tokens", 1024))),
            "web_search": False,
            "max_search_calls": 0,
        }
        instruction = connection_check_instruction(connection["protocol"])
        async with asyncio.timeout(CHECK_DEADLINE_SECONDS):
            result = await self._execute(
                chosen,
                bounded,
                credentials,
                None,
                instruction,
                _ConnectionCheckOutput,
                definitions=[check_tool],
                before_request=before_request,
                on_response=on_response,
                operation="check",
            )
        # One successful call is what proves tool support. A local client that
        # has to discover its tools first may well call the check twice while
        # exploring, and failing the whole account for that reported a broken
        # connection when nothing was wrong. Fidelity is still proven strictly
        # below: the exact one-time value has to come back in the result.
        tools_supported = check_tool.calls >= 1
        output_supported = returned_check_value(result["output"].value, check_tool.value)
        return {
            "usage": result["usage"],
            "actual_model": result["usage"].get("actual_model"),
            "tools_supported": tools_supported,
            "output_supported": output_supported,
            "checked_count": check_tool.calls,
        }

    async def close(self):
        self.closed = True
        workers = list(self.accounts.values())
        for worker in workers:
            worker.task.cancel()
        current = asyncio.current_task()
        pending = [task for task in self.active | self.executions if task is not current]
        for task in pending:
            task.cancel()
        await asyncio.gather(*(worker.task for worker in workers), *pending, return_exceptions=True)
        self.accounts.clear()
