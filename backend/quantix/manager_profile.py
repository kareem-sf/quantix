"""Persistent, engineer-customizable Tender Manager profile."""

from __future__ import annotations

import json

from .db import dump, new_id, now
from .staff_models import (
    ManagerProfile,
    ManagerProfileEdit,
    OfficeConflict,
    OfficeModel,
    Personality,
)

_SCHEMA_STATEMENTS = (
    """
    CREATE TABLE IF NOT EXISTS office_manager (
        singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
        id TEXT NOT NULL UNIQUE,
        current_version INTEGER NOT NULL CHECK (current_version >= 1),
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS office_manager_versions (
        manager_id TEXT NOT NULL REFERENCES office_manager(id),
        version INTEGER NOT NULL CHECK (version >= 1),
        display_name TEXT NOT NULL,
        title TEXT NOT NULL,
        persona TEXT NOT NULL,
        personality_json TEXT NOT NULL,
        working_preferences_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (manager_id, version)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS office_manager_versions_lookup
    ON office_manager_versions(manager_id, version)
    """,
)


_DEFAULT_PERSONALITY = Personality(
    description="A neutral, evidence-led coordinator for construction tender work.",
    traits=["careful", "clear", "practical"],
    communication_style="Use plain language, state the next action, and keep material detail traceable.",
    problem_solving_style="Break each issue into bounded checks and connect conclusions to the available evidence.",
    collaboration_style="Ask focused colleagues for the missing work and keep ownership visible.",
    uncertainty_handling="Label assumptions and explain exactly which source or decision is still needed.",
    initiative="Propose the next safe step when the available evidence supports it.",
    explanation_style="Lead with the answer, then provide concise reasoning and source references.",
    language_preferences=["English"],
    working_habits=[
        "Keep source references beside material findings",
        "State one clear next action",
    ],
)


def _default_editable_fields() -> dict:
    """Return fresh default values for the first immutable profile version."""

    # model_copy prevents callers from mutating the module-level seed lists.
    return {
        "display_name": "Tender Manager",
        "title": "Tender Manager",
        "persona": "A neutral, evidence-led coordinator for construction tenders.",
        "personality": _DEFAULT_PERSONALITY.model_copy(deep=True),
        "working_preferences": [
            "State the next action clearly",
            "Keep source references with findings",
        ],
    }


def _ensure_schema(repo) -> None:
    """Create the self-contained Manager schema and seed one neutral Manager.

    Every DDL statement is executed separately inside ``repo.atomic()``.  The
    write transaction serializes concurrent first opens and the INSERTs are
    idempotent, so a home can never receive duplicate Manager identities.
    """

    with repo.atomic() as conn:
        for statement in _SCHEMA_STATEMENTS:
            conn.execute(statement)

        manager = conn.execute("SELECT * FROM office_manager WHERE singleton=1").fetchone()
        if manager is None:
            manager_id = new_id()
            stamp = now()
            conn.execute(
                """
                INSERT INTO office_manager(singleton,id,current_version,created_at,updated_at)
                VALUES(1,?,?,?,?)
                """,
                (manager_id, 1, stamp, stamp),
            )
            defaults = _default_editable_fields()
            conn.execute(
                """
                INSERT INTO office_manager_versions(
                    manager_id,version,display_name,title,persona,personality_json,
                    working_preferences_json,created_at,updated_at
                ) VALUES(?,?,?,?,?,?,?,?,?)
                """,
                (
                    manager_id,
                    1,
                    defaults["display_name"],
                    defaults["title"],
                    defaults["persona"],
                    dump(defaults["personality"].model_dump(mode="json")),
                    dump(defaults["working_preferences"]),
                    stamp,
                    stamp,
                ),
            )
        else:
            # Validate that the saved Manager row still has its current
            # immutable version; existing preferences are never overwritten by
            # a GET or a normal reopen.
            version = int(manager["current_version"])
            version_row = conn.execute(
                "SELECT 1 FROM office_manager_versions WHERE manager_id=? AND version=?",
                (manager["id"], version),
            ).fetchone()
            if version_row is None:
                raise RuntimeError("The saved Tender Manager version is missing.")


class ManagerProfileService:
    """Read and version the one Manager profile stored for a home."""

    def __init__(self, repo):
        self.repo = repo
        _ensure_schema(repo)

    @staticmethod
    def _profile(row) -> ManagerProfile:
        if row is None:
            raise KeyError("The Tender Manager profile could not be found.")
        return ManagerProfile.model_validate(
            {
                "id": row["manager_id"],
                "version": row["version"],
                "display_name": row["display_name"],
                "title": row["title"],
                "persona": row["persona"],
                "personality": json.loads(row["personality_json"]),
                "working_preferences": json.loads(row["working_preferences_json"]),
                "created_at": row["created_at"],
                "updated_at": row["updated_at"],
            }
        )

    @staticmethod
    def _version_row(conn, manager_id: str, version: int):
        return conn.execute(
            """
            SELECT manager_id,version,display_name,title,persona,personality_json,
                   working_preferences_json,created_at,updated_at
            FROM office_manager_versions
            WHERE manager_id=? AND version=?
            """,
            (manager_id, version),
        ).fetchone()

    def get(self) -> ManagerProfile:
        """Return the current validated immutable Manager snapshot."""

        with self.repo.db.connect() as conn:
            manager = conn.execute(
                "SELECT id,current_version FROM office_manager WHERE singleton=1"
            ).fetchone()
            if manager is None:
                # This is defensive for a manually removed row; regular
                # construction has already initialized the schema.
                raise KeyError("The Tender Manager profile could not be found.")
            row = self._version_row(conn, manager["id"], manager["current_version"])
            if row is None:
                raise KeyError("The current Tender Manager version could not be found.")
            return self._profile(row)

    def profile_version(self, version: int) -> ManagerProfile:
        """Return one immutable Manager version, including historical versions."""

        if type(version) is not int or version < 1:
            raise ValueError("A Manager profile version must be a positive integer.")
        with self.repo.db.connect() as conn:
            manager = conn.execute("SELECT id FROM office_manager WHERE singleton=1").fetchone()
            if manager is None:
                raise KeyError("The Tender Manager profile could not be found.")
            row = self._version_row(conn, manager["id"], version)
            if row is None:
                raise KeyError("The requested Tender Manager version could not be found.")
            return self._profile(row)

    def update(self, edit: ManagerProfileEdit) -> ManagerProfile:
        """Persist a new immutable snapshot after an expected-version check."""

        if not isinstance(edit, OfficeModel):
            edit = ManagerProfileEdit.model_validate(edit)
        elif not isinstance(edit, ManagerProfileEdit):
            edit = ManagerProfileEdit.model_validate(edit)

        with self.repo.atomic() as conn:
            manager = conn.execute(
                "SELECT id,current_version FROM office_manager WHERE singleton=1"
            ).fetchone()
            if manager is None:
                raise KeyError("The Tender Manager profile could not be found.")
            current_version = int(manager["current_version"])
            if edit.expected_version != current_version:
                raise OfficeConflict(
                    f"The Tender Manager changed from version {edit.expected_version}; refresh before editing."
                )

            next_version = current_version + 1
            stamp = now()
            conn.execute(
                """
                INSERT INTO office_manager_versions(
                    manager_id,version,display_name,title,persona,personality_json,
                    working_preferences_json,created_at,updated_at
                ) VALUES(?,?,?,?,?,?,?,?,?)
                """,
                (
                    manager["id"],
                    next_version,
                    edit.display_name,
                    edit.title,
                    edit.persona,
                    dump(edit.personality.model_dump(mode="json")),
                    dump(edit.working_preferences),
                    stamp,
                    stamp,
                ),
            )
            changed = conn.execute(
                """
                UPDATE office_manager
                SET current_version=?,updated_at=?
                WHERE singleton=1 AND current_version=?
                """,
                (next_version, stamp, current_version),
            ).rowcount
            if changed != 1:
                raise OfficeConflict("The Tender Manager changed; refresh before editing.")
            row = self._version_row(conn, manager["id"], next_version)
            return self._profile(row)


__all__ = ["ManagerProfileService", "OfficeConflict"]
