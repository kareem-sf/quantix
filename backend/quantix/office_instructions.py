"""Engineer steering for a running Manager run, applied at its next turn."""

from __future__ import annotations

import hashlib
import json

from .db import new_id, now
from .execution_context import OfficeExecutionIdentity
from .instruction_models import InstructionAdmission, InstructionRevisionRequest
from .staff_models import OfficeConflict


def _key(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def _canonical_hash(value: dict) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")).hexdigest()

_STEERING_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS office_instruction_requests (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        root_id TEXT NOT NULL REFERENCES runs(id),
        kind TEXT NOT NULL CHECK (kind IN ('question','constraint','replace','urgent','cancel')),
        content TEXT NOT NULL,
        selection TEXT NOT NULL DEFAULT '',
        state TEXT NOT NULL DEFAULT 'admitted' CHECK (state IN ('admitted','applied')),
        idempotency_key TEXT NOT NULL,
        payload_hash TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE (tender_id, root_id, idempotency_key)
    )
    """,
)


class OfficeInstructionService:
    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            for statement in _STEERING_SCHEMA:
                conn.execute(statement)

    def admit(
        self, ctx: OfficeExecutionIdentity, request: InstructionRevisionRequest
    ) -> InstructionAdmission:
        """Record engineer steering for the next safe step. Admits nothing else.

        Admission never starts provider work and never edits running
        assignments: the controller applies admitted revisions at the next
        turn boundary, after the current step has fully completed.
        """

        if not ctx.tender_id or not ctx.root_run_id:
            raise ValueError("Steering requires the selected Tender and work root.")
        content = request.content.strip()
        if not content:
            raise ValueError("Steering needs actual instruction text.")
        key = _key(request.idempotency_key)
        payload_hash = _canonical_hash(request.model_dump(mode="json"))
        with self.repo.atomic() as conn:
            run = conn.execute(
                "SELECT status FROM runs WHERE tender_id=? AND id=?",
                (ctx.tender_id, ctx.root_run_id),
            ).fetchone()
            if run is None:
                raise KeyError("This item could not be found in the selected Tender.")
            if run["status"] not in {"queued", "running"}:
                raise OfficeConflict("Steer only while its work is still running.")
            existing = conn.execute(
                """
                SELECT * FROM office_instruction_requests
                WHERE tender_id=? AND root_id=? AND idempotency_key=?
                """,
                (ctx.tender_id, ctx.root_run_id, key),
            ).fetchone()
            if existing is not None:
                if existing["payload_hash"] != payload_hash:
                    raise OfficeConflict(
                        "This steering key was already used for different content."
                    )
                return InstructionAdmission(
                    id=existing["id"],
                    root_id=existing["root_id"],
                    kind=existing["kind"],
                    state=existing["state"],
                    replayed=True,
                    created_at=existing["created_at"],
                )
            identifier, stamp = new_id(), now()
            conn.execute(
                """
                INSERT INTO office_instruction_requests(
                    id,tender_id,root_id,kind,content,selection,state,
                    idempotency_key,payload_hash,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    identifier,
                    ctx.tender_id,
                    ctx.root_run_id,
                    request.kind,
                    content,
                    request.selection.strip(),
                    "admitted",
                    key,
                    payload_hash,
                    stamp,
                ),
            )
            return InstructionAdmission(
                id=identifier,
                root_id=ctx.root_run_id,
                kind=request.kind,
                state="admitted",
                replayed=False,
                created_at=stamp,
            )

    def pending_for_turn(self, tender_id: str, root_run_id: str) -> list[InstructionAdmission]:
        """Return admitted steering in application order without changing state."""

        with self.repo.db.connect() as conn:
            rows = conn.execute(
                """
                SELECT * FROM office_instruction_requests
                WHERE tender_id=? AND root_id=? AND state='admitted'
                ORDER BY
                    CASE kind WHEN 'cancel' THEN 0 WHEN 'urgent' THEN 1 ELSE 2 END,
                    created_at,id
                """,
                (tender_id, root_run_id),
            ).fetchall()
            return [
                InstructionAdmission(
                    id=row["id"],
                    root_id=row["root_id"],
                    kind=row["kind"],
                    state=row["state"],
                    replayed=False,
                    created_at=row["created_at"],
                )
                for row in rows
            ]

    def admission_text(self, tender_id: str, admission_id: str) -> str:
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT content FROM office_instruction_requests WHERE tender_id=? AND id=?",
                (tender_id, admission_id),
            ).fetchone()
            if row is None:
                raise KeyError("This item could not be found in the selected Tender.")
            return row["content"]

    def mark_applied(self, tender_id: str, admission_ids: list[str]) -> None:
        if not admission_ids:
            return
        with self.repo.atomic() as conn:
            for admission_id in admission_ids:
                conn.execute(
                    """
                    UPDATE office_instruction_requests SET state='applied'
                    WHERE tender_id=? AND id=? AND state='admitted'
                    """,
                    (tender_id, admission_id),
                )

    def list(self, tender_id: str, root_run_id: str) -> list[InstructionAdmission]:
        with self.repo.db.connect() as conn:
            rows = conn.execute(
                """
                SELECT * FROM office_instruction_requests
                WHERE tender_id=? AND root_id=?
                ORDER BY created_at,id
                """,
                (tender_id, root_run_id),
            ).fetchall()
            return [
                InstructionAdmission(
                    id=row["id"],
                    root_id=row["root_id"],
                    kind=row["kind"],
                    state=row["state"],
                    replayed=False,
                    created_at=row["created_at"],
                )
                for row in rows
            ]
