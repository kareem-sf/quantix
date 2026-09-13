"""Actual Manager-mediated messages and artifact handoffs for the live office."""

from __future__ import annotations

import base64
import hashlib
import json
import re
import sqlite3
from dataclasses import dataclass
from typing import Any

from .db import dump, new_id, now
from .manager_runtime import ManagerRunProfiles
from .office_events import OfficeEventService
from .office_message_models import (
    OfficeArtifactReference,
    OfficeMessage,
    OfficeMessagePage,
    OfficeOutputReferenceRequest,
    OfficeParticipant,
    OfficeRecipientTarget,
    OfficeSourceReferenceRequest,
    OfficeStaffResultReferenceRequest,
)
from .office_tools import OfficeContext
from .staff_assignment_models import StaffResult
from .staff_assignments import StaffAssignmentService
from .staff_models import OfficeConflict
from .staff_routing import StaffRoutingService
from .staff_store import StaffStore

_IDENTIFIER_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:/-]{0,159}$")
_KEY_RE = re.compile(r"^[A-Za-z0-9._~-]{1,160}$")
_ACTIVE_ROOT_STATES = {"queued", "running"}
_ACTIVE_ASSIGNMENT_STATES = {"queued", "running", "waiting"}
_TARGET_ASSIGNMENT_STATES = _ACTIVE_ASSIGNMENT_STATES | {"completed"}
_MESSAGE_KINDS = {"instruction", "note", "question", "reply", "finding", "handoff"}


_SCHEMA_STATEMENTS = (
    """
    CREATE TABLE IF NOT EXISTS office_messages (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        root_run_id TEXT NOT NULL REFERENCES runs(id),
        sender_id TEXT NOT NULL,
        sender_version INTEGER NOT NULL CHECK (sender_version >= 1),
        sender_display_name TEXT NOT NULL,
        sender_title TEXT NOT NULL,
        sender_kind TEXT NOT NULL CHECK (sender_kind IN ('manager','staff')),
        sender_assignment_id TEXT,
        assignment_id TEXT,
        kind TEXT NOT NULL CHECK (kind IN ('instruction','note','question','reply','finding','handoff')),
        text TEXT NOT NULL,
        artifact_refs_json TEXT NOT NULL DEFAULT '[]',
        reply_to TEXT,
        created_at TEXT NOT NULL
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS office_messages_tender_order
    ON office_messages(tender_id, created_at DESC, id DESC)
    """,
    """
    CREATE TABLE IF NOT EXISTS office_message_recipients (
        message_id TEXT NOT NULL REFERENCES office_messages(id),
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
        recipient_id TEXT NOT NULL,
        recipient_version INTEGER NOT NULL CHECK (recipient_version >= 1),
        recipient_display_name TEXT NOT NULL,
        recipient_title TEXT NOT NULL,
        recipient_kind TEXT NOT NULL CHECK (recipient_kind IN ('manager','staff')),
        assignment_id TEXT,
        PRIMARY KEY(message_id, ordinal),
        UNIQUE(message_id, recipient_id, assignment_id)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS office_message_recipients_lookup
    ON office_message_recipients(tender_id, recipient_id, assignment_id, message_id)
    """,
    """
    CREATE TABLE IF NOT EXISTS office_message_receipts (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        root_run_id TEXT NOT NULL REFERENCES runs(id),
        sender_id TEXT NOT NULL,
        operation TEXT NOT NULL,
        idempotency_key TEXT NOT NULL,
        payload_hash TEXT NOT NULL,
        message_id TEXT NOT NULL REFERENCES office_messages(id),
        created_at TEXT NOT NULL,
        UNIQUE(tender_id, root_run_id, sender_id, operation, idempotency_key)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS office_message_receipts_lookup
    ON office_message_receipts(tender_id, operation, idempotency_key)
    """,
    """
    CREATE TRIGGER IF NOT EXISTS office_messages_immutable_update
    BEFORE UPDATE ON office_messages
    BEGIN SELECT RAISE(ABORT, 'Office messages are immutable'); END
    """,
    """
    CREATE TRIGGER IF NOT EXISTS office_messages_immutable_delete
    BEFORE DELETE ON office_messages
    BEGIN SELECT RAISE(ABORT, 'Office messages are immutable'); END
    """,
    """
    CREATE TRIGGER IF NOT EXISTS office_message_recipients_immutable_update
    BEFORE UPDATE ON office_message_recipients
    BEGIN SELECT RAISE(ABORT, 'Office message recipients are immutable'); END
    """,
    """
    CREATE TRIGGER IF NOT EXISTS office_message_recipients_immutable_delete
    BEFORE DELETE ON office_message_recipients
    BEGIN SELECT RAISE(ABORT, 'Office message recipients are immutable'); END
    """,
    """
    CREATE TRIGGER IF NOT EXISTS office_message_receipts_immutable_update
    BEFORE UPDATE ON office_message_receipts
    BEGIN SELECT RAISE(ABORT, 'Office message receipts are immutable'); END
    """,
    """
    CREATE TRIGGER IF NOT EXISTS office_message_receipts_immutable_delete
    BEFORE DELETE ON office_message_receipts
    BEGIN SELECT RAISE(ABORT, 'Office message receipts are immutable'); END
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


def _body_hash(value: Any) -> str:
    encoded = json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
        allow_nan=False,
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def _json_load(value: str, label: str) -> Any:
    try:
        return json.loads(value)
    except (TypeError, ValueError, json.JSONDecodeError) as error:
        raise RuntimeError(f"Saved {label} contains invalid JSON.") from error


def _request_reference(value: Any):
    """Parse a reference choice, accepting the brief's id-only shorthand."""

    if isinstance(
        value,
        (
            OfficeSourceReferenceRequest,
            OfficeOutputReferenceRequest,
            OfficeStaffResultReferenceRequest,
        ),
    ):
        return value
    if not isinstance(value, dict):
        raise TypeError("An office reference request must be an object.")
    raw = dict(value)
    if "kind" not in raw:
        candidates = [
            ("source_id", OfficeSourceReferenceRequest),
            ("output_id", OfficeOutputReferenceRequest),
            ("staff_result_id", OfficeStaffResultReferenceRequest),
        ]
        found = [(key, cls) for key, cls in candidates if key in raw]
        if len(found) != 1:
            raise ValueError("Choose exactly one source, output or staff-result reference.")
        raw["kind"] = {
            "source_id": "source",
            "output_id": "output",
            "staff_result_id": "staff_result",
        }[found[0][0]]
    kind = raw.get("kind")
    cls = {
        "source": OfficeSourceReferenceRequest,
        "output": OfficeOutputReferenceRequest,
        "staff_result": OfficeStaffResultReferenceRequest,
    }.get(kind)
    if cls is None:
        raise ValueError("Choose a recognised office reference kind.")
    return cls.model_validate(raw)


def _reference_requests(values: Any) -> tuple:
    if values is None:
        return ()
    if not isinstance(values, (list, tuple)):
        raise TypeError("Office reference requests must be a list or tuple.")
    if len(values) > 30:
        raise ValueError("A message can share at most 30 references.")
    return tuple(_request_reference(item) for item in values)


@dataclass(frozen=True)
class _ResolvedTarget:
    participant: OfficeParticipant
    assignment_id: str | None
    binding: Any | None


class OfficeMessageService:
    """Persist actual office dialogue and exact handoff snapshots."""

    def __init__(self, repo):
        if repo is None or not hasattr(repo, "atomic") or not hasattr(repo, "db"):
            raise TypeError("A Repository is required for office messages.")
        self.repo = repo
        self.events = OfficeEventService(repo)
        self.manager_runs = ManagerRunProfiles(repo)
        self.staff = StaffStore(repo)
        self.routing = StaffRoutingService(repo)
        self.assignments = StaffAssignmentService(repo)
        with repo.atomic() as conn:
            for statement in _SCHEMA_STATEMENTS:
                conn.execute(statement)

    @staticmethod
    def _kind(kind: Any) -> str:
        if kind not in _MESSAGE_KINDS:
            raise ValueError("Choose a recognised office message kind.")
        return kind

    @staticmethod
    def _text(value: Any) -> str:
        if not isinstance(value, str) or not value.strip() or len(value) > 18000:
            raise ValueError("Enter message text (up to 18000 characters).")
        return value.strip()

    @staticmethod
    def _reply(value: Any) -> str | None:
        if value is None:
            return None
        return _identifier(value, "Reply message id")

    @staticmethod
    def _targets(values: Any) -> tuple[OfficeRecipientTarget, ...]:
        if not isinstance(values, (list, tuple)) or not 1 <= len(values) <= 10:
            raise ValueError("Choose between 1 and 10 office recipients.")
        result = tuple(
            value
            if isinstance(value, OfficeRecipientTarget)
            else OfficeRecipientTarget.model_validate(value)
            for value in values
        )
        identities = {(item.staff_id, item.assignment_id) for item in result}
        if len(identities) != len(result):
            raise ValueError("A message cannot repeat an office recipient target.")
        return result

    @staticmethod
    def _participant(profile, *, kind: str, assignment_id: str | None = None) -> OfficeParticipant:
        return OfficeParticipant(
            id=profile.id,
            version=profile.version,
            display_name=profile.display_name,
            title=profile.title,
            kind=kind,
            assignment_id=assignment_id,
        )

    def _manager_profile(self, tender_id: str, root_run_id: str):
        try:
            return self.manager_runs.get(tender_id, root_run_id)
        except (KeyError, ValueError) as error:
            raise ValueError(
                "The work root has no immutable Tender Manager profile pin."
            ) from error

    @staticmethod
    def _root_row(conn, tender_id: str, root_run_id: str):
        row = conn.execute(
            "SELECT id,tender_id,kind,status FROM runs WHERE tender_id=? AND id=?",
            (tender_id, root_run_id),
        ).fetchone()
        if row is None:
            raise KeyError("The work root does not belong to this Tender.")
        return row

    def _manager_base(
        self, context: OfficeContext, conn, *, require_active: bool
    ) -> OfficeParticipant:
        if not isinstance(context, OfficeContext):
            raise TypeError("An OfficeContext is required for Manager messages.")
        tender_id = _identifier(context.tender_id, "Tender id")
        root_run_id = _identifier(context.run_id, "Root run id")
        run = self._root_row(conn, tender_id, root_run_id)
        if run["kind"] not in {"manager", "conversation"}:
            raise ValueError("Office messages require a Manager or conversation work root.")
        if require_active and run["status"] not in _ACTIVE_ROOT_STATES:
            raise OfficeConflict("The work root is no longer active.")
        if context.is_staff:
            raise ValueError("A staff context cannot author a Manager message.")
        profile = self._manager_profile(tender_id, root_run_id)
        if context.actor_id is not None and context.actor_id != profile.id:
            raise OfficeConflict("The message author does not match the pinned Tender Manager.")
        return self._participant(profile, kind="manager")

    def _staff_base(self, context: OfficeContext, conn, *, require_active: bool):
        if not isinstance(context, OfficeContext) or not context.is_staff:
            raise TypeError("An active staff OfficeContext is required for staff messages.")
        tender_id = _identifier(context.tender_id, "Tender id")
        root_run_id = _identifier(context.run_id, "Root run id")
        assignment_id = _identifier(context.assignment_id, "Assignment id")
        binding_id = _identifier(context.route_binding_id, "Route binding id")
        if (
            context.actor_id is None
            or type(context.staff_version) is not int
            or context.staff_version < 1
        ):
            raise ValueError("The staff author identity is incomplete.")
        assignment = self.assignments.get(tender_id, assignment_id)
        if (
            assignment.root_run_id != root_run_id
            or assignment.staff_id != context.actor_id
            or assignment.staff_version != context.staff_version
            or assignment.route_binding_id != binding_id
        ):
            raise OfficeConflict("The staff context no longer matches its saved assignment.")
        if require_active and assignment.status not in {"running", "waiting"}:
            raise OfficeConflict("Only a running or waiting staff assignment can author a new message.")
        binding = self.routing.validate_binding(tender_id, binding_id)
        expected = {
            "tender_id": tender_id,
            "root_run_id": root_run_id,
            "staff_id": context.actor_id,
            "staff_version": context.staff_version,
            "work_order_id": assignment.work_order_id,
            "id": binding_id,
        }
        if any(getattr(binding, key, None) != value for key, value in expected.items()):
            raise OfficeConflict("The staff route binding no longer matches its assignment.")
        profile = self.staff.get_staff(tender_id, context.actor_id, context.staff_version)
        order = self.staff.get_work_order(tender_id, assignment.work_order_id)
        if order.staff_id != profile.id or order.staff_version != profile.version:
            raise OfficeConflict("The staff work order no longer matches its profile version.")
        if context.staff_profile is not None and (
            getattr(context.staff_profile, "id", None) != profile.id
            or getattr(context.staff_profile, "version", None) != profile.version
        ):
            raise OfficeConflict("The staff context contains a different saved profile version.")
        if (
            context.staff_work_order is not None
            and getattr(context.staff_work_order, "id", None) != order.id
        ):
            raise OfficeConflict("The staff context contains a different saved work order.")
        participant = self._participant(profile, kind="staff", assignment_id=assignment.id)
        manager_profile = self._manager_profile(tender_id, root_run_id)
        manager = self._participant(manager_profile, kind="manager")
        return participant, manager, assignment, binding

    def _target(
        self, tender_id: str, root_run_id: str, target: OfficeRecipientTarget, conn
    ) -> _ResolvedTarget:
        profile = self.staff.get_staff(tender_id, target.staff_id)
        if target.assignment_id is None:
            return _ResolvedTarget(self._participant(profile, kind="staff"), None, None)
        assignment = self.assignments.get(tender_id, target.assignment_id)
        if assignment.staff_id != target.staff_id or assignment.root_run_id != root_run_id:
            raise ValueError(
                "The recipient assignment does not belong to this work root and staff profile."
            )
        if assignment.status not in _TARGET_ASSIGNMENT_STATES:
            raise ValueError(
                "A cancelled, failed or interrupted assignment cannot receive an office message."
            )
        binding = self.routing.validate_binding(tender_id, assignment.route_binding_id)
        if (
            binding.root_run_id != root_run_id
            or binding.staff_id != assignment.staff_id
            or binding.staff_version != assignment.staff_version
            or binding.work_order_id != assignment.work_order_id
        ):
            raise OfficeConflict("The recipient assignment and current route binding do not match.")
        profile = self.staff.get_staff(tender_id, assignment.staff_id, assignment.staff_version)
        return _ResolvedTarget(
            self._participant(profile, kind="staff", assignment_id=assignment.id),
            assignment.id,
            binding,
        )

    @staticmethod
    def _scope_allows(
        bindings: tuple[Any, ...], artifact_id: str, version: int, content_hash: str
    ) -> bool:
        if not bindings:
            return True
        return all(
            any(
                basis.artifact_id == artifact_id
                and basis.version == version
                and basis.content_hash.lower() == content_hash.lower()
                for basis in binding.artifacts
            )
            for binding in bindings
        )

    @staticmethod
    def _artifact_row(conn, tender_id: str, artifact_id: str):
        return conn.execute(
            "SELECT id,version,content_hash,is_current FROM artifacts WHERE tender_id=? AND id=?",
            (tender_id, artifact_id),
        ).fetchone()

    def _source_reference(self, conn, tender_id: str, source_id: str, bindings: tuple[Any, ...]):
        row = conn.execute(
            """
            SELECT e.id AS source_id,e.locator,a.id AS artifact_id,a.version,a.content_hash,a.is_current
            FROM evidence e JOIN artifacts a ON a.id=e.artifact_id
            WHERE a.tender_id=? AND e.id=?
            """,
            (tender_id, source_id),
        ).fetchone()
        if row is None:
            raise ValueError("The source reference does not belong to this Tender.")
        if not row["is_current"]:
            raise ValueError("The source reference is no longer current.")
        if not self._scope_allows(
            bindings, row["artifact_id"], row["version"], row["content_hash"]
        ):
            raise ValueError("The source reference is outside a targeted staff binding scope.")
        return OfficeArtifactReference(
            kind="source",
            id=row["source_id"],
            artifact_id=row["artifact_id"],
            artifact_version=row["version"],
            content_hash=row["content_hash"].lower(),
            locator=row["locator"],
        )

    def _output_reference(self, conn, tender_id: str, output_id: str, bindings: tuple[Any, ...]):
        try:
            row = conn.execute(
                "SELECT record_json FROM generated_outputs WHERE id=? AND tender_id=?",
                (output_id, tender_id),
            ).fetchone()
        except sqlite3.OperationalError as error:
            raise ValueError("The output reference is not available in this Tender.") from error
        if row is None:
            raise ValueError("The output reference does not belong to this Tender.")
        payload = _json_load(row["record_json"], "generated output")
        if (
            not isinstance(payload, dict)
            or payload.get("id") != output_id
            or payload.get("tender_id") != tender_id
        ):
            raise ValueError("The saved output identity is invalid.")
        sha256, filename = payload.get("sha256"), payload.get("filename")
        try:
            reference = OfficeArtifactReference(
                kind="output", id=output_id, sha256=sha256, filename=filename
            )
        except (TypeError, ValueError) as error:
            raise ValueError("The saved output has no valid immutable file basis.") from error
        metadata = payload.get("metadata")
        manifest = metadata.get("source_manifest") if isinstance(metadata, dict) else None
        if not isinstance(manifest, list):
            raise ValueError("The output has no current source manifest for this handoff.")
        if bindings and not manifest:
            raise ValueError("The output has no current source scope for this staff handoff.")
        for item in manifest:
            if not isinstance(item, dict):
                raise ValueError("The output source manifest is invalid.")
            artifact_id = item.get("id") or item.get("artifact_id")
            artifact = self._artifact_row(conn, tender_id, artifact_id)
            if (
                item.get("is_current") is not True
                or artifact is None
                or not artifact["is_current"]
            ):
                raise ValueError("The output source basis is no longer current.")
            if (
                artifact["version"] != item.get("version")
                or artifact["content_hash"].lower() != str(item.get("content_hash", "")).lower()
                or not self._scope_allows(
                    bindings, artifact["id"], artifact["version"], artifact["content_hash"]
                )
            ):
                raise ValueError(
                    "The output source basis is outside a targeted staff binding scope."
                )
        return reference

    def _staff_result_reference(
        self, conn, tender_id: str, result_id: str, root_run_id: str, bindings: tuple[Any, ...]
    ):
        row = conn.execute(
            "SELECT * FROM office_staff_results WHERE tender_id=? AND id=?",
            (tender_id, result_id),
        ).fetchone()
        if row is None:
            raise ValueError("The staff result reference does not belong to this Tender.")
        try:
            result = self.assignments.get_result(tender_id, result_id)
        except (TypeError, ValueError) as error:
            raise ValueError("The saved staff result is invalid.") from error
        if result.currentness != "current":
            raise ValueError("The staff result source or working basis is no longer current.")
        if (
            result.id != result_id
            or result.tender_id != tender_id
            or result.root_run_id != root_run_id
            or row["assignment_id"] != result.assignment_id
        ):
            raise ValueError("The staff result is outside this work root.")
        assignment = self.assignments.get(tender_id, result.assignment_id)
        if (
            assignment.result_id != result.id
            or assignment.staff_id != result.staff_id
            or assignment.staff_version != result.staff_version
            or assignment.root_run_id != result.root_run_id
            or assignment.work_order_id != result.work_order_id
            or assignment.route_binding_id != result.route_binding_id
        ):
            raise ValueError("The staff result identity does not match its completed assignment.")
        binding = self.routing.validate_binding(tender_id, assignment.route_binding_id)
        if binding.root_run_id != root_run_id:
            raise ValueError("The staff result binding belongs to another work root.")
        for basis in result.source_bases:
            source = conn.execute(
                """
                SELECT e.id,a.id AS artifact_id,a.version,a.content_hash,a.is_current,e.locator
                FROM evidence e JOIN artifacts a ON a.id=e.artifact_id
                WHERE a.tender_id=? AND e.id=?
                """,
                (tender_id, basis.source_id),
            ).fetchone()
            if (
                source is None
                or not source["is_current"]
                or source["artifact_id"] != basis.artifact_id
                or source["version"] != basis.artifact_version
                or source["content_hash"].lower() != basis.artifact_hash.lower()
                or source["locator"] != basis.locator
                or not self._scope_allows(
                    bindings, source["artifact_id"], source["version"], source["content_hash"]
                )
            ):
                raise ValueError("The staff result source basis is no longer current or permitted.")
        return OfficeArtifactReference(
            kind="staff_result",
            id=result.id,
            assignment_id=result.assignment_id,
            staff_id=result.staff_id,
            staff_version=result.staff_version,
            root_run_id=result.root_run_id,
        )

    def _resolve_references(
        self, conn, tender_id: str, root_run_id: str, values: tuple, bindings: tuple[Any, ...]
    ):
        resolved = []
        for request in values:
            if isinstance(request, OfficeSourceReferenceRequest):
                resolved.append(
                    self._source_reference(conn, tender_id, request.source_id, bindings)
                )
            elif isinstance(request, OfficeOutputReferenceRequest):
                resolved.append(
                    self._output_reference(conn, tender_id, request.output_id, bindings)
                )
            else:
                resolved.append(
                    self._staff_result_reference(
                        conn, tender_id, request.staff_result_id, root_run_id, bindings
                    )
                )
        return resolved

    def _reply_check(
        self,
        conn,
        tender_id: str,
        root_run_id: str,
        reply_to: str | None,
        sender: OfficeParticipant,
    ) -> None:
        if reply_to is None:
            return
        row = conn.execute(
            "SELECT * FROM office_messages WHERE tender_id=? AND id=?", (tender_id, reply_to)
        ).fetchone()
        if row is None:
            raise ValueError("The reply target does not belong to this Tender.")
        if row["root_run_id"] != root_run_id:
            raise ValueError(
                "Cross-root replies are unsupported; continue under a new active assignment."
            )
        if sender.kind == "manager":
            participant = conn.execute(
                """
                SELECT 1 FROM office_messages
                WHERE id=? AND sender_id=? AND sender_kind='manager'
                UNION ALL
                SELECT 1 FROM office_message_recipients
                WHERE message_id=? AND recipient_id=? AND recipient_kind='manager'
                LIMIT 1
                """,
                (reply_to, sender.id, reply_to, sender.id),
            ).fetchone()
        else:
            participant = conn.execute(
                """
                SELECT 1 FROM office_messages
                WHERE id=? AND sender_id=? AND sender_kind='staff' AND sender_assignment_id=?
                UNION ALL
                SELECT 1 FROM office_message_recipients
                WHERE message_id=? AND recipient_id=? AND recipient_kind='staff' AND assignment_id=?
                LIMIT 1
                """,
                (
                    reply_to,
                    sender.id,
                    sender.assignment_id,
                    reply_to,
                    sender.id,
                    sender.assignment_id,
                ),
            ).fetchone()
        if participant is None:
            raise ValueError("The message author was not a participant in the reply target.")

    @staticmethod
    def _receipt(conn, tender_id: str, root_run_id: str, sender_id: str, operation: str, key: str):
        return conn.execute(
            """
            SELECT * FROM office_message_receipts
            WHERE tender_id=? AND root_run_id=? AND sender_id=? AND operation=? AND idempotency_key=?
            """,
            (tender_id, root_run_id, sender_id, operation, key),
        ).fetchone()

    def _replay(self, conn, receipt, payload_hash: str) -> OfficeMessage:
        if receipt["payload_hash"] != payload_hash:
            raise OfficeConflict(
                "This message idempotency key was already used for different content."
            )
        row = conn.execute(
            "SELECT * FROM office_messages WHERE tender_id=? AND id=?",
            (receipt["tender_id"], receipt["message_id"]),
        ).fetchone()
        if row is None:
            raise RuntimeError("The message receipt points to a missing immutable message.")
        return self._message_from_row(conn, row)

    @staticmethod
    def _message_from_row(conn, row) -> OfficeMessage:
        recipients = conn.execute(
            """
            SELECT recipient_id,recipient_version,recipient_display_name,recipient_title,
                   recipient_kind,assignment_id
            FROM office_message_recipients WHERE message_id=? ORDER BY ordinal
            """,
            (row["id"],),
        ).fetchall()
        receipts_table = conn.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='office_delivery_receipts'"
        ).fetchone()
        delivery = (
            conn.execute(
                """
                SELECT state,required_response FROM office_delivery_receipts
                WHERE tender_id=? AND message_id=?
                ORDER BY created_at DESC,rowid DESC LIMIT 1
                """,
                (row["tender_id"], row["id"]),
            ).fetchone()
            if receipts_table is not None
            else None
        )
        sender = OfficeParticipant(
            id=row["sender_id"],
            version=row["sender_version"],
            display_name=row["sender_display_name"],
            title=row["sender_title"],
            kind=row["sender_kind"],
            assignment_id=row["sender_assignment_id"],
        )
        return OfficeMessage(
            id=row["id"],
            tender_id=row["tender_id"],
            root_run_id=row["root_run_id"],
            sender=sender,
            recipients=[
                OfficeParticipant(
                    id=item["recipient_id"],
                    version=item["recipient_version"],
                    display_name=item["recipient_display_name"],
                    title=item["recipient_title"],
                    kind=item["recipient_kind"],
                    assignment_id=item["assignment_id"],
                )
                for item in recipients
            ],
            assignment_id=row["assignment_id"],
            kind=row["kind"],
            text=row["text"],
            artifact_refs=[
                OfficeArtifactReference.model_validate(item)
                for item in _json_load(row["artifact_refs_json"], "office message references")
            ],
            reply_to=row["reply_to"],
            created_at=row["created_at"],
            delivery_state=delivery["state"] if delivery is not None else None,
            required_response=delivery["required_response"] if delivery is not None else None,
        )

    def _save(
        self,
        conn,
        *,
        tender_id: str,
        root_run_id: str,
        sender: OfficeParticipant,
        targets: list[_ResolvedTarget],
        kind: str,
        text: str,
        refs: list[OfficeArtifactReference],
        reply_to: str | None,
        message_assignment_id: str | None,
        operation: str,
        key: str,
        payload_hash: str,
    ) -> OfficeMessage:
        message_id, stamp = new_id(), now()
        assignment_ids = {item.assignment_id for item in targets if item.assignment_id is not None}
        assignment_id = message_assignment_id
        if assignment_id is None and sender.kind == "manager":
            assignment_id = (
                next(iter(assignment_ids))
                if len(assignment_ids) == 1 and len(targets) == 1
                else None
            )
        conn.execute(
            """
            INSERT INTO office_messages(
                id,tender_id,root_run_id,sender_id,sender_version,sender_display_name,sender_title,
                sender_kind,sender_assignment_id,assignment_id,kind,text,artifact_refs_json,reply_to,created_at
            ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
            """,
            (
                message_id,
                tender_id,
                root_run_id,
                sender.id,
                sender.version,
                sender.display_name,
                sender.title,
                sender.kind,
                sender.assignment_id,
                assignment_id,
                kind,
                text,
                dump([item.model_dump(mode="json") for item in refs]),
                reply_to,
                stamp,
            ),
        )
        for ordinal, target in enumerate(targets):
            recipient = target.participant
            conn.execute(
                """
                INSERT INTO office_message_recipients(
                    message_id,tender_id,ordinal,recipient_id,recipient_version,
                    recipient_display_name,recipient_title,recipient_kind,assignment_id
                ) VALUES(?,?,?,?,?,?,?,?,?)
                """,
                (
                    message_id,
                    tender_id,
                    ordinal,
                    recipient.id,
                    recipient.version,
                    recipient.display_name,
                    recipient.title,
                    recipient.kind,
                    recipient.assignment_id,
                ),
            )
        conn.execute(
            """
            INSERT INTO office_message_receipts(
                id,tender_id,root_run_id,sender_id,operation,idempotency_key,payload_hash,message_id,created_at
            ) VALUES(?,?,?,?,?,?,?,?,?)
            """,
            (
                new_id(),
                tender_id,
                root_run_id,
                sender.id,
                operation,
                key,
                payload_hash,
                message_id,
                stamp,
            ),
        )
        self.events.append(
            tender_id,
            "message_posted",
            actor_id=sender.id,
            assignment_id=assignment_id,
            record_ref={"message_id": message_id},
            payload={
                "message_id": message_id,
                "kind": kind,
                "recipient_ids": [item.participant.id for item in targets],
            },
            idempotency_key=f"message:{message_id}:posted",
        )
        if refs:
            self.events.append(
                tender_id,
                "artifact_shared",
                actor_id=sender.id,
                assignment_id=assignment_id,
                record_ref={"message_id": message_id},
                payload={
                    "message_id": message_id,
                    "reference_ids": [item.id for item in refs],
                    "reference_kinds": [item.kind for item in refs],
                },
                idempotency_key=f"message:{message_id}:shared",
            )
        return self._message_from_row(
            conn,
            conn.execute("SELECT * FROM office_messages WHERE id=?", (message_id,)).fetchone(),
        )

    def post_manager(
        self,
        context: OfficeContext,
        recipients: list[OfficeRecipientTarget],
        kind: str,
        text: str,
        reference_requests=(),
        reply_to: str | None = None,
        idempotency_key: str | None = None,
    ) -> OfficeMessage:
        """Save one authored Manager message to exact staff target choices."""

        key = _key(idempotency_key)
        targets = self._targets(recipients)
        kind, text, reply_to = self._kind(kind), self._text(text), self._reply(reply_to)
        references = _reference_requests(reference_requests)
        tender_id = _identifier(context.tender_id, "Tender id")
        root_run_id = _identifier(context.run_id, "Root run id")
        # Target bindings and staff-result references both validate current
        # route authority.  Acquire the lock before the first SQL read.
        guard = self.routing.policy.connections.authority_guard()
        # The pinned Manager identity is enough to locate a historical receipt;
        # active-root and binding checks happen only for a new post.
        with guard, self.repo.atomic() as conn:
            sender = self._manager_base(context, conn, require_active=False)
            payload_hash = _body_hash(
                {
                    "tender_id": tender_id,
                    "root_run_id": root_run_id,
                    "sender_id": sender.id,
                    "operation": "post_manager",
                    "recipients": [item.model_dump(mode="json") for item in targets],
                    "kind": kind,
                    "text": text,
                    "references": [item.model_dump(mode="json") for item in references],
                    "reply_to": reply_to,
                }
            )
            receipt = self._receipt(conn, tender_id, root_run_id, sender.id, "post_manager", key)
            if receipt is not None:
                return self._replay(conn, receipt, payload_hash)
            sender = self._manager_base(context, conn, require_active=True)
            resolved_targets = [
                self._target(tender_id, root_run_id, item, conn) for item in targets
            ]
            bindings = tuple(item.binding for item in resolved_targets if item.binding is not None)
            self._reply_check(conn, tender_id, root_run_id, reply_to, sender)
            refs = self._resolve_references(conn, tender_id, root_run_id, references, bindings)
            return self._save(
                conn,
                tender_id=tender_id,
                root_run_id=root_run_id,
                sender=sender,
                targets=resolved_targets,
                kind=kind,
                text=text,
                refs=refs,
                reply_to=reply_to,
                message_assignment_id=None,
                operation="post_manager",
                key=key,
                payload_hash=payload_hash,
            )

    def post_staff(
        self,
        context: OfficeContext,
        kind: str,
        text: str,
        reference_requests=(),
        reply_to: str | None = None,
        idempotency_key: str | None = None,
    ) -> OfficeMessage:
        """Save an authored staff message addressed only to the Manager."""

        key = _key(idempotency_key)
        kind, text, reply_to = self._kind(kind), self._text(text), self._reply(reply_to)
        if kind not in {"question", "note", "reply"}:
            raise ValueError("Staff messages must be questions, notes or replies.")
        references = _reference_requests(reference_requests)
        tender_id = _identifier(context.tender_id, "Tender id")
        root_run_id = _identifier(context.run_id, "Root run id")
        sender_id = _identifier(context.actor_id, "Staff id")
        assignment_id = _identifier(context.assignment_id, "Assignment id")
        payload_hash = _body_hash(
            {
                "tender_id": tender_id,
                "root_run_id": root_run_id,
                "sender_id": sender_id,
                "sender_version": context.staff_version,
                "route_binding_id": context.route_binding_id,
                "assignment_id": assignment_id,
                "operation": "post_staff",
                "kind": kind,
                "text": text,
                "references": [item.model_dump(mode="json") for item in references],
                "reply_to": reply_to,
            }
        )
        with self.routing.policy.connections.authority_guard(), self.repo.atomic() as conn:
            receipt = self._receipt(conn, tender_id, root_run_id, sender_id, "post_staff", key)
            if receipt is not None:
                return self._replay(conn, receipt, payload_hash)
            sender, manager, assignment, binding = self._staff_base(
                context, conn, require_active=True
            )
            self._reply_check(conn, tender_id, root_run_id, reply_to, sender)
            refs = self._resolve_references(conn, tender_id, root_run_id, references, (binding,))
            return self._save(
                conn,
                tender_id=tender_id,
                root_run_id=root_run_id,
                sender=sender,
                targets=[_ResolvedTarget(manager, None, None)],
                kind=kind,
                text=text,
                refs=refs,
                reply_to=reply_to,
                message_assignment_id=sender.assignment_id,
                operation="post_staff",
                key=key,
                payload_hash=payload_hash,
            )

    def post_staff_result(
        self, tender_id: str, result_id: str, idempotency_key: str | None = None
    ) -> OfficeMessage:
        """Relay one saved completed staff draft with its actual authored summary."""

        tender_id, result_id, key = (
            _identifier(tender_id, "Tender id"),
            _identifier(result_id, "Staff result id"),
            _key(idempotency_key),
        )
        with self.routing.policy.connections.authority_guard(), self.repo.atomic() as conn:
            row = conn.execute(
                "SELECT * FROM office_staff_results WHERE tender_id=? AND id=?",
                (tender_id, result_id),
            ).fetchone()
            if row is None:
                raise KeyError("This staff result could not be found in the selected Tender.")
            payload = _json_load(row["payload_json"], "staff result")
            result = StaffResult.model_validate(payload)
            if result.id != result_id or result.tender_id != tender_id:
                raise ValueError("The saved staff result identity is invalid.")
            sender_id, root_run_id = result.staff_id, result.root_run_id
            payload_hash = _body_hash(
                {
                    "tender_id": tender_id,
                    "root_run_id": root_run_id,
                    "sender_id": sender_id,
                    "operation": "post_staff_result",
                    "result_id": result_id,
                }
            )
            receipt = self._receipt(
                conn, tender_id, root_run_id, sender_id, "post_staff_result", key
            )
            if receipt is not None:
                return self._replay(conn, receipt, payload_hash)
            result = StaffResult.model_validate(_json_load(row["payload_json"], "staff result"))
            assignment = self.assignments.get(tender_id, result.assignment_id)
            if assignment.status != "completed" or assignment.result_id != result.id:
                raise ValueError("Only a saved completed staff result can be relayed.")
            sender, manager, _assignment, binding = self._staff_base(
                OfficeContext(
                    self.repo,
                    tender_id,
                    result.root_run_id,
                    actor_id=result.staff_id,
                    staff_version=result.staff_version,
                    assignment_id=result.assignment_id,
                    route_binding_id=result.route_binding_id,
                ),
                conn,
                require_active=False,
            )
            # A completed assignment is permitted only because this exact
            # immutable result proves authorship; the root remains active.
            root = self._root_row(conn, tender_id, result.root_run_id)
            if root["status"] not in _ACTIVE_ROOT_STATES:
                raise OfficeConflict("The staff result's work root is no longer active.")
            reference = self._staff_result_reference(
                conn, tender_id, result.id, result.root_run_id, (binding,)
            )
            text = self._text(result.office_output.summary)
            return self._save(
                conn,
                tender_id=tender_id,
                root_run_id=result.root_run_id,
                sender=sender,
                targets=[_ResolvedTarget(manager, None, None)],
                kind="finding",
                text=text,
                refs=[reference],
                reply_to=None,
                message_assignment_id=sender.assignment_id,
                operation="post_staff_result",
                key=key,
                payload_hash=payload_hash,
            )

    @staticmethod
    def _cursor_payload(
        tender_id: str, row, staff_id: str | None, assignment_id: str | None
    ) -> str:
        payload = {
            "tender_id": tender_id,
            "position": row["_office_rowid"],
            "id": row["id"],
            "staff_id": staff_id,
            "assignment_id": assignment_id,
        }
        raw = json.dumps(
            payload, ensure_ascii=False, sort_keys=True, separators=(",", ":")
        ).encode()
        return base64.urlsafe_b64encode(raw).decode().rstrip("=")

    @staticmethod
    def _decode_cursor(
        cursor: str, tender_id: str, staff_id: str | None, assignment_id: str | None
    ) -> tuple[int, str]:
        if not isinstance(cursor, str) or not cursor:
            raise ValueError("The message cursor is invalid.")
        try:
            decoded = base64.urlsafe_b64decode(cursor + "=" * (-len(cursor) % 4))
            payload = json.loads(decoded.decode())
        except (ValueError, TypeError, UnicodeError, json.JSONDecodeError) as error:
            raise ValueError("The message cursor is invalid.") from error
        if (
            not isinstance(payload, dict)
            or payload.get("tender_id") != tender_id
            or payload.get("staff_id") != staff_id
            or payload.get("assignment_id") != assignment_id
            or type(payload.get("position")) is not int
            or payload.get("position") < 1
            or not isinstance(payload.get("id"), str)
        ):
            raise ValueError("The message cursor does not match this Tender or filter.")
        return payload["position"], payload["id"]

    def get(self, tender_id: str, message_id: str) -> OfficeMessage:
        tender_id, message_id = (
            _identifier(tender_id, "Tender id"),
            _identifier(message_id, "Message id"),
        )
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT * FROM office_messages WHERE tender_id=? AND id=?", (tender_id, message_id)
            ).fetchone()
            if row is None:
                raise KeyError("This office message could not be found in the selected Tender.")
            return self._message_from_row(conn, row)

    def page(
        self,
        tender_id: str,
        *,
        cursor: str | None = None,
        limit: int = 50,
        staff_id: str | None = None,
        assignment_id: str | None = None,
    ) -> OfficeMessagePage:
        tender_id = _identifier(tender_id, "Tender id")
        if type(limit) is not int or not 1 <= limit <= 50:
            raise ValueError("The message page limit must be between 1 and 50.")
        if staff_id is not None:
            staff_id = _identifier(staff_id, "Staff id")
        if assignment_id is not None:
            assignment_id = _identifier(assignment_id, "Assignment id")
        with self.repo.db.connect() as conn:
            if conn.execute("SELECT 1 FROM tenders WHERE id=?", (tender_id,)).fetchone() is None:
                raise KeyError("This item could not be found in the selected Tender.")
            before = None
            if cursor is not None:
                before = self._decode_cursor(cursor, tender_id, staff_id, assignment_id)
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
            items = [self._message_from_row(conn, row) for row in reversed(selected)]
            next_cursor = (
                self._cursor_payload(tender_id, selected[-1], staff_id, assignment_id)
                if has_more and selected
                else None
            )
            return OfficeMessagePage(items=items, next_cursor=next_cursor)

    def _saved_reference_allowed(
        self, conn, tender_id: str, root_run_id: str, reference, bindings: tuple[Any, ...]
    ) -> bool:
        try:
            if reference.kind == "source":
                row = conn.execute(
                    """
                    SELECT e.locator,a.id AS artifact_id,a.version,a.content_hash,a.is_current
                    FROM evidence e JOIN artifacts a ON a.id=e.artifact_id
                    WHERE a.tender_id=? AND e.id=?
                    """,
                    (tender_id, reference.id),
                ).fetchone()
                return bool(
                    row
                    and row["is_current"]
                    and row["artifact_id"] == reference.artifact_id
                    and row["version"] == reference.artifact_version
                    and row["content_hash"].lower() == reference.content_hash.lower()
                    and row["locator"] == reference.locator
                    and self._scope_allows(
                        bindings, row["artifact_id"], row["version"], row["content_hash"]
                    )
                )
            if reference.kind == "output":
                self._output_reference(conn, tender_id, reference.id, bindings)
                row = conn.execute(
                    "SELECT record_json FROM generated_outputs WHERE id=? AND tender_id=?",
                    (reference.id, tender_id),
                ).fetchone()
                payload = _json_load(row["record_json"], "generated output")
                return (
                    payload.get("sha256") == reference.sha256
                    and payload.get("filename") == reference.filename
                )
            self._staff_result_reference(conn, tender_id, reference.id, root_run_id, bindings)
            return True
        except (KeyError, ValueError, RuntimeError, sqlite3.Error):
            return False

    def for_assignment(
        self, tender_id: str, assignment_id: str, limit: int = 50
    ) -> list[OfficeMessage]:
        """Return only current, precisely targeted material for one assignment."""

        tender_id, assignment_id = (
            _identifier(tender_id, "Tender id"),
            _identifier(assignment_id, "Assignment id"),
        )
        if type(limit) is not int or not 1 <= limit <= 50:
            raise ValueError("The assignment message limit must be between 1 and 50.")
        with self.routing.policy.connections.authority_guard(), self.repo.atomic() as conn:
            assignment = self.assignments.get(tender_id, assignment_id)
            if assignment.status not in _ACTIVE_ASSIGNMENT_STATES:
                raise ValueError("Prompt material requires an active staff assignment.")
            binding = self.routing.validate_binding(tender_id, assignment.route_binding_id)
            rows = conn.execute(
                """
                SELECT m.*,m.rowid AS _office_rowid FROM office_messages m
                WHERE m.tender_id=? AND m.root_run_id=?
                  AND (
                    (m.sender_id=? AND m.sender_version=? AND m.sender_assignment_id=?)
                    OR EXISTS(
                        SELECT 1 FROM office_message_recipients r
                        WHERE r.message_id=m.id AND r.recipient_id=?
                          AND r.recipient_version=? AND r.assignment_id=?
                    )
                  )
                ORDER BY m.rowid DESC
                LIMIT ?
                """,
                (
                    tender_id,
                    assignment.root_run_id,
                    assignment.staff_id,
                    assignment.staff_version,
                    assignment.id,
                    assignment.staff_id,
                    assignment.staff_version,
                    assignment.id,
                    limit,
                ),
            ).fetchall()
            result = []
            for row in reversed(rows):
                item = self._message_from_row(conn, row)
                if all(
                    self._saved_reference_allowed(
                        conn, tender_id, assignment.root_run_id, ref, (binding,)
                    )
                    for ref in item.artifact_refs
                ):
                    result.append(item)
            return result


__all__ = ["OfficeMessageService"]
