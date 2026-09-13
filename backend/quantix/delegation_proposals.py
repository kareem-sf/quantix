"""Plan-scoped engineer delegation selections.

The proposal is an editable selection record, not a permission record.  A
separate plan approval still has to bind its exact reviewed envelope before
any delegation grant exists.
"""

from __future__ import annotations

import json

from .db import dump, now
from .plan_review_models import DelegationProposal, DelegationProposalEdit


class DelegationProposalService:
    """Persist delegation selections with a small compare-and-swap boundary."""

    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            conn.execute(
                """
                CREATE TABLE IF NOT EXISTS delegation_proposals (
                    tender_id TEXT NOT NULL REFERENCES tenders(id),
                    plan_id TEXT NOT NULL REFERENCES plans(id),
                    version INTEGER NOT NULL CHECK (version >= 1),
                    data_json TEXT NOT NULL,
                    candidate_fingerprint TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    PRIMARY KEY(tender_id, plan_id)
                )
                """
            )

    @staticmethod
    def _parse(row) -> DelegationProposal:
        data = json.loads(row["data_json"])
        return DelegationProposal(
            tender_id=row["tender_id"],
            plan_id=row["plan_id"],
            version=row["version"],
            updated_at=row["updated_at"],
            **data,
        )

    def get(self, tender_id: str, plan_id: str) -> DelegationProposal | None:
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT * FROM delegation_proposals WHERE tender_id=? AND plan_id=?",
                (tender_id, plan_id),
            ).fetchone()
        return self._parse(row) if row is not None else None

    def candidate_fingerprint(self, tender_id: str, plan_id: str) -> str | None:
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT candidate_fingerprint FROM delegation_proposals WHERE tender_id=? AND plan_id=?",
                (tender_id, plan_id),
            ).fetchone()
        return row[0] if row is not None else None

    def save(
        self,
        tender_id: str,
        plan_id: str,
        values: DelegationProposalEdit | dict,
        *,
        candidate_fingerprint: str,
    ) -> DelegationProposal:
        edit = values if isinstance(values, DelegationProposalEdit) else DelegationProposalEdit.model_validate(values)
        self.repo.get_tender(tender_id)
        with self.repo.atomic() as conn:
            plan = conn.execute(
                "SELECT status,tender_id FROM plans WHERE id=? AND tender_id=?",
                (plan_id, tender_id),
            ).fetchone()
            if plan is None:
                raise KeyError("This work plan does not belong to the selected Tender.")
            if plan["status"] != "proposed":
                raise ValueError("Delegation options can be changed only for the current proposed plan.")
            if conn.execute(
                "SELECT 1 FROM runs WHERE tender_id=? AND status IN ('queued','running') LIMIT 1",
                (tender_id,),
            ).fetchone():
                raise ValueError("Finish or stop current Tender work before changing delegation options.")
            row = conn.execute(
                "SELECT * FROM delegation_proposals WHERE tender_id=? AND plan_id=?",
                (tender_id, plan_id),
            ).fetchone()
            current_version = row["version"] if row is not None else 0
            if edit.expected_version != current_version:
                raise ValueError(
                    "The delegation options changed since they were displayed. Refresh the plan review and try again."
                )
            version = current_version + 1
            stamp = now()
            data = edit.model_dump(mode="json", exclude={"expected_version"})
            conn.execute(
                """
                INSERT INTO delegation_proposals(
                    tender_id,plan_id,version,data_json,candidate_fingerprint,updated_at
                ) VALUES(?,?,?,?,?,?)
                ON CONFLICT(tender_id,plan_id) DO UPDATE SET
                    version=excluded.version,
                    data_json=excluded.data_json,
                    candidate_fingerprint=excluded.candidate_fingerprint,
                    updated_at=excluded.updated_at
                """,
                (tender_id, plan_id, version, dump(data), candidate_fingerprint, stamp),
            )
            return DelegationProposal(
                tender_id=tender_id,
                plan_id=plan_id,
                version=version,
                updated_at=stamp,
                **data,
            )


__all__ = ["DelegationProposalService"]
