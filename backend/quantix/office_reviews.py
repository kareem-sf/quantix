"""Bounded independent review sessions. Conclusions are not quantity approvals."""

from __future__ import annotations

import json
from decimal import ROUND_HALF_UP, Decimal, InvalidOperation, localcontext
from typing import Literal

from pydantic import Field

from .db import dump, new_id, now
from .execution_context import OfficeExecutionIdentity
from .staff_models import IdentifierText, OfficeConflict, OfficeModel, TimestampText
from .units import add, multiply, quantity, subtract


class ReviewDraft(OfficeModel):
    subject_id: IdentifierText
    workings: str
    calculation_id: IdentifierText | None = None
    hide_author_conclusion: bool = True
    idempotency_key: IdentifierText


class ReviewContribution(OfficeModel):
    topic: str = Field(min_length=1, max_length=300)
    agreed: bool
    detail: str = Field(min_length=1, max_length=4000)
    idempotency_key: IdentifierText


class ReviewResolution(OfficeModel):
    resolution: str = Field(min_length=1, max_length=4000)
    idempotency_key: IdentifierText


class ReviewFinding(OfficeModel):
    id: IdentifierText
    topic: str
    agreed: bool
    detail: str


class ReviewSession(OfficeModel):
    id: IdentifierText
    subject_id: IdentifierText
    author_conclusion_hidden: bool
    findings: list[ReviewFinding]
    calculation_id: IdentifierText | None = None
    calculation_method_id: IdentifierText | None = None
    calculation_method_version: IdentifierText | None = None
    checked_basis_fingerprint: IdentifierText | None = None
    status: Literal["checked", "incomplete", "needs_review"]
    resolution: str | None = None
    resolved_at: TimestampText | None = None
    limitations: list[str]
    created_at: TimestampText


_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS office_review_sessions (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL,
        subject_id TEXT NOT NULL,
        workings TEXT NOT NULL,
        author_conclusion_hidden INTEGER NOT NULL,
        findings_json TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'incomplete',
        limitations_json TEXT NOT NULL DEFAULT '[]',
        calculation_id TEXT,
        calculation_method_id TEXT,
        calculation_method_version TEXT,
        checked_basis_fingerprint TEXT,
        resolution TEXT,
        resolved_at TEXT,
        created_at TEXT NOT NULL
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS office_review_receipts (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        session_id TEXT NOT NULL REFERENCES office_review_sessions(id),
        operation TEXT NOT NULL,
        idempotency_key TEXT NOT NULL,
        payload_hash TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE (tender_id, session_id, operation, idempotency_key)
    )
    """,
)


class OfficeReviewService:
    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)
            columns = {
                row["name"] for row in conn.execute("PRAGMA table_info(office_review_sessions)")
            }
            if "status" not in columns:
                conn.execute(
                    "ALTER TABLE office_review_sessions ADD COLUMN status TEXT NOT NULL DEFAULT 'incomplete'"
                )
            if "limitations_json" not in columns:
                conn.execute(
                    "ALTER TABLE office_review_sessions ADD COLUMN limitations_json TEXT NOT NULL DEFAULT '[]'"
                )
            for column in (
                "calculation_id",
                "calculation_method_id",
                "calculation_method_version",
                "checked_basis_fingerprint",
                "resolution",
                "resolved_at",
            ):
                if column not in columns:
                    conn.execute(f"ALTER TABLE office_review_sessions ADD COLUMN {column} TEXT")

    def start(
        self,
        ctx: OfficeExecutionIdentity,
        draft: ReviewDraft,
        *,
        author_total: str | None = None,
    ) -> ReviewSession:
        """Save a review; only a persisted typed calculation can produce a finding."""

        del author_total
        if not ctx.tender_id:
            raise ValueError("Reviews require a selected Tender.")
        identifier, stamp = new_id(), now()
        findings: list[ReviewFinding] = []
        limitations: list[str] = []
        status: Literal["checked", "incomplete", "needs_review"] = "incomplete"
        metadata = {
            "calculation_id": None,
            "calculation_method_id": None,
            "calculation_method_version": None,
            "checked_basis_fingerprint": None,
        }
        if draft.calculation_id is None:
            limitations.append(
                "No recorded typed calculation was supplied; prose workings are not independently checkable."
            )
        else:
            status, findings, limitations, metadata = self._check_calculation(
                ctx, draft.calculation_id
            )
        with self.repo.atomic() as conn:
            conn.execute(
                """
                INSERT INTO office_review_sessions(
                    id,tender_id,subject_id,workings,author_conclusion_hidden,findings_json,
                    status,limitations_json,calculation_id,calculation_method_id,
                    calculation_method_version,checked_basis_fingerprint,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    identifier,
                    ctx.tender_id,
                    draft.subject_id,
                    draft.workings,
                    1 if draft.hide_author_conclusion else 0,
                    dump([item.model_dump(mode="json") for item in findings]),
                    status,
                    dump(limitations),
                    metadata["calculation_id"],
                    metadata["calculation_method_id"],
                    metadata["calculation_method_version"],
                    metadata["checked_basis_fingerprint"],
                    stamp,
                ),
            )
        return ReviewSession(
            id=identifier,
            subject_id=draft.subject_id,
            author_conclusion_hidden=draft.hide_author_conclusion,
            findings=findings,
            calculation_id=metadata["calculation_id"],
            calculation_method_id=metadata["calculation_method_id"],
            calculation_method_version=metadata["calculation_method_version"],
            checked_basis_fingerprint=metadata["checked_basis_fingerprint"],
            status=status,
            resolution=None,
            resolved_at=None,
            limitations=limitations,
            created_at=stamp,
        )

    @staticmethod
    def _session_from_row(row) -> ReviewSession:
        import json as _json

        return ReviewSession(
            id=row["id"],
            subject_id=row["subject_id"],
            author_conclusion_hidden=bool(row["author_conclusion_hidden"]),
            findings=[
                ReviewFinding.model_validate(item)
                for item in _json.loads(row["findings_json"] or "[]")
            ],
            calculation_id=row["calculation_id"],
            calculation_method_id=row["calculation_method_id"],
            calculation_method_version=row["calculation_method_version"],
            checked_basis_fingerprint=row["checked_basis_fingerprint"],
            status=row["status"],
            resolution=row["resolution"],
            resolved_at=row["resolved_at"],
            limitations=_json.loads(row["limitations_json"] or "[]"),
            created_at=row["created_at"],
        )

    def _session_row(self, conn, tender_id: str, session_id: str):
        row = conn.execute(
            "SELECT * FROM office_review_sessions WHERE tender_id=? AND id=?",
            (tender_id, session_id),
        ).fetchone()
        if row is None:
            raise KeyError("This item could not be found in the selected Tender.")
        return row

    def _receipt_replay(self, conn, tender_id, session_id, operation, key, payload_hash):
        existing = conn.execute(
            """
            SELECT payload_hash FROM office_review_receipts
            WHERE tender_id=? AND session_id=? AND operation=? AND idempotency_key=?
            """,
            (tender_id, session_id, operation, key),
        ).fetchone()
        if existing is None:
            return False
        if existing["payload_hash"] != payload_hash:
            raise OfficeConflict("This review key was already used for different content.")
        return True

    def _write_receipt(self, conn, tender_id, session_id, operation, key, payload_hash):
        conn.execute(
            """
            INSERT INTO office_review_receipts(
                id,tender_id,session_id,operation,idempotency_key,payload_hash,created_at
            ) VALUES(?,?,?,?,?,?,?)
            """,
            (new_id(), tender_id, session_id, operation, key, payload_hash, now()),
        )

    def get(self, tender_id: str, session_id: str) -> ReviewSession:
        with self.repo.db.connect() as conn:
            return self._session_from_row(self._session_row(conn, tender_id, session_id))

    def list(
        self, tender_id: str, status: str | None = None, limit: int = 50
    ) -> list[ReviewSession]:
        if status is not None and status not in {"checked", "incomplete", "needs_review"}:
            raise ValueError("Choose checked, incomplete or needs_review reviews.")
        if not 1 <= limit <= 50:
            raise ValueError("The review page limit must be between 1 and 50.")
        with self.repo.db.connect() as conn:
            rows = conn.execute(
                """
                SELECT * FROM office_review_sessions
                WHERE tender_id=? AND (? IS NULL OR status=?)
                ORDER BY created_at DESC,id DESC LIMIT ?
                """,
                (tender_id, status, status, limit),
            ).fetchall()
            return [self._session_from_row(row) for row in rows]

    def contribute(
        self, ctx: OfficeExecutionIdentity, session_id: str, contribution: ReviewContribution
    ) -> ReviewSession:
        """Record one discussion finding without rewriting earlier conclusions."""

        from .staff_store import _canonical_hash, _key

        tender_id = ctx.tender_id
        if not tender_id:
            raise ValueError("Reviews require a selected Tender.")
        key = _key(contribution.idempotency_key)
        payload_hash = _canonical_hash(contribution.model_dump(mode="json"))
        with self.repo.atomic() as conn:
            row = self._session_row(conn, tender_id, session_id)
            if row["resolution"] is not None:
                raise OfficeConflict(
                    "This review is resolved. Start a new review for further checks."
                )
            if self._receipt_replay(conn, tender_id, session_id, "contribute", key, payload_hash):
                return self._session_from_row(self._session_row(conn, tender_id, session_id))
            findings = [
                ReviewFinding.model_validate(item)
                for item in json.loads(row["findings_json"] or "[]")
            ]
            findings.append(
                ReviewFinding(
                    id=new_id(),
                    topic=contribution.topic.strip(),
                    agreed=contribution.agreed,
                    detail=contribution.detail.strip(),
                )
            )
            conn.execute(
                "UPDATE office_review_sessions SET findings_json=? WHERE tender_id=? AND id=?",
                (dump([item.model_dump(mode="json") for item in findings]), tender_id, session_id),
            )
            self._write_receipt(conn, tender_id, session_id, "contribute", key, payload_hash)
            return self._session_from_row(self._session_row(conn, tender_id, session_id))

    def resolve(
        self, ctx: OfficeExecutionIdentity, session_id: str, resolution: ReviewResolution
    ) -> ReviewSession:
        """Resolve a review with an explicit engineer decision requirement.

        Resolution records the outcome and closes the session. It never
        mutates quantities, rates, findings, decisions or any Tender record:
        commercial effect still needs its own explicit engineer decision.
        """

        from .staff_store import _canonical_hash, _key

        tender_id = ctx.tender_id
        if not tender_id:
            raise ValueError("Reviews require a selected Tender.")
        key = _key(resolution.idempotency_key)
        payload_hash = _canonical_hash(resolution.model_dump(mode="json"))
        with self.repo.atomic() as conn:
            row = self._session_row(conn, tender_id, session_id)
            if row["resolution"] is not None:
                if self._receipt_replay(conn, tender_id, session_id, "resolve", key, payload_hash):
                    return self._session_from_row(self._session_row(conn, tender_id, session_id))
                raise OfficeConflict("This review is already resolved.")
            stamp = now()
            conn.execute(
                "UPDATE office_review_sessions SET resolution=?,resolved_at=? WHERE tender_id=? AND id=?",
                (resolution.resolution.strip(), stamp, tender_id, session_id),
            )
            self._write_receipt(conn, tender_id, session_id, "resolve", key, payload_hash)
            return self._session_from_row(self._session_row(conn, tender_id, session_id))

    def _check_calculation(
        self, ctx: OfficeExecutionIdentity, calculation_id: str
    ) -> tuple[
        Literal["checked", "incomplete", "needs_review"],
        list[ReviewFinding],
        list[str],
        dict[str, str | None],
    ]:
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT * FROM office_calculations WHERE id=? AND tender_id=?",
                (calculation_id, ctx.tender_id),
            ).fetchone()
        if row is None:
            raise ValueError("The calculation is not in the selected Tender.")
        metadata: dict[str, str | None] = {
            "calculation_id": row["id"],
            "calculation_method_id": None,
            "calculation_method_version": None,
            "checked_basis_fingerprint": row["basis_fingerprint"],
        }
        if row["status"] != "calculated":
            return (
                "incomplete",
                [],
                ["The recorded calculation is not valid for an independent check."],
                metadata,
            )
        try:
            request = json.loads(row["request_json"])
            recorded = json.loads(row["outputs_json"])
            method = request["method_id"]
            metadata["calculation_method_id"] = method
            metadata["calculation_method_version"] = request["method_version"]
            inputs = request["inputs"]
            units = request["units"]
            precision = Decimal(str(request["precision"]))
            if not precision.is_finite() or precision <= 0:
                raise ValueError("invalid precision")
            if method in {"product", "unit_calculation", "emission_factor"}:
                left = quantity(inputs["quantity"], units.get("quantity", "1"))
                right = quantity(inputs["factor"], units.get("factor", "1"))
                calculated = multiply(left, right)
                expected_key = "product"
                expected_unit = calculated.unit
                computed = calculated.value
            elif method == "sum":
                values = inputs["values"]
                if not isinstance(values, list) or not values:
                    raise ValueError("missing values")
                expected_key = "sum"
                calculated = quantity("0", units.get("quantity", "1"))
                for value in values:
                    calculated = add(calculated, quantity(value, units.get("quantity", "1")))
                expected_unit = calculated.unit
                computed = calculated.value
            elif method == "difference":
                expected_key = "difference"
                calculated = subtract(
                    quantity(inputs["left"], units.get("left", "1")),
                    quantity(inputs["right"], units.get("right", "1")),
                )
                expected_unit = calculated.unit
                computed = calculated.value
            else:
                return (
                    "incomplete",
                    [],
                    [f"The '{method}' calculation method is not independently reviewable."],
                    metadata,
                )
            if not computed.is_finite() or expected_key not in recorded:
                raise ValueError("missing recorded output")
            with localcontext() as decimal_context:
                decimal_context.rounding = ROUND_HALF_UP
                recomputed = computed.quantize(precision)
            observed = Decimal(str(recorded[expected_key]))
            if not observed.is_finite():
                raise ValueError("non-finite recorded output")
            matching = observed == recomputed and recorded.get("unit") == expected_unit
        except (KeyError, TypeError, ValueError, InvalidOperation):
            return (
                "incomplete",
                [],
                ["The recorded calculation inputs or output are incomplete for review."],
                metadata,
            )
        if matching:
            detail = f"The recorded {method} calculation reproduces {recomputed} from its saved typed inputs."
            status: Literal["checked", "incomplete", "needs_review"] = "checked"
            agreed = True
        else:
            detail = f"The recorded {method} calculation does not reproduce {recomputed} from its saved typed inputs."
            status = "needs_review"
            agreed = False
        return (
            status,
            [ReviewFinding(id=new_id(), topic="calculation", agreed=agreed, detail=detail)],
            [],
            metadata,
        )
