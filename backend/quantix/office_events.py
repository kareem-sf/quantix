"""Durable, Tender-scoped events for the live office.

This service is deliberately an internal persistence primitive.  It does not
authorize model output or expose a publishing/network endpoint; callers must
already own the domain transaction in which an event is recorded.
"""

from __future__ import annotations

import json
import sqlite3
from collections.abc import Mapping
from typing import Any

from .db import new_id, now
from .office_event_models import OFFICE_EVENT_TYPES, OfficeEvent, OfficeEventPage
from .staff_models import OfficeConflict

_MAX_IDENTIFIER_LENGTH = 500
_MAX_IDEMPOTENCY_LENGTH = 200
_MAX_JSON_BYTES = 64 * 1024

_SCHEMA_STATEMENTS = (
    """
    CREATE TABLE IF NOT EXISTS office_events (
        event_id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        sequence INTEGER NOT NULL CHECK (sequence >= 1),
        event_type TEXT NOT NULL,
        actor_id TEXT,
        assignment_id TEXT,
        record_ref_json TEXT,
        payload_json TEXT,
        idempotency_key TEXT,
        occurred_at TEXT NOT NULL,
        UNIQUE (tender_id, sequence)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS office_events_tender_sequence
    ON office_events(tender_id, sequence)
    """,
    """
    CREATE UNIQUE INDEX IF NOT EXISTS office_events_tender_idempotency
    ON office_events(tender_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL
    """,
)


def ensure_schema(repo) -> None:
    """Create the event schema in the caller's current write transaction."""

    # Repository.atomic() yields an existing connection when a domain owner
    # already opened one.  Keeping each statement separate preserves that
    # transaction's all-or-nothing behavior, including schema setup.
    with repo.atomic() as conn:
        for statement in _SCHEMA_STATEMENTS:
            conn.execute(statement)


def _bounded_string(value: Any, label: str, *, limit: int = _MAX_IDENTIFIER_LENGTH) -> str:
    if not isinstance(value, str) or not value.strip() or len(value) > limit:
        raise ValueError(f"Enter {label} (up to {limit} characters).")
    return value.strip()


def _optional_string(value: Any, label: str, *, limit: int = _MAX_IDENTIFIER_LENGTH) -> str | None:
    if value is None:
        return None
    return _bounded_string(value, label, limit=limit)


def _json_text(value: Any, label: str) -> str | None:
    if value is None:
        return None
    if not isinstance(value, Mapping):
        raise ValueError(f"The {label} must be a JSON object.")
    try:
        # JSON's default encoder accepts NaN and infinities as JavaScript-
        # incompatible extensions. Persist only finite JSON values.
        serialized = json.dumps(
            dict(value), ensure_ascii=False, separators=(",", ":"), allow_nan=False,
            sort_keys=True,
        )
    except (TypeError, ValueError, OverflowError, RecursionError) as error:
        raise ValueError(f"The {label} contains data that cannot be serialized.") from error
    if len(serialized.encode("utf-8")) > _MAX_JSON_BYTES:
        raise ValueError(f"The {label} is too large to store.")
    return serialized


def _json_value(serialized: str | None) -> dict[str, Any] | None:
    if serialized is None:
        return None
    try:
        value = json.loads(serialized)
    except (TypeError, ValueError, json.JSONDecodeError) as error:
        raise RuntimeError("A saved office event contains invalid JSON.") from error
    if not isinstance(value, dict):
        raise RuntimeError("A saved office event contains invalid JSON data.")
    return value


def _row_event(row) -> OfficeEvent:
    if row is None:
        raise KeyError("The office event could not be found.")
    return OfficeEvent(
        event_id=row["event_id"],
        sequence=row["sequence"],
        tender_id=row["tender_id"],
        event_type=row["event_type"],
        actor_id=row["actor_id"],
        assignment_id=row["assignment_id"],
        record_ref=_json_value(row["record_ref_json"]),
        payload=_json_value(row["payload_json"]),
        occurred_at=row["occurred_at"],
    )


class OfficeEventService:
    """Persist and page internal live-office events for one Repository."""

    def __init__(self, repo):
        self.repo = repo
        ensure_schema(repo)

    @staticmethod
    def _validate_tender_id(tender_id: Any) -> str:
        return _bounded_string(tender_id, "a Tender id")

    @staticmethod
    def _validate_event_type(event_type: Any) -> str:
        event_type = _bounded_string(event_type, "an office event type", limit=80)
        if event_type not in OFFICE_EVENT_TYPES:
            raise ValueError("Choose a recognised office event type.")
        return event_type

    @staticmethod
    def _validate_limit(limit: Any) -> int:
        if type(limit) is not int or not 1 <= limit <= 200:
            raise ValueError("The office event page limit must be between 1 and 200.")
        return limit

    @staticmethod
    def _same_request(row, event_type, actor_id, assignment_id, record_ref, payload) -> bool:
        return (
            row["event_type"] == event_type
            and row["actor_id"] == actor_id
            and row["assignment_id"] == assignment_id
            and _json_text(_json_value(row["record_ref_json"]), "record reference")
            == _json_text(record_ref, "record reference")
            and _json_text(_json_value(row["payload_json"]), "event payload")
            == _json_text(payload, "event payload")
        )

    def append(
        self,
        tender_id: str,
        event_type: str,
        *,
        actor_id: str | None = None,
        assignment_id: str | None = None,
        record_ref: dict | None = None,
        payload: dict | None = None,
        idempotency_key: str | None = None,
    ) -> OfficeEvent:
        """Append one validated event within the current or a new transaction."""

        tender_id = self._validate_tender_id(tender_id)
        event_type = self._validate_event_type(event_type)
        actor_id = _optional_string(actor_id, "an actor id")
        assignment_id = _optional_string(assignment_id, "an assignment id")
        idempotency_key = _optional_string(
            idempotency_key, "an idempotency key", limit=_MAX_IDEMPOTENCY_LENGTH
        )
        record_ref_json = _json_text(record_ref, "record reference")
        payload_json = _json_text(payload, "event payload")

        with self.repo.atomic() as conn:
            if conn.execute("SELECT 1 FROM tenders WHERE id=?", (tender_id,)).fetchone() is None:
                raise KeyError("This item could not be found in the selected Tender.")

            if idempotency_key is not None:
                existing = conn.execute(
                    "SELECT * FROM office_events WHERE tender_id=? AND idempotency_key=?",
                    (tender_id, idempotency_key),
                ).fetchone()
                if existing is not None:
                    if self._same_request(
                        existing,
                        event_type,
                        actor_id,
                        assignment_id,
                        _json_value(record_ref_json),
                        _json_value(payload_json),
                    ):
                        return _row_event(existing)
                    raise OfficeConflict(
                        "The office event idempotency key conflicts with a different event."
                    )

            sequence = conn.execute(
                "SELECT COALESCE(MAX(sequence), 0) + 1 FROM office_events WHERE tender_id=?",
                (tender_id,),
            ).fetchone()[0]
            event_id, occurred_at = new_id(), now()
            try:
                conn.execute(
                    """
                    INSERT INTO office_events(
                        event_id,tender_id,sequence,event_type,actor_id,assignment_id,
                        record_ref_json,payload_json,idempotency_key,occurred_at
                    ) VALUES(?,?,?,?,?,?,?,?,?,?)
                    """,
                    (
                        event_id,
                        tender_id,
                        sequence,
                        event_type,
                        actor_id,
                        assignment_id,
                        record_ref_json,
                        payload_json,
                        idempotency_key,
                        occurred_at,
                    ),
                )
            except sqlite3.IntegrityError as error:
                # BEGIN IMMEDIATE serializes sequence allocation.  This guard
                # keeps any unexpected schema collision as a domain conflict.
                raise OfficeConflict("The office event could not be appended safely.") from error
            from .office_activity import mirror_office_event
            from .run_activity import ActivityRecordingError

            try:
                mirror_office_event(self.repo, conn, tender_id, event_type, event_id,
                                    actor_id, assignment_id, record_ref, payload)
            except ActivityRecordingError:
                if event_type not in {"assignment_cancelled", "assignment_interrupted", "assignment_failed"}:
                    raise
                # Observation failure must never undo a committed revocation.
                # The run-wide recorder fence remains set independently of this transaction.
            return _row_event(
                conn.execute("SELECT * FROM office_events WHERE event_id=?", (event_id,)).fetchone()
            )

    def page(
        self,
        tender_id: str,
        *,
        after: str | None = None,
        limit: int = 100,
    ) -> OfficeEventPage:
        """Return a stable bounded page after an event-ID cursor."""

        tender_id = self._validate_tender_id(tender_id)
        limit = self._validate_limit(limit)
        if after is not None:
            after = _bounded_string(after, "an office event cursor")

        with self.repo.db.connect() as conn:
            if conn.execute("SELECT 1 FROM tenders WHERE id=?", (tender_id,)).fetchone() is None:
                raise KeyError("This item could not be found in the selected Tender.")

            after_sequence = None
            if after is not None:
                cursor_row = conn.execute(
                    "SELECT tender_id,sequence FROM office_events WHERE event_id=?", (after,)
                ).fetchone()
                if cursor_row is None:
                    current = conn.execute(
                        "SELECT event_id FROM office_events WHERE tender_id=? ORDER BY sequence DESC LIMIT 1",
                        (tender_id,),
                    ).fetchone()
                    return OfficeEventPage(
                        items=[],
                        cursor=current["event_id"] if current else None,
                        has_more=False,
                        reset_required=True,
                    )
                if cursor_row["tender_id"] != tender_id:
                    raise ValueError("The office event cursor belongs to another Tender.")
                after_sequence = cursor_row["sequence"]

            rows = conn.execute(
                """
                SELECT * FROM office_events
                WHERE tender_id=? AND (? IS NULL OR sequence>?)
                ORDER BY sequence
                LIMIT ?
                """,
                (tender_id, after_sequence, after_sequence, limit + 1),
            ).fetchall()
            has_more = len(rows) > limit
            selected = rows[:limit]
            items = [_row_event(row) for row in selected]

            if items:
                cursor = items[-1].event_id
            elif after is not None:
                cursor = after
            else:
                cursor = None
            return OfficeEventPage(
                items=items,
                cursor=cursor,
                has_more=has_more,
                reset_required=False,
            )

    def latest_cursor(self, tender_id: str) -> str | None:
        tender_id = self._validate_tender_id(tender_id)
        with self.repo.db.connect() as conn:
            if conn.execute("SELECT 1 FROM tenders WHERE id=?", (tender_id,)).fetchone() is None:
                raise KeyError("This item could not be found in the selected Tender.")
            row = conn.execute(
                "SELECT event_id FROM office_events WHERE tender_id=? ORDER BY sequence DESC LIMIT 1",
                (tender_id,),
            ).fetchone()
            return row["event_id"] if row else None


__all__ = ["OFFICE_EVENT_TYPES", "OfficeEventService", "ensure_schema"]
