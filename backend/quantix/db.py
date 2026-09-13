"""SQLite connections, schema and transaction boundaries."""

import asyncio
import json
import sqlite3
import threading
from collections.abc import Callable
from contextlib import contextmanager
from datetime import UTC, datetime
from pathlib import Path
from uuid import uuid4

from filelock import FileLock

from .storage import prepare_process_environment, resolve_home


def now() -> str:
    return datetime.now(UTC).isoformat(timespec="milliseconds")


def new_id() -> str:
    return uuid4().hex


def dump(value) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), allow_nan=False)


SCHEMA = """
CREATE TABLE IF NOT EXISTS tenders (
 id TEXT PRIMARY KEY, name TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'intake',
 revision INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
 name_source TEXT NOT NULL DEFAULT 'engineer'
);
CREATE TABLE IF NOT EXISTS artifacts (
 id TEXT PRIMARY KEY, tender_id TEXT NOT NULL REFERENCES tenders(id),
 relative_path TEXT NOT NULL, name TEXT NOT NULL, version INTEGER NOT NULL,
 content_hash TEXT NOT NULL, size INTEGER NOT NULL, kind TEXT NOT NULL,
 status TEXT NOT NULL, area TEXT NOT NULL, metadata_json TEXT NOT NULL DEFAULT '{}',
 warnings_json TEXT NOT NULL DEFAULT '[]', is_current INTEGER NOT NULL DEFAULT 1,
 created_at TEXT NOT NULL, UNIQUE(tender_id, relative_path, version)
);
CREATE INDEX IF NOT EXISTS artifacts_tender ON artifacts(tender_id,is_current);
CREATE TABLE IF NOT EXISTS evidence (
 id TEXT PRIMARY KEY, artifact_id TEXT NOT NULL REFERENCES artifacts(id), locator TEXT NOT NULL,
 text TEXT NOT NULL, page INTEGER, sheet TEXT, cell_range TEXT, kind TEXT NOT NULL,
 metadata_json TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS evidence_artifact ON evidence(artifact_id);
CREATE VIRTUAL TABLE IF NOT EXISTS evidence_fts USING fts5(evidence_id UNINDEXED, text, tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER IF NOT EXISTS evidence_fts_insert AFTER INSERT ON evidence BEGIN
 INSERT INTO evidence_fts(evidence_id,text) VALUES(new.id,new.text);
END;
CREATE TABLE IF NOT EXISTS findings (
 id TEXT PRIMARY KEY, tender_id TEXT NOT NULL REFERENCES tenders(id), title TEXT NOT NULL,
 detail TEXT NOT NULL, kind TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'proposed',
 source_ids_json TEXT NOT NULL DEFAULT '[]', origin TEXT NOT NULL, run_id TEXT,
 is_stale INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS messages (
 id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),role TEXT NOT NULL,
 content TEXT NOT NULL,source_ids_json TEXT NOT NULL DEFAULT '[]',run_id TEXT,created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS plans (
 id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),title TEXT NOT NULL,
 version INTEGER NOT NULL,status TEXT NOT NULL,run_id TEXT,created_at TEXT NOT NULL,updated_at TEXT NOT NULL,
 UNIQUE(tender_id,version)
);
CREATE TABLE IF NOT EXISTS plan_basis (
 plan_id TEXT PRIMARY KEY REFERENCES plans(id),
 tender_id TEXT NOT NULL REFERENCES tenders(id), revision INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS tasks (
 id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),plan_id TEXT NOT NULL REFERENCES plans(id),
 title TEXT NOT NULL,description TEXT NOT NULL,role TEXT NOT NULL,status TEXT NOT NULL,
 source_ids_json TEXT NOT NULL DEFAULT '[]',result_json TEXT NOT NULL DEFAULT '{}',run_id TEXT,
 created_at TEXT NOT NULL,updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS runs (
 id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),kind TEXT NOT NULL,
 instruction TEXT NOT NULL,status TEXT NOT NULL,progress INTEGER NOT NULL DEFAULT 0,
 detail TEXT NOT NULL DEFAULT '',result_json TEXT NOT NULL DEFAULT '{}',usage_json TEXT NOT NULL DEFAULT '{}',
 error TEXT,created_at TEXT NOT NULL,updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS runs_tender ON runs(tender_id,created_at);
CREATE TABLE IF NOT EXISTS run_events (
 id INTEGER PRIMARY KEY AUTOINCREMENT,run_id TEXT NOT NULL REFERENCES runs(id),kind TEXT NOT NULL,
 message TEXT NOT NULL,data_json TEXT NOT NULL DEFAULT '{}',created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS run_events_run_cursor ON run_events(run_id,id);
CREATE INDEX IF NOT EXISTS run_events_activity_operation ON run_events(run_id,json_extract(data_json,'$.operation_id'),id);
CREATE TABLE IF NOT EXISTS run_activity_operations (
 id TEXT PRIMARY KEY,run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
 metadata_json TEXT NOT NULL,created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS run_activity_payloads (
 event_id INTEGER PRIMARY KEY REFERENCES run_events(id) ON DELETE CASCADE,
 content TEXT NOT NULL,redacted INTEGER NOT NULL DEFAULT 0,unavailable_json TEXT NOT NULL DEFAULT '[]'
);
CREATE TABLE IF NOT EXISTS run_activity_failures (
 run_id TEXT PRIMARY KEY REFERENCES runs(id) ON DELETE CASCADE,created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS decisions (
 id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),target_type TEXT NOT NULL,
 target_id TEXT NOT NULL,decision TEXT NOT NULL,rationale TEXT NOT NULL,created_at TEXT NOT NULL
);
CREATE TRIGGER IF NOT EXISTS decisions_immutable_update BEFORE UPDATE ON decisions BEGIN SELECT RAISE(ABORT,'Decisions are immutable'); END;
CREATE TRIGGER IF NOT EXISTS decisions_immutable_delete BEFORE DELETE ON decisions BEGIN SELECT RAISE(ABORT,'Decisions are immutable'); END;
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY,value_json TEXT NOT NULL);
"""

CURRENT_SCHEMA_VERSION = 3
# Forward migrations keyed by the user_version they produce. Version 1 is the
# initial SCHEMA applied from an empty database. Callables receive an open
# connection already inside an immediate transaction.
def _source_boq_migration(conn):
    from .source_boq import migrate_source_rows

    migrate_source_rows(conn)


def _tender_name_source_migration(conn):
    # Where the Tender name came from: the engineer, the package folder while
    # the project is being identified, or the AI identification pass.
    columns = {row[1] for row in conn.execute("PRAGMA table_info(tenders)")}
    if "name_source" not in columns:
        conn.execute("ALTER TABLE tenders ADD COLUMN name_source TEXT NOT NULL DEFAULT 'engineer'")


FORWARD_MIGRATIONS: dict[int, Callable[[sqlite3.Connection], None]] = {
    2: _source_boq_migration,
    3: _tender_name_source_migration,
}


class SchemaMigrationError(ValueError):
    """A schema change failed; the previous database remains usable."""


def _sidecar(path: Path, suffix: str) -> Path:
    return Path(str(path) + suffix)


def _replace_database_files(source: Path, destination: Path) -> None:
    destination.write_bytes(source.read_bytes())
    for suffix in ("-wal", "-shm"):
        extra = _sidecar(source, suffix)
        target = _sidecar(destination, suffix)
        if extra.exists():
            target.write_bytes(extra.read_bytes())
        else:
            target.unlink(missing_ok=True)


def apply_forward_migrations(
    path: Path, *, migrations: dict[int, Callable[[sqlite3.Connection], None]] | None = None
) -> None:
    """Apply serial migrations after a file snapshot so failure restores the prior database."""

    mapping = dict(FORWARD_MIGRATIONS if migrations is None else migrations)
    if not mapping:
        return
    probe = sqlite3.connect(path, timeout=30)
    try:
        version = probe.execute("PRAGMA user_version").fetchone()[0]
        pending = sorted(target for target in mapping if target > version)
        if not pending:
            return
        probe.execute("PRAGMA wal_checkpoint(TRUNCATE)")
        probe.commit()
    finally:
        probe.close()

    snapshot = path.with_name(path.name + ".pre-migration")
    try:
        _replace_database_files(path, snapshot)
        conn = sqlite3.connect(path, timeout=30)
        try:
            conn.execute("BEGIN IMMEDIATE")
            for target in pending:
                mapping[target](conn)
                conn.execute(f"PRAGMA user_version={int(target)}")
            if conn.execute("PRAGMA foreign_key_check").fetchone():
                raise ValueError("The upgraded database contains a broken source reference.")
            conn.commit()
        except BaseException as error:
            try:
                conn.rollback()
            except sqlite3.Error:
                pass
            conn.close()
            conn = None
            _replace_database_files(snapshot, path)
            raise SchemaMigrationError(
                "The workspace database could not be upgraded. The previous database is still usable."
            ) from error
        finally:
            if conn is not None:
                conn.close()
    finally:
        snapshot.unlink(missing_ok=True)
        for suffix in ("-wal", "-shm"):
            _sidecar(snapshot, suffix).unlink(missing_ok=True)


class Database:
    def __init__(self, home: Path):
        # One held connection per thread and asyncio task. Coroutines sharing
        # a thread must never share a connection: an interleaved borrower
        # could otherwise see another branch's open transaction.
        self._held: dict[tuple[int, int | None], sqlite3.Connection] = {}
        self._held_lock = threading.Lock()
        self.home = resolve_home(home)
        # Keep temporary and library cache writes inside this explicit
        # workspace before opening any database or dependent service work.
        prepare_process_environment(self.home)
        self.home.mkdir(parents=True, exist_ok=True)
        self.path = self.home / "quantix.sqlite"
        # Journal mode persists in SQLite. Serialize its initial transition;
        # changing it on every connection can fail when another reader is
        # initializing the same workspace, even with a busy timeout.
        with FileLock(self.home / "runtime" / "database-init.lock", timeout=30):
            with self.connect() as conn:
                if conn.execute("PRAGMA journal_mode").fetchone()[0].lower() != "wal":
                    conn.execute("PRAGMA journal_mode=WAL")
                version = conn.execute("PRAGMA user_version").fetchone()[0]
                known = {0, 1, CURRENT_SCHEMA_VERSION, *FORWARD_MIGRATIONS}
                if version not in known:
                    raise ValueError("This workspace was created by an unsupported version of Quantix.")
                conn.executescript(SCHEMA)
                if version == 0:
                    conn.execute(f"PRAGMA user_version={CURRENT_SCHEMA_VERSION}")
            apply_forward_migrations(self.path)

    @staticmethod
    def _owner() -> tuple[int, int | None]:
        try:
            task = asyncio.current_task()
        except RuntimeError:
            task = None
        return (threading.get_ident(), None if task is None else id(task))

    def held_connection(self):
        """Return this thread/task's open connection, if it holds one."""

        with self._held_lock:
            return self._held.get(self._owner())

    @contextmanager
    def connect(self, *, write=False):
        owner = self._owner()
        with self._held_lock:
            existing = self._held.get(owner)
        if existing is not None:
            yield existing
            return
        conn = sqlite3.connect(self.path, timeout=30)
        conn.row_factory = sqlite3.Row
        conn.execute("PRAGMA foreign_keys=ON")
        conn.execute("PRAGMA synchronous=FULL")
        try:
            if write:
                conn.execute("BEGIN IMMEDIATE")
            with self._held_lock:
                self._held[owner] = conn
            yield conn
            conn.commit()
        except BaseException:
            conn.rollback()
            raise
        finally:
            with self._held_lock:
                self._held.pop(owner, None)
            conn.close()


def record(row) -> dict:
    if row is None:
        raise KeyError("This item could not be found in the selected Tender.")
    result = {}
    for key in row.keys():
        value = row[key]
        if key.endswith("_json"):
            result[key[:-5]] = json.loads(value)
        elif key in {"is_current", "is_stale"}:
            result[key] = bool(value)
        else:
            result[key] = value
    return result
