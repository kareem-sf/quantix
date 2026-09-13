"""Versioned provider-independent professional definition library."""

from __future__ import annotations

import hashlib
import json
import re

from .agent_definition_models import (
    AgentDefinitionCreate,
    AgentDefinitionDuplicate,
    AgentDefinitionEdit,
    AgentDefinitionExport,
    AgentDefinitionRecord,
    AgentDefinitionRetire,
)
from .db import dump, new_id, now
from .staff_models import OfficeConflict

_SCHEMA_STATEMENTS = (
    """
    CREATE TABLE IF NOT EXISTS agent_definitions(
        id TEXT PRIMARY KEY,
        current_version INTEGER NOT NULL CHECK(current_version >= 1),
        lifecycle TEXT NOT NULL CHECK(lifecycle IN ('active','retired')),
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS agent_definition_versions(
        definition_id TEXT NOT NULL REFERENCES agent_definitions(id),
        version INTEGER NOT NULL CHECK(version >= 1),
        profile_json TEXT NOT NULL,
        generation_settings_json TEXT NOT NULL,
        fingerprint TEXT NOT NULL,
        source_definition_id TEXT,
        source_definition_version INTEGER CHECK(source_definition_version >= 1),
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY(definition_id,version)
    )
    """,
    """
    CREATE INDEX IF NOT EXISTS agent_definitions_order
    ON agent_definitions(updated_at,id)
    """,
    """
    CREATE TABLE IF NOT EXISTS agent_definition_receipts(
        id TEXT PRIMARY KEY,
        operation TEXT NOT NULL,
        idempotency_key TEXT NOT NULL,
        payload_hash TEXT NOT NULL,
        definition_id TEXT NOT NULL REFERENCES agent_definitions(id),
        definition_version INTEGER NOT NULL CHECK(definition_version >= 1),
        created_at TEXT NOT NULL,
        UNIQUE(operation,idempotency_key)
    )
    """,
)

_KEY_RE = re.compile(r"[A-Za-z0-9._~-]{1,160}\Z")


def _key(value: str) -> str:
    if not isinstance(value, str) or _KEY_RE.fullmatch(value) is None:
        raise ValueError(
            "The idempotency key must contain 1 to 160 permitted identifier characters."
        )
    return value


def _hash(value: dict) -> str:
    encoded = json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
        allow_nan=False,
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


class AgentDefinitionService:
    """Persist reusable definitions without creating staff or authority."""

    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            for statement in _SCHEMA_STATEMENTS:
                conn.execute(statement)

    @staticmethod
    def _row(conn, definition_id: str, version: int | None = None):
        if version is None:
            return conn.execute(
                """
                SELECT d.id,d.current_version,d.lifecycle,d.created_at AS identity_created_at,
                       d.updated_at AS identity_updated_at,v.*
                FROM agent_definitions d
                JOIN agent_definition_versions v
                  ON v.definition_id=d.id AND v.version=d.current_version
                WHERE d.id=?
                """,
                (definition_id,),
            ).fetchone()
        if type(version) is not int or version < 1:
            raise ValueError("A definition version must be a positive integer.")
        return conn.execute(
            """
            SELECT d.id,d.current_version,d.lifecycle,d.created_at AS identity_created_at,
                   d.updated_at AS identity_updated_at,v.*
            FROM agent_definitions d
            JOIN agent_definition_versions v ON v.definition_id=d.id
            WHERE d.id=? AND v.version=?
            """,
            (definition_id, version),
        ).fetchone()

    @staticmethod
    def _record(row) -> AgentDefinitionRecord:
        if row is None:
            raise KeyError("This professional definition could not be found.")
        return AgentDefinitionRecord(
            id=row["definition_id"],
            version=int(row["version"]),
            lifecycle=row["lifecycle"],
            profile=json.loads(row["profile_json"]),
            generation_settings=json.loads(row["generation_settings_json"]),
            fingerprint=row["fingerprint"],
            source_definition_id=row["source_definition_id"],
            source_definition_version=row["source_definition_version"],
            created_at=row["created_at"],
            updated_at=row["updated_at"],
        )

    @staticmethod
    def _receipt(conn, operation: str, key: str):
        return conn.execute(
            """
            SELECT operation,idempotency_key,payload_hash,definition_id,definition_version
            FROM agent_definition_receipts
            WHERE operation=? AND idempotency_key=?
            """,
            (operation, key),
        ).fetchone()

    @classmethod
    def _replay(cls, conn, operation: str, key: str, payload_hash: str):
        receipt = cls._receipt(conn, operation, key)
        if receipt is None:
            return None
        if receipt["payload_hash"] != payload_hash:
            raise OfficeConflict(
                "This definition request key was already used for different professional data."
            )
        return cls._record(
            cls._row(conn, receipt["definition_id"], int(receipt["definition_version"]))
        )

    @staticmethod
    def _write_receipt(
        conn,
        operation: str,
        key: str,
        payload_hash: str,
        definition_id: str,
        version: int,
    ) -> None:
        conn.execute(
            """
            INSERT INTO agent_definition_receipts(
                id,operation,idempotency_key,payload_hash,definition_id,
                definition_version,created_at
            ) VALUES(?,?,?,?,?,?,?)
            """,
            (new_id(), operation, key, payload_hash, definition_id, version, now()),
        )

    @staticmethod
    def _content(command: AgentDefinitionCreate | AgentDefinitionEdit) -> tuple[dict, dict, str]:
        profile = command.profile.model_dump(mode="json")
        settings = command.generation_settings.model_dump(mode="json")
        return profile, settings, _hash({"profile": profile, "generation_settings": settings})

    def create(self, command: AgentDefinitionCreate | dict) -> AgentDefinitionRecord:
        command = AgentDefinitionCreate.model_validate(command)
        key = _key(command.idempotency_key)
        body_hash = _hash(command.model_dump(mode="json", exclude={"idempotency_key"}))
        with self.repo.atomic() as conn:
            replay = self._replay(conn, "create", key, body_hash)
            if replay is not None:
                return replay
            profile, settings, fingerprint = self._content(command)
            definition_id, stamp = new_id(), now()
            conn.execute(
                """
                INSERT INTO agent_definitions(id,current_version,lifecycle,created_at,updated_at)
                VALUES(?,1,'active',?,?)
                """,
                (definition_id, stamp, stamp),
            )
            conn.execute(
                """
                INSERT INTO agent_definition_versions(
                    definition_id,version,profile_json,generation_settings_json,fingerprint,
                    source_definition_id,source_definition_version,created_at,updated_at
                ) VALUES(?,1,?,?,?,?,?,?,?)
                """,
                (
                    definition_id,
                    dump(profile),
                    dump(settings),
                    fingerprint,
                    None,
                    None,
                    stamp,
                    stamp,
                ),
            )
            self._write_receipt(conn, "create", key, body_hash, definition_id, 1)
            return self._record(self._row(conn, definition_id, 1))

    def get(self, definition_id: str, version: int | None = None) -> AgentDefinitionRecord:
        with self.repo.db.connect() as conn:
            return self._record(self._row(conn, definition_id, version))

    def list(self, *, include_retired: bool = False) -> list[AgentDefinitionRecord]:
        if type(include_retired) is not bool:
            raise ValueError("The retired-definition filter must be true or false.")
        with self.repo.db.connect() as conn:
            rows = conn.execute(
                """
                SELECT id,current_version FROM agent_definitions
                WHERE lifecycle='active' OR ?
                ORDER BY updated_at DESC,id DESC
                """,
                (1 if include_retired else 0,),
            ).fetchall()
            return [
                self._record(self._row(conn, row["id"], row["current_version"])) for row in rows
            ]

    def versions(
        self, definition_id: str, *, offset: int = 0, limit: int = 50
    ) -> list[AgentDefinitionRecord]:
        if type(offset) is not int or offset < 0:
            raise ValueError("The definition version offset must be nonnegative.")
        if type(limit) is not int or not 1 <= limit <= 200:
            raise ValueError("The definition version limit must be between 1 and 200.")
        with self.repo.db.connect() as conn:
            if self._row(conn, definition_id) is None:
                raise KeyError("This professional definition could not be found.")
            rows = conn.execute(
                """
                SELECT version FROM agent_definition_versions
                WHERE definition_id=? ORDER BY version DESC LIMIT ? OFFSET ?
                """,
                (definition_id, limit, offset),
            ).fetchall()
            return [self._record(self._row(conn, definition_id, row["version"])) for row in rows]

    def revise(
        self, definition_id: str, command: AgentDefinitionEdit | dict
    ) -> AgentDefinitionRecord:
        command = AgentDefinitionEdit.model_validate(command)
        key = _key(command.idempotency_key)
        body_hash = _hash(
            {
                "definition_id": definition_id,
                **command.model_dump(mode="json", exclude={"idempotency_key"}),
            }
        )
        with self.repo.atomic() as conn:
            replay = self._replay(conn, "revise", key, body_hash)
            if replay is not None:
                return replay
            identity = conn.execute(
                "SELECT current_version,lifecycle FROM agent_definitions WHERE id=?",
                (definition_id,),
            ).fetchone()
            if identity is None:
                raise KeyError("This professional definition could not be found.")
            if identity["lifecycle"] == "retired":
                raise OfficeConflict(
                    "This professional definition is retired and cannot be edited."
                )
            current_version = int(identity["current_version"])
            if command.expected_version != current_version:
                raise OfficeConflict(
                    f"The professional definition changed from version {command.expected_version}; refresh before editing."
                )
            previous = self._record(self._row(conn, definition_id, current_version))
            profile, settings, fingerprint = self._content(command)
            next_version, stamp = current_version + 1, now()
            conn.execute(
                """
                INSERT INTO agent_definition_versions(
                    definition_id,version,profile_json,generation_settings_json,fingerprint,
                    source_definition_id,source_definition_version,created_at,updated_at
                ) VALUES(?,?,?,?,?,?,?,?,?)
                """,
                (
                    definition_id,
                    next_version,
                    dump(profile),
                    dump(settings),
                    fingerprint,
                    previous.source_definition_id,
                    previous.source_definition_version,
                    stamp,
                    stamp,
                ),
            )
            changed = conn.execute(
                """
                UPDATE agent_definitions SET current_version=?,updated_at=?
                WHERE id=? AND current_version=? AND lifecycle='active'
                """,
                (next_version, stamp, definition_id, current_version),
            ).rowcount
            if changed != 1:
                raise OfficeConflict("The professional definition changed; refresh before editing.")
            self._write_receipt(conn, "revise", key, body_hash, definition_id, next_version)
            return self._record(self._row(conn, definition_id, next_version))

    def duplicate(
        self, definition_id: str, command: AgentDefinitionDuplicate | dict
    ) -> AgentDefinitionRecord:
        command = AgentDefinitionDuplicate.model_validate(command)
        key = _key(command.idempotency_key)
        body_hash = _hash(
            {
                "definition_id": definition_id,
                **command.model_dump(mode="json", exclude={"idempotency_key"}),
            }
        )
        with self.repo.atomic() as conn:
            replay = self._replay(conn, "duplicate", key, body_hash)
            if replay is not None:
                return replay
            source = self._record(self._row(conn, definition_id, command.source_version))
            profile = source.profile.model_copy(update={"display_name": command.display_name})
            create_like = AgentDefinitionCreate(
                profile=profile,
                generation_settings=source.generation_settings,
                idempotency_key=key,
            )
            profile_data, settings, fingerprint = self._content(create_like)
            copy_id, stamp = new_id(), now()
            conn.execute(
                """
                INSERT INTO agent_definitions(id,current_version,lifecycle,created_at,updated_at)
                VALUES(?,1,'active',?,?)
                """,
                (copy_id, stamp, stamp),
            )
            conn.execute(
                """
                INSERT INTO agent_definition_versions(
                    definition_id,version,profile_json,generation_settings_json,fingerprint,
                    source_definition_id,source_definition_version,created_at,updated_at
                ) VALUES(?,1,?,?,?,?,?,?,?)
                """,
                (
                    copy_id,
                    dump(profile_data),
                    dump(settings),
                    fingerprint,
                    definition_id,
                    command.source_version,
                    stamp,
                    stamp,
                ),
            )
            self._write_receipt(conn, "duplicate", key, body_hash, copy_id, 1)
            return self._record(self._row(conn, copy_id, 1))

    def retire(
        self, definition_id: str, command: AgentDefinitionRetire | dict
    ) -> AgentDefinitionRecord:
        command = AgentDefinitionRetire.model_validate(command)
        key = _key(command.idempotency_key)
        body_hash = _hash(
            {
                "definition_id": definition_id,
                **command.model_dump(mode="json", exclude={"idempotency_key"}),
            }
        )
        with self.repo.atomic() as conn:
            replay = self._replay(conn, "retire", key, body_hash)
            if replay is not None:
                return replay
            identity = conn.execute(
                "SELECT current_version,lifecycle FROM agent_definitions WHERE id=?",
                (definition_id,),
            ).fetchone()
            if identity is None:
                raise KeyError("This professional definition could not be found.")
            current_version = int(identity["current_version"])
            if command.expected_version != current_version:
                raise OfficeConflict(
                    f"The professional definition changed from version {command.expected_version}; refresh before retiring."
                )
            if identity["lifecycle"] != "retired":
                conn.execute(
                    "UPDATE agent_definitions SET lifecycle='retired',updated_at=? WHERE id=?",
                    (now(), definition_id),
                )
            self._write_receipt(conn, "retire", key, body_hash, definition_id, current_version)
            return self._record(self._row(conn, definition_id, current_version))

    def export(self, definition_id: str, *, version: int | None = None) -> AgentDefinitionExport:
        record = self.get(definition_id, version)
        return AgentDefinitionExport(
            definition_id=record.id,
            version=record.version,
            fingerprint=record.fingerprint,
            profile=record.profile,
            generation_settings=record.generation_settings,
        )


__all__ = ["AgentDefinitionService"]
