"""General versioned work products with sanitized views and exact paging."""

from __future__ import annotations

import hashlib
import json
import re

from .db import dump, new_id, now
from .execution_context import OfficeExecutionIdentity
from .research_dependencies import DependencyService
from .research_service import ResearchService
from .staff_models import OfficeConflict
from .work_product_models import (
    WorkProductDraft,
    WorkProductPage,
    WorkProductRowPage,
    WorkProductVersion,
    WorkProductVersionSummary,
)

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS work_products (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL,
        current_version INTEGER NOT NULL,
        created_at TEXT NOT NULL
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS work_product_versions (
        id TEXT PRIMARY KEY,
        product_id TEXT NOT NULL,
        tender_id TEXT NOT NULL,
        version INTEGER NOT NULL,
        kind TEXT NOT NULL,
        title TEXT NOT NULL,
        schema_json TEXT NOT NULL,
        rows_json TEXT NOT NULL,
        content TEXT NOT NULL,
        sanitized TEXT NOT NULL,
        source_refs_json TEXT NOT NULL,
        method_refs_json TEXT NOT NULL,
        author TEXT NOT NULL,
        basis TEXT NOT NULL,
        status TEXT NOT NULL,
        sha256 TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE(product_id, version)
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS work_product_receipts (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL,
        idempotency_key TEXT NOT NULL,
        payload_hash TEXT NOT NULL,
        version_id TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE(tender_id, idempotency_key)
    )
    """,
)

_SCRIPT = re.compile(r"<script\b[^>]*>.*?</script>", re.I | re.S)
_JS_URL = re.compile(r"javascript:", re.I)


def sanitize_text(value: str) -> str:
    cleaned = _SCRIPT.sub("", value)
    return _JS_URL.sub("", cleaned)


def _hash(payload: dict) -> str:
    canonical = json.dumps(payload, ensure_ascii=False, separators=(",", ":"), sort_keys=True)
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


class WorkProductService:
    def __init__(self, repo):
        self.repo = repo
        self.dependencies = DependencyService(repo)
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)

    def save_draft(
        self, ctx: OfficeExecutionIdentity, draft: WorkProductDraft
    ) -> WorkProductVersion:
        if not ctx.tender_id:
            raise ValueError("Work products require a selected Tender.")
        self.repo.get_tender(ctx.tender_id)
        from .capability_models import validate_result_value

        validate_result_value(draft.model_dump(mode="json"))
        public_refs = [item for item in draft.source_refs if item.startswith("public_citation:")]
        local_refs = [item for item in draft.source_refs if not item.startswith("public_citation:")]
        if public_refs or any(
            item.startswith(("http://", "https://")) for item in draft.source_refs
        ):
            ResearchService(self.repo).validate_work_product_refs(ctx, draft.source_refs)
        for source_id in local_refs:
            self.repo.get_evidence(ctx.tender_id, source_id)
        payload = draft.model_dump(mode="json")
        payload_hash = _hash(payload)
        with self.repo.atomic() as conn:
            receipt = conn.execute(
                """
                SELECT version_id, payload_hash FROM work_product_receipts
                WHERE tender_id=? AND idempotency_key=?
                """,
                (ctx.tender_id, draft.idempotency_key),
            ).fetchone()
            if receipt:
                if receipt["payload_hash"] != payload_hash:
                    raise OfficeConflict("This work-product key already saved different content.")
                return self._version(conn, receipt["version_id"])
            if draft.product_id is None:
                product_id, version = new_id(), 1
                conn.execute(
                    "INSERT INTO work_products(id,tender_id,current_version,created_at) VALUES(?,?,?,?)",
                    (product_id, ctx.tender_id, 1, now()),
                )
            else:
                existing = conn.execute(
                    "SELECT id,current_version FROM work_products WHERE tender_id=? AND id=?",
                    (ctx.tender_id, draft.product_id),
                ).fetchone()
                if existing is None:
                    raise KeyError("This work product does not belong to the selected Tender.")
                if existing["current_version"] != draft.expected_version:
                    raise OfficeConflict(
                        "The work product version changed. Read its latest version before revising it."
                    )
                product_id = existing["id"]
                version = int(existing["current_version"]) + 1
                conn.execute(
                    "UPDATE work_products SET current_version=? WHERE id=?",
                    (version, product_id),
                )
            identifier, stamp = new_id(), now()
            sanitized = sanitize_text(draft.content)
            digest = _hash(
                {"rows": draft.rows, "content": sanitized, "kind": draft.kind, "version": version}
            )
            conn.execute(
                """
                INSERT INTO work_product_versions(
                    id,product_id,tender_id,version,kind,title,schema_json,rows_json,content,sanitized,
                    source_refs_json,method_refs_json,author,basis,status,sha256,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    identifier,
                    product_id,
                    ctx.tender_id,
                    version,
                    draft.kind,
                    draft.title,
                    dump(draft.view_schema),
                    dump(draft.rows),
                    draft.content,
                    sanitized,
                    dump(draft.source_refs),
                    dump(draft.method_refs),
                    ctx.actor_id,
                    digest,
                    "draft",
                    digest,
                    stamp,
                ),
            )
            conn.execute(
                """
                INSERT INTO work_product_receipts(
                    id,tender_id,idempotency_key,payload_hash,version_id,created_at
                ) VALUES(?,?,?,?,?,?)
                """,
                (new_id(), ctx.tender_id, draft.idempotency_key, payload_hash, identifier, stamp),
            )
            self.dependencies.capture(
                ctx.tender_id,
                "work_product_version",
                identifier,
                local_refs,
            )
            return self._version(conn, identifier)

    def get(self, tender_id: str, product_id: str, version: int) -> WorkProductVersion:
        with self.repo.atomic() as conn:
            row = conn.execute(
                """
                SELECT * FROM work_product_versions
                WHERE tender_id=? AND product_id=? AND version=?
                """,
                (tender_id, product_id, version),
            ).fetchone()
            if row is None:
                raise KeyError("This work product version could not be found.")
            return self._from_row(row)

    def list(self, tender_id: str, offset: int = 0, limit: int = 50) -> WorkProductPage:
        self.repo.get_tender(tender_id)
        if offset < 0 or not 1 <= limit <= 100:
            raise ValueError("Work-product paging is outside the supported range.")
        with self.repo.db.connect() as conn:
            total = int(
                conn.execute(
                    "SELECT COUNT(*) FROM work_products WHERE tender_id=?",
                    (tender_id,),
                ).fetchone()[0]
            )
            rows = conn.execute(
                """
                SELECT v.id,v.product_id,v.tender_id,v.version,v.kind,v.title,
                       v.source_refs_json,v.method_refs_json,v.author,v.basis,
                       v.status,v.sha256,v.created_at,json_array_length(v.rows_json) AS row_count
                FROM work_products p
                JOIN work_product_versions v
                  ON v.product_id=p.id AND v.version=p.current_version
                WHERE p.tender_id=?
                ORDER BY v.created_at DESC,v.id DESC
                LIMIT ? OFFSET ?
                """,
                (tender_id, limit, offset),
            ).fetchall()
        items = [self._summary(row) for row in rows]
        next_offset = offset + len(items) if offset + len(items) < total else None
        return WorkProductPage(items=items, next_offset=next_offset, total=total)

    def versions(
        self,
        tender_id: str,
        product_id: str,
        offset: int = 0,
        limit: int = 50,
    ) -> WorkProductPage:
        self.repo.get_tender(tender_id)
        if offset < 0 or not 1 <= limit <= 100:
            raise ValueError("Work-product paging is outside the supported range.")
        with self.repo.db.connect() as conn:
            product = conn.execute(
                "SELECT 1 FROM work_products WHERE tender_id=? AND id=?",
                (tender_id, product_id),
            ).fetchone()
            if product is None:
                raise KeyError("This work product does not belong to the selected Tender.")
            total = int(
                conn.execute(
                    """
                    SELECT COUNT(*) FROM work_product_versions
                    WHERE tender_id=? AND product_id=?
                    """,
                    (tender_id, product_id),
                ).fetchone()[0]
            )
            rows = conn.execute(
                """
                SELECT id,product_id,tender_id,version,kind,title,
                       source_refs_json,method_refs_json,author,basis,status,
                       sha256,created_at,json_array_length(rows_json) AS row_count
                FROM work_product_versions
                WHERE tender_id=? AND product_id=?
                ORDER BY version DESC
                LIMIT ? OFFSET ?
                """,
                (tender_id, product_id, limit, offset),
            ).fetchall()
        items = [self._summary(row) for row in rows]
        next_offset = offset + len(items) if offset + len(items) < total else None
        return WorkProductPage(items=items, next_offset=next_offset, total=total)

    def export_rows(self, tender_id: str, product_id: str, version: int) -> list[dict]:
        item = self.get(tender_id, product_id, version)
        return list(item.rows)

    def page(
        self, tender_id: str, product_id: str, version: int, offset: int, limit: int
    ) -> WorkProductRowPage:
        if offset < 0 or not 1 <= limit <= 100:
            raise ValueError("Work-product row paging is outside the supported range.")
        rows = self.export_rows(tender_id, product_id, version)
        return WorkProductRowPage(
            total=len(rows), items=rows[offset : offset + limit], missing=0
        )

    def _version(self, conn, version_id: str) -> WorkProductVersion:
        row = conn.execute(
            "SELECT * FROM work_product_versions WHERE id=?", (version_id,)
        ).fetchone()
        return self._from_row(row)

    def _from_row(self, row) -> WorkProductVersion:
        sanitized = row["sanitized"]
        dependency = self.dependencies.status(row["tender_id"], "work_product_version", row["id"])
        source_refs = json.loads(row["source_refs_json"])
        return WorkProductVersion(
            id=row["id"],
            product_id=row["product_id"],
            version=row["version"],
            kind=row["kind"],
            title=row["title"],
            view_schema=json.loads(row["schema_json"]),
            rows=json.loads(row["rows_json"]),
            content=row["content"],
            sanitized_content=sanitized,
            source_refs=source_refs,
            method_refs=json.loads(row["method_refs_json"]),
            author=row["author"],
            basis=row["basis"],
            status=row["status"],
            dependency_state=dependency.state,
            review_reasons=dependency.review_reasons,
            public_citation_refs=[
                item for item in source_refs if item.startswith("public_citation:")
            ],
            sha256=row["sha256"],
            executed_scripts=0,
            created_at=row["created_at"],
        )

    def _summary(self, row) -> WorkProductVersionSummary:
        dependency = self.dependencies.status(
            row["tender_id"], "work_product_version", row["id"]
        )
        source_refs = json.loads(row["source_refs_json"])
        return WorkProductVersionSummary(
            id=row["id"],
            product_id=row["product_id"],
            version=row["version"],
            kind=row["kind"],
            title=row["title"],
            author=row["author"],
            basis=row["basis"],
            status=row["status"],
            dependency_state=dependency.state,
            review_reasons=dependency.review_reasons,
            source_refs=source_refs,
            method_refs=json.loads(row["method_refs_json"]),
            public_citation_refs=[
                item for item in source_refs if item.startswith("public_citation:")
            ],
            sha256=row["sha256"],
            row_count=row["row_count"],
            created_at=row["created_at"],
        )
