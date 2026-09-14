from __future__ import annotations

import hashlib

from .company_library_models import CompanyAsset, CompanyAssetDraft
from .db import dump, new_id, now
from .execution_context import OfficeExecutionIdentity

_SCHEMA = (
    """CREATE TABLE IF NOT EXISTS company_assets(
        id TEXT PRIMARY KEY, kind TEXT NOT NULL, title TEXT NOT NULL, valid_from TEXT, valid_until TEXT,
        verified INTEGER NOT NULL, revoked INTEGER NOT NULL, sensitive INTEGER NOT NULL,
        permitted_reuse TEXT NOT NULL, payload_json TEXT NOT NULL, fingerprint TEXT NOT NULL,
        created_at TEXT NOT NULL)""",
)


class CompanyLibraryService:
    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)

    def propose(self, ctx: OfficeExecutionIdentity, draft: CompanyAssetDraft) -> CompanyAsset:
        del ctx
        identifier, stamp = new_id(), now()
        fingerprint = hashlib.sha256(dump(draft.model_dump(mode="json")).encode()).hexdigest()
        with self.repo.atomic() as conn:
            conn.execute(
                """INSERT INTO company_assets(id,kind,title,valid_from,valid_until,verified,revoked,
                    sensitive,permitted_reuse,payload_json,fingerprint,created_at)
                    VALUES(?,?,?,?,?,?,0,?,?,?,?,?)""",
                (
                    identifier,
                    draft.kind,
                    draft.title,
                    draft.valid_from,
                    draft.valid_until,
                    1 if draft.verified else 0,
                    1 if draft.sensitive else 0,
                    draft.permitted_reuse,
                    dump(draft.payload),
                    fingerprint,
                    stamp,
                ),
            )
        return self.get(identifier)

    def get(self, asset_id: str) -> CompanyAsset:
        with self.repo.atomic() as conn:
            row = conn.execute("SELECT * FROM company_assets WHERE id=?", (asset_id,)).fetchone()
        if row is None:
            raise KeyError("This company asset could not be found.")
        return CompanyAsset(
            id=row["id"],
            kind=row["kind"],
            title=row["title"],
            valid_from=row["valid_from"],
            valid_until=row["valid_until"],
            verified=bool(row["verified"]),
            revoked=bool(row["revoked"]),
            sensitive=bool(row["sensitive"]),
            permitted_reuse=row["permitted_reuse"],
            fingerprint=row["fingerprint"],
        )

    def revoke(self, asset_id: str) -> CompanyAsset:
        with self.repo.atomic() as conn:
            conn.execute("UPDATE company_assets SET revoked=1 WHERE id=?", (asset_id,))
        return self.get(asset_id)

    def eligibility(self, asset_id: str, *, on: str) -> dict:
        asset = self.get(asset_id)
        expired = bool(asset.valid_until and asset.valid_until < on)
        return {
            "eligibility_satisfied": bool(asset.verified and not asset.revoked and not expired),
            "unauthorized_prompt_bytes": 0,
            "history_retained": True,
        }
