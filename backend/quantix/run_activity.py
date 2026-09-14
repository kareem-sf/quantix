"""Durable operation recording shared by SDK, MCP and nested tool execution."""

from contextlib import contextmanager
from contextvars import ContextVar
from datetime import datetime

from .activity_privacy import sanitize
from .db import dump, new_id, now

_operation = ContextVar("quantix_activity_operation", default=None)
# Independent of SQLite transactions: a capture failure inside a rolled-back
# source/publication transaction must still fence sibling contexts immediately.
_failed_runs: set[tuple[str, str]] = set()


def current_operation():
    return _operation.get()


@contextmanager
def activity_scope(operation_id):
    token = _operation.set(operation_id)
    try:
        yield
    finally:
        _operation.reset(token)


class ActivityRecordingError(ValueError):
    """No further work may be admitted after losing the execution record."""


def register_activity_secrets(context, values):
    if context is not None:
        previous = getattr(context, "_activity_secrets", ())
        context._activity_secrets = tuple(
            set((*previous, *(v for v in values if isinstance(v, str) and v)))
        )


class ActivityRecorder:
    def __init__(self, context):
        self.context = context
        self.repo = getattr(context, "repo", None)
        self.run_id = getattr(context, "run_id", None)
        self.enabled = self.run_id is not None and hasattr(self.repo, "db")

    def require_recording(self):
        if not self.enabled:
            return
        with self.repo.db.connect() as conn:
            failed = conn.execute(
                "SELECT 1 FROM run_activity_failures WHERE run_id=?", (self.run_id,)
            ).fetchone()
        if (
            failed
            or (str(self.repo.db.path), self.run_id) in _failed_runs
            or getattr(self.context, "_activity_capture_failed", False)
        ):
            raise ActivityRecordingError(
                "Execution recording failed. Further work was stopped; review the last confirmed step before resuming."
            )

    def start(
        self,
        category,
        message,
        payload=None,
        *,
        phase="started",
        tool=None,
        provider=None,
        model=None,
        provider_call_id=None,
        parent_operation_id=None,
        capture_status="complete",
        unavailable_fields=None,
    ):
        operation_id = new_id()
        if not self.enabled:
            return operation_id
        self.require_recording()
        parent = (
            parent_operation_id
            or current_operation()
            or getattr(self.context, "_activity_parent_operation", None)
        )
        metadata = {
            "category": category,
            "operation_id": operation_id,
            "parent_operation_id": parent,
            "actor_id": getattr(self.context, "actor_id", None) or "manager",
            "actor_label": "Staff member" if getattr(self.context, "assignment_id", None) else "Tender Manager",
            "assignment_id": getattr(self.context, "assignment_id", None),
            "tool": tool,
            "provider": provider,
            "model": model,
            "provider_call_id": provider_call_id,
        }
        self._write(
            operation_id,
            category,
            phase,
            message,
            payload,
            metadata=metadata,
            capture_status=capture_status,
            unavailable_fields=unavailable_fields,
        )
        return operation_id

    def record(
        self,
        operation_id,
        category,
        phase,
        message,
        payload=None,
        *,
        tool=None,
        provider=None,
        model=None,
        elapsed_ms=None,
        capture_status="complete",
        unavailable_fields=None,
    ):
        if not self.enabled:
            return
        self._write(
            operation_id,
            category,
            phase,
            message,
            payload,
            metadata={"tool": tool, "provider": provider, "model": model},
            elapsed_ms=elapsed_ms,
            capture_status=capture_status,
            unavailable_fields=unavailable_fields,
        )

    def _write(self, *args, **kwargs):
        try:
            self._append(*args, **kwargs)
        except (InterruptedError, ActivityRecordingError):
            raise
        except Exception as error:
            self.context._activity_capture_failed = True
            _failed_runs.add((str(self.repo.db.path), self.run_id))
            # A disk failure may prevent even the failure receipt. The in-memory
            # fence still stops this context and the job owner receives an error.
            try:
                with self.repo.atomic() as conn:
                    conn.execute(
                        "INSERT OR IGNORE INTO run_activity_failures VALUES(?,?)",
                        (self.run_id, now()),
                    )
                    conn.execute(
                        "UPDATE runs SET detail=? WHERE id=?",
                        (
                            "Execution recording failed; no further operations will start.",
                            self.run_id,
                        ),
                    )
            except Exception:
                pass
            raise ActivityRecordingError(
                "Execution recording failed. Further work was stopped; an operation already dispatched may have an uncertain outcome."
            ) from error

    def _append(
        self,
        operation_id,
        category,
        phase,
        message,
        payload,
        *,
        metadata,
        capture_status,
        unavailable_fields=None,
        elapsed_ms=None,
    ):
        import json

        stamp = now()
        secrets = getattr(self.context, "_activity_secrets", ())
        content, redacted, unavailable = sanitize(payload if payload is not None else {}, secrets)
        safe_message, message_redacted, _ = sanitize(message, secrets)
        with self.repo.atomic() as conn:
            run = conn.execute(
                "SELECT tender_id,status FROM runs WHERE id=?", (self.run_id,)
            ).fetchone()
            if run is None or run["tender_id"] != self.context.tender_id:
                raise KeyError("This work does not belong to the selected Tender.")
            if phase in {"started", "prepared", "queued", "delta"} and run["status"] not in {
                "queued",
                "running",
            }:
                raise InterruptedError("This Tender run has stopped.")
            saved = conn.execute(
                "SELECT metadata_json,created_at FROM run_activity_operations WHERE id=? AND run_id=?",
                (operation_id, self.run_id),
            ).fetchone()
            if saved:
                original = json.loads(saved[0])
                original.update(
                    {key: value for key, value in metadata.items() if value is not None}
                )
                metadata = original
                if elapsed_ms is None:
                    elapsed_ms = max(
                        0,
                        int(
                            (
                                datetime.fromisoformat(stamp) - datetime.fromisoformat(saved[1])
                            ).total_seconds()
                            * 1000
                        ),
                    )
            else:
                if "operation_id" not in metadata:
                    raise ValueError("The activity operation has not been started.")
                parent = metadata.get("parent_operation_id")
                if parent:
                    parent_row = conn.execute(
                        "SELECT metadata_json FROM run_activity_operations WHERE id=? AND run_id=?",
                        (parent, self.run_id),
                    ).fetchone()
                    if parent_row:
                        parent_data = json.loads(parent_row[0])
                        for key in ("provider", "model"):
                            metadata[key] = metadata.get(key) or parent_data.get(key)
                    else:
                        metadata["parent_operation_id"] = None
                metadata, _, _ = sanitize(metadata, secrets)
                conn.execute(
                    "INSERT INTO run_activity_operations VALUES(?,?,?,?)",
                    (operation_id, self.run_id, dump(metadata), stamp),
                )
            preview = content.get("text", "") if isinstance(content, dict) else ""
            if not isinstance(preview, str):
                preview = ""
            data = {
                **metadata,
                "category": category,
                "phase": phase,
                "capture_status": capture_status,
                "elapsed_ms": elapsed_ms,
                "preview": preview[:2000],
                "preview_truncated": len(preview) > 2000,
                "source_ids": _source_ids(content),
                "artifact_refs": _artifact_refs(content),
            }
            event = conn.execute(
                "INSERT INTO run_events(run_id,kind,message,data_json,created_at) VALUES(?,?,?,?,?)",
                (self.run_id, "activity", safe_message, dump(data), stamp),
            )
            conn.execute(
                "INSERT INTO run_activity_payloads VALUES(?,?,?,?)",
                (
                    event.lastrowid,
                    dump(content),
                    int(redacted or message_redacted),
                    dump(list(dict.fromkeys([*(unavailable_fields or []), *unavailable]))),
                ),
            )
            if phase in {"started", "prepared", "queued", "waiting"}:
                conn.execute(
                    "UPDATE runs SET detail=?,updated_at=? WHERE id=? AND status IN ('queued','running')",
                    (safe_message[:2000], stamp, self.run_id),
                )


def _source_ids(value):
    found = set()

    def visit(item):
        if isinstance(item, dict):
            for key, child in item.items():
                if key in {"source_id", "source_ids"}:
                    found.update(
                        v
                        for v in (child if isinstance(child, list) else [child])
                        if isinstance(v, str)
                    )
                elif isinstance(child, (dict, list)):
                    visit(child)
                elif isinstance(child, str) and child.lstrip().startswith(("{", "[")):
                    import json

                    try:
                        visit(json.loads(child))
                    except (ValueError, RecursionError):
                        pass
        elif isinstance(item, list):
            for child in item:
                visit(child)

    visit(value)
    return sorted(found)


def _artifact_refs(value):
    found = {}

    def visit(item):
        if isinstance(item, dict):
            if isinstance(item.get("artifact_id"), str):
                page = item.get("page")
                page = page if type(page) is int and page >= 0 else None
                found[(item["artifact_id"], page)] = {
                    "artifact_id": item["artifact_id"],
                    "page": page,
                }
            for child in item.values():
                if isinstance(child, (dict, list)):
                    visit(child)
        elif isinstance(item, list):
            for child in item:
                visit(child)

    visit(value)
    return list(found.values())


def settle_open_operations(repo, run_id, phase, message):
    """Close unfinished observations after durable authority has been revoked."""
    import json
    from types import SimpleNamespace

    run = repo.get_run(run_id)
    with repo.db.connect() as conn:
        operations = conn.execute(
            "SELECT id,metadata_json FROM run_activity_operations WHERE run_id=?", (run_id,)
        ).fetchall()
        pending = []
        for operation in operations:
            metadata = json.loads(operation["metadata_json"])
            last = conn.execute(
                "SELECT data_json FROM run_events WHERE run_id=? AND json_extract(data_json,'$.operation_id')=? ORDER BY id DESC LIMIT 1",
                (run_id, operation["id"]),
            ).fetchone()
            state = json.loads(last[0]).get("phase") if last else None
            if (
                state in {"prepared", "started", "queued", "delta"}
                or state == "observed"
                and metadata.get("category") == "run"
            ):
                pending.append((operation["id"], metadata))
    recorder = ActivityRecorder(
        SimpleNamespace(repo=repo, run_id=run_id, tender_id=run["tender_id"])
    )
    for operation, metadata in pending:
        recorder.record(
            operation,
            metadata["category"],
            phase,
            message,
            {"outcome": "No later completion was confirmed before the run ended."},
        )


def persist_recording_failure(repo, run_id):
    """Called by the job owner after failed work has unwound its transaction."""
    if (str(repo.db.path), run_id) in _failed_runs:
        with repo.atomic() as conn:
            conn.execute("INSERT OR IGNORE INTO run_activity_failures VALUES(?,?)", (run_id, now()))
