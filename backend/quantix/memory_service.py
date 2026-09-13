"""Tender working memory layered over existing decisions and approved knowledge."""

from __future__ import annotations

import hashlib
import json
from contextlib import nullcontext
from datetime import UTC, date, datetime
from typing import Callable

from .db import dump, new_id, now
from .knowledge import KnowledgeService
from .memory_models import (
    MemoryOverview,
    MemoryPromotionRequest,
    WorkingMemoryCommand,
    WorkingMemoryRecord,
)
from .research_dependencies import DependencyService
from .staff_models import OfficeConflict

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS tender_working_memory(
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        run_id TEXT NOT NULL,
        actor_id TEXT NOT NULL,
        payload_json TEXT NOT NULL,
        payload_hash TEXT NOT NULL,
        idempotency_key TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE(tender_id,run_id,idempotency_key)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS tender_working_memory_order
    ON tender_working_memory(tender_id,created_at,id)
    """,
    """
    CREATE TABLE IF NOT EXISTS memory_promotions(
        memory_id TEXT PRIMARY KEY REFERENCES tender_working_memory(id),
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        knowledge_id TEXT NOT NULL,
        request_hash TEXT NOT NULL,
        created_at TEXT NOT NULL
    )
    """,
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


class MemoryService:
    def __init__(self, repo):
        self.repo = repo
        self.dependencies = DependencyService(repo)
        self.knowledge = KnowledgeService(repo)
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)

    @staticmethod
    def _identity(identity) -> tuple[str, str, str, str]:
        tender_id = getattr(identity, "tender_id", None)
        if not isinstance(tender_id, str) or not tender_id:
            raise ValueError("Working memory requires a selected Tender.")
        return (
            tender_id,
            getattr(identity, "root_run_id", None) or "",
            getattr(identity, "actor_id", None) or "engineer",
            getattr(identity, "actor_kind", None) or "engineer",
        )

    def save(
        self,
        identity,
        command: WorkingMemoryCommand | dict,
        *,
        inspected_source_ids: set[str] | None = None,
        write_guard: Callable[[], object] | None = None,
        validate_commit: Callable[[], None] | None = None,
    ) -> WorkingMemoryRecord:
        command = WorkingMemoryCommand.model_validate(command)
        tender_id, run_id, actor_id, actor_kind = self._identity(identity)
        self.repo.get_tender(tender_id)
        if actor_kind != "engineer" and any(
            item not in (inspected_source_ids or set()) for item in command.source_ids
        ):
            raise ValueError("Working memory can cite only Tender sources inspected by this actor.")
        payload = command.model_dump(mode="json", exclude={"idempotency_key"})
        payload_hash = _hash(payload)
        with (write_guard() if write_guard else nullcontext()), self.repo.atomic() as conn:
            if validate_commit:
                validate_commit()
            existing = conn.execute(
                """
                SELECT id,payload_hash FROM tender_working_memory
                WHERE tender_id=? AND run_id=? AND idempotency_key=?
                """,
                (tender_id, run_id, command.idempotency_key),
            ).fetchone()
            if existing is not None:
                if existing["payload_hash"] != payload_hash:
                    raise OfficeConflict(
                        "This working-memory key was already used for different content."
                    )
                return self.get(tender_id, existing["id"])
            identifier, stamp = new_id(), now()
            conn.execute(
                """
                INSERT INTO tender_working_memory(
                    id,tender_id,run_id,actor_id,payload_json,payload_hash,
                    idempotency_key,created_at
                ) VALUES(?,?,?,?,?,?,?,?)
                """,
                (
                    identifier,
                    tender_id,
                    run_id,
                    actor_id,
                    dump(payload),
                    payload_hash,
                    command.idempotency_key,
                    stamp,
                ),
            )
            self.dependencies.capture(tender_id, "working_memory", identifier, command.source_ids)
            return self.get(tender_id, identifier)

    def get(self, tender_id: str, memory_id: str) -> WorkingMemoryRecord:
        with self.repo.db.connect() as conn:
            row = conn.execute(
                """
                SELECT * FROM tender_working_memory
                WHERE tender_id=? AND id=?
                """,
                (tender_id, memory_id),
            ).fetchone()
        if row is None:
            raise KeyError("This working-memory note could not be found.")
        payload = json.loads(row["payload_json"])
        impact = self.dependencies.status(tender_id, "working_memory", memory_id)
        reasons = list(impact.review_reasons)
        if (
            payload.get("valid_until")
            and date.fromisoformat(payload["valid_until"]) < datetime.now(UTC).date()
        ):
            reasons.append("validity_date_reached")
        reasons = list(dict.fromkeys(reasons))
        return WorkingMemoryRecord(
            id=row["id"],
            tender_id=row["tender_id"],
            run_id=row["run_id"] or None,
            actor_id=row["actor_id"],
            created_at=row["created_at"],
            dependencies=impact.dependencies,
            state="needs_review" if reasons else "current",
            review_reasons=reasons,
            **payload,
        )

    def list(self, tender_id: str, *, offset: int = 0, limit: int = 100):
        self.repo.get_tender(tender_id)
        if offset < 0 or not 1 <= limit <= 100:
            raise ValueError("Read 1 to 100 working notes from a nonnegative offset.")
        with self.repo.db.connect() as conn:
            rows = conn.execute(
                """
                SELECT id FROM tender_working_memory WHERE tender_id=?
                ORDER BY created_at DESC,id DESC LIMIT ? OFFSET ?
                """,
                (tender_id, limit, offset),
            ).fetchall()
        return [self.get(tender_id, row["id"]) for row in rows]

    def list_for_identity(
        self, identity, *, offset: int = 0, limit: int = 100
    ) -> tuple[list[WorkingMemoryRecord], int]:
        """Return only scratch notes belonging to this exact actor root."""

        tender_id, run_id, actor_id, _actor_kind = self._identity(identity)
        self.repo.get_tender(tender_id)
        if offset < 0 or not 1 <= limit <= 100:
            raise ValueError("Read 1 to 100 working notes from a nonnegative offset.")
        with self.repo.db.connect() as conn:
            all_count = int(
                conn.execute(
                    "SELECT COUNT(*) FROM tender_working_memory WHERE tender_id=?",
                    (tender_id,),
                ).fetchone()[0]
            )
            allowed_count = int(
                conn.execute(
                    """
                    SELECT COUNT(*) FROM tender_working_memory
                    WHERE tender_id=? AND run_id=? AND actor_id=?
                    """,
                    (tender_id, run_id, actor_id),
                ).fetchone()[0]
            )
            rows = conn.execute(
                """
                SELECT id FROM tender_working_memory
                WHERE tender_id=? AND run_id=? AND actor_id=?
                ORDER BY created_at DESC,id DESC LIMIT ? OFFSET ?
                """,
                (tender_id, run_id, actor_id, limit, offset),
            ).fetchall()
        return (
            [self.get(tender_id, row["id"]) for row in rows],
            all_count - allowed_count,
        )

    def affected(self, tender_id: str):
        return self.dependencies.affected(tender_id)

    def promote(
        self,
        tender_id: str,
        memory_id: str,
        request: MemoryPromotionRequest | dict,
    ) -> dict:
        request = MemoryPromotionRequest.model_validate(request)
        saved = self.get(tender_id, memory_id)
        if saved.state != "current":
            raise ValueError(
                "Review this working note against current sources before promoting it."
            )
        request_hash = _hash(request.model_dump(mode="json"))
        with self.repo.atomic() as conn:
            existing = conn.execute(
                "SELECT knowledge_id,request_hash FROM memory_promotions WHERE memory_id=?",
                (memory_id,),
            ).fetchone()
            if existing is not None:
                if existing["request_hash"] != request_hash:
                    raise OfficeConflict(
                        "This working note was already promoted with a different decision."
                    )
                return self.knowledge.get(existing["knowledge_id"])
            knowledge = self.knowledge.create(
                {
                    "title": saved.title,
                    "content": saved.content,
                    "category": request.category,
                    "source_tender_id": tender_id if saved.source_ids else None,
                    "source_ids": saved.source_ids,
                    "verified_on": request.verified_on,
                    "recheck_after": request.recheck_after,
                    "engineer_confirmed": True,
                    "rationale": request.rationale,
                }
            )
            conn.execute(
                """
                INSERT INTO memory_promotions(
                    memory_id,tender_id,knowledge_id,request_hash,created_at
                ) VALUES(?,?,?,?,?)
                """,
                (memory_id, tender_id, knowledge["id"], request_hash, now()),
            )
            return knowledge

    def overview(self, tender_id: str) -> MemoryOverview:
        self.repo.get_tender(tender_id)
        with self.repo.db.connect() as conn:
            decisions = [
                dict(row)
                for row in conn.execute(
                    "SELECT * FROM decisions WHERE tender_id=? ORDER BY created_at DESC,id DESC",
                    (tender_id,),
                )
            ]
        return MemoryOverview(
            working_memory=self.list(tender_id),
            approved_decisions=decisions,
            company_knowledge=self.knowledge.list(),
        )


__all__ = ["MemoryService"]
