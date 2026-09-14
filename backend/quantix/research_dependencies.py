"""Exact source-version dependencies with derived revision impact."""

from __future__ import annotations

import hashlib
import json

from .db import new_id, now
from .memory_models import DependencyImpact, MemoryDependency

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS research_dependencies(
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        owner_kind TEXT NOT NULL,
        owner_id TEXT NOT NULL,
        source_id TEXT NOT NULL,
        artifact_id TEXT NOT NULL,
        artifact_version INTEGER NOT NULL,
        content_hash TEXT NOT NULL,
        evidence_hash TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE(tender_id,owner_kind,owner_id,source_id)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS research_dependencies_tender
    ON research_dependencies(tender_id,owner_kind,owner_id)
    """,
)


def _evidence_hash(evidence: dict) -> str:
    content = {
        key: evidence.get(key)
        for key in (
            "id",
            "artifact_id",
            "locator",
            "text",
            "page",
            "sheet",
            "cell_range",
            "kind",
            "metadata",
        )
    }
    canonical = json.dumps(content, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


class DependencyService:
    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)

    def capture(
        self,
        tender_id: str,
        owner_kind: str,
        owner_id: str,
        source_ids: list[str],
    ) -> list[MemoryDependency]:
        source_ids = list(dict.fromkeys(source_ids))
        with self.repo.atomic() as conn:
            for source_id in source_ids:
                evidence = self.repo.get_evidence(tender_id, source_id)
                artifact = self.repo.get_artifact(tender_id, evidence["artifact_id"])
                if not artifact["is_current"]:
                    raise ValueError("Record dependencies using current Tender sources.")
                conn.execute(
                    """
                    INSERT INTO research_dependencies(
                        id,tender_id,owner_kind,owner_id,source_id,artifact_id,
                        artifact_version,content_hash,evidence_hash,created_at
                    ) VALUES(?,?,?,?,?,?,?,?,?,?)
                    """,
                    (
                        new_id(),
                        tender_id,
                        owner_kind,
                        owner_id,
                        source_id,
                        artifact["id"],
                        artifact["version"],
                        artifact["content_hash"],
                        _evidence_hash(evidence),
                        now(),
                    ),
                )
        return self.status(tender_id, owner_kind, owner_id).dependencies

    @staticmethod
    def _saved(row, *, current: bool, reason: str | None) -> MemoryDependency:
        return MemoryDependency(
            source_id=row["source_id"],
            artifact_id=row["artifact_id"],
            artifact_version=row["artifact_version"],
            content_hash=row["content_hash"],
            evidence_hash=row["evidence_hash"],
            current=current,
            reason=reason,
        )

    def status(self, tender_id: str, owner_kind: str, owner_id: str) -> DependencyImpact:
        with self.repo.db.connect() as conn:
            rows = conn.execute(
                """
                SELECT * FROM research_dependencies
                WHERE tender_id=? AND owner_kind=? AND owner_id=?
                ORDER BY created_at,id
                """,
                (tender_id, owner_kind, owner_id),
            ).fetchall()
        dependencies: list[MemoryDependency] = []
        reasons: list[str] = []
        for row in rows:
            reason = None
            try:
                evidence = self.repo.get_evidence(tender_id, row["source_id"])
                artifact = self.repo.get_artifact(tender_id, row["artifact_id"])
                if (
                    not artifact["is_current"]
                    or artifact["version"] != row["artifact_version"]
                    or artifact["content_hash"] != row["content_hash"]
                ):
                    reason = "source_revision_changed"
                elif evidence.get("extraction_current") is False:
                    reason = "source_extraction_changed"
                elif _evidence_hash(evidence) != row["evidence_hash"]:
                    reason = "source_evidence_changed"
            except KeyError:
                reason = "source_unavailable"
            if reason:
                reasons.append(reason)
            dependencies.append(self._saved(row, current=reason is None, reason=reason))
        unique = list(dict.fromkeys(reasons))
        return DependencyImpact(
            owner_kind=owner_kind,
            owner_id=owner_id,
            state="needs_review" if unique else "current",
            review_reasons=unique,
            dependencies=dependencies,
        )

    def affected(self, tender_id: str) -> list[DependencyImpact]:
        self.repo.get_tender(tender_id)
        with self.repo.db.connect() as conn:
            owners = conn.execute(
                """
                SELECT DISTINCT owner_kind,owner_id FROM research_dependencies
                WHERE tender_id=? ORDER BY owner_kind,owner_id
                """,
                (tender_id,),
            ).fetchall()
        return [
            impact
            for row in owners
            if (impact := self.status(tender_id, row["owner_kind"], row["owner_id"])).state
            == "needs_review"
        ]


__all__ = ["DependencyService"]
