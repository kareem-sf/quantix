"""Shared local-client boundaries. No provider credentials are recovered from clients."""

import asyncio
import inspect
import logging
import os
from contextlib import asynccontextmanager
from pathlib import Path

_cleanup_tasks = set()
_logger = logging.getLogger(__name__)


def _retain_cleanup(task):
    """Keep cancelled SDK cleanup owned without blocking application shutdown."""
    _cleanup_tasks.add(task)

    def completed(done):
        _cleanup_tasks.discard(done)
        if not done.cancelled():
            done.exception()  # Retrieve a possible exception without logging secrets.

    task.add_done_callback(completed)


async def _bounded_close(operation, seconds):
    task = asyncio.create_task(operation())
    try:
        done, _ = await asyncio.wait({task}, timeout=max(0, seconds))
        if task in done:
            if task.cancelled():
                return False
            await task
            return True
        return False
    except Exception:
        return False
    finally:
        if not task.done():
            task.cancel()
            _retain_cleanup(task)


class RuntimeUnavailable(ValueError):
    """The selected original client cannot perform this operation as configured."""


class RuntimeConnectionFailure(ConnectionError):
    """Sanitized transient runtime failure eligible for an approved fallback."""


def transient_status(value) -> bool:
    return type(value) is int and (value in {408, 409, 429} or 500 <= value <= 599)


@asynccontextmanager
async def owned_client(client, *, close_method="close", force_method=None):
    """Own startup as well as the entered session, including late thread startup.

    The pinned clients do not clean up every interrupted __aenter__ path. Shield
    that task while closing during a bounded grace period. A still-unresolved
    thread-backed startup is reported rather than blocking application shutdown.
    No private SDK transport is accessed.
    """
    close = getattr(client, close_method)
    force = getattr(client, force_method) if force_method else close
    startup = asyncio.create_task(client.__aenter__())
    primary_error = None

    async def shutdown():
        deadline = asyncio.get_running_loop().time() + 5
        while not startup.done() and asyncio.get_running_loop().time() < deadline:
            remaining = deadline - asyncio.get_running_loop().time()
            await _bounded_close(force, min(1, remaining))
            await asyncio.wait({startup}, timeout=min(0.1, max(0, deadline - asyncio.get_running_loop().time())))
        unresolved_start = not startup.done()
        if unresolved_start:
            startup.cancel()
            _retain_cleanup(startup)
        else:
            await asyncio.gather(startup, return_exceptions=True)
        closed = await _bounded_close(force if unresolved_start else close, 2)
        if not closed and force_method:
            closed = await _bounded_close(force, 1)
        if unresolved_start or not closed:
            _logger.warning("Local AI client cleanup was not confirmed; a late startup or client process may still need to be stopped.")
        return closed and not unresolved_start

    try:
        await asyncio.shield(startup)
        yield client
    except BaseException as exc:
        primary_error = exc
        raise
    finally:
        cleanup = asyncio.create_task(shutdown())
        try:
            clean = await asyncio.shield(cleanup)
        except asyncio.CancelledError:
            cleanup.cancel()
            _retain_cleanup(cleanup)
            if not startup.done():
                startup.cancel()
                _retain_cleanup(startup)
            _logger.warning("Local AI client cleanup was interrupted; process termination remains unconfirmed.")
            await _bounded_close(force, 1)
            raise
        if not clean and not isinstance(primary_error, (ValueError, InterruptedError, asyncio.CancelledError)):
            raise RuntimeUnavailable("The original client could not close cleanly. Its result was withheld; stop its process before retrying.") from None


def child_environment(home: Path) -> dict[str, str]:
    """Also neutralise inherited keys in SDKs that merge env with os.environ."""
    allowed = {
        "SYSTEMROOT", "WINDIR", "COMSPEC", "PATH", "PATHEXT", "SYSTEMDRIVE", "PROGRAMDATA",
        "NUMBER_OF_PROCESSORS", "PROCESSOR_ARCHITECTURE", "LANG", "LC_ALL",
        "SSL_CERT_FILE", "SSL_CERT_DIR", "REQUESTS_CA_BUNDLE", "QUANTIX_AI_COMPONENT_ROOT", "QUANTIX_COMPONENT_MANAGED",
    }
    env = {key: value if key.upper() in allowed else "" for key, value in os.environ.items()}
    for directory in (home, home / "tmp", home / "config", home / "data", home / "cache",
                      home / "codex"):
        directory.mkdir(parents=True, exist_ok=True)
    env.update({
        "HOME": str(home), "USERPROFILE": str(home),
        "APPDATA": str(home / "config"), "LOCALAPPDATA": str(home / "data"),
        "XDG_CONFIG_HOME": str(home / "config"), "XDG_DATA_HOME": str(home / "data"),
        "XDG_CACHE_HOME": str(home / "cache"),
        "TEMP": str(home / "tmp"), "TMP": str(home / "tmp"),
        "CODEX_HOME": str(home / "codex"),
        "DISABLE_TELEMETRY": "1", "DO_NOT_TRACK": "1",
        "OTEL_SDK_DISABLED": "true",
        "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull,
    })
    return env


def execution_limits(connection: dict, route: dict) -> tuple[int, int, int]:
    limits = connection.get("_execution_limits") or {}
    requests = min(int(limits.get("max_requests", 12)), int(connection.get("settings", {}).get("max_turns", 12)))
    output = min(int(route.get("max_output_tokens", 8192)), int(limits.get("max_output_tokens", 8192)))
    timeout = int(connection.get("settings", {}).get("runtime_timeout_seconds", 900))
    if not 1 <= requests <= 100 or not 128 <= output <= 200000 or not 1 <= timeout <= 1800:
        raise ValueError("The local client's request, output or time limit is invalid.")
    return requests, output, timeout


async def call_hook(callback, *args, **kwargs):
    if callback is None:
        return None
    try:
        result = callback(*args, **kwargs)
        return await result if inspect.isawaitable(result) else result
    except ValueError as exc:
        # These callbacks belong to Quantix's policy layer, not to a provider.
        raise RuntimeUnavailable(str(exc)) from None


async def reserve_runtime(connection, route, instruction, before_request):
    requests, output, _ = execution_limits(connection, route)
    # Reserving the full declared context per inference bounds history growth and
    # tool responses; the application policy decides whether the price is known.
    capabilities = (connection.get("_model") or {}).get("capabilities") or {}
    context_window = capabilities.get("context_window")
    if context_window is None and connection.get("billing") in {"metered", "unknown"}:
        raise RuntimeUnavailable("Record this model's context window before metered local-client work so its complete agent loop can be reserved.")
    input_bound = int(context_window or max(len(instruction), 131072))
    reservation = await call_hook(before_request, input_bound, output, requests=requests)
    return reservation, requests, output


def runtime_usage(connection: dict, route: dict, **values) -> dict:
    return {
        "requests": 0, "input_tokens": 0, "output_tokens": 0,
        "actual_model": None, "billing": connection.get("billing", "unknown"),
        "usage_complete": False, "runtime": connection["protocol"],
        **values,
    }
