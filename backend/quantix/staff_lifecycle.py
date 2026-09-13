"""Manager-authorized staff identity lifecycle. Assignment activity stays separate."""

from __future__ import annotations

from .db import dump, new_id, now
from .execution_context import OfficeExecutionIdentity
from .staff_assignments import StaffAssignmentService
from .staff_lifecycle_models import (
    StaffLifecycleReceipt,
    StaffLifecycleRequest,
    StaffPreferenceRequest,
)
from .staff_models import OfficeConflict, StaffProfileDraft
from .staff_store import StaffStore, _canonical_hash, _key
from .staff_store_models import StaffProfileRecord

_ACTIVE_ASSIGNMENTS = ("queued", "running", "waiting")
_ALLOWED = {
    "available": {"retired", "archived"},
    "retired": {"available", "archived"},
    "archived": {"available"},
    "planned": {"available", "retired", "archived"},
}
_BUSY = (
    "This colleague still has active work. Stop or transfer that work before changing their identity state."
)
_ADMISSION_BLOCKED = (
    "This colleague is no longer available for new work. Reactivate the identity before assigning work."
)


_RECEIPT_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS office_staff_lifecycle_receipts (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        staff_id TEXT NOT NULL REFERENCES office_staff(id),
        operation TEXT NOT NULL,
        idempotency_key TEXT NOT NULL,
        payload_hash TEXT NOT NULL,
        staff_version INTEGER NOT NULL CHECK (staff_version >= 1),
        created_at TEXT NOT NULL,
        UNIQUE (tender_id, staff_id, operation, idempotency_key)
    )
    """,
)


def _identity_tender(ctx: OfficeExecutionIdentity) -> str:
    if not ctx.tender_id:
        raise ValueError("Staff lifecycle changes require a selected Tender.")
    if ctx.actor_kind not in {"engineer", "manager"}:
        raise ValueError("Only the engineer or Tender Manager can change a colleague's identity state.")
    return ctx.tender_id


def ensure_staff_can_admit_work(
    conn,
    tender_id: str,
    staff_id: str,
    *,
    assignment_id: str | None = None,
):
    """Guard new work admission against the mutable current identity state.

    Existing route bindings and assignments retain their pinned profile version;
    this helper is for the synchronous boundary that would admit *new* work.
    The caller must invoke it inside its existing authority lock and SQLite
    transaction so retirement cannot race route admission.
    """

    row = conn.execute(
        "SELECT id,tender_id,current_version,lifecycle FROM office_staff WHERE tender_id=? AND id=?",
        (tender_id, staff_id),
    ).fetchone()
    if row is None:
        raise KeyError("This item could not be found in the selected Tender.")
    if row["lifecycle"] not in {"available", "planned"}:
        raise OfficeConflict(_ADMISSION_BLOCKED)
    if assignment_id is not None:
        assignment = conn.execute(
            "SELECT tender_id,staff_id FROM office_assignments WHERE id=?",
            (assignment_id,),
        ).fetchone()
        if assignment is None or assignment["tender_id"] != tender_id or assignment["staff_id"] != staff_id:
            raise OfficeConflict("The saved staff assignment does not belong to this colleague.")
    return row


class StaffLifecycleService:
    def __init__(self, repo):
        self.repo = repo
        self.store = StaffStore(repo)
        self.assignments = StaffAssignmentService(repo)
        with repo.atomic() as conn:
            for statement in _RECEIPT_SCHEMA:
                conn.execute(statement)

    def _busy(self, conn, tender_id: str, staff_id: str) -> bool:
        table = conn.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='office_assignments'"
        ).fetchone()
        if table is None:
            return False
        row = conn.execute(
            f"""
            SELECT 1 FROM office_assignments
            WHERE tender_id=? AND staff_id=? AND status IN ({",".join("?" for _ in _ACTIVE_ASSIGNMENTS)})
            """,
            (tender_id, staff_id, *_ACTIVE_ASSIGNMENTS),
        ).fetchone()
        return row is not None

    def _replay(self, conn, tender_id: str, staff_id: str, operation: str, key: str, payload_hash: str):
        existing = conn.execute(
            """
            SELECT payload_hash,staff_version FROM office_staff_lifecycle_receipts
            WHERE tender_id=? AND staff_id=? AND operation=? AND idempotency_key=?
            """,
            (tender_id, staff_id, operation, key),
        ).fetchone()
        if existing is None:
            return None
        if existing["payload_hash"] != payload_hash:
            raise OfficeConflict("This lifecycle key was already used for a different request.")
        staff = self.store._staff_record(conn, tender_id, staff_id, existing["staff_version"])
        return StaffLifecycleReceipt(staff=staff, replayed=True)

    def _write_receipt(self, conn, tender_id, staff_id, operation, key, payload_hash, version):
        conn.execute(
            """
            INSERT INTO office_staff_lifecycle_receipts(
                id,tender_id,staff_id,operation,idempotency_key,payload_hash,staff_version,created_at
            ) VALUES(?,?,?,?,?,?,?,?)
            """,
            (new_id(), tender_id, staff_id, operation, key, payload_hash, version, now()),
        )

    def transition(self, ctx: OfficeExecutionIdentity, request: StaffLifecycleRequest) -> StaffLifecycleReceipt:
        tender_id = _identity_tender(ctx)
        key = _key(request.idempotency_key)
        payload_hash = _canonical_hash(request.model_dump(mode="json"))
        with self.repo.atomic() as conn:
            replayed = self._replay(conn, tender_id, request.staff_id, "transition", key, payload_hash)
            if replayed is not None:
                return replayed
            row = conn.execute(
                "SELECT id,current_version,lifecycle FROM office_staff WHERE tender_id=? AND id=?",
                (tender_id, request.staff_id),
            ).fetchone()
            if row is None:
                raise KeyError("This item could not be found in the selected Tender.")
            if row["current_version"] != request.expected_version:
                raise OfficeConflict(
                    f"The staff profile changed from version {request.expected_version}; refresh before continuing."
                )
            current = row["lifecycle"]
            if current == "planned":
                current = "available"
            if current == request.target:
                staff = self.store._staff_record(conn, tender_id, request.staff_id, row["current_version"])
                self._write_receipt(
                    conn, tender_id, request.staff_id, "transition", key, payload_hash, row["current_version"]
                )
                return StaffLifecycleReceipt(staff=staff, replayed=False)
            if request.target not in _ALLOWED.get(current, set()):
                raise ValueError(
                    f"A colleague who is {current} cannot be moved to {request.target}."
                )
            if request.target in {"retired", "archived"} and self._busy(conn, tender_id, request.staff_id):
                raise OfficeConflict(_BUSY)
            current_staff = self.store._staff_record(
                conn, tender_id, request.staff_id, request.expected_version
            )
            payload = current_staff.model_dump(mode="json")
            for name in (
                "id",
                "tender_id",
                "version",
                "creator_run_id",
                "manager_profile_version",
                "created_at",
                "updated_at",
                "lifecycle",
                "portrait",
                "definition_id",
                "definition_version",
            ):
                payload.pop(name, None)
            next_version, stamp = request.expected_version + 1, now()
            draft = StaffProfileDraft.model_validate(payload)
            stored = draft.model_dump(mode="json")
            stored["portrait"] = current_staff.portrait.model_dump(mode="json")
            conn.execute(
                """
                INSERT INTO office_staff_versions(
                    staff_id,tender_id,version,profile_json,creator_run_id,
                    manager_profile_version,lifecycle,created_at,updated_at
                ) VALUES(?,?,?,?,?,?,?,?,?)
                """,
                (
                    request.staff_id,
                    tender_id,
                    next_version,
                    dump(stored),
                    current_staff.creator_run_id,
                    current_staff.manager_profile_version,
                    request.target,
                    stamp,
                    stamp,
                ),
            )
            changed = conn.execute(
                """
                UPDATE office_staff SET lifecycle=?,current_version=?,updated_at=?
                WHERE tender_id=? AND id=? AND current_version=?
                """,
                (request.target, next_version, stamp, tender_id, request.staff_id, request.expected_version),
            ).rowcount
            if changed != 1:
                raise OfficeConflict("The staff profile changed; refresh before continuing.")
            self._write_receipt(
                conn, tender_id, request.staff_id, "transition", key, payload_hash, next_version
            )
            staff = self.store._staff_record(conn, tender_id, request.staff_id, next_version)
            return StaffLifecycleReceipt(staff=staff, replayed=False)

    def revise_preferences(
        self, ctx: OfficeExecutionIdentity, request: StaffPreferenceRequest
    ) -> StaffProfileRecord:
        tender_id = _identity_tender(ctx)
        key = _key(request.idempotency_key)
        payload_hash = _canonical_hash(request.model_dump(mode="json"))
        with self.repo.atomic() as conn:
            replayed = self._replay(
                conn, tender_id, request.staff_id, "revise_preferences", key, payload_hash
            )
            if replayed is not None:
                return replayed.staff
            current = self.store._staff_record(conn, tender_id, request.staff_id, request.expected_version)
            if current.version != request.expected_version:
                raise OfficeConflict(
                    f"The staff profile changed from version {request.expected_version}; refresh before continuing."
                )
            payload = current.model_dump(mode="json")
            for name in (
                "id",
                "tender_id",
                "version",
                "creator_run_id",
                "manager_profile_version",
                "created_at",
                "updated_at",
                "lifecycle",
                "portrait",
                "definition_id",
                "definition_version",
            ):
                payload.pop(name, None)
            personality = payload.get("personality") or {}
            personality["working_habits"] = list(request.preferences)
            payload["personality"] = personality
            draft = StaffProfileDraft.model_validate(payload)
            next_version, stamp = request.expected_version + 1, now()
            stored = draft.model_dump(mode="json")
            stored["portrait"] = current.portrait.model_dump(mode="json")
            conn.execute(
                """
                INSERT INTO office_staff_versions(
                    staff_id,tender_id,version,profile_json,creator_run_id,
                    manager_profile_version,lifecycle,created_at,updated_at
                ) VALUES(?,?,?,?,?,?,?,?,?)
                """,
                (
                    request.staff_id,
                    tender_id,
                    next_version,
                    dump(stored),
                    current.creator_run_id,
                    current.manager_profile_version,
                    current.lifecycle if current.lifecycle != "planned" else "available",
                    stamp,
                    stamp,
                ),
            )
            changed = conn.execute(
                """
                UPDATE office_staff SET current_version=?,updated_at=?
                WHERE tender_id=? AND id=? AND current_version=?
                """,
                (next_version, stamp, tender_id, request.staff_id, request.expected_version),
            ).rowcount
            if changed != 1:
                raise OfficeConflict("The staff profile changed; refresh before continuing.")
            self._write_receipt(
                conn, tender_id, request.staff_id, "revise_preferences", key, payload_hash, next_version
            )
            # Existing work orders keep their original staff_version.
            return self.store._staff_record(conn, tender_id, request.staff_id, next_version)
