"""Bounded, authenticated-route projections of durable execution events."""

import base64
import hashlib
import json
from uuid import uuid4

from .activity_privacy import sanitize
from .run_activity_models import RunActivity, RunActivityDetail, RunActivityPage

_PHASES = {
    "started",
    "completed",
    "failed",
    "cancelled",
    "interrupted",
    "queued",
    "waiting",
    "blocked",
}
_CATEGORIES = {
    "assistant_text_delta": "draft",
    "assistant_reasoning_summary": "reasoning_summary",
    "sources_read": "source",
    "source_inspected": "source",
    "runtime_waiting": "waiting",
    "model_response": "model_request",
    "runtime_model_reported": "model_request",
    "ai_route_selected": "routing",
    "analysis_started": "routing",
}


def project_activity(row):
    """Never infer success or an actor from model-authored prose."""
    data = json.loads(row["data_json"] or "{}")
    modern = row["kind"] == "activity" and bool(data.get("operation_id"))
    kind = row["kind"]
    category = (
        data.get("category", "activity")
        if modern
        else _CATEGORIES.get(
            kind, "tool" if "tool" in kind else "assignment" if "assignment" in kind else "activity"
        )
    )
    phase = (
        data.get("phase", "observed")
        if modern
        else next(
            (value for value in _PHASES if kind == value or kind.endswith("_" + value)), "observed"
        )
    )
    preview = (
        data.get("preview", "")
        if modern
        else data.get("text", "")
        if kind in {"assistant_text_delta", "assistant_reasoning_summary"}
        else ""
    )
    preview = preview if isinstance(preview, str) else ""
    preview, _, _ = sanitize(preview)
    message, _, _ = sanitize(row["message"])
    actor = data.get("actor_id")
    return RunActivity(
        event_id=row["id"],
        run_id=row["run_id"],
        operation_id=data.get("operation_id") if modern else None,
        parent_operation_id=data.get("parent_operation_id") if modern else None,
        actor_id=actor,
        actor_label=data.get("actor_label")
        or (
            "Specialist"
            if data.get("assignment_id")
            else "Tender Manager"
            if actor == "manager"
            else "Unknown actor"
        ),
        assignment_id=data.get("assignment_id"),
        category=category,
        phase=phase,
        message=message,
        created_at=row["created_at"],
        tool=data.get("tool"),
        provider=data.get("provider"),
        model=data.get("model"),
        provider_call_id=data.get("provider_call_id") if modern else None,
        preview=preview[:2000],
        preview_truncated=bool(data.get("preview_truncated")) or len(preview) > 2000,
        detail_available=True,
        capture_status=data.get("capture_status", "complete") if modern else "historical",
        elapsed_ms=data.get("elapsed_ms") if modern else None,
        source_ids=[value for value in data.get("source_ids", []) if isinstance(value, str)],
        artifact_refs=data.get("artifact_refs", []) if modern else [],
    )


def _fingerprint(row):
    return hashlib.sha256(
        (
            str(row["id"]) + row["created_at"] + row["kind"] + row["message"] + row["data_json"]
        ).encode()
    ).hexdigest()[:24]


def event_cursor(run_id, row):
    return (
        base64.urlsafe_b64encode(
            json.dumps(
                [run_id, row["id"] if row else 0, _fingerprint(row) if row else "empty"]
            ).encode()
        )
        .decode()
        .rstrip("=")
    )


class RunActivityService:
    def __init__(self, repo, *, should_stop=None):
        self.repo = repo
        self.should_stop = should_stop or (lambda: False)
        if not getattr(repo, "_activity_instance", None):
            repo._activity_instance = uuid4().hex

    def _require(self, conn, tender_id, run_id):
        run = conn.execute(
            "SELECT * FROM runs WHERE id=? AND tender_id=?", (run_id, tender_id)
        ).fetchone()
        if run is None:
            raise KeyError("This work could not be found in the selected Tender.")
        return run

    def _check_open(self):
        if self.should_stop():
            raise InterruptedError("Quantix is closing this workspace. Reopen it to view activity.")

    def _decode(self, conn, run_id, cursor):
        if not isinstance(cursor, str) or len(cursor) > 1024:
            raise ValueError("The activity cursor is invalid.")
        try:
            owner, identifier, fingerprint = json.loads(
                base64.urlsafe_b64decode(cursor + "=" * (-len(cursor) % 4))
            )
            if owner != run_id or type(identifier) is not int or identifier < 0:
                raise ValueError()
        except (ValueError, TypeError, UnicodeError):
            raise ValueError("The activity cursor does not belong to this run.") from None
        if identifier == 0:
            return 0, fingerprint != "empty"
        row = conn.execute(
            "SELECT * FROM run_events WHERE id=? AND run_id=?", (identifier, run_id)
        ).fetchone()
        return identifier, row is None or _fingerprint(row) != fingerprint

    def page(
        self,
        tender_id,
        run_id,
        *,
        after=None,
        before=None,
        limit=100,
        q=None,
        actor_id=None,
        category=None,
        errors_only=False,
    ):
        self._check_open()
        if not 1 <= limit <= 200 or after and before:
            raise ValueError("Choose one activity direction and a page of 1–200 steps.")
        with self.repo.db.connect() as conn:
            # Explicit BEGIN gives run, cursor and event window one snapshot.
            if not conn.in_transaction:
                conn.execute("BEGIN")
            run = self._require(conn, tender_id, run_id)
            last_event = conn.execute(
                "SELECT created_at FROM run_events WHERE run_id=? ORDER BY id DESC LIMIT 1",
                (run_id,),
            ).fetchone()
            first = conn.execute(
                "SELECT * FROM run_events WHERE run_id=? ORDER BY id LIMIT 1", (run_id,)
            ).fetchone()
            history = f"{self.repo._activity_instance}:{_fingerprint(first) if first else run['created_at']}"
            boundary, reset = (
                self._decode(conn, run_id, after or before) if after or before else (0, False)
            )
            filters = [
                "e.run_id=?",
                "NOT (e.kind IN ('assistant_text_delta','assistant_reasoning_summary') AND json_extract(e.data_json,'$.activity_operation_id') IS NOT NULL)",
            ]
            values = [run_id]
            if after and not reset:
                filters.append("e.id>?")
                values.append(boundary)
            elif before and not reset:
                filters.append("e.id<?")
                values.append(boundary)
            if q:
                filters.append(
                    "(instr(lower(e.message),lower(?))>0 OR instr(lower(e.data_json),lower(?))>0 OR instr(lower(p.content),lower(?))>0)"
                )
                values.extend([q, q, q])
            if actor_id:
                filters.append("json_extract(e.data_json,'$.actor_id')=?")
                values.append(actor_id)
            if category:
                filters.append(
                    "coalesce(json_extract(e.data_json,'$.category'), CASE WHEN e.kind IN ('source_inspected','sources_read') THEN 'source' WHEN e.kind='runtime_waiting' THEN 'waiting' WHEN e.kind IN ('model_response','runtime_model_reported') THEN 'model_request' WHEN e.kind IN ('ai_route_selected','analysis_started') THEN 'routing' WHEN e.kind LIKE '%tool%' THEN 'tool' WHEN e.kind LIKE '%assignment%' THEN 'assignment' WHEN e.kind='assistant_reasoning_summary' THEN 'reasoning_summary' WHEN e.kind='assistant_text_delta' THEN 'draft' ELSE 'activity' END)=?"
                )
                values.append(category)
            if errors_only:
                filters.append(
                    "(coalesce(json_extract(e.data_json,'$.phase'),'') IN ('blocked','failed','cancelled','interrupted','uncertain','uncertain_external_outcome') OR e.kind LIKE '%failed' OR e.kind LIKE '%cancelled' OR e.kind LIKE '%interrupted' OR e.kind LIKE '%blocked')"
                )
            forward = bool(after) and not reset
            rows = conn.execute(
                "SELECT e.* FROM run_events e LEFT JOIN run_activity_payloads p ON p.event_id=e.id WHERE "
                + " AND ".join(filters)
                + f" ORDER BY e.id {'ASC' if forward else 'DESC'} LIMIT ?",
                (*values, limit + 1),
            ).fetchall()
            extra = len(rows) > limit
            rows = rows[:limit]
            if not forward:
                rows = list(reversed(rows))
            latest = rows[-1] if rows else None
            if latest is None and not after:
                latest = conn.execute(
                    "SELECT * FROM run_events WHERE run_id=? ORDER BY id DESC LIMIT 1", (run_id,)
                ).fetchone()
            cursor = (
                event_cursor(run_id, latest)
                if latest
                else (None if reset else after) or event_cursor(run_id, None)
            )
            earliest = rows[0] if rows else None
            earlier_filters = [value for value in filters if value not in {"e.id>?", "e.id<?"}]
            earlier_values = [values[0], *values[2:]] if (after or before) and not reset else values
            has_earlier = bool(
                earliest
                and conn.execute(
                    "SELECT 1 FROM run_events e LEFT JOIN run_activity_payloads p ON p.event_id=e.id WHERE "
                    + " AND ".join(earlier_filters)
                    + " AND e.id<? LIMIT 1",
                    (*earlier_values, earliest["id"]),
                ).fetchone()
            )
            return RunActivityPage(
                items=[project_activity(row) for row in rows],
                cursor=cursor,
                before_cursor=event_cursor(run_id, earliest) if earliest else None,
                has_more=extra if forward else False,
                has_earlier=has_earlier,
                reset_required=reset,
                run_status=run["status"],
                run_detail=run["detail"],
                run_updated_at=max(
                    run["updated_at"], last_event[0] if last_event else run["updated_at"]
                ),
                history_key=history,
            )

    def detail(self, tender_id, run_id, event_id, *, offset=0, limit=16000):
        self._check_open()
        if offset < 0 or not 1 <= limit <= 64000:
            raise ValueError("Choose a valid detail page.")
        with self.repo.db.connect() as conn:
            if not conn.in_transaction:
                conn.execute("BEGIN")
            self._require(conn, tender_id, run_id)
            row = conn.execute(
                "SELECT * FROM run_events WHERE run_id=? AND id=?", (run_id, event_id)
            ).fetchone()
            if row is None:
                raise KeyError("This activity does not belong to the selected run.")
            payload = conn.execute(
                "SELECT substr(content,?,?),length(content),redacted,unavailable_json FROM run_activity_payloads WHERE event_id=?",
                (offset + 1, limit, event_id),
            ).fetchone()
            if payload:
                text, total, redacted, unavailable = payload
                unavailable = json.loads(unavailable)
            else:
                safe, redacted, unavailable = sanitize(json.loads(row["data_json"] or "{}"))
                content = json.dumps({"recorded_event": safe}, ensure_ascii=False)
                text, total = content[offset : offset + limit], len(content)
                unavailable = [*unavailable, "inputs", "outputs", "complete lifecycle"]
            operation_id = json.loads(row["data_json"] or "{}").get("operation_id")
            input_event = output_event = None
            if operation_id:
                input_event = conn.execute(
                    "SELECT id FROM run_events WHERE run_id=? AND json_extract(data_json,'$.operation_id')=? ORDER BY id LIMIT 1",
                    (run_id, operation_id),
                ).fetchone()
                output_event = conn.execute(
                    """SELECT id FROM run_events WHERE run_id=? AND json_extract(data_json,'$.operation_id')=?
                    AND (json_extract(data_json,'$.phase') IN ('completed','failed','blocked','interrupted','cancelled','uncertain')
                         OR json_extract(data_json,'$.category')='model_output')
                    ORDER BY CASE WHEN json_extract(data_json,'$.phase') IN ('failed','blocked','interrupted','cancelled','uncertain') THEN 0
                                  WHEN json_extract(data_json,'$.category')='model_output' THEN 1 ELSE 2 END, id DESC LIMIT 1""",
                    (run_id, operation_id),
                ).fetchone()
            return RunActivityDetail(
                activity=project_activity(row),
                text=text,
                offset=offset,
                next_offset=offset + len(text),
                total_chars=total,
                has_more=offset + len(text) < total,
                redacted=bool(redacted),
                unavailable_fields=unavailable,
                input_event_id=input_event[0] if input_event else None,
                output_event_id=output_event[0] if output_event else None,
            )
