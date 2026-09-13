"""Fenced ownership claims for assignment execution."""

from __future__ import annotations

from pydantic import Field

from .db import new_id, now
from .execution_context import OfficeExecutionIdentity
from .staff_models import IdentifierText, OfficeConflict, OfficeModel
from .staff_store import _key

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS office_ownership_leases (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL,
        assignment_key TEXT NOT NULL,
        owner_id TEXT NOT NULL,
        epoch INTEGER NOT NULL,
        expected_revision INTEGER NOT NULL,
        expires_at TEXT,
        active INTEGER NOT NULL DEFAULT 1,
        created_at TEXT NOT NULL,
        UNIQUE (tender_id, assignment_key, expected_revision)
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS office_ownership_transfers (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL,
        assignment_key TEXT NOT NULL,
        from_owner_id TEXT NOT NULL,
        to_owner_id TEXT NOT NULL,
        from_epoch INTEGER NOT NULL,
        to_epoch INTEGER NOT NULL,
        reason TEXT NOT NULL,
        created_at TEXT NOT NULL
    )
    """,
)


class OwnershipLease(OfficeModel):
    id: IdentifierText
    assignment_key: str
    owner_id: str
    epoch: int
    active: bool


class TransferOwnershipRequest(OfficeModel):
    assignment_key: str = Field(min_length=1, max_length=160)
    expected_epoch: int = Field(ge=1)
    new_owner_id: str = Field(min_length=1, max_length=160)
    reason: str = Field(min_length=1, max_length=2000)


class OfficeOwnershipService:
    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)

    def claim(
        self,
        ctx: OfficeExecutionIdentity,
        assignment_key: str,
        expected_revision: int,
        *,
        idempotency_key: str,
        expires_at: str | None = None,
    ) -> OwnershipLease:
        if not ctx.tender_id:
            raise ValueError("Ownership claims require a Tender.")
        if not assignment_key or not assignment_key.strip():
            raise ValueError("Ownership claims require an assignment.")
        if type(expected_revision) is not int or expected_revision < 1:
            raise ValueError("Ownership claims require a positive assignment revision.")
        _key(idempotency_key)
        with self.repo.atomic() as conn:
            existing = conn.execute(
                """
                SELECT * FROM office_ownership_leases
                WHERE tender_id=? AND assignment_key=? AND expected_revision=?
                """,
                (ctx.tender_id, assignment_key, expected_revision),
            ).fetchone()
            if existing is not None:
                if not existing["active"]:
                    raise OfficeConflict(
                        "The ownership lease is released. Claim the current revision again."
                    )
                if self._expired(existing["expires_at"]):
                    conn.execute(
                        """
                        UPDATE office_ownership_leases
                        SET owner_id=?,epoch=?,expires_at=?,active=1 WHERE id=?
                        """,
                        (ctx.actor_id, existing["epoch"] + 1, expires_at, existing["id"]),
                    )
                    return OwnershipLease(
                        id=existing["id"],
                        assignment_key=existing["assignment_key"],
                        owner_id=ctx.actor_id,
                        epoch=existing["epoch"] + 1,
                        active=True,
                    )
                if existing["owner_id"] != ctx.actor_id:
                    raise OfficeConflict("Another owner already claimed this assignment revision.")
                return OwnershipLease(
                    id=existing["id"],
                    assignment_key=existing["assignment_key"],
                    owner_id=existing["owner_id"],
                    epoch=existing["epoch"],
                    active=True,
                )
            identifier, stamp = new_id(), now()
            conn.execute(
                """
                INSERT INTO office_ownership_leases(
                    id,tender_id,assignment_key,owner_id,epoch,expected_revision,expires_at,active,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?)
                """,
                (
                    identifier,
                    ctx.tender_id,
                    assignment_key,
                    ctx.actor_id,
                    1,
                    expected_revision,
                    expires_at,
                    1,
                    stamp,
                ),
            )
            return OwnershipLease(
                id=identifier,
                assignment_key=assignment_key,
                owner_id=ctx.actor_id,
                epoch=1,
                active=True,
            )

    def transfer(
        self, ctx: OfficeExecutionIdentity, request: TransferOwnershipRequest
    ) -> OwnershipLease:
        """Move a live lease to a new owner under epoch fencing.

        Only the current owner (or the engineer) may transfer, and only from
        the exact current epoch. A transfer retires the old epoch so late
        writes under it cannot publish.
        """

        if not ctx.tender_id:
            raise ValueError("Ownership transfers require a Tender.")
        if not request.assignment_key.strip() or not request.new_owner_id.strip():
            raise ValueError("Ownership transfers require an assignment and a new owner.")
        with self.repo.atomic() as conn:
            current = conn.execute(
                """
                SELECT * FROM office_ownership_leases
                WHERE tender_id=? AND assignment_key=?
                ORDER BY epoch DESC LIMIT 1
                """,
                (ctx.tender_id, request.assignment_key),
            ).fetchone()
            if current is None or not current["active"] or self._expired(current["expires_at"]):
                raise OfficeConflict("There is no live ownership lease to transfer.")
            if current["epoch"] != request.expected_epoch:
                raise OfficeConflict("The ownership epoch changed. Refresh before transferring it.")
            if ctx.actor_kind != "engineer" and current["owner_id"] != ctx.actor_id:
                raise OfficeConflict("Only the current owner can transfer this lease.")
            stamp = now()
            conn.execute(
                """
                INSERT INTO office_ownership_transfers(
                    id,tender_id,assignment_key,from_owner_id,to_owner_id,
                    from_epoch,to_epoch,reason,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?)
                """,
                (
                    new_id(),
                    ctx.tender_id,
                    request.assignment_key,
                    current["owner_id"],
                    request.new_owner_id,
                    current["epoch"],
                    current["epoch"] + 1,
                    request.reason,
                    stamp,
                ),
            )
            conn.execute(
                "UPDATE office_ownership_leases SET owner_id=?,epoch=?,active=1 WHERE id=?",
                (request.new_owner_id, current["epoch"] + 1, current["id"]),
            )
            return OwnershipLease(
                id=current["id"],
                assignment_key=request.assignment_key,
                owner_id=request.new_owner_id,
                epoch=current["epoch"] + 1,
                active=True,
            )

    def release(self, ctx: OfficeExecutionIdentity, assignment_key: str) -> bool:
        """Release the caller's live lease without deleting its history."""

        if not ctx.tender_id or not assignment_key:
            raise ValueError("Ownership release requires the Tender and assignment.")
        with self.repo.atomic() as conn:
            current = conn.execute(
                """
                SELECT * FROM office_ownership_leases
                WHERE tender_id=? AND assignment_key=?
                ORDER BY epoch DESC LIMIT 1
                """,
                (ctx.tender_id, assignment_key),
            ).fetchone()
            if current is None or not current["active"]:
                return False
            if ctx.actor_kind != "engineer" and current["owner_id"] != ctx.actor_id:
                raise OfficeConflict("Only the current owner can release this lease.")
            conn.execute("UPDATE office_ownership_leases SET active=0 WHERE id=?", (current["id"],))
            return True

    @staticmethod
    def _expired(expires_at: str | None) -> bool:
        return expires_at is not None and expires_at <= now()
