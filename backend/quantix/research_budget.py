"""Durable public-research slots in the shared reviewed root search allowance."""

from __future__ import annotations

import hashlib
import json

from .ai_policy import search_accounting
from .db import new_id, now
from .staff_models import OfficeConflict
from .staff_routing import StaffRoutingService

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS research_search_reservations(
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        root_run_id TEXT NOT NULL REFERENCES runs(id),
        actor_id TEXT NOT NULL,
        route_binding_id TEXT,
        idempotency_key TEXT NOT NULL,
        payload_hash TEXT NOT NULL,
        url TEXT NOT NULL,
        status TEXT NOT NULL CHECK(status IN ('reserved','completed','failed')),
        receipt_id TEXT,
        created_at TEXT NOT NULL,
        completed_at TEXT,
        UNIQUE(tender_id,root_run_id,idempotency_key)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS research_search_reservations_root
    ON research_search_reservations(tender_id,root_run_id,status)
    """,
)


def research_search_usage(conn, tender_id: str, root_run_id: str) -> int:
    """Count local public fetch slots, including held and attempted calls."""

    table = conn.execute(
        """
        SELECT 1 FROM sqlite_master
        WHERE type='table' AND name='research_search_reservations'
        """
    ).fetchone()
    if table is None:
        return 0
    return int(
        conn.execute(
            """
            SELECT COUNT(*) FROM research_search_reservations
            WHERE tender_id=? AND root_run_id=?
            """,
            (tender_id, root_run_id),
        ).fetchone()[0]
    )


def _hash(value: dict) -> str:
    canonical = json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
        allow_nan=False,
    )
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


class ResearchBudgetService:
    def __init__(self, repo):
        self.repo = repo
        self.routing = StaffRoutingService(repo)
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)

    @staticmethod
    def _plan_id(context) -> str:
        binding = getattr(context, "route_binding", None)
        if binding is not None and getattr(binding, "plan_id", None):
            return binding.plan_id
        approved = getattr(context, "approved_scope", None) or {}
        plan_id = approved.get("plan_id") if isinstance(approved, dict) else None
        if not isinstance(plan_id, str) or not plan_id:
            raise ValueError(
                "Public research needs an approved work review with an online-search allowance."
            )
        return plan_id

    def reserve(self, context, *, url: str, idempotency_key: str) -> dict:
        tender_id = getattr(context, "tender_id", None)
        root_run_id = getattr(context, "run_id", None)
        actor_id = getattr(context, "actor_id", None) or "manager"
        if not all(isinstance(value, str) and value for value in (tender_id, root_run_id)):
            raise ValueError("Public research needs an active Tender root.")
        plan_id = self._plan_id(context)
        payload_hash = _hash(
            {
                "tender_id": tender_id,
                "root_run_id": root_run_id,
                "actor_id": actor_id,
                "route_binding_id": getattr(context, "route_binding_id", None),
                "url": url,
            }
        )
        with self.routing.policy.connections.authority_guard(), self.repo.atomic() as conn:
            existing = conn.execute(
                """
                SELECT * FROM research_search_reservations
                WHERE tender_id=? AND root_run_id=? AND idempotency_key=?
                """,
                (tender_id, root_run_id, idempotency_key),
            ).fetchone()
            if existing is not None:
                if existing["payload_hash"] != payload_hash:
                    raise OfficeConflict(
                        "This public-research key was already used for a different URL."
                    )
                return dict(existing)

            grant, _policy, _run = self.routing._validate_root_in_conn(
                conn, tender_id, root_run_id, plan_id
            )
            if getattr(context, "is_staff", False):
                binding = self.routing.validate_binding(tender_id, context.route_binding_id)
                if binding.root_run_id != root_run_id or binding.plan_id != plan_id:
                    raise ValueError(
                        "The staff research request does not match its reviewed route binding."
                    )

            native_calls = 0
            for row in conn.execute(
                "SELECT data_json FROM ai_usage WHERE tender_id=? AND run_id=?",
                (tender_id, root_run_id),
            ):
                accounting = search_accounting(json.loads(row[0]))
                if accounting["unknown"] or accounting["overrun"]:
                    raise ValueError(
                        "Online-search usage needs review before another public source is opened."
                    )
                native_calls += int(accounting["calls"])
            local_calls = research_search_usage(conn, tender_id, root_run_id)
            if native_calls + local_calls + 1 > grant.envelope.max_search_calls:
                raise ValueError(
                    "The approved online-search allowance cannot cover another public source."
                )

            identifier, stamp = new_id(), now()
            conn.execute(
                """
                INSERT INTO research_search_reservations(
                    id,tender_id,root_run_id,actor_id,route_binding_id,
                    idempotency_key,payload_hash,url,status,receipt_id,created_at,completed_at
                ) VALUES(?,?,?,?,?,?,?,?,'reserved',NULL,?,NULL)
                """,
                (
                    identifier,
                    tender_id,
                    root_run_id,
                    actor_id,
                    getattr(context, "route_binding_id", None),
                    idempotency_key,
                    payload_hash,
                    url,
                    stamp,
                ),
            )
            return dict(
                conn.execute(
                    "SELECT * FROM research_search_reservations WHERE id=?",
                    (identifier,),
                ).fetchone()
            )

    def complete(
        self, reservation_id: str, *, receipt_id: str | None, failed: bool = False
    ) -> None:
        with self.repo.atomic() as conn:
            row = conn.execute(
                "SELECT status FROM research_search_reservations WHERE id=?",
                (reservation_id,),
            ).fetchone()
            if row is None:
                raise KeyError("The public-research reservation could not be found.")
            if row["status"] != "reserved":
                return
            conn.execute(
                """
                UPDATE research_search_reservations
                SET status=?,receipt_id=?,completed_at=?
                WHERE id=? AND status='reserved'
                """,
                ("failed" if failed else "completed", receipt_id, now(), reservation_id),
            )


__all__ = ["ResearchBudgetService", "research_search_usage"]
