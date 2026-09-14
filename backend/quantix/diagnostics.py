"""Dependency-free bounded local JSONL diagnostics.

The writer is copied into managed worker packages by ``AIComponentService``.
Keep this module self-contained: workers do not install the Quantix service or
its third-party dependencies.
"""

from __future__ import annotations

import contextlib
import contextvars
import datetime as _datetime
import json
import os
import re
import sys
import threading
import time
import traceback
from collections import deque
from pathlib import Path
from uuid import uuid4

MAX_FILE_BYTES = 5 * 1024 * 1024
BACKUP_COUNT = 2
RETENTION_DAYS = 14
MAX_TOTAL_BYTES = 100 * 1024 * 1024
MAX_RECORD_BYTES = 64 * 1024
RENDERER_RATE_LIMIT = 60
RENDERER_RATE_WINDOW_SECONDS = 60.0

_ID = re.compile(r"^[a-f0-9]{32}$")
_SAFE_TOKEN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._:/+@-]{0,299}$")
_SAFE_EVENT = re.compile(r"^[a-z][a-z0-9_.-]{0,79}$")
_SAFE_ROUTE = re.compile(r"^/[A-Za-z0-9_./{}:-]{0,399}$")
_SAFE_COMPONENT = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$")
_OWNED_FILE = re.compile(
    r"^quantix-[A-Za-z0-9][A-Za-z0-9._-]{0,79}-(\d+)-([a-f0-9]{32})\.jsonl(?:\.\d+)?$"
)

_context: contextvars.ContextVar[dict[str, object]] = contextvars.ContextVar(
    "quantix_diagnostic_context", default={}
)
_writer: "DiagnosticWriter | None" = None
_writer_lock = threading.Lock()


def _utc_now() -> str:
    return _datetime.datetime.now(_datetime.timezone.utc).isoformat(timespec="milliseconds")


def _token(value, *, pattern=_SAFE_TOKEN, limit=300):
    if (
        not isinstance(value, str)
        or len(value) > limit
        or not pattern.fullmatch(value)
        or pattern is _SAFE_TOKEN
        and (
            "\\" in value
            or value.startswith("/")
            or re.match(r"^[A-Za-z]:[\\/]", value)
            or re.match(r"^[A-Za-z][A-Za-z0-9+.-]*://", value)
        )
    ):
        return None
    return value


def _bounded_int(value, *, maximum=10**12):
    return value if type(value) is int and 0 <= value <= maximum else None


def route_template(scope) -> str | None:
    """Return a Starlette route template without retaining the requested URL."""
    route = scope.get("route") if isinstance(scope, dict) else None
    value = getattr(route, "path", None)
    return _token(value, pattern=_SAFE_ROUTE, limit=400)


def exception_structure(error: BaseException, *, max_depth=4, max_frames=8) -> dict:
    """Return bounded exception class/frame structure with no messages or paths."""
    budget = [64]

    def one(value, depth):
        if budget[0] <= 0:
            return {"type": type(value).__name__[:120], "truncated": True}
        budget[0] -= 1
        if depth >= max_depth:
            return {"type": type(value).__name__[:120], "truncated": True}
        item = {"type": type(value).__name__[:120]}
        module = getattr(type(value), "__module__", None)
        if isinstance(module, str) and len(module) <= 200:
            item["module"] = module
        try:
            status_code = getattr(value, "status_code", None)
            if type(status_code) is int and 100 <= status_code <= 599:
                item["status_code"] = status_code
        except Exception:
            pass
        try:
            errno = getattr(value, "errno", None)
            if type(errno) is int and 0 <= errno <= 10**6:
                item["errno"] = errno
        except Exception:
            pass
        frames = []
        tb = value.__traceback__
        observed_frames = 0
        tail = deque(maxlen=max_frames)
        for frame, line in traceback.walk_tb(tb):
            observed_frames += 1
            tail.append((frame, line))
        all_frames = list(tail)
        for frame, line in all_frames:
            name = frame.f_code.co_name
            module_name = frame.f_globals.get("__name__")
            entry = {"function": name[:120], "line": line}
            if isinstance(module_name, str) and len(module_name) <= 200:
                entry["module"] = module_name
            frames.append(entry)
        if frames:
            item["frames"] = frames
        if observed_frames > max_frames:
            item["frames_truncated"] = True
        cause = value.__cause__ or value.__context__
        if isinstance(cause, BaseException):
            item["cause"] = one(cause, depth + 1)
        if isinstance(value, BaseExceptionGroup):
            children = [child for child in value.exceptions if isinstance(child, BaseException)]
            if children:
                item["children"] = [one(child, depth + 1) for child in children[:4]]
                if len(children) > 4:
                    item["children_truncated"] = True
        return item

    return one(error, 0)


def _safe_error(error):
    try:
        return exception_structure(error)
    except Exception:
        return {"type": type(error).__name__[:120]}


def _safe_error_payload(value, *, depth=0, budget=None):
    """Validate the shape of an exception structure supplied to ``record``."""
    if budget is None:
        budget = [64]
    if depth > 4 or budget[0] <= 0 or not isinstance(value, dict):
        return {"truncated": True}
    budget[0] -= 1
    result = {}
    for key, item in value.items():
        if key in {"type", "module", "function"}:
            if (
                isinstance(item, str)
                and len(item) <= 200
                and re.fullmatch(r"[A-Za-z0-9_.-]+", item)
            ):
                result[key] = item
        elif key in {"line", "status_code", "errno"}:
            safe = _bounded_int(item, maximum=10**12)
            if safe is not None:
                result[key] = safe
        elif key in {"frames", "children"}:
            if isinstance(item, list):
                result[key] = [
                    _safe_error_payload(child, depth=depth + 1, budget=budget)
                    for child in item[:8]
                    if isinstance(child, dict)
                ]
        elif key in {"cause"} and isinstance(item, dict):
            result[key] = _safe_error_payload(item, depth=depth + 1, budget=budget)
        elif key in {"truncated", "frames_truncated", "children_truncated"} and type(item) is bool:
            result[key] = item
    return result


class DiagnosticWriter:
    """One-process JSONL writer with bounded rotation and best-effort failure."""

    def __init__(
        self, directory=None, *, component="service", session_id=None, require_directory=False
    ):
        self.directory: Path | None = None
        try:
            if require_directory and directory is None:
                raise ValueError("An explicit diagnostics directory is required.")
            selected = (
                Path(directory) if directory is not None else Path.home() / ".quantix" / "logs"
            )
            self.directory = selected.expanduser().resolve()
        except Exception:
            # Diagnostics must never prevent the service or worker from
            # starting, and failed resolution must never trigger a fallback
            # write in the current directory.
            pass
        self.component = _token(component, pattern=_SAFE_COMPONENT, limit=80) or "service"
        self.session_id = session_id if _ID.fullmatch(session_id or "") else uuid4().hex
        self.process_id = os.getpid()
        self.path: Path | None = None
        self.lock_path: Path | None = None
        if self.directory is not None:
            self.path = (
                self.directory
                / f"quantix-{self.component}-{self.process_id}-{self.session_id}.jsonl"
            )
            self.lock_path = self.path.with_name(self.path.name + ".lock")
        self._lock_stream = None
        self.available = False
        self.detail = "Diagnostic recording is unavailable. Quantix work can continue."
        self._lock = threading.RLock()
        self._warned = False
        self._renderer_events = deque()
        if self.directory is not None:
            self._open_or_mark()
        else:
            self._failure()

    @classmethod
    def from_environment(cls, *, component="worker"):
        directory = os.environ.get("QUANTIX_DIAGNOSTICS_DIRECTORY")
        session_id = os.environ.get("QUANTIX_DIAGNOSTICS_SESSION_ID")
        return cls(directory, component=component, session_id=session_id, require_directory=True)

    def _open_or_mark(self):
        if self.directory is None or self.path is None or self.lock_path is None:
            self._failure()
            return
        try:
            self.directory.mkdir(parents=True, exist_ok=True)
            with self.path.open("a", encoding="utf-8"):
                pass
            self._lock_stream = self.lock_path.open("a+b")
            if os.name == "nt":
                import msvcrt

                self._lock_stream.seek(0, 2)
                if self._lock_stream.tell() == 0:
                    self._lock_stream.write(b"\0")
                    self._lock_stream.flush()
                self._lock_stream.seek(0)
                msvcrt.locking(self._lock_stream.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl

                fcntl.flock(self._lock_stream.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            self.available = True
            self.detail = "Recording local diagnostics."
            self._prune()
        except Exception:
            self._failure()

    def _failure(self):
        self.available = False
        self.detail = "Diagnostic recording is unavailable. Quantix work can continue."
        if not self._warned:
            self._warned = True
            try:
                print("Quantix diagnostics are unavailable; work continues.", file=sys.stderr)
            except Exception:
                pass

    def status(self):
        return {
            "available": bool(self.available),
            "directory": str(self.directory) if self.directory is not None else "",
            "detail": self.detail,
            "session_id": self.session_id,
            "retention_days": RETENTION_DAYS,
            "max_total_bytes": MAX_TOTAL_BYTES,
            "max_file_bytes": MAX_FILE_BYTES,
            "backup_count": BACKUP_COUNT,
        }

    def _owned_completed(self):
        if self.directory is None:
            return []
        try:
            return [
                path
                for path in self.directory.glob("quantix-*.jsonl*")
                if path.is_file() and _OWNED_FILE.fullmatch(path.name)
            ]
        except Exception:
            return []

    def _prune(self):
        now = time.time()
        paths = self._owned_completed()
        for path in paths:
            try:
                if path == self.path or self._is_live(path):
                    continue
                if now - path.stat().st_mtime > RETENTION_DAYS * 86400:
                    self._delete_owned(path)
            except Exception:
                continue
        paths = self._owned_completed()
        entries = []
        total = 0
        for path in paths:
            if path == self.path or self._is_live(path):
                continue
            try:
                size = path.stat().st_size
                modified = path.stat().st_mtime
            except Exception:
                continue
            total += size
            entries.append((modified, path, size))
        for _, path, size in sorted(entries):
            if total <= MAX_TOTAL_BYTES:
                break
            try:
                self._delete_owned(path)
                total -= size
            except OSError:
                pass

    def _is_live(self, path):
        """Check an owned writer marker using a native file lock."""
        match = _OWNED_FILE.fullmatch(path.name)
        if not match:
            return True
        base = path.name.split(".jsonl", 1)[0] + ".jsonl"
        marker = path.with_name(base + ".lock")
        if marker == self.lock_path:
            return True
        if marker.exists():
            stream = None
            try:
                stream = marker.open("a+b")
                if os.name == "nt":
                    import msvcrt

                    stream.seek(0, 2)
                    if stream.tell() == 0:
                        stream.write(b"\0")
                        stream.flush()
                    stream.seek(0)
                    msvcrt.locking(stream.fileno(), msvcrt.LK_NBLCK, 1)
                    msvcrt.locking(stream.fileno(), msvcrt.LK_UNLCK, 1)
                else:
                    import fcntl

                    fcntl.flock(stream.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
                    fcntl.flock(stream.fileno(), fcntl.LOCK_UN)
            except Exception:
                return True
            finally:
                if stream is not None:
                    stream.close()
        # Native writers may not use a Python marker. Keep their files when
        # their owning PID is still live; unknown/access-denied states are live.
        try:
            return self._pid_live(int(match.group(1)))
        except Exception:
            return True

    @staticmethod
    def _pid_live(pid):
        if pid <= 0:
            return False
        if os.name != "nt":
            try:
                os.kill(pid, 0)
                return True
            except ProcessLookupError:
                return False
            except PermissionError:
                return True
            except OSError:
                return True
        try:
            import ctypes
            from ctypes import wintypes

            kernel = ctypes.WinDLL("kernel32", use_last_error=True)
            kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
            kernel.OpenProcess.restype = wintypes.HANDLE
            kernel.GetExitCodeProcess.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]
            kernel.GetExitCodeProcess.restype = wintypes.BOOL
            kernel.CloseHandle.argtypes = [wintypes.HANDLE]
            kernel.CloseHandle.restype = wintypes.BOOL
            handle = kernel.OpenProcess(0x1000 | 0x00100000, False, pid)
            if not handle:
                # ERROR_INVALID_PARAMETER means the PID is gone. Every other
                # failure (including access/resource errors) stays retained.
                return ctypes.get_last_error() != 87
            try:
                code = wintypes.DWORD()
                if not kernel.GetExitCodeProcess(handle, ctypes.byref(code)):
                    return True
                return code.value == 259
            finally:
                kernel.CloseHandle(handle)
        except Exception:
            return True

    def _delete_owned(self, path):
        path.unlink(missing_ok=True)
        marker = path.with_name(path.name.split(".jsonl", 1)[0] + ".jsonl.lock")
        if marker != self.lock_path and not self._is_live(path):
            marker.unlink(missing_ok=True)

    def _rotate(self, incoming=0):
        if self.path is None:
            return
        try:
            if not self.path.exists() or self.path.stat().st_size + incoming < MAX_FILE_BYTES:
                return
            oldest = self.path.with_name(self.path.name + f".{BACKUP_COUNT}")
            oldest.unlink(missing_ok=True)
            for index in range(BACKUP_COUNT - 1, 0, -1):
                source = self.path.with_name(self.path.name + f".{index}")
                target = self.path.with_name(self.path.name + f".{index + 1}")
                if source.exists():
                    os.replace(source, target)
            os.replace(self.path, self.path.with_name(self.path.name + ".1"))
            self._prune()
        except Exception:
            self._failure()

    def _field(self, key, value):
        if key in {
            "duration_ms",
            "http_status",
            "status_code",
            "errno",
            "line",
            "column",
            "round",
            "rounds",
            "exit_code",
            "requests",
            "max_requests",
            "requests_used",
            "input_tokens",
            "output_tokens",
            "cached_input_tokens",
            "reasoning_tokens",
            "total_tokens",
            "tool_calls",
            "web_search_calls",
            "progress",
        }:
            maximum = 100_000_000 if key == "duration_ms" else 10**12
            if key == "exit_code":
                return value if type(value) is int and -(10**6) <= value <= maximum else None
            return _bounded_int(value, maximum=maximum)
        if key in {
            "submitted",
            "submission_received",
            "available",
            "usage_complete",
            "provider_cost_is_partial",
            "provider_usage_is_incomplete",
            "tool_in_allowlist",
        }:
            return value if type(value) is bool else None
        if key in {"error", "exception"}:
            return _safe_error_payload(value)
        if key == "route":
            return _token(value, pattern=_SAFE_ROUTE, limit=400)
        if key in {"request_id", "operation_id", "run_id", "session_id", "error_reference"}:
            return value if isinstance(value, str) and _ID.fullmatch(value) else None
        if key in {"tools", "tool_names"}:
            if not isinstance(value, list):
                return None
            return [item for item in value if _token(item)][:30]
        if key in {"stop_reason"}:
            return (
                value
                if value
                in {
                    "end_turn",
                    "cancelled",
                    "max_turns",
                    "max_tokens",
                    "max_turn_requests",
                    "refusal",
                    "error",
                    "other",
                    "missing",
                }
                else "other"
            )
        if key in {"submission_status"}:
            return value if value in {"received", "missing", "rejected", "unknown"} else "unknown"
        if key in {
            "phase",
            "outcome",
            "protocol",
            "model",
            "actual_model",
            "component_version",
            "component_id",
            "connection_id",
            "tool_name",
            "operation",
            "kind",
            "source",
            "error_type",
            "method",
        }:
            return _token(value)
        return None

    def _record(self, event, level="info", **fields):
        if not _SAFE_EVENT.fullmatch(event):
            return False
        record = {
            "timestamp": _utc_now(),
            "level": level if level in {"debug", "info", "warning", "error"} else "info",
            "component": self.component,
            "event": event,
            "process_id": self.process_id,
            "session_id": self.session_id,
        }
        context = _context.get()
        for key, value in {**context, **fields}.items():
            if key in {"timestamp", "level", "component", "event", "process_id"}:
                continue
            safe = self._field(key, value)
            if safe is not None:
                record[key] = safe
        encoded = (json.dumps(record, ensure_ascii=False, separators=(",", ":")) + "\n").encode(
            "utf-8"
        )
        if len(encoded) > MAX_RECORD_BYTES:
            return False
        with self._lock:
            if not self.available:
                return False
            try:
                self._rotate(len(encoded))
                if not self.available:
                    return False
                with self.path.open("ab") as stream:
                    stream.write(encoded)
                    stream.flush()
                return True
            except Exception:
                self._failure()
                return False

    def record(self, event, *, level="info", **fields):
        return self._record(event, level=level, **fields)

    def record_exception(self, event, error, *, level="error", **fields):
        supplied = fields.get("error_reference")
        reference = (
            supplied if isinstance(supplied, str) and _ID.fullmatch(supplied) else uuid4().hex
        )
        fields = dict(fields)
        fields["error_reference"] = reference
        fields["error"] = _safe_error(error)
        self._record(event, level=level, **fields)
        return reference

    def close(self):
        """Release this writer's liveness marker during orderly shutdown."""
        with self._lock:
            stream, self._lock_stream = self._lock_stream, None
            if stream is None:
                return
            try:
                if os.name != "nt":
                    import fcntl

                    fcntl.flock(stream.fileno(), fcntl.LOCK_UN)
                else:
                    import msvcrt

                    stream.seek(0)
                    msvcrt.locking(stream.fileno(), msvcrt.LK_UNLCK, 1)
                stream.close()
                self.lock_path.unlink(missing_ok=True)
            except Exception:
                try:
                    stream.close()
                except Exception:
                    pass

    def accept_renderer_event(self, event):
        now = time.monotonic()
        with self._lock:
            while (
                self._renderer_events
                and now - self._renderer_events[0] >= RENDERER_RATE_WINDOW_SECONDS
            ):
                self._renderer_events.popleft()
            if len(self._renderer_events) >= RENDERER_RATE_LIMIT:
                return False
            self._renderer_events.append(now)
        return self._record(
            event.event,
            source=event.source or "renderer",
            error_type=event.error_type or "Unknown",
            line=event.line,
            column=event.column,
            request_id=event.request_id,
            phase="renderer",
            outcome="reported",
        )


def initialize(directory=None, *, component="service", session_id=None, require_directory=False):
    global _writer
    with _writer_lock:
        selected = None
        directory_error = False
        if require_directory and directory is None:
            directory_error = True
        elif directory is not None:
            try:
                selected = Path(directory).expanduser().resolve()
            except Exception:
                directory_error = True
        same_directory = not directory_error and (
            selected is None or (_writer is not None and selected == _writer.directory)
        )
        same_session = session_id is None or (
            _writer is not None and session_id == _writer.session_id
        )
        if (
            _writer is None
            or not _writer.available
            or component != _writer.component
            or not same_directory
            or not same_session
        ):
            previous = _writer
            if previous is not None and not directory_error:
                previous.close()
            _writer = DiagnosticWriter(
                directory,
                component=component,
                session_id=session_id,
                require_directory=require_directory,
            )
        return _writer


def get_writer():
    global _writer
    if _writer is None:
        _writer = initialize()
    return _writer


def record(event, *, level="info", **fields):
    return get_writer().record(event, level=level, **fields)


def record_exception(event, error, *, level="error", **fields):
    return get_writer().record_exception(event, error, level=level, **fields)


@contextlib.contextmanager
def diagnostic_context(**values):
    safe = {
        key: value
        for key, value in values.items()
        if key in {"request_id", "operation_id", "run_id", "session_id"}
    }
    token = _context.set(safe)
    try:
        yield
    finally:
        _context.reset(token)
