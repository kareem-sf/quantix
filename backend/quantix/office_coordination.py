"""Idempotent delivery, acknowledgements and question grouping."""

from __future__ import annotations

from .db import new_id, now
from .execution_context import OfficeExecutionIdentity
from .office_delivery_models import (
    CoordinationRequest,
    DeliveryReceipt,
    ReceiptRequest,
)
from .office_messages import OfficeMessageService
from .office_tools import OfficeContext
from .staff_models import OfficeConflict
from .staff_store import _canonical_hash, _key

_FORBIDDEN = ("checked", "accepted", "engineer_confirmed", "decision")

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS office_delivery_receipts (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        message_id TEXT NOT NULL,
        recipient_assignment_id TEXT,
        state TEXT NOT NULL,
        extent TEXT NOT NULL,
        idempotency_key TEXT NOT NULL,
        payload_hash TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE (tender_id, idempotency_key, state)
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS office_question_groups (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL,
        root_run_id TEXT NOT NULL,
        correlation_key TEXT NOT NULL,
        message_id TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE (tender_id, root_run_id, correlation_key)
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS office_ownership_requests (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL,
        assignment_id TEXT,
        kind TEXT NOT NULL,
        state TEXT NOT NULL,
        created_at TEXT NOT NULL
    )
    """,
)


def _reject_forged_status(payload: dict) -> None:
    for key in _FORBIDDEN:
        if payload.get(key) is True:
            raise ValueError(
                "Checked and accepted states cannot be set on a message. They come from engineer review."
            )


def _correlation(text: str, source_ids: list[str], source_versions: list[int]) -> str:
    return _canonical_hash(
        {
            "text": " ".join(text.lower().split()),
            "source_ids": list(source_ids),
            "source_versions": list(source_versions),
        }
    )


class OfficeCoordinationService:
    def __init__(self, repo):
        self.repo = repo
        self.messages = OfficeMessageService(repo)
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)
            columns = {
                row["name"]
                for row in conn.execute("PRAGMA table_info(office_delivery_receipts)")
            }
            if "required_response" not in columns:
                conn.execute(
                    "ALTER TABLE office_delivery_receipts ADD COLUMN required_response TEXT"
                )

    def _office_context(self, ctx: OfficeExecutionIdentity) -> OfficeContext:
        if not ctx.tender_id or not ctx.root_run_id:
            raise ValueError("Office coordination requires a Tender work root.")
        context = OfficeContext(self.repo, ctx.tender_id, ctx.root_run_id)
        from .manager_runtime import ManagerRunProfiles

        pinned = ManagerRunProfiles(self.repo).get(ctx.tender_id, ctx.root_run_id)
        context.actor_id = pinned.id
        context.assignment_id = ctx.assignment_id
        context.route_binding_id = ctx.route_binding_id
        return context

    def deliver(self, ctx: OfficeExecutionIdentity, request: CoordinationRequest) -> DeliveryReceipt:
        _reject_forged_status(request.model_dump(mode="json"))
        tender_id = ctx.tender_id
        root_run_id = ctx.root_run_id or request.root_run_id
        if not tender_id or not root_run_id:
            raise ValueError("Office coordination requires a Tender work root.")
        ctx = OfficeExecutionIdentity(
            tender_id=tender_id,
            actor_kind=ctx.actor_kind,
            actor_id=ctx.actor_id,
            root_run_id=root_run_id,
            budget_scope_id=ctx.budget_scope_id,
            assignment_id=ctx.assignment_id,
            profile_version=ctx.profile_version,
            route_binding_id=ctx.route_binding_id,
            instruction_revision_id=ctx.instruction_revision_id,
            grant_fingerprint=ctx.grant_fingerprint,
            ownership_epoch=ctx.ownership_epoch,
            trusted_invocation_id=ctx.trusted_invocation_id,
        )
        key = _key(request.idempotency_key)
        payload_hash = _canonical_hash(request.model_dump(mode="json"))
        context = self._office_context(ctx)
        from .office_message_models import OfficeRecipientTarget, OfficeSourceReferenceRequest

        refs = [
            OfficeSourceReferenceRequest(source_id=item)
            for item in request.source_ids
        ]
        correlation = _correlation(request.text, request.source_ids, request.source_versions)
        with self.repo.atomic() as conn:
            existing = conn.execute(
                """
                SELECT message_id,payload_hash,created_at,recipient_assignment_id,extent,required_response
                FROM office_delivery_receipts
                WHERE tender_id=? AND idempotency_key=? AND state='delivered'
                """,
                (tender_id, key),
            ).fetchone()
            if existing is not None:
                if existing["payload_hash"] != payload_hash:
                    raise OfficeConflict("This delivery key was already used for different content.")
                return DeliveryReceipt(
                    message_id=existing["message_id"],
                    recipient_assignment_id=existing["recipient_assignment_id"],
                    state="delivered",
                    extent=existing["extent"],
                    required_response=existing["required_response"],
                    at=existing["created_at"],
                    replayed=True,
                )
            grouped = conn.execute(
                """
                SELECT message_id FROM office_question_groups
                WHERE tender_id=? AND root_run_id=? AND correlation_key=?
                """,
                (tender_id, ctx.root_run_id, correlation),
            ).fetchone()
            if grouped is not None and request.kind == "question":
                message_id = grouped["message_id"]
            else:
                message = self.messages.post_manager(
                    context,
                    [
                        OfficeRecipientTarget(
                            staff_id=request.recipient_staff_id,
                            assignment_id=request.recipient_assignment_id,
                        )
                    ],
                    "question" if request.kind == "question" else "instruction",
                    request.text,
                    reference_requests=refs,
                    reply_to=request.reply_to,
                    idempotency_key=key,
                )
                message_id = message.id
                if request.kind == "question":
                    conn.execute(
                        """
                        INSERT OR IGNORE INTO office_question_groups(
                            id,tender_id,root_run_id,correlation_key,message_id,created_at
                        ) VALUES(?,?,?,?,?,?)
                        """,
                        (new_id(), tender_id, ctx.root_run_id, correlation, message_id, now()),
                    )
            stamp = now()
            conn.execute(
                """
                INSERT INTO office_delivery_receipts(
                    id,tender_id,message_id,recipient_assignment_id,state,extent,
                    idempotency_key,payload_hash,required_response,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    new_id(),
                    tender_id,
                    message_id,
                    request.recipient_assignment_id,
                    "delivered",
                    "received",
                    key,
                    payload_hash,
                    request.required_response,
                    stamp,
                ),
            )
            if request.kind == "work_transfer":
                conn.execute(
                    """
                    INSERT INTO office_ownership_requests(
                        id,tender_id,assignment_id,kind,state,created_at
                    ) VALUES(?,?,?,?,?,?)
                    """,
                    (
                        new_id(),
                        tender_id,
                        request.recipient_assignment_id,
                        request.kind,
                        "requested",
                        stamp,
                    ),
                )
            return DeliveryReceipt(
                message_id=message_id,
                recipient_assignment_id=request.recipient_assignment_id,
                state="delivered",
                extent="received",
                required_response=request.required_response,
                at=stamp,
                replayed=False,
            )

    def acknowledge(self, ctx: OfficeExecutionIdentity, request: ReceiptRequest) -> DeliveryReceipt:
        _reject_forged_status(request.model_dump(mode="json"))
        if request.extent in {"checked", "accepted"}:
            raise ValueError("Received does not mean checked or accepted.")
        tender_id = ctx.tender_id
        if not tender_id:
            raise ValueError("Office coordination requires a selected Tender.")
        key = _key(request.idempotency_key)
        payload_hash = _canonical_hash(request.model_dump(mode="json"))
        with self.repo.atomic() as conn:
            existing = conn.execute(
                """
                SELECT message_id,payload_hash,created_at,recipient_assignment_id,extent,required_response
                FROM office_delivery_receipts
                WHERE tender_id=? AND idempotency_key=? AND state='acknowledged'
                """,
                (tender_id, key),
            ).fetchone()
            if existing is not None:
                if existing["payload_hash"] != payload_hash:
                    raise OfficeConflict("This acknowledgement key was already used for different content.")
                return DeliveryReceipt(
                    message_id=existing["message_id"],
                    recipient_assignment_id=existing["recipient_assignment_id"],
                    state="acknowledged",
                    extent=existing["extent"],
                    required_response=existing["required_response"],
                    at=existing["created_at"],
                    replayed=True,
                )
            delivered = conn.execute(
                """
                SELECT required_response FROM office_delivery_receipts
                WHERE tender_id=? AND message_id=? AND state='delivered'
                """,
                (tender_id, request.message_id),
            ).fetchone()
            if delivered is None:
                raise ValueError("Acknowledge a message after it has been delivered.")
            stamp = now()
            conn.execute(
                """
                INSERT INTO office_delivery_receipts(
                    id,tender_id,message_id,recipient_assignment_id,state,extent,
                    idempotency_key,payload_hash,required_response,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    new_id(),
                    tender_id,
                    request.message_id,
                    request.recipient_assignment_id,
                    "acknowledged",
                    request.extent,
                    key,
                    payload_hash,
                    delivered["required_response"],
                    stamp,
                ),
            )
            return DeliveryReceipt(
                message_id=request.message_id,
                recipient_assignment_id=request.recipient_assignment_id,
                state="acknowledged",
                extent=request.extent,
                required_response=delivered["required_response"],
                at=stamp,
                replayed=False,
            )

    def delivery_for(self, tender_id: str, message_id: str) -> DeliveryReceipt:
        """Return the latest durable delivery state for one retained message."""
        if not tender_id or not message_id:
            raise ValueError("Delivery state requires the selected Tender and message.")
        with self.repo.db.connect() as conn:
            message = conn.execute(
                "SELECT id FROM office_messages WHERE tender_id=? AND id=?",
                (tender_id, message_id),
            ).fetchone()
            if message is None:
                raise KeyError("This item could not be found in the selected Tender.")
            row = conn.execute(
                """
                SELECT message_id,recipient_assignment_id,state,extent,required_response,created_at
                FROM office_delivery_receipts
                WHERE tender_id=? AND message_id=?
                ORDER BY created_at DESC,rowid DESC LIMIT 1
                """,
                (tender_id, message_id),
            ).fetchone()
            if row is None:
                raise KeyError("This item could not be found in the selected Tender.")
            return DeliveryReceipt(
                message_id=row["message_id"],
                recipient_assignment_id=row["recipient_assignment_id"],
                state=row["state"],
                extent=row["extent"],
                required_response=row["required_response"],
                at=row["created_at"],
            )

    def count(
        self,
        tender_id: str,
        message_id: str,
        state: str,
        *,
        idempotency_key: str | None = None,
    ) -> int:
        with self.repo.db.connect() as conn:
            row = conn.execute(
                """
                SELECT COUNT(*) FROM office_delivery_receipts
                WHERE tender_id=? AND message_id=? AND state=?
                  AND (? IS NULL OR idempotency_key=?)
                """,
                (tender_id, message_id, state, idempotency_key, idempotency_key),
            ).fetchone()
            return int(row[0])
