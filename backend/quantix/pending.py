"""Durable, Tender-scoped pending Manager instructions.

Pending instructions are deliberately kept separate from the engineer/Manager
dialogue.  A draft can wait behind work, be held after an unsafe stop or
failure, and be consumed exactly once by an explicit confirmation.
"""

from __future__ import annotations

import hashlib
import re
from typing import TYPE_CHECKING, Literal

from pydantic import BaseModel, ConfigDict, Field

from .db import dump, new_id, now, record

if TYPE_CHECKING:
    from .repository import Repository


_KEY_RE = re.compile(r"^[A-Za-z0-9._~-]{1,160}$")
PendingStatus = Literal["pending", "held"]
PendingHoldReason = Literal[
    "failed",
    "cancelled",
    "interrupted",
    "stopped",
    "restored",
    "permission",
    "model",
    "budget",
    "source",
    "work",
]


class PendingInstruction(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: str
    tender_id: str
    content: str
    action: Literal["review_documents"] | None = None
    status: PendingStatus
    hold_reason: PendingHoldReason | None = None
    idempotency_key: str
    wait_for_run_ids: list[str] = Field(default_factory=list)
    revision: int
    created_at: str
    updated_at: str


class PendingInstructionEdit(BaseModel):
    model_config = ConfigDict(extra="forbid")

    content: str = Field(min_length=1, max_length=20000)
    pending_id: str
    expected_revision: int = Field(ge=1)
    action: Literal["review_documents"] | None = None


class PendingInstructionConfirm(BaseModel):
    model_config = ConfigDict(extra="forbid")

    pending_id: str
    expected_revision: int = Field(ge=1)


class PendingInstructionCancel(BaseModel):
    model_config = ConfigDict(extra="forbid")

    pending_id: str
    expected_revision: int = Field(ge=1)


_SCHEMA = """
CREATE TABLE IF NOT EXISTS pending_instructions (
 id TEXT PRIMARY KEY,
 tender_id TEXT NOT NULL REFERENCES tenders(id),
 content TEXT NOT NULL,
 action TEXT,
 content_hash TEXT NOT NULL,
 idempotency_key TEXT NOT NULL,
 wait_for_run_ids_json TEXT NOT NULL DEFAULT '[]',
 status TEXT NOT NULL DEFAULT 'pending',
 hold_reason TEXT,
 revision INTEGER NOT NULL DEFAULT 1,
 created_at TEXT NOT NULL,
 updated_at TEXT NOT NULL,
 UNIQUE(tender_id)
);
CREATE INDEX IF NOT EXISTS pending_instructions_wait ON pending_instructions(status,updated_at);
CREATE TABLE IF NOT EXISTS message_idempotency (
 tender_id TEXT NOT NULL REFERENCES tenders(id),
 idempotency_key TEXT NOT NULL,
 content_hash TEXT NOT NULL,
 outcome_kind TEXT NOT NULL,
 pending_id TEXT,
 run_id TEXT,
 outcome_json TEXT NOT NULL DEFAULT '{}',
 created_at TEXT NOT NULL,
 updated_at TEXT NOT NULL,
 PRIMARY KEY(tender_id,idempotency_key)
);
"""


def ensure_schema(repo: "Repository") -> None:
    with repo.db.connect(write=True) as conn:
        for statement in _SCHEMA.split(";"):
            if statement.strip():
                conn.execute(statement)
        columns = {row[1] for row in conn.execute("PRAGMA table_info(pending_instructions)")}
        if "action" not in columns:
            conn.execute("ALTER TABLE pending_instructions ADD COLUMN action TEXT")
        if "revision" not in columns:
            conn.execute(
                "ALTER TABLE pending_instructions ADD COLUMN revision INTEGER NOT NULL DEFAULT 1"
            )


def _content_hash(content: str, action: str | None = None) -> str:
    return hashlib.sha256(f"{action or ''}\0{content}".encode("utf-8")).hexdigest()


def _clean_content(content: str) -> str:
    if not isinstance(content, str) or not content.strip() or len(content) > 20000:
        raise ValueError("Enter a message (up to 20000 characters).")
    return content.strip()


def _clean_key(key: str | None) -> str:
    if key is None:
        return new_id()
    if not isinstance(key, str) or not _KEY_RE.fullmatch(key):
        raise ValueError("Use a valid idempotency key.")
    return key


class PendingInstructionService:
    """Read and mutate the one current pending instruction for a Tender."""

    def __init__(self, repo: "Repository"):
        self.repo = repo
        ensure_schema(repo)

    @staticmethod
    def _public(row: dict | None) -> dict | None:
        if row is None or row.get("status") in {"cancelled", "consumed"}:
            return None
        return {
            key: row[key]
            for key in (
                "id",
                "tender_id",
                "content",
                "action",
                "status",
                "hold_reason",
                "idempotency_key",
                "wait_for_run_ids",
                "revision",
                "created_at",
                "updated_at",
            )
        }

    def get(self, tender_id: str) -> dict | None:
        self.repo.get_tender(tender_id)
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT * FROM pending_instructions WHERE tender_id=?", (tender_id,)
            ).fetchone()
        return self._public(record(row) if row else None)

    def lookup_idempotency(
        self, tender_id: str, key: str, content: str, action: str | None = None
    ) -> dict | None:
        """Return an existing submission, or raise on body reuse with changed content."""

        key = _clean_key(key)
        content = _clean_content(content)
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT * FROM message_idempotency WHERE tender_id=? AND idempotency_key=?",
                (tender_id, key),
            ).fetchone()
        if row is None:
            return None
        saved = record(row)
        if saved["content_hash"] != _content_hash(content, action):
            raise ValueError("This idempotency key was already used with a different message.")
        if saved.get("pending_id"):
            with self.repo.db.connect() as conn:
                current = conn.execute(
                    "SELECT content_hash FROM pending_instructions WHERE id=?",
                    (saved["pending_id"],),
                ).fetchone()
            if current is not None and current["content_hash"] != saved["content_hash"]:
                raise ValueError(
                    "This idempotency key no longer identifies the current pending message."
                )
        return saved

    def remember_idempotency(
        self,
        tender_id: str,
        key: str,
        content: str,
        *,
        outcome_kind: str,
        pending_id: str | None = None,
        run_id: str | None = None,
        outcome: dict | None = None,
        action: str | None = None,
    ) -> None:
        key = _clean_key(key)
        content = _clean_content(content)
        if outcome_kind not in {"immediate", "pending"}:
            raise ValueError("Unknown message submission outcome.")
        stamp = now()
        with self.repo.db.connect() as conn:
            conn.execute(
                """INSERT INTO message_idempotency
                (tender_id,idempotency_key,content_hash,outcome_kind,pending_id,run_id,outcome_json,created_at,updated_at)
                VALUES(?,?,?,?,?,?,?,?,?)
                ON CONFLICT(tender_id,idempotency_key) DO UPDATE SET
                outcome_kind=excluded.outcome_kind,pending_id=excluded.pending_id,run_id=excluded.run_id,outcome_json=excluded.outcome_json,updated_at=excluded.updated_at""",
                (
                    tender_id,
                    key,
                    _content_hash(content, action),
                    outcome_kind,
                    pending_id,
                    run_id,
                    dump(outcome or {}),
                    stamp,
                    stamp,
                ),
            )

    def upsert(
        self,
        tender_id: str,
        content: str,
        idempotency_key: str | None,
        wait_for_run_ids: list[str] | None,
        action: str | None = None,
    ) -> dict:
        """Insert or edit the current draft while preserving its single identity."""

        self.repo.get_tender(tender_id)
        content = _clean_content(content)
        key = _clean_key(idempotency_key)
        if action not in {None, "review_documents"}:
            raise ValueError("Unknown message action.")
        digest = _content_hash(content, action)
        waits = list(dict.fromkeys(str(value) for value in (wait_for_run_ids or [])))
        if len(waits) > 200:
            raise ValueError("A pending instruction cannot wait for more than 200 work items.")
        existing_submission = self.lookup_idempotency(tender_id, key, content, action)
        if existing_submission and existing_submission.get("pending_id"):
            with self.repo.db.connect() as conn:
                row = conn.execute(
                    "SELECT * FROM pending_instructions WHERE id=?",
                    (existing_submission["pending_id"],),
                ).fetchone()
            if row:
                return record(row)
        with self.repo.db.connect(write=True) as conn:
            row = conn.execute(
                "SELECT * FROM pending_instructions WHERE tender_id=?", (tender_id,)
            ).fetchone()
            stamp = now()
            if row:
                current = record(row)
                if current["status"] in {"pending", "held"}:
                    if current["idempotency_key"] != key:
                        raise ValueError(
                            "A pending instruction already exists. Edit or cancel it before sending another."
                        )
                    # A newer draft extends the wait set.  A held draft stays
                    # held until the engineer explicitly confirms it.
                    waits = list(dict.fromkeys(current["wait_for_run_ids"] + waits))
                    conn.execute(
                        """UPDATE pending_instructions SET content=?,action=?,content_hash=?,idempotency_key=?,
                        wait_for_run_ids_json=?,revision=revision+1,updated_at=? WHERE id=?""",
                        (content, action, digest, key, dump(waits), stamp, current["id"]),
                    )
                    identifier = current["id"]
                else:
                    # A terminal row must never be revived under its old
                    # identity: stale edit/cancel requests then cannot target
                    # a later instruction.
                    identifier = new_id()
                    conn.execute("DELETE FROM pending_instructions WHERE id=?", (current["id"],))
                    conn.execute(
                        """INSERT INTO pending_instructions
                        (id,tender_id,content,action,content_hash,idempotency_key,wait_for_run_ids_json,status,hold_reason,revision,created_at,updated_at)
                        VALUES(?,?,?,?,?,?,?, 'pending',NULL,1,?,?)""",
                        (
                            identifier,
                            tender_id,
                            content,
                            action,
                            digest,
                            key,
                            dump(waits),
                            stamp,
                            stamp,
                        ),
                    )
            else:
                identifier = new_id()
                conn.execute(
                    """INSERT INTO pending_instructions
                    (id,tender_id,content,action,content_hash,idempotency_key,wait_for_run_ids_json,status,hold_reason,revision,created_at,updated_at)
                    VALUES(?,?,?,?,?,?,?, 'pending',NULL,1,?,?)""",
                    (
                        identifier,
                        tender_id,
                        content,
                        action,
                        digest,
                        key,
                        dump(waits),
                        stamp,
                        stamp,
                    ),
                )
            saved = record(
                conn.execute(
                    "SELECT * FROM pending_instructions WHERE id=?", (identifier,)
                ).fetchone()
            )
            self.remember_idempotency(
                tender_id,
                key,
                content,
                action=action,
                outcome_kind="pending",
                pending_id=identifier,
                outcome={},
            )
        return saved

    def edit(
        self,
        tender_id: str,
        content: str,
        *,
        pending_id: str,
        expected_revision: int,
        action: str | None = None,
    ) -> dict:
        content = _clean_content(content)
        with self.repo.db.connect(write=True) as conn:
            row = conn.execute(
                "SELECT * FROM pending_instructions WHERE tender_id=?", (tender_id,)
            ).fetchone()
            current = record(row) if row else None
            if current is None or current["status"] not in {"pending", "held"}:
                raise ValueError("There is no pending instruction to edit.")
            if current["id"] != pending_id or current["revision"] != expected_revision:
                raise ValueError("The pending instruction changed. Refresh it before editing.")
            if action not in {None, "review_documents"}:
                raise ValueError("Unknown message action.")
            action = current["action"] if action is None else action
            stamp = now()
            key = new_id()
            conn.execute(
                "UPDATE pending_instructions SET content=?,action=?,content_hash=?,idempotency_key=?,revision=revision+1,updated_at=? WHERE id=? AND revision=?",
                (
                    content,
                    action,
                    _content_hash(content, action),
                    key,
                    stamp,
                    current["id"],
                    expected_revision,
                ),
            )
            saved = record(
                conn.execute(
                    "SELECT * FROM pending_instructions WHERE id=?", (current["id"],)
                ).fetchone()
            )
            self.remember_idempotency(
                tender_id,
                key,
                content,
                action=action,
                outcome_kind="pending",
                pending_id=saved["id"],
                outcome={},
            )
        return saved

    def cancel(self, tender_id: str, *, pending_id: str, expected_revision: int) -> dict:
        with self.repo.db.connect(write=True) as conn:
            row = conn.execute(
                "SELECT * FROM pending_instructions WHERE tender_id=?", (tender_id,)
            ).fetchone()
            current = record(row) if row else None
            if current is None or current["status"] not in {"pending", "held"}:
                raise ValueError("There is no pending instruction to cancel.")
            if current["id"] != pending_id or current["revision"] != expected_revision:
                raise ValueError("The pending instruction changed. Refresh it before cancelling.")
            conn.execute(
                "UPDATE pending_instructions SET status='cancelled',hold_reason='cancelled',updated_at=? WHERE id=?",
                (now(), current["id"]),
            )
        return {"ok": True}

    def hold_for_tender(self, tender_id: str, reason: str) -> bool:
        if reason not in {
            "failed",
            "cancelled",
            "interrupted",
            "stopped",
            "restored",
            "permission",
            "model",
            "budget",
            "source",
            "work",
        }:
            raise ValueError("Unknown pending instruction hold reason.")
        with self.repo.db.connect(write=True) as conn:
            changed = conn.execute(
                "UPDATE pending_instructions SET status='held',hold_reason=?,updated_at=? WHERE tender_id=? AND status IN ('pending','held')",
                (reason, now(), tender_id),
            ).rowcount
        return bool(changed)

    def release(self, tender_id: str, *, pending_id: str | None = None) -> dict:
        with self.repo.db.connect(write=True) as conn:
            row = conn.execute(
                "SELECT * FROM pending_instructions WHERE tender_id=?", (tender_id,)
            ).fetchone()
            current = record(row) if row else None
            if current is None or current["status"] not in {"pending", "held"}:
                raise ValueError("There is no pending instruction to confirm.")
            if pending_id and current["id"] != pending_id:
                raise ValueError("The pending instruction changed. Refresh it before confirming.")
            if current["status"] == "held":
                conn.execute(
                    "UPDATE pending_instructions SET status='pending',hold_reason=NULL,updated_at=? WHERE id=?",
                    (now(), current["id"]),
                )
            return record(
                conn.execute(
                    "SELECT * FROM pending_instructions WHERE id=?", (current["id"],)
                ).fetchone()
            )

    def consume(self, tender_id: str, *, pending_id: str, expected_revision: int) -> dict:
        """Atomically mark the current draft consumed; caller queues its run in the same transaction."""

        with self.repo.db.connect(write=True) as conn:
            row = conn.execute(
                "SELECT * FROM pending_instructions WHERE tender_id=?", (tender_id,)
            ).fetchone()
            current = record(row) if row else None
            if current is None or current["status"] not in {"pending", "held"}:
                raise ValueError("There is no pending instruction to send.")
            if current["id"] != pending_id or current["revision"] != expected_revision:
                raise ValueError("The pending instruction changed. Refresh it before sending.")
            if current["status"] == "held":
                # Calling consume is the explicit confirmation.  No separate
                # release write is needed, which keeps message+run atomic.
                pass
            conn.execute(
                "UPDATE pending_instructions SET status='consumed',hold_reason='consumed',updated_at=? WHERE id=?",
                (now(), current["id"]),
            )
            current["status"] = "consumed"
            current["hold_reason"] = "consumed"
            return current

    def on_run_finished(self, run_id: str, status: str) -> None:
        """Hold on any intervening failed work, including later queued tasks."""

        if status not in {"completed", "failed", "cancelled", "interrupted"}:
            return
        with self.repo.db.connect(write=True) as conn:
            finished = conn.execute("SELECT tender_id FROM runs WHERE id=?", (run_id,)).fetchone()
            if finished is None:
                return
            rows = conn.execute(
                "SELECT * FROM pending_instructions WHERE tender_id=? AND status IN ('pending','held')",
                (finished["tender_id"],),
            ).fetchall()
            for row in rows:
                current = record(row)
                if status != "completed":
                    conn.execute(
                        "UPDATE pending_instructions SET status='held',hold_reason=?,updated_at=? WHERE id=?",
                        (status, now(), current["id"]),
                    )
                    continue
                if run_id not in current["wait_for_run_ids"]:
                    continue
                terminal = []
                for identifier in current["wait_for_run_ids"]:
                    found = conn.execute(
                        "SELECT status FROM runs WHERE id=?", (identifier,)
                    ).fetchone()
                    terminal.append(found["status"] if found else "failed")
                failures = {"failed", "cancelled", "interrupted"}.intersection(terminal)
                if failures:
                    reason = next(
                        value
                        for value in ("failed", "cancelled", "interrupted")
                        if value in failures
                    )
                    conn.execute(
                        "UPDATE pending_instructions SET status='held',hold_reason=?,updated_at=? WHERE id=?",
                        (reason, now(), current["id"]),
                    )
                elif terminal and all(item == "completed" for item in terminal):
                    conn.execute(
                        "UPDATE pending_instructions SET status='pending',hold_reason=NULL,updated_at=? WHERE id=? AND status='pending'",
                        (now(), current["id"]),
                    )

    def hold_for_interrupted_runs(self) -> None:
        with self.repo.db.connect(write=True) as conn:
            conn.execute(
                "UPDATE pending_instructions SET status='held',hold_reason='interrupted',updated_at=? WHERE status IN ('pending','held')",
                (now(),),
            )
