"""Consistent, read-only projections for the live Tender Office."""

from __future__ import annotations

import base64
import binascii
import json
import re
from contextlib import contextmanager
from typing import Any

from .db import new_id
from .manager_profile import ManagerProfileService
from .office_events import OfficeEventService, _row_event
from .office_message_models import OfficeMessagePage
from .office_messages import OfficeMessageService
from .office_read_models import (
    AssignmentPage,
    OfficeEventPageWithInstance,
    OfficePartialFlags,
    OfficeSnapshot,
    StaffDesk,
    StaffPage,
    StaffReceiptPage,
    StaffResultPage,
    StaffVersionPage,
    StaffWorkOrderPage,
)
from .staff_assignments import StaffAssignmentService
from .staff_context import StaffContextService
from .staff_store import StaffStore

_IDENTIFIER_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:/-]{0,159}$")
_ACTIVE_ASSIGNMENT_STATES = ("queued", "running", "waiting")
_TERMINAL_ASSIGNMENT_STATES = ("completed", "failed", "cancelled", "interrupted")
_STAFF_SNAPSHOT_LIMIT = 200
_ACTIVE_STAFF_SNAPSHOT_LIMIT = 100
_ASSIGNMENT_SNAPSHOT_LIMIT = 100
_TERMINAL_ASSIGNMENT_SNAPSHOT_LIMIT = 100
_MESSAGE_SNAPSHOT_LIMIT = 50
_STAFF_DESK_WORK_ORDER_LIMIT = 50
_STAFF_DESK_RESULT_LIMIT = 50
_STAFF_DESK_RECEIPT_LIMIT = 200
_STAFF_DESK_VERSION_LIMIT = 200


def _identifier(value: Any, label: str) -> str:
    if not isinstance(value, str) or _IDENTIFIER_RE.fullmatch(value) is None:
        raise ValueError(f"{label} is invalid.")
    return value


def _decode_json_cursor(cursor: str, label: str) -> dict[str, Any]:
    if not isinstance(cursor, str) or not cursor or len(cursor) > 1000:
        raise ValueError(f"The {label} is invalid.")
    try:
        decoded = base64.urlsafe_b64decode(cursor + "=" * (-len(cursor) % 4))
        payload = json.loads(decoded.decode("utf-8"))
    except (binascii.Error, UnicodeError, TypeError, ValueError, json.JSONDecodeError) as error:
        raise ValueError(f"The {label} is invalid.") from error
    if not isinstance(payload, dict):
        raise ValueError(f"The {label} is invalid.")
    return payload


def _encode_json_cursor(payload: dict[str, Any]) -> str:
    return (
        base64.urlsafe_b64encode(
            json.dumps(payload, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode(
                "utf-8"
            )
        )
        .decode("ascii")
        .rstrip("=")
    )


class OfficeReadService:
    """Read the current office without creating work or invoking a provider.

    The service is intended to be constructed once by normal application
    startup.  Its constructor owns schema setup through the existing domain
    services; read methods use only the cached converters and direct SQL over
    one connection.
    """

    def __init__(self, repo):
        if repo is None or not hasattr(repo, "db") or not hasattr(repo, "atomic"):
            raise TypeError("A Repository is required for the office read service.")
        self.repo = repo
        self.manager_profiles = ManagerProfileService(repo)
        self.staff = StaffStore(repo)
        self.assignments = StaffAssignmentService(repo)
        self.messages = OfficeMessageService(repo)
        self.events = OfficeEventService(repo)
        self.context = StaffContextService(repo)
        self.instance_id = new_id()

    @contextmanager
    def _read_transaction(self):
        """Open an explicit SQLite read transaction when no outer tx exists."""

        existing = self.repo.db.held_connection()
        with self.repo.db.connect() as conn:
            if existing is None:
                # A plain sqlite SELECT starts a deferred transaction lazily.
                # BEGIN here makes the complete multi-query projection use one
                # database snapshot, including the event cursor.
                conn.execute("BEGIN")
            yield conn

    @staticmethod
    def _tender(conn, tender_id: str) -> None:
        if conn.execute("SELECT 1 FROM tenders WHERE id=?", (tender_id,)).fetchone() is None:
            raise KeyError("This item could not be found in the selected Tender.")

    def _manager(self, conn):
        row = conn.execute(
            """
            SELECT m.id AS manager_id, v.version, v.display_name, v.title, v.persona,
                   v.personality_json, v.working_preferences_json, v.created_at, v.updated_at
            FROM office_manager m
            JOIN office_manager_versions v
              ON v.manager_id=m.id AND v.version=m.current_version
            WHERE m.singleton=1
            """
        ).fetchone()
        return self.manager_profiles._profile(row)

    @staticmethod
    def _staff_ids(conn, tender_id: str, limit: int, *, active_only: bool = False) -> list[str]:
        if active_only:
            rows = conn.execute(
                """
                SELECT a.staff_id,MAX(a.created_at) AS latest_created_at,
                       MAX(a.id) AS latest_id
                FROM office_assignments a
                WHERE a.tender_id=? AND a.status IN ('queued','running','waiting')
                GROUP BY a.staff_id
                ORDER BY latest_created_at DESC,latest_id DESC
                LIMIT ?
                """,
                (tender_id, limit),
            ).fetchall()
        else:
            rows = conn.execute(
                """
                SELECT id FROM office_staff
                WHERE tender_id=?
                ORDER BY created_at DESC,id DESC
                LIMIT ?
                """,
                (tender_id, limit),
            ).fetchall()
        result: list[str] = []
        seen: set[str] = set()
        for row in rows:
            identifier = row["staff_id"] if active_only else row["id"]
            if identifier not in seen:
                result.append(identifier)
                seen.add(identifier)
        return result

    def _staff_records(self, conn, tender_id: str, staff_ids: list[str]):
        if not staff_ids:
            return []
        placeholders = ",".join("?" for _ in staff_ids)
        rows = conn.execute(
            f"""
            SELECT id,current_version,created_at
            FROM office_staff
            WHERE tender_id=? AND id IN ({placeholders})
            ORDER BY created_at,id
            """,
            (tender_id, *staff_ids),
        ).fetchall()
        return [
            self.staff._staff_record(conn, tender_id, row["id"], row["current_version"])
            for row in rows
        ]

    def _staff_projection(self, conn, tender_id: str):
        staff_total = int(
            conn.execute(
                "SELECT COUNT(*) FROM office_staff WHERE tender_id=?", (tender_id,)
            ).fetchone()[0]
        )
        recent_ids = self._staff_ids(conn, tender_id, _STAFF_SNAPSHOT_LIMIT)
        active_total = int(
            conn.execute(
                """
                SELECT COUNT(DISTINCT staff_id) FROM office_assignments
                WHERE tender_id=? AND status IN ('queued','running','waiting')
                """,
                (tender_id,),
            ).fetchone()[0]
        )
        active_ids = self._staff_ids(
            conn, tender_id, _ACTIVE_STAFF_SNAPSHOT_LIMIT, active_only=True
        )
        selected_ids = list(dict.fromkeys([*recent_ids, *active_ids]))
        staff = self._staff_records(conn, tender_id, selected_ids)
        return staff, staff_total, len(selected_ids) < staff_total or active_total > len(active_ids)

    @staticmethod
    def _assignment_records(conn, rows):
        return [StaffAssignmentService._assignment_from_row(row) for row in rows]

    def _assignment_projection(self, conn, tender_id: str, *, staff_id: str | None = None):
        where = "tender_id=?"
        params: list[Any] = [tender_id]
        if staff_id is not None:
            where += " AND staff_id=?"
            params.append(staff_id)
        total = int(
            conn.execute(
                f"SELECT COUNT(*) FROM office_assignments WHERE {where}", params
            ).fetchone()[0]
        )
        active_rows = conn.execute(
            f"""
            SELECT * FROM office_assignments
            WHERE {where} AND status IN ('queued','running','waiting')
            ORDER BY created_at,id LIMIT ?
            """,
            (*params, _ASSIGNMENT_SNAPSHOT_LIMIT + 1),
        ).fetchall()
        terminal_rows = conn.execute(
            f"""
            SELECT * FROM office_assignments
            WHERE {where} AND status IN ('completed','failed','cancelled','interrupted')
            ORDER BY created_at DESC,id DESC LIMIT ?
            """,
            (*params, _TERMINAL_ASSIGNMENT_SNAPSHOT_LIMIT + 1),
        ).fetchall()
        active_partial = len(active_rows) > _ASSIGNMENT_SNAPSHOT_LIMIT
        terminal_partial = len(terminal_rows) > _TERMINAL_ASSIGNMENT_SNAPSHOT_LIMIT
        active_rows = active_rows[:_ASSIGNMENT_SNAPSHOT_LIMIT]
        terminal_rows = list(reversed(terminal_rows[:_TERMINAL_ASSIGNMENT_SNAPSHOT_LIMIT]))
        rows = [*active_rows, *terminal_rows]
        rows.sort(key=lambda row: (row["created_at"], row["id"]))
        return self._assignment_records(conn, rows), total, active_partial or terminal_partial

    def _message_page_conn(
        self,
        conn,
        tender_id: str,
        *,
        cursor: str | None = None,
        limit: int = 50,
        staff_id: str | None = None,
        assignment_id: str | None = None,
    ) -> OfficeMessagePage:
        if type(limit) is not int or not 1 <= limit <= 50:
            raise ValueError("The message page limit must be between 1 and 50.")
        before = None
        if cursor is not None:
            before = self.messages._decode_cursor(cursor, tender_id, staff_id, assignment_id)
            cursor_row = conn.execute(
                "SELECT id FROM office_messages WHERE tender_id=? AND rowid=?",
                (tender_id, before[0]),
            ).fetchone()
            if cursor_row is None or cursor_row["id"] != before[1]:
                raise ValueError("The message cursor is invalid or no longer available.")
        rows = conn.execute(
            """
            SELECT m.*,m.rowid AS _office_rowid FROM office_messages m
            WHERE m.tender_id=?
              AND (? IS NULL OR m.sender_id=? OR EXISTS(
                  SELECT 1 FROM office_message_recipients r
                  WHERE r.message_id=m.id AND r.recipient_id=?
              ))
              AND (? IS NULL OR m.sender_assignment_id=? OR m.assignment_id=? OR EXISTS(
                  SELECT 1 FROM office_message_recipients r
                  WHERE r.message_id=m.id AND r.assignment_id=?
              ))
              AND (? IS NULL OR m.rowid < ?)
            ORDER BY m.rowid DESC
            LIMIT ?
            """,
            (
                tender_id,
                staff_id,
                staff_id,
                staff_id,
                assignment_id,
                assignment_id,
                assignment_id,
                assignment_id,
                before[0] if before else None,
                before[0] if before else None,
                limit + 1,
            ),
        ).fetchall()
        has_more = len(rows) > limit
        selected = rows[:limit]
        items = [self.messages._message_from_row(conn, row) for row in reversed(selected)]
        next_cursor = (
            self.messages._cursor_payload(tender_id, selected[-1], staff_id, assignment_id)
            if has_more and selected
            else None
        )
        return OfficeMessagePage(items=items, next_cursor=next_cursor)

    def snapshot(self, tender_id: str) -> OfficeSnapshot:
        tender_id = _identifier(tender_id, "Tender id")
        with self._read_transaction() as conn:
            self._tender(conn, tender_id)
            manager = self._manager(conn)
            staff, staff_total, staff_partial = self._staff_projection(conn, tender_id)
            assignments, assignments_total, assignments_partial = self._assignment_projection(
                conn, tender_id
            )
            message_rows = conn.execute(
                "SELECT m.*,m.rowid AS _office_rowid FROM office_messages m WHERE m.tender_id=? ORDER BY m.rowid DESC LIMIT ?",
                (tender_id, _MESSAGE_SNAPSHOT_LIMIT + 1),
            ).fetchall()
            message_selected = message_rows[:_MESSAGE_SNAPSHOT_LIMIT]
            messages = OfficeMessagePage(
                items=[
                    self.messages._message_from_row(conn, row) for row in reversed(message_selected)
                ],
                next_cursor=(
                    self.messages._cursor_payload(tender_id, message_selected[-1], None, None)
                    if len(message_rows) > _MESSAGE_SNAPSHOT_LIMIT
                    else None
                ),
            )
            event = conn.execute(
                "SELECT event_id,sequence FROM office_events WHERE tender_id=? ORDER BY sequence DESC LIMIT 1",
                (tender_id,),
            ).fetchone()
            interrupted = int(
                conn.execute(
                    "SELECT COUNT(*) FROM office_assignments WHERE tender_id=? AND status='interrupted'",
                    (tender_id,),
                ).fetchone()[0]
            )
            return OfficeSnapshot(
                manager=manager,
                staff=staff,
                assignments=assignments,
                messages=messages,
                cursor=event["event_id"] if event else None,
                sequence=int(event["sequence"]) if event else 0,
                instance_id=self.instance_id,
                interrupted_assignment_count=interrupted,
                staff_total=staff_total,
                assignments_total=assignments_total,
                partial_flags=OfficePartialFlags(
                    staff=staff_partial,
                    assignments=assignments_partial,
                    messages=messages.next_cursor is not None,
                ),
            )

    @staticmethod
    def _decode_staff_cursor(cursor: str, tender_id: str) -> tuple[str, str]:
        payload = _decode_json_cursor(cursor, "staff cursor")
        if (
            payload.get("tender_id") != tender_id
            or not isinstance(payload.get("created_at"), str)
            or not isinstance(payload.get("id"), str)
        ):
            raise ValueError("The staff cursor does not match this Tender.")
        return payload["created_at"], payload["id"]

    @staticmethod
    def _staff_cursor(tender_id: str, created_at: str, staff_id: str) -> str:
        return _encode_json_cursor(
            {"tender_id": tender_id, "created_at": created_at, "id": staff_id}
        )

    def staff_page(
        self,
        tender_id: str,
        *,
        cursor: str | None = None,
        limit: int = 50,
        lifecycle: str | None = None,
    ) -> StaffPage:
        tender_id = _identifier(tender_id, "Tender id")
        if type(limit) is not int or not 1 <= limit <= 50:
            raise ValueError("The staff page limit must be between 1 and 50.")
        if lifecycle not in {None, "available", "retired", "archived", "history"}:
            raise ValueError("Choose available, retired, archived or history colleagues.")
        with self._read_transaction() as conn:
            self._tender(conn, tender_id)
            after = None
            if cursor is not None:
                after = self._decode_staff_cursor(cursor, tender_id)
                cursor_row = conn.execute(
                    "SELECT created_at,id FROM office_staff WHERE tender_id=? AND id=?",
                    (tender_id, after[1]),
                ).fetchone()
                if cursor_row is None or (cursor_row["created_at"], cursor_row["id"]) != after:
                    raise ValueError("The staff cursor is invalid or no longer available.")
            lifecycle_sql = "1=1"
            lifecycle_args: tuple = ()
            if lifecycle == "history":
                lifecycle_sql = "lifecycle IN ('retired','archived')"
            elif lifecycle:
                lifecycle_sql = "lifecycle=?"
                lifecycle_args = (lifecycle,)
            rows = conn.execute(
                f"""
                SELECT id,current_version,created_at FROM office_staff
                WHERE tender_id=? AND {lifecycle_sql}
                  AND (? IS NULL OR created_at>? OR (created_at=? AND id>?))
                ORDER BY created_at,id LIMIT ?
                """,
                (
                    tender_id,
                    *lifecycle_args,
                    after[0] if after else None,
                    after[0] if after else None,
                    after[0] if after else None,
                    after[1] if after else None,
                    limit + 1,
                ),
            ).fetchall()
            has_more = len(rows) > limit
            selected = rows[:limit]
            return StaffPage(
                items=[
                    self.staff._staff_record(conn, tender_id, row["id"], row["current_version"])
                    for row in selected
                ],
                next_cursor=(
                    self._staff_cursor(tender_id, selected[-1]["created_at"], selected[-1]["id"])
                    if has_more and selected
                    else None
                ),
            )

    def _validate_staff(self, conn, tender_id: str, staff_id: str) -> None:
        if (
            conn.execute(
                "SELECT 1 FROM office_staff WHERE tender_id=? AND id=?", (tender_id, staff_id)
            ).fetchone()
            is None
        ):
            raise KeyError("This item could not be found in the selected Tender.")

    def staff_desk(self, tender_id: str, staff_id: str, *, version: int | None = None) -> StaffDesk:
        tender_id, staff_id = _identifier(tender_id, "Tender id"), _identifier(staff_id, "Staff id")
        if version is not None and (type(version) is not int or version < 1):
            raise ValueError("A staff profile version must be a positive integer.")
        with self._read_transaction() as conn:
            self._tender(conn, tender_id)
            self._validate_staff(conn, tender_id, staff_id)
            current_version = int(
                conn.execute(
                    "SELECT current_version FROM office_staff WHERE tender_id=? AND id=?",
                    (tender_id, staff_id),
                ).fetchone()[0]
            )
            selected_version = current_version if version is None else version
            profile = self.staff._staff_record(conn, tender_id, staff_id, selected_version)
            all_version_numbers = [
                int(row["version"])
                for row in conn.execute(
                    "SELECT version FROM office_staff_versions WHERE tender_id=? AND staff_id=? ORDER BY version",
                    (tender_id, staff_id),
                )
            ]
            versions_partial = len(all_version_numbers) > _STAFF_DESK_VERSION_LIMIT
            version_numbers = all_version_numbers[-_STAFF_DESK_VERSION_LIMIT:]

            work_order_rows = conn.execute(
                """
                SELECT id FROM office_work_orders
                WHERE tender_id=? AND staff_id=?
                ORDER BY created_at DESC,id DESC LIMIT ?
                """,
                (tender_id, staff_id, _STAFF_DESK_WORK_ORDER_LIMIT + 1),
            ).fetchall()
            work_orders_partial = len(work_order_rows) > _STAFF_DESK_WORK_ORDER_LIMIT
            work_orders = [
                self.staff._work_order_record(conn, tender_id, row["id"])
                for row in reversed(work_order_rows[:_STAFF_DESK_WORK_ORDER_LIMIT])
            ]
            assignments, _assignment_total, assignments_partial = self._assignment_projection(
                conn, tender_id, staff_id=staff_id
            )
            result_rows = conn.execute(
                """
                SELECT r.* FROM office_staff_results r
                JOIN office_assignments a ON a.tender_id=r.tender_id AND a.id=r.assignment_id
                WHERE r.tender_id=? AND a.staff_id=?
                ORDER BY r.created_at DESC,r.id DESC LIMIT ?
                """,
                (tender_id, staff_id, _STAFF_DESK_RESULT_LIMIT + 1),
            ).fetchall()
            results_partial = len(result_rows) > _STAFF_DESK_RESULT_LIMIT
            results = [
                self.assignments._result_from_row(conn, row)
                for row in reversed(result_rows[:_STAFF_DESK_RESULT_LIMIT])
            ]
            receipt_rows = conn.execute(
                """
                SELECT r.* FROM staff_source_receipts r
                JOIN office_assignments a ON a.tender_id=r.tender_id AND a.id=r.assignment_id
                WHERE r.tender_id=? AND a.staff_id=?
                ORDER BY r.created_at DESC,r.id DESC LIMIT ?
                """,
                (tender_id, staff_id, _STAFF_DESK_RECEIPT_LIMIT + 1),
            ).fetchall()
            receipts_partial = len(receipt_rows) > _STAFF_DESK_RECEIPT_LIMIT
            receipts = [
                self.context._receipt_model(row)
                for row in reversed(receipt_rows[:_STAFF_DESK_RECEIPT_LIMIT])
            ]
            messages = self._message_page_conn(conn, tender_id, staff_id=staff_id, limit=50)
            return StaffDesk(
                profile=profile,
                version_numbers=version_numbers,
                work_orders=work_orders,
                assignments=assignments,
                results=results,
                receipts=receipts,
                messages=messages,
                partial_flags=OfficePartialFlags(
                    assignments=assignments_partial,
                    messages=messages.next_cursor is not None,
                    work_orders=work_orders_partial,
                    results=results_partial,
                    receipts=receipts_partial,
                    versions=versions_partial,
                ),
            )

    def versions_page(
        self, tender_id: str, staff_id: str, *, offset: int = 0, limit: int = 50
    ) -> StaffVersionPage:
        tender_id, staff_id = _identifier(tender_id, "Tender id"), _identifier(staff_id, "Staff id")
        if type(offset) is not int or offset < 0:
            raise ValueError("The profile-version offset must be nonnegative.")
        if type(limit) is not int or not 1 <= limit <= 50:
            raise ValueError("The profile-version limit must be between 1 and 50.")
        with self._read_transaction() as conn:
            self._tender(conn, tender_id)
            self._validate_staff(conn, tender_id, staff_id)
            rows = conn.execute(
                """
                SELECT version FROM office_staff_versions
                WHERE tender_id=? AND staff_id=? ORDER BY version LIMIT ? OFFSET ?
                """,
                (tender_id, staff_id, limit + 1, offset),
            ).fetchall()
            selected = rows[:limit]
            has_more = len(rows) > limit
            return StaffVersionPage(
                items=[
                    self.staff._staff_record(conn, tender_id, staff_id, int(row["version"]))
                    for row in selected
                ],
                next_offset=offset + limit if has_more else None,
                has_more=has_more,
            )

    def work_orders_page(
        self, tender_id: str, staff_id: str, *, offset: int = 0, limit: int = 100
    ) -> StaffWorkOrderPage:
        tender_id, staff_id = _identifier(tender_id, "Tender id"), _identifier(staff_id, "Staff id")
        if type(offset) is not int or offset < 0:
            raise ValueError("The work-order offset must be nonnegative.")
        if type(limit) is not int or not 1 <= limit <= 200:
            raise ValueError("The work-order limit must be between 1 and 200.")
        with self._read_transaction() as conn:
            self._tender(conn, tender_id)
            self._validate_staff(conn, tender_id, staff_id)
            rows = conn.execute(
                """
                SELECT id FROM office_work_orders
                WHERE tender_id=? AND staff_id=? ORDER BY created_at,id LIMIT ? OFFSET ?
                """,
                (tender_id, staff_id, limit + 1, offset),
            ).fetchall()
            selected = rows[:limit]
            has_more = len(rows) > limit
            return StaffWorkOrderPage(
                items=[
                    self.staff._work_order_record(conn, tender_id, row["id"]) for row in selected
                ],
                next_offset=offset + limit if has_more else None,
                has_more=has_more,
            )

    def results_page(
        self, tender_id: str, staff_id: str, *, offset: int = 0, limit: int = 50
    ) -> StaffResultPage:
        tender_id, staff_id = _identifier(tender_id, "Tender id"), _identifier(staff_id, "Staff id")
        if type(offset) is not int or offset < 0:
            raise ValueError("The result offset must be nonnegative.")
        if type(limit) is not int or not 1 <= limit <= 50:
            raise ValueError("The result limit must be between 1 and 50.")
        with self._read_transaction() as conn:
            self._tender(conn, tender_id)
            self._validate_staff(conn, tender_id, staff_id)
            rows = conn.execute(
                """
                SELECT r.* FROM office_staff_results r
                JOIN office_assignments a ON a.tender_id=r.tender_id AND a.id=r.assignment_id
                WHERE r.tender_id=? AND a.staff_id=?
                ORDER BY r.created_at,r.id LIMIT ? OFFSET ?
                """,
                (tender_id, staff_id, limit + 1, offset),
            ).fetchall()
            selected = rows[:limit]
            has_more = len(rows) > limit
            return StaffResultPage(
                items=[self.assignments._result_from_row(conn, row) for row in selected],
                next_offset=offset + limit if has_more else None,
                has_more=has_more,
            )

    def events_page(self, tender_id: str, *, after: str | None = None, limit: int = 100):
        tender_id = _identifier(tender_id, "Tender id")
        if type(limit) is not int or not 1 <= limit <= 100:
            raise ValueError("The office event page limit must be between 1 and 100.")
        with self._read_transaction() as conn:
            self._tender(conn, tender_id)
            after_sequence = None
            if after is not None:
                if not isinstance(after, str) or not after or len(after) > 500:
                    raise ValueError("The office event cursor is invalid.")
                cursor_row = conn.execute(
                    "SELECT tender_id,sequence FROM office_events WHERE event_id=?", (after,)
                ).fetchone()
                if cursor_row is None:
                    current = conn.execute(
                        "SELECT event_id FROM office_events WHERE tender_id=? ORDER BY sequence DESC LIMIT 1",
                        (tender_id,),
                    ).fetchone()
                    return OfficeEventPageWithInstance(
                        items=[],
                        cursor=current["event_id"] if current else None,
                        has_more=False,
                        reset_required=True,
                        instance_id=self.instance_id,
                    )
                if cursor_row["tender_id"] != tender_id:
                    raise ValueError("The office event cursor belongs to another Tender.")
                after_sequence = int(cursor_row["sequence"])
            rows = conn.execute(
                """
                SELECT * FROM office_events
                WHERE tender_id=? AND (? IS NULL OR sequence>?)
                ORDER BY sequence LIMIT ?
                """,
                (tender_id, after_sequence, after_sequence, limit + 1),
            ).fetchall()
            selected = rows[:limit]
            return OfficeEventPageWithInstance(
                items=[_row_event(row) for row in selected],
                cursor=(selected[-1]["event_id"] if selected else after),
                has_more=len(rows) > limit,
                reset_required=False,
                instance_id=self.instance_id,
            )

    def messages_page(
        self,
        tender_id: str,
        *,
        cursor: str | None = None,
        limit: int = 50,
        staff_id: str | None = None,
        assignment_id: str | None = None,
    ) -> OfficeMessagePage:
        tender_id = _identifier(tender_id, "Tender id")
        if staff_id is not None:
            staff_id = _identifier(staff_id, "Staff id")
        if assignment_id is not None:
            assignment_id = _identifier(assignment_id, "Assignment id")
        with self._read_transaction() as conn:
            self._tender(conn, tender_id)
            if staff_id is not None:
                self._validate_staff(conn, tender_id, staff_id)
            if (
                assignment_id is not None
                and conn.execute(
                    "SELECT 1 FROM office_assignments WHERE tender_id=? AND id=?",
                    (tender_id, assignment_id),
                ).fetchone()
                is None
            ):
                raise KeyError("This item could not be found in the selected Tender.")
            return self._message_page_conn(
                conn,
                tender_id,
                cursor=cursor,
                limit=limit,
                staff_id=staff_id,
                assignment_id=assignment_id,
            )

    def assignments_page(
        self,
        tender_id: str,
        *,
        staff_id: str | None = None,
        root_run_id: str | None = None,
        offset: int = 0,
        limit: int = 100,
    ) -> AssignmentPage:
        tender_id = _identifier(tender_id, "Tender id")
        if staff_id is not None:
            staff_id = _identifier(staff_id, "Staff id")
        if root_run_id is not None:
            root_run_id = _identifier(root_run_id, "Root run id")
        if type(offset) is not int or offset < 0:
            raise ValueError("The assignment offset must be nonnegative.")
        if type(limit) is not int or not 1 <= limit <= 200:
            raise ValueError("The assignment limit must be between 1 and 200.")
        with self._read_transaction() as conn:
            self._tender(conn, tender_id)
            if staff_id is not None:
                self._validate_staff(conn, tender_id, staff_id)
            if (
                root_run_id is not None
                and conn.execute(
                    "SELECT 1 FROM runs WHERE tender_id=? AND id=?", (tender_id, root_run_id)
                ).fetchone()
                is None
            ):
                raise KeyError("This item could not be found in the selected Tender.")
            where = "tender_id=?"
            params: list[Any] = [tender_id]
            if staff_id is not None:
                where += " AND staff_id=?"
                params.append(staff_id)
            if root_run_id is not None:
                where += " AND root_run_id=?"
                params.append(root_run_id)
            rows = conn.execute(
                f"SELECT * FROM office_assignments WHERE {where} ORDER BY created_at,id LIMIT ? OFFSET ?",
                (*params, limit + 1, offset),
            ).fetchall()
            selected = rows[:limit]
            has_more = len(rows) > limit
            return AssignmentPage(
                items=self._assignment_records(conn, selected),
                next_offset=offset + limit if has_more else None,
                has_more=has_more,
            )

    def assignment(self, tender_id: str, assignment_id: str):
        tender_id, assignment_id = (
            _identifier(tender_id, "Tender id"),
            _identifier(assignment_id, "Assignment id"),
        )
        with self._read_transaction() as conn:
            self._tender(conn, tender_id)
            row = conn.execute(
                "SELECT * FROM office_assignments WHERE tender_id=? AND id=?",
                (tender_id, assignment_id),
            ).fetchone()
            if row is None:
                raise KeyError("This item could not be found in the selected Tender.")
            return StaffAssignmentService._assignment_from_row(row)

    def result(self, tender_id: str, result_id: str):
        tender_id, result_id = (
            _identifier(tender_id, "Tender id"),
            _identifier(result_id, "Result id"),
        )
        with self._read_transaction() as conn:
            self._tender(conn, tender_id)
            row = conn.execute(
                "SELECT * FROM office_staff_results WHERE tender_id=? AND id=?",
                (tender_id, result_id),
            ).fetchone()
            if row is None:
                raise KeyError("This item could not be found in the selected Tender.")
            return self.assignments._result_from_row(conn, row)

    def receipts_page(
        self, tender_id: str, assignment_id: str, *, offset: int = 0, limit: int = 200
    ) -> StaffReceiptPage:
        tender_id, assignment_id = (
            _identifier(tender_id, "Tender id"),
            _identifier(assignment_id, "Assignment id"),
        )
        if type(offset) is not int or offset < 0:
            raise ValueError("The receipt offset must be nonnegative.")
        if type(limit) is not int or not 1 <= limit <= 200:
            raise ValueError("The receipt limit must be between 1 and 200.")
        with self._read_transaction() as conn:
            self._tender(conn, tender_id)
            if (
                conn.execute(
                    "SELECT 1 FROM office_assignments WHERE tender_id=? AND id=?",
                    (tender_id, assignment_id),
                ).fetchone()
                is None
            ):
                raise KeyError("This item could not be found in the selected Tender.")
            rows = conn.execute(
                """
                SELECT * FROM staff_source_receipts
                WHERE tender_id=? AND assignment_id=?
                ORDER BY created_at,id LIMIT ? OFFSET ?
                """,
                (tender_id, assignment_id, limit + 1, offset),
            ).fetchall()
            selected = rows[:limit]
            has_more = len(rows) > limit
            return StaffReceiptPage(
                items=[self.context._receipt_model(row) for row in selected],
                next_offset=offset + limit if has_more else None,
                has_more=has_more,
            )


__all__ = ["OfficeReadService"]
