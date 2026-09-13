"""Immutable Manager profile pins for durable Tender work runs."""

from __future__ import annotations

from .db import now
from .manager_profile import ManagerProfileService

_SCHEMA_STATEMENTS = (
    """
    CREATE TABLE IF NOT EXISTS office_manager_run_profiles (
        run_id TEXT PRIMARY KEY REFERENCES runs(id),
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        manager_id TEXT NOT NULL REFERENCES office_manager(id),
        manager_version INTEGER NOT NULL CHECK (manager_version >= 1),
        captured_at TEXT NOT NULL
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS office_manager_run_profiles_tender
    ON office_manager_run_profiles(tender_id, captured_at)
    """,
)


def _ensure_schema(repo) -> None:
    """Create the run-profile table with independent statements in one write."""

    with repo.atomic() as conn:
        for statement in _SCHEMA_STATEMENTS:
            conn.execute(statement)


class ManagerRunProfiles:
    """Capture and retrieve the exact Manager version used by a run."""

    def __init__(self, repo, profile_service: ManagerProfileService | None = None):
        self.repo = repo
        # The profile service owns the stable Manager identity and immutable
        # version rows. Construct it once so reads never reseed or reinitialize
        # the profile on every request.
        self.profile_service = profile_service or ManagerProfileService(repo)
        _ensure_schema(repo)

    @staticmethod
    def _run(conn, tender_id: str, run_id: str):
        row = conn.execute(
            "SELECT id,tender_id,status FROM runs WHERE id=?", (run_id,)
        ).fetchone()
        if row is None or row["tender_id"] != tender_id:
            raise KeyError("This work run does not belong to the selected Tender.")
        return row

    def _profile_for_version(self, manager_id: str, version: int):
        # Manager IDs are stable server-owned identity. The existing profile
        # service verifies that identity while loading the immutable version.
        profile = self.profile_service.profile_version(version)
        if profile.id != manager_id:
            raise KeyError("The saved Tender Manager version could not be found.")
        return profile

    def capture(self, tender_id: str, run_id: str):
        """Pin the current Manager version once for a queued/running run.

        A caller may invoke this while an admission transaction is already
        open. ``Repository.atomic`` then yields that same connection, so a
        later outer rollback removes both the run and this pin.
        """

        with self.repo.atomic() as conn:
            run = self._run(conn, tender_id, run_id)
            existing = conn.execute(
                """
                SELECT manager_id,manager_version
                FROM office_manager_run_profiles
                WHERE run_id=?
                """,
                (run_id,),
            ).fetchone()
            if existing is not None:
                return self._profile_for_version(
                    existing["manager_id"], int(existing["manager_version"])
                )
            if run["status"] not in {"queued", "running"}:
                raise ValueError("A Manager profile can only be captured for queued or running work.")

            current = self.profile_service.get()
            conn.execute(
                """
                INSERT INTO office_manager_run_profiles(
                    run_id,tender_id,manager_id,manager_version,captured_at
                ) VALUES(?,?,?,?,?)
                """,
                (run_id, tender_id, current.id, current.version, now()),
            )
            return current

    def get(self, tender_id: str, run_id: str):
        """Return a previously pinned Manager version; never capture on read."""

        with self.repo.db.connect() as conn:
            self._run(conn, tender_id, run_id)
            row = conn.execute(
                """
                SELECT manager_id,manager_version
                FROM office_manager_run_profiles
                WHERE run_id=? AND tender_id=?
                """,
                (run_id, tender_id),
            ).fetchone()
            if row is None:
                raise KeyError("This work run has no captured Tender Manager profile.")
            return self._profile_for_version(row["manager_id"], int(row["manager_version"]))


def prompt_profile(profile) -> dict:
    """Project a complete user-authored profile for Manager prompt context.

    Identity timestamps and all account, source, grant and spending authority
    remain outside this projection. A model-dumped copy prevents prompt code
    from mutating the detached profile instance or its nested lists.
    """

    return profile.model_dump(
        mode="json",
        include={"display_name", "title", "persona", "personality", "working_preferences"},
    )


__all__ = ["ManagerRunProfiles", "prompt_profile"]
