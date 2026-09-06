"""SQLite connections, schema and transaction boundaries."""

import json
import sqlite3
import threading
from contextlib import contextmanager
from datetime import UTC, datetime
from pathlib import Path
from uuid import uuid4


def now() -> str:
    return datetime.now(UTC).isoformat(timespec="milliseconds")


def new_id() -> str:
    return uuid4().hex


def dump(value) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), allow_nan=False)


SCHEMA = """
CREATE TABLE IF NOT EXISTS tenders (
 id TEXT PRIMARY KEY, name TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'intake',
 revision INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
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
CREATE TABLE IF NOT EXISTS decisions (
 id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),target_type TEXT NOT NULL,
 target_id TEXT NOT NULL,decision TEXT NOT NULL,rationale TEXT NOT NULL,created_at TEXT NOT NULL
);
CREATE TRIGGER IF NOT EXISTS decisions_immutable_update BEFORE UPDATE ON decisions BEGIN SELECT RAISE(ABORT,'Decisions are immutable'); END;
CREATE TRIGGER IF NOT EXISTS decisions_immutable_delete BEFORE DELETE ON decisions BEGIN SELECT RAISE(ABORT,'Decisions are immutable'); END;
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY,value_json TEXT NOT NULL);
"""


class Database:
    def __init__(self, home: Path):
        self._local = threading.local()
        self.home = home.resolve()
        self.home.mkdir(parents=True, exist_ok=True)
        self.path = self.home / "quantix.sqlite"
        with self.connect() as conn:
            version = conn.execute("PRAGMA user_version").fetchone()[0]
            if version not in (0, 1):
                raise ValueError("This workspace was created by an unsupported version of Quantix.")
            conn.executescript(SCHEMA)
            conn.execute("PRAGMA user_version=1")

    @contextmanager
    def connect(self, *, write=False):
        existing = getattr(self._local, "connection", None)
        if existing is not None:
            yield existing
            return
        conn = sqlite3.connect(self.path, timeout=30)
        conn.row_factory = sqlite3.Row
        conn.execute("PRAGMA foreign_keys=ON")
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute("PRAGMA synchronous=FULL")
        try:
            if write:
                conn.execute("BEGIN IMMEDIATE")
            self._local.connection = conn
            yield conn
            conn.commit()
        except BaseException:
            conn.rollback()
            raise
        finally:
            self._local.connection = None
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
