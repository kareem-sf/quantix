"""Atomic request/money reservations against one budget scope."""

from __future__ import annotations

from .db import new_id, now
from .execution_context import OfficeExecutionIdentity

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS office_budget_scopes (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL,
        root_id TEXT NOT NULL,
        max_requests INTEGER NOT NULL,
        stopped INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL,
        UNIQUE (tender_id, root_id)
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS office_usage_reservations (
        id TEXT PRIMARY KEY,
        scope_id TEXT NOT NULL,
        admitted INTEGER NOT NULL,
        late INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL
    )
    """,
)


class ResourceLeaseService:
    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)

    def ensure_scope(self, ctx: OfficeExecutionIdentity, *, max_requests: int) -> str:
        if not ctx.tender_id or not ctx.root_run_id:
            raise ValueError("Budget reservations require a Tender root.")
        with self.repo.atomic() as conn:
            row = conn.execute(
                "SELECT id FROM office_budget_scopes WHERE tender_id=? AND root_id=?",
                (ctx.tender_id, ctx.root_run_id),
            ).fetchone()
            if row:
                return row["id"]
            identifier, stamp = new_id(), now()
            conn.execute(
                """
                INSERT INTO office_budget_scopes(id,tender_id,root_id,max_requests,stopped,created_at)
                VALUES(?,?,?,?,0,?)
                """,
                (identifier, ctx.tender_id, ctx.root_run_id, max_requests, stamp),
            )
            return identifier

    def stop(self, ctx: OfficeExecutionIdentity) -> None:
        with self.repo.atomic() as conn:
            conn.execute(
                "UPDATE office_budget_scopes SET stopped=1 WHERE tender_id=? AND root_id=?",
                (ctx.tender_id, ctx.root_run_id),
            )

    def reserve(self, ctx: OfficeExecutionIdentity) -> bool:
        if not ctx.tender_id or not ctx.root_run_id:
            raise ValueError("Budget reservations require a Tender root.")
        with self.repo.atomic() as conn:
            scope = conn.execute(
                "SELECT * FROM office_budget_scopes WHERE tender_id=? AND root_id=?",
                (ctx.tender_id, ctx.root_run_id),
            ).fetchone()
            if scope is None:
                raise ValueError("Create a budget scope before reserving work.")
            if scope["stopped"]:
                conn.execute(
                    """
                    INSERT INTO office_usage_reservations(id,scope_id,admitted,late,created_at)
                    VALUES(?,?,0,1,?)
                    """,
                    (new_id(), scope["id"], now()),
                )
                return False
            used = conn.execute(
                "SELECT COUNT(*) FROM office_usage_reservations WHERE scope_id=? AND admitted=1",
                (scope["id"],),
            ).fetchone()[0]
            admitted = 1 if used < scope["max_requests"] else 0
            conn.execute(
                """
                INSERT INTO office_usage_reservations(id,scope_id,admitted,late,created_at)
                VALUES(?,?,?,?,?)
                """,
                (new_id(), scope["id"], admitted, 0, now()),
            )
            return bool(admitted)

    def counts(self, ctx: OfficeExecutionIdentity) -> dict:
        with self.repo.db.connect() as conn:
            scope = conn.execute(
                "SELECT id FROM office_budget_scopes WHERE tender_id=? AND root_id=?",
                (ctx.tender_id, ctx.root_run_id),
            ).fetchone()
            if scope is None:
                return {"admitted": 0, "late": 0}
            admitted = conn.execute(
                "SELECT COUNT(*) FROM office_usage_reservations WHERE scope_id=? AND admitted=1",
                (scope["id"],),
            ).fetchone()[0]
            late = conn.execute(
                "SELECT COUNT(*) FROM office_usage_reservations WHERE scope_id=? AND late=1",
                (scope["id"],),
            ).fetchone()[0]
            return {"admitted": int(admitted), "late": int(late)}
