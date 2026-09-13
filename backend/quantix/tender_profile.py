"""Tender profile facts. Company defaults are not confirmed Tender values."""

from __future__ import annotations

import json

from .db import dump, now
from .execution_context import OfficeExecutionIdentity
from .staff_models import OfficeConflict
from .tender_profile_models import ProfilePatch, TenderProfile

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS tender_profiles (
        tender_id TEXT PRIMARY KEY,
        revision INTEGER NOT NULL,
        payload_json TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )
    """,
)


class TenderProfileService:
    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)

    def get(self, tender_id: str) -> TenderProfile:
        with self.repo.atomic() as conn:
            row = conn.execute(
                "SELECT * FROM tender_profiles WHERE tender_id=?", (tender_id,)
            ).fetchone()
        if row is None:
            return TenderProfile(tender_id=tender_id, revision=1)
        payload = json.loads(row["payload_json"])
        payload["tender_id"] = tender_id
        payload["revision"] = row["revision"]
        return TenderProfile.model_validate(payload)

    def update(self, ctx: OfficeExecutionIdentity, patch: ProfilePatch) -> TenderProfile:
        current = self.get(ctx.tender_id)
        if current.revision != patch.expected_revision:
            raise OfficeConflict("This Tender profile has changed. Reload it before saving.")
        data = current.model_dump()
        for name in (
            "contract_type",
            "country",
            "geography",
            "currencies",
            "working_languages",
            "timezone",
            "measurement_method",
            "edition",
            "source_refs",
            "permitted_destinations",
        ):
            value = getattr(patch, name)
            if value is not None:
                data[name] = value
        data["revision"] = current.revision + 1
        with self.repo.atomic() as conn:
            conn.execute(
                """
                INSERT INTO tender_profiles(tender_id,revision,payload_json,updated_at)
                VALUES(?,?,?,?)
                ON CONFLICT(tender_id) DO UPDATE SET
                    revision=excluded.revision, payload_json=excluded.payload_json,
                    updated_at=excluded.updated_at
                """,
                (ctx.tender_id, data["revision"], dump(data), now()),
            )
        return self.get(ctx.tender_id)
