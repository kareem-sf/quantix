"""Tender-scoped persistence for Manager-generated staff and assignments."""

from __future__ import annotations

import hashlib
import json
import re

from .db import dump, new_id, now
from .manager_profile import ManagerProfileService
from .staff_models import (
    ManagerCreationContext,
    OfficeConflict,
    StaffProfileDraft,
    StaffWorkOrder,
)
from .staff_store_models import (
    StaffCreationReceipt,
    StaffProfileRecord,
    StaffRevisionReceipt,
    StaffWorkOrderReceipt,
    StaffWorkOrderRecord,
)

_SCHEMA_STATEMENTS = (
    """
    CREATE TABLE IF NOT EXISTS office_staff (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        current_version INTEGER NOT NULL CHECK (current_version >= 1),
        lifecycle TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS office_staff_versions (
        staff_id TEXT NOT NULL REFERENCES office_staff(id),
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        version INTEGER NOT NULL CHECK (version >= 1),
        profile_json TEXT NOT NULL,
        creator_run_id TEXT NOT NULL,
        manager_profile_version INTEGER NOT NULL CHECK (manager_profile_version >= 1),
        lifecycle TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (staff_id, version)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS office_staff_tender_order
    ON office_staff(tender_id, created_at, id)
    """,
    """
    CREATE INDEX IF NOT EXISTS office_staff_versions_tender
    ON office_staff_versions(tender_id, staff_id, version)
    """,
    """
    CREATE TABLE IF NOT EXISTS office_work_orders (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        staff_id TEXT NOT NULL REFERENCES office_staff(id),
        staff_version INTEGER NOT NULL CHECK (staff_version >= 1),
        creator_run_id TEXT NOT NULL,
        scope_id TEXT NOT NULL,
        work_order_json TEXT NOT NULL,
        created_at TEXT NOT NULL
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS office_work_orders_tender_order
    ON office_work_orders(tender_id, created_at, id)
    """,
    """
    CREATE TABLE IF NOT EXISTS office_generation_receipts (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        creator_run_id TEXT NOT NULL,
        operation TEXT NOT NULL,
        idempotency_key TEXT NOT NULL,
        payload_hash TEXT NOT NULL,
        staff_id TEXT NOT NULL REFERENCES office_staff(id),
        staff_version INTEGER NOT NULL CHECK (staff_version >= 1),
        work_order_id TEXT REFERENCES office_work_orders(id),
        created_at TEXT NOT NULL,
        UNIQUE (tender_id, creator_run_id, operation, idempotency_key)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS office_generation_receipts_tender
    ON office_generation_receipts(tender_id, creator_run_id, operation)
    """,
    """
    CREATE TABLE IF NOT EXISTS office_staff_definition_sources (
        staff_id TEXT PRIMARY KEY REFERENCES office_staff(id),
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        definition_id TEXT NOT NULL,
        definition_version INTEGER NOT NULL CHECK (definition_version >= 1),
        created_at TEXT NOT NULL,
        UNIQUE(tender_id,staff_id,definition_id,definition_version)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS office_staff_definition_lookup
    ON office_staff_definition_sources(tender_id,definition_id,definition_version)
    """,
)

_IDENTIFIER_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.:/-]{0,159}\Z")
_KEY_RE = re.compile(r"[A-Za-z0-9._~-]{1,160}\Z")
_ACTIVE_RUN_STATUSES = {"queued", "running"}


def _identifier(value: str, label: str) -> str:
    if not isinstance(value, str) or _IDENTIFIER_RE.fullmatch(value) is None:
        raise ValueError(
            f"{label} must contain 1 to 160 permitted identifier characters."
        )
    return value


def _key(value: str) -> str:
    if not isinstance(value, str) or _KEY_RE.fullmatch(value) is None:
        raise ValueError("The idempotency key must contain 1 to 160 permitted identifier characters.")
    return value


def _canonical_hash(value: dict) -> str:
    encoded = json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
        allow_nan=False,
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


class StaffStore:
    """Persist generated staff without granting execution authority."""

    def __init__(self, repo):
        self.repo = repo
        # L01 owns the Manager schema and profile validation. Keeping a service
        # reference here makes version existence explicit for every mutation.
        self.manager_profiles = ManagerProfileService(repo)
        with repo.atomic() as conn:
            for statement in _SCHEMA_STATEMENTS:
                conn.execute(statement)

    @staticmethod
    def _profile(value: StaffProfileDraft | dict) -> StaffProfileDraft:
        if isinstance(value, StaffProfileDraft):
            return value
        return StaffProfileDraft.model_validate(value)

    @staticmethod
    def _work_order(value: StaffWorkOrder | dict) -> StaffWorkOrder:
        if isinstance(value, StaffWorkOrder):
            return value
        return StaffWorkOrder.model_validate(value)

    @staticmethod
    def _tender(conn, tender_id: str) -> None:
        if conn.execute("SELECT 1 FROM tenders WHERE id=?", (tender_id,)).fetchone() is None:
            raise KeyError("This item could not be found in the selected Tender.")

    def _validate_context(self, conn, context: ManagerCreationContext) -> None:
        self._validate_context_shape(context)
        self._tender(conn, context.tender_id)
        run = conn.execute(
            "SELECT tender_id,status FROM runs WHERE id=? AND tender_id=?",
            (context.run_id, context.tender_id),
        ).fetchone()
        if run is None:
            raise KeyError("The Manager creation run could not be found in this Tender.")
        if run["status"] not in _ACTIVE_RUN_STATUSES:
            raise OfficeConflict("The Manager creation run is no longer active.")

        try:
            self.manager_profiles.profile_version(context.manager_profile_version)
        except KeyError:
            raise KeyError("The requested Tender Manager profile version could not be found.") from None

    @staticmethod
    def _validate_context_shape(context: ManagerCreationContext) -> None:
        if not isinstance(context, ManagerCreationContext):
            raise TypeError("ManagerCreationContext is required for generated staff.")
        _identifier(context.tender_id, "Tender id")
        _identifier(context.run_id, "Run id")
        _identifier(context.scope_id, "Scope id")
        if type(context.manager_profile_version) is not int or context.manager_profile_version < 1:
            raise ValueError("The Manager profile version must be a positive integer.")

    @staticmethod
    def _receipt(conn, context: ManagerCreationContext, operation: str, key: str):
        return conn.execute(
            """
            SELECT id,tender_id,creator_run_id,operation,idempotency_key,payload_hash,
                   staff_id,staff_version,work_order_id,created_at
            FROM office_generation_receipts
            WHERE tender_id=? AND creator_run_id=? AND operation=? AND idempotency_key=?
            """,
            (context.tender_id, context.run_id, operation, key),
        ).fetchone()

    @staticmethod
    def _staff_version_row(conn, tender_id: str, staff_id: str, version: int):
        return conn.execute(
            """
            SELECT staff_id,tender_id,version,profile_json,creator_run_id,
                   manager_profile_version,lifecycle,created_at,updated_at
            FROM office_staff_versions
            WHERE tender_id=? AND staff_id=? AND version=?
            """,
            (tender_id, staff_id, version),
        ).fetchone()

    @staticmethod
    def _definition_ref(conn, tender_id: str, staff_id: str) -> tuple[str | None, int | None]:
        row = conn.execute(
            """
            SELECT definition_id,definition_version
            FROM office_staff_definition_sources
            WHERE tender_id=? AND staff_id=?
            """,
            (tender_id, staff_id),
        ).fetchone()
        if row is None:
            return None, None
        return row["definition_id"], int(row["definition_version"])

    @classmethod
    def _staff_record(cls, conn, tender_id: str, staff_id: str, version: int) -> StaffProfileRecord:
        row = cls._staff_version_row(conn, tender_id, staff_id, version)
        if row is None:
            raise KeyError("This item could not be found in the selected Tender.")
        payload = json.loads(row["profile_json"])
        definition_id, definition_version = cls._definition_ref(conn, tender_id, staff_id)
        payload.update(
            {
                "id": row["staff_id"],
                "tender_id": row["tender_id"],
                "version": row["version"],
                "creator_run_id": row["creator_run_id"],
                "manager_profile_version": row["manager_profile_version"],
                "created_at": row["created_at"],
                "updated_at": row["updated_at"],
                "lifecycle": row["lifecycle"],
                "definition_id": definition_id,
                "definition_version": definition_version,
            }
        )
        return StaffProfileRecord.model_validate(payload)

    @staticmethod
    def _work_order_record(conn, tender_id: str, work_order_id: str) -> StaffWorkOrderRecord:
        row = conn.execute(
            """
            SELECT id,tender_id,staff_id,staff_version,creator_run_id,scope_id,
                   work_order_json,created_at
            FROM office_work_orders
            WHERE tender_id=? AND id=?
            """,
            (tender_id, work_order_id),
        ).fetchone()
        if row is None:
            raise KeyError("This item could not be found in the selected Tender.")
        payload = {
            "work_order": json.loads(row["work_order_json"]),
        }
        definition_id, definition_version = StaffStore._definition_ref(
            conn, tender_id, row["staff_id"]
        )
        payload.update(
            {
                "id": row["id"],
                "tender_id": row["tender_id"],
                "staff_id": row["staff_id"],
                "staff_version": row["staff_version"],
                "creator_run_id": row["creator_run_id"],
                "scope_id": row["scope_id"],
                "created_at": row["created_at"],
                "definition_id": definition_id,
                "definition_version": definition_version,
            }
        )
        return StaffWorkOrderRecord.model_validate(payload)

    @classmethod
    def _creation_receipt(
        cls, conn, row, *, replayed: bool
    ) -> StaffCreationReceipt:
        staff = cls._staff_record(conn, row["tender_id"], row["staff_id"], row["staff_version"])
        if row["work_order_id"] is None:
            raise KeyError("The saved staff creation assignment is incomplete.")
        order = cls._work_order_record(conn, row["tender_id"], row["work_order_id"])
        return StaffCreationReceipt(staff=staff, work_order=order, replayed=replayed)

    @classmethod
    def _revision_receipt(cls, conn, row, *, replayed: bool) -> StaffRevisionReceipt:
        staff = cls._staff_record(conn, row["tender_id"], row["staff_id"], row["staff_version"])
        return StaffRevisionReceipt(staff=staff, replayed=replayed)

    def list_staff(self, tender_id: str) -> list[StaffProfileRecord]:
        with self.repo.db.connect() as conn:
            self._tender(conn, tender_id)
            rows = conn.execute(
                """
                SELECT id,current_version FROM office_staff
                WHERE tender_id=? ORDER BY created_at,id
                """,
                (tender_id,),
            ).fetchall()
            return [self._staff_record(conn, tender_id, row["id"], row["current_version"]) for row in rows]

    def get_staff(
        self, tender_id: str, staff_id: str, version: int | None = None
    ) -> StaffProfileRecord:
        with self.repo.db.connect() as conn:
            self._tender(conn, tender_id)
            staff = conn.execute(
                "SELECT current_version FROM office_staff WHERE tender_id=? AND id=?",
                (tender_id, staff_id),
            ).fetchone()
            if staff is None:
                raise KeyError("This item could not be found in the selected Tender.")
            selected_version = staff["current_version"] if version is None else version
            if type(selected_version) is not int or selected_version < 1:
                raise ValueError("A staff profile version must be a positive integer.")
            return self._staff_record(conn, tender_id, staff_id, selected_version)

    def get_work_order(self, tender_id: str, work_order_id: str) -> StaffWorkOrderRecord:
        with self.repo.db.connect() as conn:
            self._tender(conn, tender_id)
            return self._work_order_record(conn, tender_id, work_order_id)

    def list_work_orders(
        self,
        tender_id: str,
        staff_id: str,
        *,
        offset: int = 0,
        limit: int = 100,
    ) -> list[StaffWorkOrderRecord]:
        """Return bounded immutable work orders for one scoped staff identity."""

        _identifier(tender_id, "Tender id")
        _identifier(staff_id, "Staff id")
        if type(offset) is not int or offset < 0:
            raise ValueError("The work-order offset must be nonnegative.")
        if type(limit) is not int or not 1 <= limit <= 200:
            raise ValueError("The work-order limit must be between 1 and 200.")
        with self.repo.db.connect() as conn:
            self._tender(conn, tender_id)
            staff = conn.execute(
                "SELECT 1 FROM office_staff WHERE tender_id=? AND id=?",
                (tender_id, staff_id),
            ).fetchone()
            if staff is None:
                raise KeyError("This item could not be found in the selected Tender.")
            rows = conn.execute(
                """
                SELECT id FROM office_work_orders
                WHERE tender_id=? AND staff_id=?
                ORDER BY created_at,id
                LIMIT ? OFFSET ?
                """,
                (tender_id, staff_id, limit, offset),
            ).fetchall()
            return [self._work_order_record(conn, tender_id, row["id"]) for row in rows]

    def create_generated(
        self,
        context: ManagerCreationContext,
        profile: StaffProfileDraft | dict,
        work_order: StaffWorkOrder | dict,
        idempotency_key: str,
        *,
        definition_id: str | None = None,
        definition_version: int | None = None,
    ) -> StaffCreationReceipt:
        prepared_profile = self._profile(profile)
        prepared_order = self._work_order(work_order)
        self._validate_context_shape(context)
        if (definition_id is None) != (definition_version is None):
            raise ValueError("A definition identity and version must be supplied together.")
        if definition_id is not None:
            _identifier(definition_id, "Definition id")
            if type(definition_version) is not int or definition_version < 1:
                raise ValueError("The definition version must be a positive integer.")
        key = _key(idempotency_key)
        body_hash = _canonical_hash(
            {
                "profile": prepared_profile.model_dump(mode="json"),
                "work_order": prepared_order.model_dump(mode="json"),
                "manager_profile_version": context.manager_profile_version,
                "scope_id": context.scope_id,
                "definition_id": definition_id,
                "definition_version": definition_version,
            }
        )
        with self.repo.atomic() as conn:
            existing = self._receipt(conn, context, "create_generated", key)
            if existing is not None:
                if existing["payload_hash"] != body_hash:
                    raise OfficeConflict("This creation key was already used for different staff data.")
                return self._creation_receipt(conn, existing, replayed=True)

            self._validate_context(conn, context)
            if definition_id is not None:
                definition = conn.execute(
                    """
                    SELECT d.lifecycle,v.profile_json
                    FROM agent_definitions d
                    JOIN agent_definition_versions v ON v.definition_id=d.id
                    WHERE d.id=? AND v.version=?
                    """,
                    (definition_id, definition_version),
                ).fetchone()
                if definition is None:
                    raise KeyError("This professional definition could not be found.")
                if definition["lifecycle"] != "active":
                    raise OfficeConflict("This professional definition is retired and cannot be reused.")
                if json.loads(definition["profile_json"]) != prepared_profile.model_dump(mode="json"):
                    raise OfficeConflict("The selected professional definition no longer matches this profile.")
            self.repo._check_sources(conn, context.tender_id, prepared_order.source_ids)
            staff_id, work_order_id, receipt_id, stamp = new_id(), new_id(), new_id(), now()
            profile_payload = prepared_profile.model_dump(mode="json")
            profile_payload["portrait"] = {"style": "notionists-v1", "seed": staff_id}
            order_payload = prepared_order.model_dump(mode="json")
            conn.execute(
                """
                INSERT INTO office_staff(
                    id,tender_id,current_version,lifecycle,created_at,updated_at
                ) VALUES(?,?,?,?,?,?)
                """,
                (staff_id, context.tender_id, 1, "available", stamp, stamp),
            )
            conn.execute(
                """
                INSERT INTO office_staff_versions(
                    staff_id,tender_id,version,profile_json,creator_run_id,
                    manager_profile_version,lifecycle,created_at,updated_at
                ) VALUES(?,?,?,?,?,?,?,?,?)
                """,
                (
                    staff_id,
                    context.tender_id,
                    1,
                    dump(profile_payload),
                    context.run_id,
                    context.manager_profile_version,
                    "available",
                    stamp,
                    stamp,
                ),
            )
            conn.execute(
                """
                INSERT INTO office_work_orders(
                    id,tender_id,staff_id,staff_version,creator_run_id,scope_id,
                    work_order_json,created_at
                ) VALUES(?,?,?,?,?,?,?,?)
                """,
                (
                    work_order_id,
                    context.tender_id,
                    staff_id,
                    1,
                    context.run_id,
                    context.scope_id,
                    dump(order_payload),
                    stamp,
                ),
            )
            conn.execute(
                """
                INSERT INTO office_generation_receipts(
                    id,tender_id,creator_run_id,operation,idempotency_key,payload_hash,
                    staff_id,staff_version,work_order_id,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    receipt_id,
                    context.tender_id,
                    context.run_id,
                    "create_generated",
                    key,
                    body_hash,
                    staff_id,
                    1,
                    work_order_id,
                    stamp,
                ),
            )
            if definition_id is not None:
                conn.execute(
                    """
                    INSERT INTO office_staff_definition_sources(
                        staff_id,tender_id,definition_id,definition_version,created_at
                    ) VALUES(?,?,?,?,?)
                    """,
                    (
                        staff_id,
                        context.tender_id,
                        definition_id,
                        definition_version,
                        stamp,
                    ),
                )
            row = conn.execute(
                "SELECT * FROM office_generation_receipts WHERE id=?", (receipt_id,)
            ).fetchone()
            return self._creation_receipt(conn, row, replayed=False)

    def create_from_definition(
        self,
        context: ManagerCreationContext,
        definition_id: str,
        definition_version: int,
        work_order: StaffWorkOrder | dict,
        idempotency_key: str,
    ) -> StaffCreationReceipt:
        """Instantiate one exact definition as ordinary planned Tender staff.

        The result uses the existing staff and work-order path. It adds no
        assignment, route binding, source scope or execution grant.
        """

        from .agent_definitions import AgentDefinitionService

        definition = AgentDefinitionService(self.repo).get(
            definition_id, definition_version
        )
        if definition.lifecycle != "active":
            raise OfficeConflict("This professional definition is retired and cannot be reused.")
        return self.create_generated(
            context,
            definition.profile,
            work_order,
            idempotency_key,
            definition_id=definition.id,
            definition_version=definition.version,
        )

    def revise_generated(
        self,
        context: ManagerCreationContext,
        staff_id: str,
        expected_version: int,
        profile: StaffProfileDraft | dict,
        idempotency_key: str,
    ) -> StaffRevisionReceipt:
        prepared_profile = self._profile(profile)
        self._validate_context_shape(context)
        key = _key(idempotency_key)
        if type(expected_version) is not int or expected_version < 1:
            raise ValueError("The expected staff profile version must be a positive integer.")
        body_hash = _canonical_hash(
            {
                "staff_id": staff_id,
                "expected_version": expected_version,
                "profile": prepared_profile.model_dump(mode="json"),
                "manager_profile_version": context.manager_profile_version,
                "scope_id": context.scope_id,
            }
        )
        with self.repo.atomic() as conn:
            existing = self._receipt(conn, context, "revise_generated", key)
            if existing is not None:
                if existing["payload_hash"] != body_hash:
                    raise OfficeConflict("This revision key was already used for different staff data.")
                return self._revision_receipt(conn, existing, replayed=True)

            self._validate_context(conn, context)
            staff = conn.execute(
                """
                SELECT id,current_version,lifecycle
                FROM office_staff WHERE tender_id=? AND id=?
                """,
                (context.tender_id, staff_id),
            ).fetchone()
            if staff is None:
                raise KeyError("This item could not be found in the selected Tender.")
            if staff["current_version"] != expected_version:
                raise OfficeConflict(
                    f"The staff profile changed from version {expected_version}; refresh before revising."
                )
            next_version, receipt_id, stamp = expected_version + 1, new_id(), now()
            profile_payload = prepared_profile.model_dump(mode="json")
            current_profile = self._staff_record(conn, context.tender_id, staff_id, expected_version)
            profile_payload["portrait"] = current_profile.portrait.model_dump(mode="json")
            conn.execute(
                """
                INSERT INTO office_staff_versions(
                    staff_id,tender_id,version,profile_json,creator_run_id,
                    manager_profile_version,lifecycle,created_at,updated_at
                ) VALUES(?,?,?,?,?,?,?,?,?)
                """,
                (
                    staff_id,
                    context.tender_id,
                    next_version,
                    dump(profile_payload),
                    context.run_id,
                    context.manager_profile_version,
                    staff["lifecycle"],
                    stamp,
                    stamp,
                ),
            )
            changed = conn.execute(
                """
                UPDATE office_staff
                SET current_version=?,updated_at=?
                WHERE tender_id=? AND id=? AND current_version=?
                """,
                (next_version, stamp, context.tender_id, staff_id, expected_version),
            ).rowcount
            if changed != 1:
                raise OfficeConflict("The staff profile changed; refresh before revising.")
            conn.execute(
                """
                INSERT INTO office_generation_receipts(
                    id,tender_id,creator_run_id,operation,idempotency_key,payload_hash,
                    staff_id,staff_version,work_order_id,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    receipt_id,
                    context.tender_id,
                    context.run_id,
                    "revise_generated",
                    key,
                    body_hash,
                    staff_id,
                    next_version,
                    None,
                    stamp,
                ),
            )
            row = conn.execute(
                "SELECT * FROM office_generation_receipts WHERE id=?", (receipt_id,)
            ).fetchone()
            return self._revision_receipt(conn, row, replayed=False)

    def create_work_order(
        self,
        context: ManagerCreationContext,
        staff_id: str,
        expected_version: int,
        work_order: StaffWorkOrder | dict,
        idempotency_key: str,
    ) -> StaffWorkOrderReceipt:
        """Create a planned order fixed to the expected current profile version."""

        prepared_order = self._work_order(work_order)
        self._validate_context_shape(context)
        _identifier(staff_id, "Staff id")
        if type(expected_version) is not int or expected_version < 1:
            raise ValueError("The expected staff profile version must be a positive integer.")
        key = _key(idempotency_key)
        body_hash = _canonical_hash(
            {
                "staff_id": staff_id,
                "expected_version": expected_version,
                "work_order": prepared_order.model_dump(mode="json"),
                "manager_profile_version": context.manager_profile_version,
                "scope_id": context.scope_id,
            }
        )
        with self.repo.atomic() as conn:
            existing = self._receipt(conn, context, "create_work_order", key)
            if existing is not None:
                if existing["payload_hash"] != body_hash:
                    raise OfficeConflict(
                        "This work-order key was already used for different work data."
                    )
                if existing["work_order_id"] is None:
                    raise KeyError("The saved work-order receipt is incomplete.")
                return StaffWorkOrderReceipt(
                    work_order=self._work_order_record(
                        conn, context.tender_id, existing["work_order_id"]
                    ),
                    replayed=True,
                )

            self._validate_context(conn, context)
            staff = conn.execute(
                """
                SELECT id,current_version FROM office_staff
                WHERE tender_id=? AND id=?
                """,
                (context.tender_id, staff_id),
            ).fetchone()
            if staff is None:
                raise KeyError("This item could not be found in the selected Tender.")
            if staff["current_version"] != expected_version:
                raise OfficeConflict(
                    f"The staff profile changed from version {expected_version}; refresh before planning work."
                )
            self.repo._check_sources(conn, context.tender_id, prepared_order.source_ids)
            work_order_id, receipt_id, stamp = new_id(), new_id(), now()
            conn.execute(
                """
                INSERT INTO office_work_orders(
                    id,tender_id,staff_id,staff_version,creator_run_id,scope_id,
                    work_order_json,created_at
                ) VALUES(?,?,?,?,?,?,?,?)
                """,
                (
                    work_order_id,
                    context.tender_id,
                    staff_id,
                    expected_version,
                    context.run_id,
                    context.scope_id,
                    dump(prepared_order.model_dump(mode="json")),
                    stamp,
                ),
            )
            conn.execute(
                """
                INSERT INTO office_generation_receipts(
                    id,tender_id,creator_run_id,operation,idempotency_key,payload_hash,
                    staff_id,staff_version,work_order_id,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    receipt_id,
                    context.tender_id,
                    context.run_id,
                    "create_work_order",
                    key,
                    body_hash,
                    staff_id,
                    expected_version,
                    work_order_id,
                    stamp,
                ),
            )
            return StaffWorkOrderReceipt(
                work_order=self._work_order_record(conn, context.tender_id, work_order_id),
                replayed=False,
            )


__all__ = ["StaffStore"]
