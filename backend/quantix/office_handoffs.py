"""Manager-authorized handoff of complete staff-result tables."""

from __future__ import annotations

import base64
import binascii
import json

from .db import dump, new_id, now
from .execution_context import OfficeExecutionIdentity
from .office_handoff_models import (
    Handoff,
    HandoffPage,
    HandoffPayload,
    HandoffSelection,
    HandoffView,
    TransferRequest,
)
from .staff_models import OfficeConflict
from .staff_store import _canonical_hash, _key

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS office_handoffs (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        sender_staff_id TEXT NOT NULL,
        recipient_staff_id TEXT NOT NULL,
        result_id TEXT NOT NULL,
        purpose TEXT NOT NULL,
        basis_fingerprint TEXT NOT NULL,
        payload_json TEXT NOT NULL,
        applicability TEXT NOT NULL,
        created_at TEXT NOT NULL
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS office_handoff_receipts (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL,
        idempotency_key TEXT NOT NULL,
        payload_hash TEXT NOT NULL,
        handoff_id TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE (tender_id, idempotency_key)
    )
    """,
)


def _cursor(created_at: str, handoff_id: str) -> str:
    raw = json.dumps([created_at, handoff_id], separators=(",", ":")).encode()
    return base64.urlsafe_b64encode(raw).decode()


def _decode_handoff_cursor(cursor: str) -> tuple[str, str]:
    try:
        created_at, handoff_id = json.loads(base64.urlsafe_b64decode(cursor.encode()).decode())
    except (ValueError, binascii.Error) as error:
        raise ValueError("The handoff cursor is invalid.") from error
    if not isinstance(created_at, str) or not isinstance(handoff_id, str):
        raise ValueError("The handoff cursor is invalid.")
    return created_at, handoff_id


class OfficeHandoffService:
    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)

    def save_result_table(self, tender_id: str, staff_id: str, rows: list[dict], *, fingerprint: str) -> str:
        """Store a synthetic or saved staff-result table for later transfer."""

        identifier, stamp = new_id(), now()
        with self.repo.atomic() as conn:
            conn.execute(
                """
                CREATE TABLE IF NOT EXISTS office_staff_result_tables (
                    id TEXT PRIMARY KEY,
                    tender_id TEXT NOT NULL,
                    staff_id TEXT NOT NULL,
                    fingerprint TEXT NOT NULL,
                    rows_json TEXT NOT NULL,
                    created_at TEXT NOT NULL
                )
                """
            )
            conn.execute(
                """
                INSERT INTO office_staff_result_tables(id,tender_id,staff_id,fingerprint,rows_json,created_at)
                VALUES(?,?,?,?,?,?)
                """,
                (identifier, tender_id, staff_id, fingerprint, dump(rows), stamp),
            )
        return identifier

    def transfer_saved_result(self, ctx: OfficeExecutionIdentity, request: TransferRequest) -> Handoff:
        tender_id = ctx.tender_id
        if not tender_id:
            raise ValueError("Handoffs require a selected Tender.")
        key = _key(request.idempotency_key)
        payload_hash = _canonical_hash(request.model_dump(mode="json"))
        with self.repo.atomic() as conn:
            existing = conn.execute(
                """
                SELECT payload_hash,handoff_id FROM office_handoff_receipts
                WHERE tender_id=? AND idempotency_key=?
                """,
                (tender_id, key),
            ).fetchone()
            if existing is not None:
                if existing["payload_hash"] != payload_hash:
                    raise OfficeConflict("This handoff key was already used for different content.")
                return self._handoff(conn, existing["handoff_id"])
            table = conn.execute(
                "SELECT * FROM office_staff_result_tables WHERE id=? AND tender_id=?",
                (request.result_id, tender_id),
            ).fetchone()
            if table is None:
                raise KeyError("This item could not be found in the selected Tender.")
            if table["fingerprint"] != request.expected_basis_fingerprint:
                raise ValueError("The saved result basis changed. Review the current sources before transferring it.")
            if table["staff_id"] == request.recipient_staff_id:
                raise ValueError("A handoff needs a different recipient.")
            recipient = conn.execute(
                "SELECT 1 FROM office_staff WHERE tender_id=? AND id=?",
                (tender_id, request.recipient_staff_id),
            ).fetchone()
            if recipient is None:
                raise KeyError("This item could not be found in the selected Tender.")
            identifier, stamp = new_id(), now()
            conn.execute(
                """
                INSERT INTO office_handoffs(
                    id,tender_id,sender_staff_id,recipient_staff_id,result_id,purpose,
                    basis_fingerprint,payload_json,applicability,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    identifier,
                    tender_id,
                    table["staff_id"],
                    request.recipient_staff_id,
                    request.result_id,
                    request.purpose,
                    table["fingerprint"],
                    table["rows_json"],
                    "current",
                    stamp,
                ),
            )
            conn.execute(
                """
                INSERT INTO office_handoff_receipts(id,tender_id,idempotency_key,payload_hash,handoff_id,created_at)
                VALUES(?,?,?,?,?,?)
                """,
                (new_id(), tender_id, key, payload_hash, identifier, stamp),
            )
            return self._handoff(conn, identifier)

    def mark_source_revised(self, tender_id: str, fingerprint: str) -> None:
        with self.repo.atomic() as conn:
            conn.execute(
                "UPDATE office_handoffs SET applicability='needs_review' WHERE tender_id=? AND basis_fingerprint=?",
                (tender_id, fingerprint),
            )

    def read_handoff(self, ctx: OfficeExecutionIdentity, selection: HandoffSelection) -> HandoffPayload:
        tender_id = ctx.tender_id
        if not tender_id:
            raise ValueError("Handoffs require a selected Tender.")
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT * FROM office_handoffs WHERE id=? AND tender_id=?",
                (selection.handoff_id, tender_id),
            ).fetchone()
            if row is None:
                raise KeyError("This item could not be found in the selected Tender.")
            if ctx.actor_kind == "staff" and ctx.actor_id not in {row["recipient_staff_id"], row["sender_staff_id"]}:
                raise KeyError("This item could not be found in the selected Tender.")
            rows = json.loads(row["payload_json"])
            selected = rows[selection.offset : selection.offset + selection.limit]
            next_offset = selection.offset + selection.limit
            return HandoffPayload(
                handoff=self._record(row),
                items=selected,
                next_offset=next_offset if next_offset < len(rows) else None,
                total_rows=len(rows),
                exact_versions={"result_id": row["result_id"], "basis_fingerprint": row["basis_fingerprint"]},
                applicability=row["applicability"],
            )

    def list_handoffs(
        self,
        tender_id: str,
        *,
        staff_id: str,
        direction: str = "received",
        limit: int = 50,
        cursor: str | None = None,
    ) -> HandoffPage:
        if not tender_id or not staff_id or not staff_id.strip():
            raise ValueError("Handoff history requires the selected Tender and colleague.")
        if direction not in {"received", "sent"}:
            raise ValueError("Choose received or sent handoffs.")
        if not 1 <= limit <= 50:
            raise ValueError("The handoff page limit must be between 1 and 50.")
        staff_id = staff_id.strip()
        after = _decode_handoff_cursor(cursor) if cursor else None
        column = "recipient_staff_id" if direction == "received" else "sender_staff_id"
        counterpart_column = "sender_staff_id" if direction == "received" else "recipient_staff_id"
        with self.repo.db.connect() as conn:
            staff = conn.execute(
                "SELECT 1 FROM office_staff WHERE tender_id=? AND id=?",
                (tender_id, staff_id),
            ).fetchone()
            if staff is None:
                raise KeyError("This item could not be found in the selected Tender.")
            total = conn.execute(
                f"SELECT COUNT(*) FROM office_handoffs WHERE tender_id=? AND {column}=?",
                (tender_id, staff_id),
            ).fetchone()[0]
            params: list = [tender_id, staff_id]
            paging = ""
            if after is not None:
                anchor = conn.execute(
                    f"SELECT created_at,id FROM office_handoffs WHERE tender_id=? AND {column}=? AND id=?",
                    (tender_id, staff_id, after[1]),
                ).fetchone()
                if anchor is None or (anchor["created_at"], anchor["id"]) != after:
                    raise ValueError("The handoff cursor is invalid or no longer available.")
                paging = "AND (created_at<? OR (created_at=? AND id<?))"
                params.extend([after[0], after[0], after[1]])
            rows = conn.execute(
                f"""
                SELECT * FROM office_handoffs
                WHERE tender_id=? AND {column}=? {paging}
                ORDER BY created_at DESC,id DESC LIMIT ?
                """,
                (*params, limit + 1),
            ).fetchall()
            has_more = len(rows) > limit
            selected = rows[:limit]
            items = []
            for row in selected:
                record = self._record(row)
                counterpart_id = row[counterpart_column]
                name_row = conn.execute(
                    """
                    SELECT v.profile_json FROM office_staff_versions v
                    JOIN office_staff s ON s.id=v.staff_id AND s.tender_id=v.tender_id
                    WHERE v.staff_id=? AND v.tender_id=? AND v.version=s.current_version
                    """,
                    (counterpart_id, tender_id),
                ).fetchone()
                display_name = "Unknown colleague"
                if name_row is not None:
                    try:
                        display_name = str(json.loads(name_row["profile_json"]).get("display_name") or display_name)
                    except (ValueError, AttributeError):
                        pass
                items.append(
                    HandoffView(
                        handoff=record,
                        direction=direction,  # type: ignore[arg-type]
                        counterpart_staff_id=counterpart_id,
                        counterpart_display_name=display_name[:300] or "Unknown colleague",
                    )
                )
            return HandoffPage(
                items=items,
                next_cursor=_cursor(selected[-1]["created_at"], selected[-1]["id"]) if has_more and selected else None,
                total=int(total),
            )

    def _handoff(self, conn, handoff_id: str) -> Handoff:
        row = conn.execute("SELECT * FROM office_handoffs WHERE id=?", (handoff_id,)).fetchone()
        return self._record(row)

    @staticmethod
    def _record(row) -> Handoff:
        return Handoff(
            id=row["id"],
            tender_id=row["tender_id"],
            sender_staff_id=row["sender_staff_id"],
            recipient_staff_id=row["recipient_staff_id"],
            result_id=row["result_id"],
            purpose=row["purpose"],
            basis_fingerprint=row["basis_fingerprint"],
            applicability=row["applicability"],
            created_at=row["created_at"],
        )
