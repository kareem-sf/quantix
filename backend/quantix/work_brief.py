"""Versioned working brief that carries the Tender Manager's progress between turns.

The brief is the Manager's own continuity record: outcome, checks, steps,
settled points, open questions, saved drafts and the next step. It is never
source evidence, a citation basis or an approval of any kind.
"""

from __future__ import annotations

import hashlib
import json

from .db import dump, new_id, now
from .research_dependencies import DependencyService
from .staff_models import OfficeConflict
from .work_brief_models import (
    MAX_BRIEF_CHARACTERS,
    BriefWorkProduct,
    WorkBrief,
    WorkBriefDraft,
)
from .work_products import WorkProductService

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS work_briefs (
        tender_id TEXT PRIMARY KEY,
        current_version INTEGER NOT NULL,
        updated_at TEXT NOT NULL
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS work_brief_versions (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL,
        version INTEGER NOT NULL,
        run_id TEXT NOT NULL,
        author TEXT NOT NULL,
        payload_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE(tender_id, version)
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS work_brief_receipts (
        tender_id TEXT NOT NULL,
        idempotency_key TEXT NOT NULL,
        payload_hash TEXT NOT NULL,
        version_id TEXT NOT NULL,
        created_at TEXT NOT NULL,
        PRIMARY KEY(tender_id, idempotency_key)
    )
    """,
)

_OWNER_KIND = "work_brief_version"


class WorkBriefService:
    def __init__(self, repo):
        self.repo = repo
        self.dependencies = DependencyService(repo)
        # Work products referenced by a brief live in their own tables.
        WorkProductService(repo)
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)

    def current(self, tender_id: str) -> WorkBrief | None:
        self.repo.get_tender(tender_id)
        with self.repo.db.connect() as conn:
            row = conn.execute(
                """
                SELECT v.* FROM work_briefs b
                JOIN work_brief_versions v
                  ON v.tender_id=b.tender_id AND v.version=b.current_version
                WHERE b.tender_id=?
                """,
                (tender_id,),
            ).fetchone()
        return None if row is None else self._from_row(row)

    def save(
        self,
        tender_id: str,
        run_id: str,
        author: str,
        draft: WorkBriefDraft,
        *,
        expected_version: int,
        inspected_source_ids: set[str],
        idempotency_key: str,
    ) -> WorkBrief:
        """Replace the whole brief as a new version.

        A settled point may cite a source read in this run, or one the previous
        version already carried. Carried sources must still be current.
        """

        self.repo.get_tender(tender_id)
        payload = draft.model_dump(mode="json")
        size = len(dump(payload))
        if size > MAX_BRIEF_CHARACTERS:
            raise ValueError(
                f"The brief is {size:,} characters; keep it under {MAX_BRIEF_CHARACTERS:,}. "
                "Put detail in a work product and list its ID instead."
            )
        payload_hash = hashlib.sha256(
            dump({"draft": payload, "expected_version": expected_version}).encode()
        ).hexdigest()
        with self.repo.atomic() as conn:
            receipt = conn.execute(
                "SELECT payload_hash,version_id FROM work_brief_receipts WHERE tender_id=? AND idempotency_key=?",
                (tender_id, idempotency_key),
            ).fetchone()
            if receipt:
                if receipt["payload_hash"] != payload_hash:
                    raise OfficeConflict("This brief save already recorded different content.")
                return self._from_row(self._version_row(conn, receipt["version_id"]))
            head = conn.execute(
                "SELECT current_version FROM work_briefs WHERE tender_id=?", (tender_id,)
            ).fetchone()
            current_version = int(head["current_version"]) if head else 0
            if expected_version != current_version:
                raise OfficeConflict(
                    f"The working brief is at version {current_version}. "
                    f"Save again with expected_version {current_version}."
                )
            carried: set[str] = set()
            if current_version:
                previous = conn.execute(
                    "SELECT payload_json FROM work_brief_versions WHERE tender_id=? AND version=?",
                    (tender_id, current_version),
                ).fetchone()
                carried = {
                    source_id
                    for point in json.loads(previous["payload_json"]).get("settled", [])
                    for source_id in point.get("source_ids", [])
                }
            source_ids = list(
                dict.fromkeys(
                    source_id for point in draft.settled for source_id in point.source_ids
                )
            )
            for source_id in source_ids:
                self._check_source(tender_id, source_id, inspected_source_ids, carried)
            for product_id in dict.fromkeys(draft.work_product_ids):
                found = conn.execute(
                    "SELECT 1 FROM work_products WHERE tender_id=? AND id=?",
                    (tender_id, product_id),
                ).fetchone()
                if found is None:
                    raise ValueError(
                        f"Work product {product_id} is not saved in this Tender. "
                        "Use list_work_products for the saved IDs."
                    )
            version, identifier, stamp = current_version + 1, new_id(), now()
            conn.execute(
                """
                INSERT INTO work_brief_versions(id,tender_id,version,run_id,author,payload_json,created_at)
                VALUES(?,?,?,?,?,?,?)
                """,
                (identifier, tender_id, version, run_id, author, dump(payload), stamp),
            )
            conn.execute(
                """
                INSERT INTO work_briefs(tender_id,current_version,updated_at) VALUES(?,?,?)
                ON CONFLICT(tender_id) DO UPDATE SET current_version=excluded.current_version,
                                                     updated_at=excluded.updated_at
                """,
                (tender_id, version, stamp),
            )
            conn.execute(
                """
                INSERT INTO work_brief_receipts(tender_id,idempotency_key,payload_hash,version_id,created_at)
                VALUES(?,?,?,?,?)
                """,
                (tender_id, idempotency_key, payload_hash, identifier, stamp),
            )
            self.dependencies.capture(tender_id, _OWNER_KIND, identifier, source_ids)
            return self._from_row(self._version_row(conn, identifier))

    def prompt_view(self, tender_id: str) -> dict:
        """A compact view for the Manager's next turn."""

        brief = self.current(tender_id)
        if brief is None:
            return {"version": 0, "note": "No working brief is saved yet."}
        view = brief.model_dump(
            mode="json",
            exclude={
                "id",
                "tender_id",
                "run_id",
                "author",
                "work_product_ids",
                "work_products",
                "dependency_state",
                "review_reasons",
                "created_at",
            },
            exclude_defaults=True,
        )
        view.update(
            version=brief.version,
            saved_at=brief.created_at,
            saved_work_products=[item.model_dump(mode="json") for item in brief.work_products],
            note="Your own progress record from earlier turns. It is not source evidence or approval.",
        )
        if brief.dependency_state != "current":
            view["sources_changed"] = (
                "A source behind a settled point has changed since this brief was saved. "
                "Recheck it before relying on that point."
            )
        return view

    def _check_source(
        self, tender_id: str, source_id: str, inspected: set[str], carried: set[str]
    ) -> None:
        if source_id not in inspected and source_id not in carried:
            raise ValueError(
                f"Source {source_id} was not read in this run. Read it before recording it as a settled point."
            )
        try:
            evidence = self.repo.get_evidence(tender_id, source_id)
            artifact = self.repo.get_artifact(tender_id, evidence["artifact_id"])
        except KeyError:
            raise ValueError(f"Source {source_id} is not part of this Tender.") from None
        if not artifact["is_current"]:
            raise ValueError(
                f"Source {source_id} comes from a superseded document. "
                "Read the current source or remove that point."
            )

    @staticmethod
    def _version_row(conn, version_id: str):
        return conn.execute(
            "SELECT * FROM work_brief_versions WHERE id=?", (version_id,)
        ).fetchone()

    def _products(self, tender_id: str, product_ids: list[str]) -> list[BriefWorkProduct]:
        if not product_ids:
            return []
        products = []
        with self.repo.db.connect() as conn:
            for product_id in product_ids:
                row = conn.execute(
                    """
                    SELECT v.id,v.product_id,v.title,v.kind,v.version FROM work_products p
                    JOIN work_product_versions v ON v.product_id=p.id AND v.version=p.current_version
                    WHERE p.tender_id=? AND p.id=?
                    """,
                    (tender_id, product_id),
                ).fetchone()
                if row is None:
                    continue
                state = self.dependencies.status(tender_id, "work_product_version", row["id"]).state
                products.append(
                    BriefWorkProduct(
                        product_id=row["product_id"],
                        title=row["title"],
                        kind=row["kind"],
                        version=row["version"],
                        dependency_state=state,
                    )
                )
        return products

    def _from_row(self, row) -> WorkBrief:
        from .work_progress import later_work_run

        payload = json.loads(row["payload_json"])
        impact = self.dependencies.status(row["tender_id"], _OWNER_KIND, row["id"])
        later_run = later_work_run(self.repo, row)
        return WorkBrief(
            **payload,
            id=row["id"],
            tender_id=row["tender_id"],
            version=row["version"],
            run_id=row["run_id"],
            author=row["author"],
            created_at=row["created_at"],
            work_products=self._products(row["tender_id"], payload.get("work_product_ids", [])),
            dependency_state=impact.state,
            review_reasons=impact.review_reasons,
            progress_current=later_run is None,
            latest_work_run_id=later_run,
        )
