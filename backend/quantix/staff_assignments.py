"""Transactional persistence for dynamic staff assignments and draft results.

This module owns durable assignment state.  It deliberately stops at a
validated draft: provider execution, Manager consolidation and publication
belong to the runtime/root job layers.
"""

from __future__ import annotations

import hashlib
import json
import re
import sqlite3
from typing import Any

from .db import dump, new_id, now
from .manager_runtime import ManagerRunProfiles
from .office_events import OfficeEventService
from .staff_assignment_models import (
    PreparedStaffDraft,
    StaffAssignment,
    StaffResult,
    StaffSourceBasis,
)
from .staff_models import ManagerCreationContext, OfficeConflict
from .staff_routing import StaffRoutingService
from .staff_routing_models import RouteBinding, canonical_json

_IDENTIFIER_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:/-]{0,159}$")
_KEY_RE = re.compile(r"^[A-Za-z0-9._~-]{1,160}$")
_ACTIVE_STATUSES = {"queued", "running", "waiting"}
_TERMINAL_STATUSES = {"completed", "failed", "cancelled", "interrupted"}
_TERMINAL_RUN_STATUSES = _TERMINAL_STATUSES
_TRANSITION_EVENT = {
    "running": "assignment_started",
    "waiting": "assignment_waiting",
    "completed": "assignment_completed",
    "failed": "assignment_failed",
    "cancelled": "assignment_cancelled",
    "interrupted": "assignment_interrupted",
}
_DRAFT_INTRINSIC_FIELDS = {"summary", "source_ids"}


_SCHEMA_STATEMENTS = (
    """
    CREATE TABLE IF NOT EXISTS office_assignments (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        root_run_id TEXT NOT NULL REFERENCES runs(id),
        staff_id TEXT NOT NULL REFERENCES office_staff(id),
        staff_version INTEGER NOT NULL CHECK (staff_version >= 1),
        work_order_id TEXT NOT NULL REFERENCES office_work_orders(id),
        route_binding_id TEXT NOT NULL REFERENCES office_route_bindings(id),
        status TEXT NOT NULL CHECK (status IN ('queued','running','waiting','completed','failed','cancelled','interrupted')),
        revision INTEGER NOT NULL CHECK (revision >= 1),
        detail TEXT NOT NULL,
        result_id TEXT,
        parent_assignment_id TEXT,
        depth INTEGER NOT NULL DEFAULT 0,
        prerequisites_json TEXT NOT NULL DEFAULT '[]',
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        UNIQUE (tender_id, route_binding_id)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS office_assignments_tender_order
    ON office_assignments(tender_id, created_at, id)
    """,
    """
    CREATE INDEX IF NOT EXISTS office_assignments_root_status
    ON office_assignments(tender_id, root_run_id, status)
    """,
    """
    CREATE TABLE IF NOT EXISTS office_assignment_receipts (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        root_run_id TEXT NOT NULL REFERENCES runs(id),
        idempotency_key TEXT NOT NULL,
        payload_hash TEXT NOT NULL,
        binding_id TEXT NOT NULL REFERENCES office_route_bindings(id),
        assignment_id TEXT NOT NULL REFERENCES office_assignments(id),
        created_at TEXT NOT NULL,
        UNIQUE (tender_id, root_run_id, idempotency_key)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS office_assignment_receipts_tender
    ON office_assignment_receipts(tender_id, root_run_id)
    """,
    """
    CREATE TABLE IF NOT EXISTS office_assignment_replies (
        reply_message_id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        assignment_id TEXT NOT NULL REFERENCES office_assignments(id),
        queued_revision INTEGER NOT NULL,
        created_at TEXT NOT NULL
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS office_staff_results (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        assignment_id TEXT NOT NULL REFERENCES office_assignments(id),
        payload_hash TEXT NOT NULL,
        payload_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE (tender_id, assignment_id)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS office_staff_results_tender_order
    ON office_staff_results(tender_id, created_at, id)
    """,
    """
    CREATE TRIGGER IF NOT EXISTS office_staff_results_immutable
    BEFORE UPDATE ON office_staff_results
    BEGIN SELECT RAISE(ABORT, 'Staff results are immutable'); END
    """,
    """
    CREATE TRIGGER IF NOT EXISTS office_staff_results_no_delete
    BEFORE DELETE ON office_staff_results
    BEGIN SELECT RAISE(ABORT, 'Staff results are immutable'); END
    """,
)


def _identifier(value: Any, label: str) -> str:
    if not isinstance(value, str) or _IDENTIFIER_RE.fullmatch(value) is None:
        raise ValueError(f"{label} is invalid.")
    return value


def _key(value: Any) -> str:
    if not isinstance(value, str) or _KEY_RE.fullmatch(value) is None:
        raise ValueError("The idempotency key is invalid.")
    return value


def _detail(value: Any, label: str = "a work detail") -> str:
    if not isinstance(value, str) or not value.strip() or len(value) > 2000:
        raise ValueError(f"Enter {label} (up to 2000 characters).")
    return value.strip()


def _limit(offset: Any, limit: Any) -> tuple[int, int]:
    if type(offset) is not int or offset < 0:
        raise ValueError("The assignment offset must be nonnegative.")
    if type(limit) is not int or not 1 <= limit <= 200:
        raise ValueError("The assignment limit must be between 1 and 200.")
    return offset, limit


def _hash(value: Any) -> str:
    return hashlib.sha256(canonical_json(value).encode("utf-8")).hexdigest()


def _json_load(value: str, label: str) -> Any:
    try:
        return json.loads(value)
    except (TypeError, ValueError, json.JSONDecodeError) as error:
        raise RuntimeError(f"Saved {label} contains invalid JSON.") from error


class StaffAssignmentService:
    """Own assignment state, transition events and immutable staff results."""

    def __init__(self, repo):
        if repo is None or not hasattr(repo, "atomic") or not hasattr(repo, "db"):
            raise TypeError("A Repository is required for staff assignments.")
        self.repo = repo
        self.routing = StaffRoutingService(repo)
        self.manager_runs = ManagerRunProfiles(repo)
        self.events = OfficeEventService(repo)
        with repo.atomic() as conn:
            for statement in _SCHEMA_STATEMENTS:
                conn.execute(statement)
            columns = {
                row["name"] for row in conn.execute("PRAGMA table_info(office_assignments)")
            }
            for name, definition in (
                ("parent_assignment_id", "TEXT"),
                ("depth", "INTEGER NOT NULL DEFAULT 0"),
                ("prerequisites_json", "TEXT NOT NULL DEFAULT '[]'"),
            ):
                if name not in columns:
                    conn.execute(f"ALTER TABLE office_assignments ADD COLUMN {name} {definition}")

    @staticmethod
    def _context_shape(context: ManagerCreationContext) -> None:
        if not isinstance(context, ManagerCreationContext):
            raise TypeError("ManagerCreationContext is required for staff assignments.")
        _identifier(context.tender_id, "Tender id")
        _identifier(context.run_id, "Root run id")
        _identifier(context.scope_id, "Plan scope id")
        if type(context.manager_profile_version) is not int or context.manager_profile_version < 1:
            raise ValueError("The Manager profile version is invalid.")

    @staticmethod
    def _tender(conn, tender_id: str) -> None:
        if conn.execute("SELECT 1 FROM tenders WHERE id=?", (tender_id,)).fetchone() is None:
            raise KeyError("This item could not be found in the selected Tender.")

    @staticmethod
    def _assignment_row(conn, tender_id: str, assignment_id: str):
        row = conn.execute(
            "SELECT * FROM office_assignments WHERE tender_id=? AND id=?",
            (tender_id, assignment_id),
        ).fetchone()
        if row is None:
            raise KeyError("This staff assignment could not be found in the selected Tender.")
        return row

    @staticmethod
    def _assignment_from_row(row) -> StaffAssignment:
        return StaffAssignment(
            id=row["id"],
            tender_id=row["tender_id"],
            root_run_id=row["root_run_id"],
            staff_id=row["staff_id"],
            staff_version=row["staff_version"],
            work_order_id=row["work_order_id"],
            route_binding_id=row["route_binding_id"],
            status=row["status"],
            revision=row["revision"],
            detail=row["detail"],
            result_id=row["result_id"],
            parent_assignment_id=row["parent_assignment_id"],
            depth=int(row["depth"] or 0),
            prerequisites=_json_load(row["prerequisites_json"] or "[]", "assignment prerequisites"),
            created_at=row["created_at"],
            updated_at=row["updated_at"],
        )

    @staticmethod
    def _binding_identity(assignment, binding: RouteBinding) -> None:
        pairs = (
            ("Tender", assignment["tender_id"], binding.tender_id),
            ("root run", assignment["root_run_id"], binding.root_run_id),
            ("staff", assignment["staff_id"], binding.staff_id),
            ("staff version", assignment["staff_version"], binding.staff_version),
            ("work order", assignment["work_order_id"], binding.work_order_id),
            ("route binding", assignment["route_binding_id"], binding.id),
        )
        for label, expected, actual in pairs:
            if expected != actual:
                raise OfficeConflict(f"The saved staff assignment has a mismatched {label} identity.")

    def _validate_live_binding(self, conn, assignment):
        binding = self.routing.validate_binding(
            assignment["tender_id"], assignment["route_binding_id"]
        )
        self._binding_identity(assignment, binding)
        return binding

    def _validate_context_binding(
        self, conn, context: ManagerCreationContext, binding_id: str
    ) -> RouteBinding:
        self._context_shape(context)
        binding_id = _identifier(binding_id, "Route binding id")
        binding = self.routing.validate_binding(context.tender_id, binding_id)
        if binding.tender_id != context.tender_id:
            raise OfficeConflict("The route binding belongs to another Tender.")
        if binding.root_run_id != context.run_id:
            raise OfficeConflict("The route binding belongs to another Manager work root.")
        if binding.plan_id != context.scope_id:
            raise OfficeConflict("The route binding belongs to another approved work plan.")
        try:
            pinned = self.manager_runs.get(context.tender_id, context.run_id)
        except (KeyError, ValueError) as error:
            raise ValueError("The delegated work root has no immutable Tender Manager profile pin.") from error
        if pinned.version != context.manager_profile_version:
            raise OfficeConflict("The Manager profile pin changed for this work root.")
        return binding

    @staticmethod
    def _queue_payload(context: ManagerCreationContext, binding_id: str) -> dict[str, Any]:
        return {
            "tender_id": context.tender_id,
            "root_run_id": context.run_id,
            "scope_id": context.scope_id,
            "manager_profile_version": context.manager_profile_version,
            "binding_id": binding_id,
        }

    def _queue_receipt(self, conn, tender_id: str, root_run_id: str, key: str):
        return conn.execute(
            """
            SELECT * FROM office_assignment_receipts
            WHERE tender_id=? AND root_run_id=? AND idempotency_key=?
            """,
            (tender_id, root_run_id, key),
        ).fetchone()

    def queue(
        self,
        context: ManagerCreationContext,
        binding_id: str,
        idempotency_key: str,
    ) -> StaffAssignment:
        """Persist one queued assignment for one immutable route binding."""

        self._context_shape(context)
        binding_id = _identifier(binding_id, "Route binding id")
        key = _key(idempotency_key)
        payload_hash = _hash(self._queue_payload(context, binding_id))
        with self.routing.policy.connections.authority_guard(), self.repo.atomic() as conn:
            existing = self._queue_receipt(conn, context.tender_id, context.run_id, key)
            if existing is not None:
                if existing["payload_hash"] != payload_hash:
                    raise OfficeConflict("This assignment key was already used for different work.")
                row = self._assignment_row(conn, context.tender_id, existing["assignment_id"])
                return self._assignment_from_row(row)

            binding = self._validate_context_binding(conn, context, binding_id)
            from .staff_lifecycle import ensure_staff_can_admit_work
            ensure_staff_can_admit_work(conn, context.tender_id, binding.staff_id)
            consumed = conn.execute(
                "SELECT 1 FROM office_assignments WHERE tender_id=? AND route_binding_id=?",
                (context.tender_id, binding.id),
            ).fetchone()
            if consumed is not None:
                raise OfficeConflict("This exact route binding already has an assignment.")

            # Use the reviewed L10 API directly; the queue must never infer a
            # grant from a legacy plan or a convenience alias.
            grant = self.routing.approved_grant(context.tender_id, context.scope_id)
            assignment_count = conn.execute(
                "SELECT COUNT(*) FROM office_route_bindings WHERE tender_id=? AND root_run_id=?",
                (context.tender_id, context.run_id),
            ).fetchone()[0]
            if assignment_count > grant.envelope.max_assignments:
                raise ValueError("The delegation has reached its maximum assignment count.")

            assignment_id, stamp = new_id(), now()
            conn.execute(
                """
                INSERT INTO office_assignments(
                    id,tender_id,root_run_id,staff_id,staff_version,work_order_id,
                    route_binding_id,status,revision,detail,result_id,
                    parent_assignment_id,depth,prerequisites_json,created_at,updated_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    assignment_id,
                    context.tender_id,
                    context.run_id,
                    binding.staff_id,
                    binding.staff_version,
                    binding.work_order_id,
                    binding.id,
                    "queued",
                    1,
                    "Queued for the reviewed staff route.",
                    None,
                    None,
                    0,
                    dump([]),
                    stamp,
                    stamp,
                ),
            )
            conn.execute(
                """
                INSERT INTO office_assignment_receipts(
                    id,tender_id,root_run_id,idempotency_key,payload_hash,binding_id,assignment_id,created_at
                ) VALUES(?,?,?,?,?,?,?,?)
                """,
                (
                    new_id(),
                    context.tender_id,
                    context.run_id,
                    key,
                    payload_hash,
                    binding.id,
                    assignment_id,
                    stamp,
                ),
            )
            self._append_transition_event(conn, context.tender_id, assignment_id, binding.staff_id, "queued", 1, "Queued for the reviewed staff route.", None)
            return self._assignment_from_row(
                self._assignment_row(conn, context.tender_id, assignment_id)
            )

    def queue_child(
        self,
        context: ManagerCreationContext,
        binding_id: str,
        parent_assignment_id: str,
        prerequisites: list[str],
        idempotency_key: str,
    ) -> StaffAssignment:
        """Queue one descendant assignment under the same root budget owner.

        The child inherits the reviewed root scope and spending allowance; it
        never mints a new budget. Depth is enforced against the reviewed
        delegation envelope, never against a model-supplied cap.
        """

        self._context_shape(context)
        binding_id = _identifier(binding_id, "Route binding id")
        parent_assignment_id = _identifier(parent_assignment_id, "Parent assignment id")
        key = _key(idempotency_key)
        clean_prerequisites = [
            _identifier(prerequisite_id, "Prerequisite assignment id")
            for prerequisite_id in prerequisites or []
        ]
        payload_hash = _hash(
            {
                "tender_id": context.tender_id,
                "root_run_id": context.run_id,
                "binding_id": binding_id,
                "parent_assignment_id": parent_assignment_id,
                "prerequisites": clean_prerequisites,
            }
        )
        with self.routing.policy.connections.authority_guard(), self.repo.atomic() as conn:
            existing = self._queue_receipt(conn, context.tender_id, context.run_id, key)
            if existing is not None:
                if existing["payload_hash"] != payload_hash:
                    raise OfficeConflict("This assignment key was already used for different work.")
                row = self._assignment_row(conn, context.tender_id, existing["assignment_id"])
                return self._assignment_from_row(row)

            binding = self._validate_context_binding(conn, context, binding_id)
            from .staff_lifecycle import ensure_staff_can_admit_work
            ensure_staff_can_admit_work(conn, context.tender_id, binding.staff_id)
            consumed = conn.execute(
                "SELECT 1 FROM office_assignments WHERE tender_id=? AND route_binding_id=?",
                (context.tender_id, binding.id),
            ).fetchone()
            if consumed is not None:
                raise OfficeConflict("This exact route binding already has an assignment.")

            parent = self._assignment_row(conn, context.tender_id, parent_assignment_id)
            if parent["root_run_id"] != context.run_id:
                raise OfficeConflict("A child assignment must stay under its parent work root.")
            if parent["status"] not in {"queued", "running", "waiting"}:
                raise OfficeConflict("Request a child only while the parent assignment is still active.")
            if binding.staff_id != parent["staff_id"]:
                raise OfficeConflict("A child assignment stays with its parent colleague.")
            for prerequisite_id in clean_prerequisites:
                prerequisite = self._assignment_row(conn, context.tender_id, prerequisite_id)
                if prerequisite["root_run_id"] != context.run_id:
                    raise OfficeConflict("A prerequisite must belong to the same work root.")

            grant = self.routing.approved_grant(context.tender_id, context.scope_id)
            depth = int(parent["depth"] or 0) + 1
            if depth > grant.envelope.max_depth:
                raise ValueError(
                    f"Depth {depth} is outside the reviewed cap of {grant.envelope.max_depth}."
                )
            assignment_count = conn.execute(
                "SELECT COUNT(*) AS count FROM office_route_bindings WHERE tender_id=? AND root_run_id=?",
                (context.tender_id, context.run_id),
            ).fetchone()[0]
            if assignment_count > grant.envelope.max_assignments:
                raise ValueError("The delegation has reached its maximum assignment count.")

            assignment_id, stamp = new_id(), now()
            detail = f"Queued as a depth-{depth} child of its parent assignment."
            conn.execute(
                """
                INSERT INTO office_assignments(
                    id,tender_id,root_run_id,staff_id,staff_version,work_order_id,
                    route_binding_id,status,revision,detail,result_id,
                    parent_assignment_id,depth,prerequisites_json,created_at,updated_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    assignment_id,
                    context.tender_id,
                    context.run_id,
                    binding.staff_id,
                    binding.staff_version,
                    binding.work_order_id,
                    binding.id,
                    "queued",
                    1,
                    detail,
                    None,
                    parent_assignment_id,
                    depth,
                    dump(clean_prerequisites),
                    stamp,
                    stamp,
                ),
            )
            conn.execute(
                """
                INSERT INTO office_assignment_receipts(
                    id,tender_id,root_run_id,idempotency_key,payload_hash,binding_id,assignment_id,created_at
                ) VALUES(?,?,?,?,?,?,?,?)
                """,
                (
                    new_id(),
                    context.tender_id,
                    context.run_id,
                    key,
                    payload_hash,
                    binding.id,
                    assignment_id,
                    stamp,
                ),
            )
            self._append_transition_event(conn, context.tender_id, assignment_id, binding.staff_id, "queued", 1, detail, None)
            return self._assignment_from_row(
                self._assignment_row(conn, context.tender_id, assignment_id)
            )

    def get(self, tender_id: str, assignment_id: str) -> StaffAssignment:
        tender_id = _identifier(tender_id, "Tender id")
        assignment_id = _identifier(assignment_id, "Assignment id")
        with self.repo.db.connect() as conn:
            self._tender(conn, tender_id)
            return self._assignment_from_row(self._assignment_row(conn, tender_id, assignment_id))

    def list(
        self,
        tender_id: str,
        *,
        root_run_id: str | None = None,
        staff_id: str | None = None,
        offset: int = 0,
        limit: int = 100,
    ) -> list[StaffAssignment]:
        tender_id = _identifier(tender_id, "Tender id")
        if root_run_id is not None:
            root_run_id = _identifier(root_run_id, "Root run id")
        if staff_id is not None:
            staff_id = _identifier(staff_id, "Staff id")
        offset, limit = _limit(offset, limit)
        with self.repo.db.connect() as conn:
            self._tender(conn, tender_id)
            rows = conn.execute(
                """
                SELECT * FROM office_assignments
                WHERE tender_id=?
                  AND (? IS NULL OR root_run_id=?)
                  AND (? IS NULL OR staff_id=?)
                ORDER BY created_at,id LIMIT ? OFFSET ?
                """,
                (tender_id, root_run_id, root_run_id, staff_id, staff_id, limit, offset),
            ).fetchall()
            return [self._assignment_from_row(row) for row in rows]

    def _append_transition_event(
        self,
        conn,
        tender_id: str,
        assignment_id: str,
        actor_id: str,
        status: str,
        revision: int,
        detail: str,
        result_id: str | None,
    ) -> None:
        event_type = "assignment_queued" if status == "queued" else _TRANSITION_EVENT[status]
        self.events.append(
            tender_id,
            event_type,
            actor_id=actor_id,
            assignment_id=assignment_id,
            record_ref={"assignment_id": assignment_id, "revision": revision},
            payload={
                "status": status,
                "revision": revision,
                "detail": detail,
                "result_id": result_id,
            },
            idempotency_key=f"assignment:{assignment_id}:{event_type}:r{revision}",
        )

    def _transition_live(
        self,
        tender_id: str,
        assignment_id: str,
        expected_revision: int,
        target: str,
        detail: str,
    ) -> StaffAssignment:
        tender_id = _identifier(tender_id, "Tender id")
        assignment_id = _identifier(assignment_id, "Assignment id")
        if type(expected_revision) is not int or expected_revision < 1:
            raise ValueError("The expected assignment revision must be positive.")
        detail = _detail(detail)
        with self.routing.policy.connections.authority_guard(), self.repo.atomic() as conn:
            assignment = self._assignment_row(conn, tender_id, assignment_id)
            if assignment["revision"] != expected_revision:
                raise OfficeConflict("The assignment changed; refresh before changing its state.")
            if target == "running" and assignment["status"] != "queued":
                raise OfficeConflict("Only a queued staff assignment can be started.")
            if target == "waiting" and assignment["status"] != "running":
                raise OfficeConflict("Only a running staff assignment can wait for a reply.")
            if target == "queued" and assignment["status"] != "waiting":
                raise OfficeConflict("Only a waiting staff assignment can continue after a reply.")
            binding = self._validate_live_binding(conn, assignment)
            revision = expected_revision + 1
            conn.execute(
                """
                UPDATE office_assignments
                SET status=?,revision=?,detail=?,updated_at=?
                WHERE tender_id=? AND id=? AND status=? AND revision=?
                """,
                (
                    target,
                    revision,
                    detail,
                    now(),
                    tender_id,
                    assignment_id,
                    assignment["status"],
                    expected_revision,
                ),
            )
            self._append_transition_event(
                conn, tender_id, assignment_id, binding.staff_id, target, revision, detail, None
            )
            return self._assignment_from_row(self._assignment_row(conn, tender_id, assignment_id))

    def start(self, tender_id: str, assignment_id: str, expected_revision: int) -> StaffAssignment:
        return self._transition_live(
            tender_id,
            assignment_id,
            expected_revision,
            "running",
            "Started under the reviewed staff route.",
        )

    def wait_for_reply(
        self,
        tender_id: str,
        assignment_id: str,
        expected_revision: int,
        detail: str,
    ) -> StaffAssignment:
        return self._transition_live(tender_id, assignment_id, expected_revision, "waiting", detail)

    def resume_after_reply(self, tender_id: str, assignment_id: str, reply_message_id: str) -> StaffAssignment:
        """Continue only after the pinned Manager answers the latest actual question."""
        reply_message_id = _identifier(reply_message_id, "Reply message id")
        with self.routing.policy.connections.authority_guard(), self.repo.atomic() as conn:
            assignment = self.get(tender_id, assignment_id)
            prior = conn.execute("SELECT * FROM office_assignment_replies WHERE reply_message_id=?", (reply_message_id,)).fetchone()
            if prior is not None:
                if prior["tender_id"] != tender_id or prior["assignment_id"] != assignment_id:
                    raise OfficeConflict("This reply belongs to a different staff assignment.")
                return assignment
            if assignment.status != "waiting":
                raise OfficeConflict("Only a waiting staff assignment can continue after a reply.")
            manager = self.manager_runs.get(tender_id, assignment.root_run_id)
            reply = conn.execute("SELECT * FROM office_messages WHERE tender_id=? AND id=?", (tender_id, reply_message_id)).fetchone()
            question = conn.execute(
                "SELECT * FROM office_messages WHERE tender_id=? AND root_run_id=? AND sender_assignment_id=? AND sender_kind='staff' AND kind='question' ORDER BY rowid DESC LIMIT 1",
                (tender_id, assignment.root_run_id, assignment_id),
            ).fetchone()
            if (reply is None or question is None or reply["kind"] != "reply"
                    or reply["root_run_id"] != assignment.root_run_id
                    or reply["sender_kind"] != "manager" or reply["sender_id"] != manager.id
                    or reply["sender_version"] != manager.version
                    or reply["reply_to"] != question["id"]
                    or question["sender_id"] != assignment.staff_id
                    or question["sender_version"] != assignment.staff_version
                    or reply["created_at"] < assignment.updated_at):
                raise ValueError("A current Manager reply to this assignment's latest question is required.")
            target = conn.execute(
                "SELECT 1 FROM office_message_recipients WHERE message_id=? AND tender_id=? AND recipient_id=? AND recipient_version=? AND assignment_id=?",
                (reply_message_id, tender_id, assignment.staff_id, assignment.staff_version, assignment_id),
            ).fetchone()
            if target is None:
                raise ValueError("The Manager's question reply is not addressed to this assignment.")
            queued = self._transition_live(tender_id, assignment_id, assignment.revision, "queued", "The Manager's reply is saved. This assignment is queued to continue.")
            conn.execute("INSERT INTO office_assignment_replies VALUES(?,?,?,?,?)", (reply_message_id, tender_id, assignment_id, queued.revision, now()))
            return queued

    def _terminal_transition(
        self,
        tender_id: str,
        assignment_id: str,
        target: str,
        detail: str,
    ) -> StaffAssignment:
        tender_id = _identifier(tender_id, "Tender id")
        assignment_id = _identifier(assignment_id, "Assignment id")
        with self.repo.atomic() as conn:
            assignment = self._assignment_row(conn, tender_id, assignment_id)
            if assignment["status"] == target:
                return self._assignment_from_row(assignment)
            if assignment["status"] in _TERMINAL_STATUSES:
                raise OfficeConflict("The staff assignment already has another terminal state.")
            detail = _detail(detail)
            revision = assignment["revision"] + 1
            conn.execute(
                """
                UPDATE office_assignments
                SET status=?,revision=?,detail=?,updated_at=?
                WHERE tender_id=? AND id=? AND status=? AND revision=?
                """,
                (
                    target,
                    revision,
                    detail,
                    now(),
                    tender_id,
                    assignment_id,
                    assignment["status"],
                    assignment["revision"],
                ),
            )
            self._append_transition_event(
                conn,
                tender_id,
                assignment_id,
                assignment["staff_id"],
                target,
                revision,
                detail,
                None,
            )
            return self._assignment_from_row(self._assignment_row(conn, tender_id, assignment_id))

    def fail(self, tender_id: str, assignment_id: str, detail: str) -> StaffAssignment:
        return self._terminal_transition(tender_id, assignment_id, "failed", detail)

    def cancel(
        self,
        tender_id: str,
        assignment_id: str,
        detail: str = "Stopped at the engineer’s request.",
    ) -> StaffAssignment:
        return self._terminal_transition(tender_id, assignment_id, "cancelled", detail)

    def interrupt_root(self, tender_id: str, root_run_id: str, detail: str) -> list[StaffAssignment]:
        tender_id = _identifier(tender_id, "Tender id")
        root_run_id = _identifier(root_run_id, "Root run id")
        detail = _detail(detail, "an interruption detail")
        with self.repo.atomic() as conn:
            self._tender(conn, tender_id)
            root = conn.execute(
                "SELECT 1 FROM runs WHERE tender_id=? AND id=?", (tender_id, root_run_id)
            ).fetchone()
            if root is None:
                raise KeyError("This work root could not be found in the selected Tender.")
            rows = conn.execute(
                """
                SELECT * FROM office_assignments
                WHERE tender_id=? AND root_run_id=? AND status IN ('queued','running','waiting')
                ORDER BY created_at,id
                """,
                (tender_id, root_run_id),
            ).fetchall()
            for assignment in rows:
                revision = assignment["revision"] + 1
                conn.execute(
                    """
                    UPDATE office_assignments
                    SET status='interrupted',revision=?,detail=?,updated_at=?
                    WHERE tender_id=? AND id=? AND status IN ('queued','running','waiting') AND revision=?
                    """,
                    (revision, detail, now(), tender_id, assignment["id"], assignment["revision"]),
                )
                self._append_transition_event(
                    conn,
                    tender_id,
                    assignment["id"],
                    assignment["staff_id"],
                    "interrupted",
                    revision,
                    detail,
                    None,
                )
            return [
                self._assignment_from_row(self._assignment_row(conn, tender_id, row["id"]))
                for row in rows
            ]

    def recover_interrupted(self) -> int:
        """Mark abandoned assignments after the parent runs are recovered."""

        with self.repo.atomic() as conn:
            rows = conn.execute(
                """
                SELECT a.* FROM office_assignments a
                JOIN runs r ON r.id=a.root_run_id AND r.tender_id=a.tender_id
                WHERE a.status IN ('queued','running','waiting')
                  AND r.status IN ('completed','failed','cancelled','interrupted')
                ORDER BY a.created_at,a.id
                """
            ).fetchall()
            for assignment in rows:
                revision = assignment["revision"] + 1
                detail = "The parent work root ended before this assignment finished. Review and resume under a new active root."
                conn.execute(
                    """
                    UPDATE office_assignments
                    SET status='interrupted',revision=?,detail=?,updated_at=?
                    WHERE tender_id=? AND id=? AND status IN ('queued','running','waiting') AND revision=?
                    """,
                    (revision, detail, now(), assignment["tender_id"], assignment["id"], assignment["revision"]),
                )
                self._append_transition_event(
                    conn,
                    assignment["tender_id"],
                    assignment["id"],
                    assignment["staff_id"],
                    "interrupted",
                    revision,
                    detail,
                    None,
                )
            return len(rows)

    @staticmethod
    def _prepared_payload(draft: PreparedStaffDraft) -> dict[str, Any]:
        prepared = draft.prepared
        return {
            "assignment_id": draft.assignment_id,
            "staff_id": draft.staff_id,
            "staff_version": draft.staff_version,
            "route_binding_id": draft.route_binding_id,
            "prepared": {
                "tender_id": prepared.tender_id,
                "run_id": prepared.run_id,
                "actor_id": prepared.actor_id,
                "staff_version": prepared.staff_version,
                "prepared_assignment_id": prepared.assignment_id,
                "prepared_route_binding_id": prepared.route_binding_id,
                "output": prepared.output.model_dump(mode="json"),
                "usage": prepared.usage,
                "source_ids_read": list(prepared.source_ids_read),
                "web_sources": list(prepared.web_sources),
                "item_bases": [list(item) for item in prepared.item_bases],
                "trusted_recipients": list(prepared.trusted_recipients),
                "source_recipients": [
                    [source_id, list(addresses)]
                    for source_id, addresses in prepared.source_recipients
                ],
                "approved_plan_id": prepared.approved_plan_id,
            },
            "authored_notes": list(draft.authored_notes),
        }

    @classmethod
    def _draft_hash(cls, draft: PreparedStaffDraft) -> str:
        return _hash(cls._prepared_payload(draft))

    @staticmethod
    def _output_permissions(output, binding: RouteBinding) -> None:
        values = output.model_dump(mode="json")
        allowed = set(binding.allowed_draft_outputs)
        for field, value in values.items():
            if field in _DRAFT_INTRINSIC_FIELDS:
                continue
            nonempty = value not in (None, [], {}, "")
            if nonempty and field not in allowed:
                raise ValueError(
                    f"The staff draft output '{field}' is outside the reviewed delegation."
                )

    @staticmethod
    def _source_bases(conn, tender_id: str, source_ids, binding: RouteBinding) -> tuple[StaffSourceBasis, ...]:
        allowed = {item.artifact_id: item for item in binding.artifacts}
        bases: list[StaffSourceBasis] = []
        seen: set[str] = set()
        for source_id in source_ids:
            source_id = _identifier(source_id, "Source id")
            if source_id in seen:
                raise ValueError("A staff draft cannot repeat a read source.")
            seen.add(source_id)
            row = conn.execute(
                """
                SELECT e.id AS source_id,e.locator,a.id AS artifact_id,a.version,a.content_hash,a.is_current
                FROM evidence e JOIN artifacts a ON a.id=e.artifact_id
                WHERE a.tender_id=? AND e.id=?
                """,
                (tender_id, source_id),
            ).fetchone()
            if row is None:
                raise ValueError("A staff draft read source does not belong to this Tender.")
            expected = allowed.get(row["artifact_id"])
            if expected is None:
                raise ValueError("A staff draft read source is outside the reviewed delegation scope.")
            if (
                not row["is_current"]
                or row["version"] != expected.version
                or row["content_hash"].lower() != expected.content_hash.lower()
            ):
                raise ValueError("A staff draft read source changed from the reviewed delegation scope.")
            bases.append(
                StaffSourceBasis(
                    source_id=source_id,
                    artifact_id=row["artifact_id"],
                    artifact_version=row["version"],
                    artifact_hash=row["content_hash"].lower(),
                    locator=row["locator"],
                )
            )
        return tuple(bases)

    def _validate_item_bases(self, tender_id: str, item_bases) -> None:
        """Recheck every commercial basis captured by the staff context."""

        if not item_bases:
            return
        from .estimates import EstimateService

        estimates = EstimateService(self.repo)
        for item_id, expected in item_bases:
            if not isinstance(item_id, str) or not isinstance(expected, str):
                raise ValueError("A staff draft has an invalid BOQ item basis.")
            try:
                actual = estimates.rate_basis(tender_id, item_id)["fingerprint"]
            except (KeyError, ValueError) as error:
                raise ValueError("The BOQ item basis is no longer available. Review the current estimate.") from error
            if actual != expected:
                raise ValueError("The BOQ item changed from the basis read by this staff assignment.")

    def _result_payload(
        self,
        draft: PreparedStaffDraft,
        assignment,
        source_bases: tuple[StaffSourceBasis, ...],
        result_id: str,
        created_at: str,
    ) -> dict[str, Any]:
        prepared = draft.prepared
        basis_json = [basis.model_dump(mode="json") for basis in source_bases]
        return {
            "id": result_id,
            "tender_id": assignment["tender_id"],
            "assignment_id": assignment["id"],
            "root_run_id": assignment["root_run_id"],
            "staff_id": assignment["staff_id"],
            "staff_version": assignment["staff_version"],
            "work_order_id": assignment["work_order_id"],
            "route_binding_id": assignment["route_binding_id"],
            "office_output": prepared.output.model_dump(mode="json"),
            "authored_notes": list(draft.authored_notes),
            "source_ids_read": list(prepared.source_ids_read),
            "source_bases": basis_json,
            "web_sources": list(prepared.web_sources),
            "item_bases": [list(item) for item in prepared.item_bases],
            "trusted_recipients": list(prepared.trusted_recipients),
            "source_recipients": [
                [source_id, list(addresses)]
                for source_id, addresses in prepared.source_recipients
            ],
            "approved_plan_id": prepared.approved_plan_id,
            "usage": prepared.usage,
            "created_at": created_at,
            "currentness": "current",
        }

    def _currentness(self, conn, tender_id: str, payload: dict[str, Any]) -> str:
        for basis in payload.get("source_bases", []):
            row = conn.execute(
                """
                SELECT a.version,a.content_hash,a.is_current,e.id,e.locator
                FROM evidence e JOIN artifacts a ON a.id=e.artifact_id
                WHERE a.tender_id=? AND e.id=?
                """,
                (tender_id, basis.get("source_id")),
            ).fetchone()
            if row is None or (
                not row["is_current"]
                or row["version"] != basis.get("artifact_version")
                or row["content_hash"].lower() != str(basis.get("artifact_hash", "")).lower()
                or row["locator"] != basis.get("locator")
            ):
                return "needs_review"
        # Commercial bases are fingerprints captured by the existing office
        # tools.  A missing/currently changed BOQ item makes the result stale,
        # while a terminal parent run by itself does not.
        if payload.get("item_bases"):
            try:
                # Avoid constructing a service that can create tables during a
                # historical read; compare only when the existing table is present.
                has_items = conn.execute(
                    "SELECT 1 FROM sqlite_master WHERE type='table' AND name='boq_items'"
                ).fetchone()
                if not has_items:
                    return "needs_review"
                if has_items:
                    from .estimates import EstimateService

                    # ``rate_basis`` is read-only; bypass the EstimateService
                    # constructor here so a historical result GET never runs
                    # schema DDL as a side effect.
                    service = EstimateService.__new__(EstimateService)
                    service.repo = self.repo
                    for item_id, fingerprint in payload["item_bases"]:
                        if service.rate_basis(tender_id, item_id)["fingerprint"] != fingerprint:
                            return "needs_review"
            except (ImportError, KeyError, ValueError, sqlite3.Error):
                return "needs_review"
        return "current"

    def _result_from_row(self, conn, row) -> StaffResult:
        payload = _json_load(row["payload_json"], "staff result")
        payload["currentness"] = self._currentness(conn, row["tender_id"], payload)
        return StaffResult.model_validate(payload)

    def get_result(self, tender_id: str, result_id: str) -> StaffResult:
        tender_id = _identifier(tender_id, "Tender id")
        result_id = _identifier(result_id, "Result id")
        with self.repo.db.connect() as conn:
            self._tender(conn, tender_id)
            row = conn.execute(
                "SELECT * FROM office_staff_results WHERE tender_id=? AND id=?",
                (tender_id, result_id),
            ).fetchone()
            if row is None:
                raise KeyError("This staff result could not be found in the selected Tender.")
            return self._result_from_row(conn, row)

    def save_result(
        self,
        tender_id: str,
        assignment_id: str,
        draft: PreparedStaffDraft,
        *,
        checkpoint_input_fingerprint: str | None = None,
    ) -> StaffResult:
        """Validate and atomically save one completed staff draft."""

        tender_id = _identifier(tender_id, "Tender id")
        assignment_id = _identifier(assignment_id, "Assignment id")
        if not isinstance(draft, PreparedStaffDraft):
            raise TypeError("PreparedStaffDraft is required for staff result storage.")
        draft_hash = self._draft_hash(draft)
        with self.routing.policy.connections.authority_guard(), self.repo.atomic() as conn:
            assignment = self._assignment_row(conn, tender_id, assignment_id)
            if assignment["result_id"] is not None:
                existing = conn.execute(
                    "SELECT * FROM office_staff_results WHERE tender_id=? AND id=?",
                    (tender_id, assignment["result_id"]),
                ).fetchone()
                if existing is None:
                    raise RuntimeError("The assignment points to a missing immutable staff result.")
                if existing["payload_hash"] != draft_hash:
                    raise OfficeConflict("This assignment already has a different saved staff result.")
                return self._result_from_row(conn, existing)
            if assignment["status"] != "running":
                raise OfficeConflict("Only a running staff assignment can save a result.")
            if (
                draft.assignment_id != assignment["id"]
                or draft.staff_id != assignment["staff_id"]
                or draft.staff_version != assignment["staff_version"]
                or draft.route_binding_id != assignment["route_binding_id"]
            ):
                raise OfficeConflict("The prepared staff draft identity does not match its assignment.")
            prepared = draft.prepared
            if prepared.tender_id != tender_id or prepared.run_id != assignment["root_run_id"]:
                raise OfficeConflict("The prepared staff draft belongs to a different Tender or root run.")
            if (
                prepared.actor_id != assignment["staff_id"]
                or prepared.staff_version != assignment["staff_version"]
                or prepared.assignment_id != assignment["id"]
                or prepared.route_binding_id != assignment["route_binding_id"]
            ):
                raise OfficeConflict("The prepared result actor identity does not match its assignment.")

            binding = self._validate_live_binding(conn, assignment)
            if prepared.approved_plan_id != binding.plan_id:
                raise OfficeConflict("The prepared staff draft belongs to a different approved work plan.")
            self._output_permissions(prepared.output, binding)
            source_bases = self._source_bases(conn, tender_id, prepared.source_ids_read, binding)
            self._validate_item_bases(tender_id, prepared.item_bases)
            # This is the pure validation projection.  It checks source,
            # research, commercial and output bases and performs no publishing.
            from .office import validate_prepared

            validate_prepared(self.repo, prepared)
            result_id, stamp = new_id(), now()
            payload = self._result_payload(draft, assignment, source_bases, result_id, stamp)
            payload_hash = draft_hash
            conn.execute(
                """
                INSERT INTO office_staff_results(
                    id,tender_id,assignment_id,payload_hash,payload_json,created_at
                ) VALUES(?,?,?,?,?,?)
                """,
                (
                    result_id,
                    tender_id,
                    assignment_id,
                    payload_hash,
                    dump(payload),
                    stamp,
                ),
            )
            revision = assignment["revision"] + 1
            detail = "Completed draft saved for Manager review."
            conn.execute(
                """
                UPDATE office_assignments
                SET status='completed',revision=?,detail=?,result_id=?,updated_at=?
                WHERE tender_id=? AND id=? AND status='running' AND revision=? AND result_id IS NULL
                """,
                (
                    revision,
                    detail,
                    result_id,
                    stamp,
                    tender_id,
                    assignment_id,
                    assignment["revision"],
                ),
            )
            self._append_transition_event(
                conn,
                tender_id,
                assignment_id,
                assignment["staff_id"],
                "completed",
                revision,
                detail,
                result_id,
            )
            from .office_checkpoints import OfficeCheckpointService

            if checkpoint_input_fingerprint is not None:
                OfficeCheckpointService(self.repo).checkpoint_staff_result(
                    tender_id, result_id, binding, checkpoint_input_fingerprint
                )
            return self._result_from_row(
                conn,
                conn.execute(
                    "SELECT * FROM office_staff_results WHERE tender_id=? AND id=?",
                    (tender_id, result_id),
                ).fetchone(),
            )


__all__ = ["StaffAssignmentService"]
