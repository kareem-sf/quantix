"""Durable Tender records and the narrow access surface used by the office."""

import base64
import json
import re
from pathlib import Path, PurePosixPath

from .db import Database, dump, new_id, now, record
from .diagnostics import initialize
from .pending import ensure_schema as ensure_pending_schema
from .storage import logs_dir

CURRENT_EVIDENCE_SQL = "a.is_current=1 AND COALESCE(e.is_current,1)=1"


def _tender_item(row):
    item = record(row)
    item.pop("retrieval_generation", None)
    return item


def _artifact_item(row):
    item = record(row)
    item.pop("active_extraction_id", None)
    return item


def _evidence_item(row):
    item = record(row)
    if "is_current" in item:
        item["extraction_current"] = bool(item.pop("is_current"))
    return item


def text(value: str, label: str, limit: int = 20000) -> str:
    if not isinstance(value, str) or not value.strip() or len(value) > limit:
        raise ValueError(f"Enter {label} (up to {limit} characters).")
    return value.strip()


class Repository:
    def __init__(self, home: Path):
        self.db = Database(home)
        self.home = self.db.home
        initialize(logs_dir(self.home))
        self.objects = self.home / "objects"
        self.objects.mkdir(exist_ok=True)
        (self.home / "extractions").mkdir(exist_ok=True)
        ensure_pending_schema(self)
        self.on_retrieval_generation = None

    def retrieval_generation(self, tender_id) -> int:
        with self.db.connect() as conn:
            row = conn.execute(
                "SELECT retrieval_generation FROM tenders WHERE id=?", (tender_id,)
            ).fetchone()
        if row is None:
            raise KeyError(tender_id)
        return int(row[0] or 0)

    def current_evidence_stats(self, tender_id) -> tuple[int, int]:
        self.get_tender(tender_id)
        with self.db.connect() as conn:
            row = conn.execute(
                """SELECT COUNT(*), COALESCE(SUM(LENGTH(e.text)), 0)
                FROM evidence e JOIN artifacts a ON a.id=e.artifact_id
                WHERE a.tender_id=? AND a.is_current=1 AND COALESCE(e.is_current,1)=1""",
                (tender_id,),
            ).fetchone()
        return int(row[0] or 0), int(row[1] or 0)

    def advance_retrieval_generation(self, tender_id, conn) -> int:
        """Advance the desired retrieval generation inside an open write transaction."""
        stamp = now()
        conn.execute(
            """UPDATE tenders SET retrieval_generation=retrieval_generation+1,
            revision=revision+1, updated_at=? WHERE id=?""",
            (stamp, tender_id),
        )
        row = conn.execute(
            "SELECT retrieval_generation FROM tenders WHERE id=?", (tender_id,)
        ).fetchone()
        if row is None:
            raise KeyError(tender_id)
        return int(row[0])

    def notify_retrieval_generation(self, tender_id) -> None:
        callback = self.on_retrieval_generation
        if callback:
            callback(tender_id)

    def atomic(self):
        """Group validated synchronous domain operations in one SQLite transaction."""
        return self.db.connect(write=True)

    def list_tenders(self):
        with self.db.connect() as conn:
            return [
                _tender_item(r)
                for r in conn.execute("SELECT * FROM tenders ORDER BY updated_at DESC,id")
            ]

    def create_tender(self, name: str | None = None):
        # Without a name the Tender is named by analysing its package.
        source = "engineer" if name is not None else "pending"
        name = text(name, "a Tender name", 200) if name is not None else "New tender"
        identifier, stamp = new_id(), now()
        with self.db.connect(write=True) as conn:
            conn.execute(
                "INSERT INTO tenders(id,name,name_source,created_at,updated_at) VALUES(?,?,?,?,?)",
                (identifier, name, source, stamp, stamp),
            )
        return self.get_tender(identifier)

    def rename_tender(self, tender_id, name: str, *, source: str = "engineer"):
        if source not in {"engineer", "pending", "package", "ai"}:
            raise ValueError("Unknown Tender name source.")
        name = text(name, "a Tender name", 200)
        self.get_tender(tender_id)
        with self.db.connect(write=True) as conn:
            conn.execute(
                "UPDATE tenders SET name=?, name_source=?, revision=revision+1, updated_at=? WHERE id=?",
                (name, source, now(), tender_id),
            )
        return self.get_tender(tender_id)

    def get_tender(self, tender_id):
        with self.db.connect() as conn:
            return _tender_item(
                conn.execute("SELECT * FROM tenders WHERE id=?", (tender_id,)).fetchone()
            )

    def list_artifacts(self, tender_id, *, current_only=True):
        self.get_tender(tender_id)
        with self.db.connect() as conn:
            query = "SELECT * FROM artifacts WHERE tender_id=?"
            if current_only:
                query += " AND is_current=1"
            query += " ORDER BY relative_path,version DESC"
            return [_artifact_item(r) for r in conn.execute(query, (tender_id,))]

    def get_artifact(self, tender_id, artifact_id):
        with self.db.connect() as conn:
            return _artifact_item(
                conn.execute(
                    "SELECT * FROM artifacts WHERE tender_id=? AND id=?", (tender_id, artifact_id)
                ).fetchone()
            )

    def object_path(self, tender_id, artifact_id):
        artifact = self.get_artifact(tender_id, artifact_id)
        digest = artifact["content_hash"]
        if not re.fullmatch(r"[a-f0-9]{64}", digest):
            raise ValueError("No readable source was registered for this file.")
        path = self.objects / digest
        if not path.is_file():
            raise ValueError(
                "The saved source file is missing. Restore a backup or import it again."
            )
        return path

    def register_artifact(self, tender_id, relative_path, content_hash, size, extraction):
        self.get_tender(tender_id)
        relative_path = relative_path.replace("\\", "/")
        relative = PurePosixPath(relative_path)
        if relative.is_absolute() or ".." in relative.parts or not relative.name:
            raise ValueError("A source file has an unsafe relative path.")
        stamp, identifier = now(), new_id()
        bumped = False
        with self.db.connect(write=True) as conn:
            old = conn.execute(
                "SELECT * FROM artifacts WHERE tender_id=? AND relative_path=? AND is_current=1",
                (tender_id, relative_path),
            ).fetchone()
            if (
                old
                and old["content_hash"] == content_hash
                and old["status"] in {"extracted", "needs_attention", "unsupported"}
            ):
                return record(old), False
            version = (old["version"] + 1) if old else 1
            if old:
                conn.execute("UPDATE artifacts SET is_current=0 WHERE id=?", (old["id"],))
                source_ids = {
                    r[0]
                    for r in conn.execute(
                        "SELECT id FROM evidence WHERE artifact_id=?", (old["id"],)
                    )
                }
                for finding_row in conn.execute(
                    "SELECT * FROM findings WHERE tender_id=?", (tender_id,)
                ).fetchall():
                    finding = record(finding_row)
                    if source_ids.intersection(finding["source_ids"]):
                        conn.execute(
                            "UPDATE findings SET is_stale=1,updated_at=? WHERE id=?",
                            (stamp, finding["id"]),
                        )
                for task_row in conn.execute(
                    "SELECT * FROM tasks WHERE tender_id=? AND status IN ('awaiting_approval','ready','completed','failed','cancelled','interrupted')",
                    (tender_id,),
                ).fetchall():
                    task = record(task_row)
                    dependencies = set(task["source_ids"])
                    dependencies.update(task["result"].get("source_ids", []))
                    dependencies.update(task["result"].get("source_ids_read", []))
                    if source_ids.intersection(dependencies):
                        conn.execute(
                            "UPDATE tasks SET status='needs_review',updated_at=? WHERE id=?",
                            (stamp, task["id"]),
                        )
            area = relative.parts[0] if len(relative.parts) > 1 else "General"
            conn.execute(
                """INSERT INTO artifacts(id,tender_id,relative_path,name,version,content_hash,size,kind,status,area,metadata_json,warnings_json,created_at)
                VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)""",
                (
                    identifier,
                    tender_id,
                    relative_path,
                    relative.name,
                    version,
                    content_hash,
                    size,
                    extraction["kind"],
                    extraction["status"],
                    area,
                    dump(extraction.get("metadata", {})),
                    dump(extraction.get("warnings", [])),
                    stamp,
                ),
            )
            for segment in extraction.get("segments", []):
                conn.execute(
                    """INSERT INTO evidence(
                        id,artifact_id,locator,text,page,sheet,cell_range,kind,metadata_json,extraction_id,is_current
                    ) VALUES(?,?,?,?,?,?,?,?,?,?,1)""",
                    (
                        new_id(),
                        identifier,
                        segment["locator"],
                        segment["text"],
                        segment.get("page"),
                        segment.get("sheet"),
                        segment.get("cell_range"),
                        segment.get("kind", "text"),
                        dump(segment.get("metadata", {})),
                        None,
                    ),
                )
            searchable = any(
                str(segment.get("text") or "").strip() for segment in extraction.get("segments", [])
            )
            if old or searchable:
                conn.execute(
                    """UPDATE tenders SET revision=revision+1, retrieval_generation=retrieval_generation+1,
                    updated_at=? WHERE id=?""",
                    (stamp, tender_id),
                )
                bumped = True
            else:
                conn.execute(
                    "UPDATE tenders SET revision=revision+1,updated_at=? WHERE id=?",
                    (stamp, tender_id),
                )
        if bumped:
            self.notify_retrieval_generation(tender_id)
        return self.get_artifact(tender_id, identifier), True

    @staticmethod
    def _cell_range_bounds(value):
        from openpyxl.utils.cell import range_boundaries

        try:
            if not isinstance(value, str) or not value.strip() or len(value) > 80:
                raise ValueError
            first_col, first_row, last_col, last_row = range_boundaries(value.strip().upper())
            first_col, last_col = first_col or 1, last_col or 16384
            first_row = 1 if first_row is None else first_row
            last_row = 1048576 if last_row is None else last_row
            if not (1 <= first_col <= last_col <= 16384 and 1 <= first_row <= last_row <= 1048576):
                raise ValueError
            return first_col, first_row, last_col, last_row
        except (TypeError, ValueError):
            raise ValueError("Enter a valid cell range such as A1:D20.") from None

    def artifact_evidence(
        self, tender_id, artifact_id, offset=0, limit=50, *, sheet=None, cell_range=None
    ):
        self.get_artifact(tender_id, artifact_id)
        bounds = self._cell_range_bounds(cell_range) if cell_range is not None else None
        offset, limit = max(0, int(offset)), max(1, min(int(limit), 200))
        with self.db.connect() as conn:
            query = """SELECT e.*,a.name AS artifact_name,a.relative_path FROM evidence e JOIN artifacts a ON a.id=e.artifact_id
                WHERE a.tender_id=? AND a.id=? AND COALESCE(e.is_current,1)=1 AND (? IS NULL OR e.sheet=?) ORDER BY e.rowid"""
            parameters = (tender_id, artifact_id, sheet, sheet)
            if bounds is None:
                return [
                    _evidence_item(row) | {"score": 0.0}
                    for row in conn.execute(
                        query + " LIMIT ? OFFSET ?", (*parameters, limit, offset)
                    )
                ]
            matches, skipped = [], 0
            for row in conn.execute(query, parameters):
                item = _evidence_item(row)
                source_range = item.get("cell_range")
                if not source_range and type(item.get("metadata", {}).get("row")) is int:
                    source_range = f"A{item['metadata']['row']}:XFD{item['metadata']['row']}"
                if not source_range:
                    continue
                try:
                    source = self._cell_range_bounds(source_range)
                except ValueError:
                    continue
                if (
                    source[0] > bounds[2]
                    or source[2] < bounds[0]
                    or source[1] > bounds[3]
                    or source[3] < bounds[1]
                ):
                    continue
                if skipped < offset:
                    skipped += 1
                    continue
                matches.append(item | {"score": 0.0})
                if len(matches) == limit:
                    break
            return matches

    def get_evidence(self, tender_id, evidence_id):
        with self.db.connect() as conn:
            row = conn.execute(
                """SELECT e.*,a.name AS artifact_name,a.relative_path FROM evidence e JOIN artifacts a ON a.id=e.artifact_id
                WHERE a.tender_id=? AND e.id=?""",
                (tender_id, evidence_id),
            ).fetchone()
            return _evidence_item(row) | {"score": 0.0}

    def search(
        self,
        tender_id,
        query,
        limit=20,
        *,
        area=None,
        status=None,
        document_kind=None,
        artifact_ids=None,
        evidence_ids=None,
        ceiling=None,
    ):
        hits, _meta = self.search_keyword(
            tender_id,
            query,
            limit=limit,
            area=area,
            status=status,
            document_kind=document_kind,
            artifact_ids=artifact_ids,
            evidence_ids=evidence_ids,
            ceiling=ceiling,
        )
        for hit in hits:
            hit.pop("content_hash", None)
            hit.pop("source_version", None)
            hit.pop("document_kind", None)
            hit.pop("_duplicates", None)
        return hits

    def search_keyword(
        self,
        tender_id,
        query,
        limit=20,
        *,
        area=None,
        status=None,
        document_kind=None,
        artifact_ids=None,
        evidence_ids=None,
        ceiling=None,
        skip=0,
    ):
        """Distinct current keyword hits with scope/kind applied before the limit."""

        from .retrieval_ranking import (
            KEYWORD_SCAN_CEILING,
            filename_needle,
            group_distinct,
            keyword_expression,
        )

        self.get_tender(tender_id)
        if artifact_ids is not None and len(artifact_ids) == 0:
            return [], {"scanned": 0, "truncated": False, "more": False, "ceiling": 0}
        if evidence_ids is not None and len(evidence_ids) == 0:
            return [], {"scanned": 0, "truncated": False, "more": False, "ceiling": 0}
        limit = min(max(int(limit), 1), 100)
        skip = max(int(skip), 0)
        ceiling = (
            KEYWORD_SCAN_CEILING
            if ceiling is None
            else min(max(int(ceiling), 1), KEYWORD_SCAN_CEILING)
        )
        expression = keyword_expression(query)
        needle = filename_needle(query)
        if not expression and not needle:
            return [], {"scanned": 0, "truncated": False, "more": False, "ceiling": ceiling}
        with self.db.connect() as conn:
            scope_sql, scope_params = self._retrieval_scope(conn, artifact_ids, evidence_ids)
            filters = f"""a.tender_id=? AND {CURRENT_EVIDENCE_SQL}
                AND (? IS NULL OR a.area=?) AND (? IS NULL OR a.status=?)
                AND (? IS NULL OR a.kind=?)"""
            filter_params = (tender_id, area, area, status, status, document_kind, document_kind)
            rows = []
            if expression:
                rows.extend(
                    conn.execute(
                        f"""SELECT e.*,a.name AS artifact_name,a.relative_path,a.content_hash,
                        a.version AS source_version,a.kind AS document_kind,bm25(evidence_fts) AS score
                        FROM evidence_fts JOIN evidence e ON e.id=evidence_fts.evidence_id
                        JOIN artifacts a ON a.id=e.artifact_id
                        WHERE evidence_fts MATCH ? AND {filters} {scope_sql}
                        ORDER BY score, a.relative_path, e.id LIMIT ?""",
                        (expression, *filter_params, *scope_params, ceiling),
                    ).fetchall()
                )
            seen_ids = {row["id"] for row in rows}
            if needle and len(rows) < ceiling:
                for row in conn.execute(
                    f"""SELECT e.*,a.name AS artifact_name,a.relative_path,a.content_hash,
                    a.version AS source_version,a.kind AS document_kind,1000.0 AS score
                    FROM evidence e JOIN artifacts a ON a.id=e.artifact_id
                    WHERE {filters} {scope_sql}
                    AND (instr(lower(a.name), ?) > 0 OR instr(lower(a.relative_path), ?) > 0)
                    ORDER BY a.relative_path, e.id LIMIT ?""",
                    (*filter_params, *scope_params, needle, needle, ceiling),
                ):
                    if row["id"] in seen_ids:
                        continue
                    rows.append(row)
                    seen_ids.add(row["id"])
                    if len(rows) >= ceiling:
                        break
        scanned = len(rows)
        items = [_evidence_item(row) for row in rows]
        hits, more = group_distinct(items, limit=limit, skip=skip)
        truncated = scanned >= ceiling
        return hits, {
            "scanned": scanned,
            "truncated": truncated,
            "more": more or truncated,
            "ceiling": ceiling,
        }

    def _retrieval_scope(self, conn, artifact_ids, evidence_ids):
        clauses = []
        params: list = []
        if artifact_ids is not None:
            conn.execute("CREATE TEMP TABLE IF NOT EXISTS _perm_art (id TEXT PRIMARY KEY)")
            conn.execute("DELETE FROM _perm_art")
            conn.executemany(
                "INSERT OR IGNORE INTO _perm_art(id) VALUES (?)", ((item,) for item in artifact_ids)
            )
            clauses.append("AND a.id IN (SELECT id FROM _perm_art)")
        if evidence_ids is not None:
            conn.execute("CREATE TEMP TABLE IF NOT EXISTS _perm_ev (id TEXT PRIMARY KEY)")
            conn.execute("DELETE FROM _perm_ev")
            conn.executemany(
                "INSERT OR IGNORE INTO _perm_ev(id) VALUES (?)", ((item,) for item in evidence_ids)
            )
            clauses.append("AND e.id IN (SELECT id FROM _perm_ev)")
        return " ".join(clauses), params

    def _check_sources(self, conn, tender_id, source_ids):
        if not isinstance(source_ids, list) or len(source_ids) > 200:
            raise ValueError("Provide a bounded list of source references.")
        for source_id in source_ids:
            found = conn.execute(
                "SELECT a.is_current FROM evidence e JOIN artifacts a ON e.artifact_id=a.id WHERE a.tender_id=? AND e.id=?",
                (tender_id, source_id),
            ).fetchone()
            if not found:
                raise ValueError("A source reference does not belong to this Tender.")
            if not found[0]:
                raise ValueError(
                    "A source reference has been superseded. Review the current source."
                )

    def list_findings(self, tender_id):
        self.get_tender(tender_id)
        with self.db.connect() as conn:
            return [
                record(r)
                for r in conn.execute(
                    "SELECT * FROM findings WHERE tender_id=? ORDER BY created_at,id", (tender_id,)
                )
            ]

    def add_finding(self, tender_id, title, detail, kind, source_ids, origin="agent", run_id=None):
        self.get_tender(tender_id)
        title, detail = text(title, "a finding title", 300), text(detail, "the finding details")
        if kind not in {
            "requirement",
            "risk",
            "question",
            "assumption",
            "observation",
            "exclusion",
        }:
            raise ValueError("Choose a recognised finding type.")
        identifier, stamp = new_id(), now()
        with self.db.connect(write=True) as conn:
            self._check_sources(conn, tender_id, source_ids)
            conn.execute(
                "INSERT INTO findings(id,tender_id,title,detail,kind,source_ids_json,origin,run_id,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)",
                (
                    identifier,
                    tender_id,
                    title,
                    detail,
                    kind,
                    dump(source_ids),
                    origin,
                    run_id,
                    stamp,
                    stamp,
                ),
            )
            return record(
                conn.execute("SELECT * FROM findings WHERE id=?", (identifier,)).fetchone()
            )

    def decide_finding(self, tender_id, finding_id, decision, rationale):
        rationale = text(rationale, "a reason for the decision", 4000)
        states = {"accept": "accepted", "reject": "rejected", "resolve": "resolved"}
        if decision not in states:
            raise ValueError("Choose accept, reject or resolve.")
        with self.db.connect(write=True) as conn:
            finding = record(
                conn.execute(
                    "SELECT * FROM findings WHERE tender_id=? AND id=?", (tender_id, finding_id)
                ).fetchone()
            )
            if finding["is_stale"] and decision != "reject":
                raise ValueError(
                    "The supporting source has changed. Review and replace this finding first."
                )
            stamp = now()
            conn.execute(
                "UPDATE findings SET state=?,updated_at=? WHERE id=?",
                (states[decision], stamp, finding_id),
            )
            conn.execute(
                "INSERT INTO decisions VALUES(?,?,?,?,?,?,?)",
                (new_id(), tender_id, "finding", finding_id, decision, rationale, stamp),
            )
            return record(
                conn.execute("SELECT * FROM findings WHERE id=?", (finding_id,)).fetchone()
            )

    @staticmethod
    def _message_cursor(message_id: str) -> str:
        return (
            base64.urlsafe_b64encode(f"messages-before:{message_id}".encode()).decode().rstrip("=")
        )

    @staticmethod
    def _message_anchor(cursor: str | None) -> str | None:
        if not cursor:
            return None
        if not isinstance(cursor, str) or len(cursor) > 100:
            raise ValueError("The message history cursor is invalid.")
        try:
            padded = cursor + "=" * (-len(cursor) % 4)
            decoded = base64.urlsafe_b64decode(padded.encode()).decode()
            prefix, value = decoded.split(":", 1)
            if prefix != "messages-before" or not re.fullmatch(r"[0-9a-f]{32}", value):
                raise ValueError
            return value
        except (ValueError, UnicodeDecodeError, base64.binascii.Error):
            raise ValueError("The message history cursor is invalid.") from None

    def _message_result_links(self, conn, tender_id: str, run_id: str | None) -> list[dict]:
        """Build result cards from records persisted by this exact run."""

        if not run_id:
            return []
        run = conn.execute(
            "SELECT id FROM runs WHERE id=? AND tender_id=?", (run_id, tender_id)
        ).fetchone()
        if run is None:
            return []
        links = []
        for row in conn.execute(
            "SELECT id,title FROM findings WHERE tender_id=? AND run_id=? ORDER BY rowid",
            (tender_id, run_id),
        ):
            links.append(
                {
                    "kind": "finding",
                    "id": row["id"],
                    "title": row["title"],
                    "target": f"/tenders/{tender_id}/work?view=decisions&record={row['id']}",
                }
            )
        for row in conn.execute(
            "SELECT id,title FROM plans WHERE tender_id=? AND run_id=? ORDER BY rowid",
            (tender_id, run_id),
        ):
            links.append(
                {
                    "kind": "plan",
                    "id": row["id"],
                    "title": row["title"],
                    "target": f"/tenders/{tender_id}/work?view=plan&record={row['id']}",
                }
            )
        for task in conn.execute(
            "SELECT id,title FROM tasks WHERE tender_id=? AND run_id=? ORDER BY rowid",
            (tender_id, run_id),
        ):
            links.append(
                {
                    "kind": "task",
                    "id": task["id"],
                    "title": task["title"],
                    "target": f"/tenders/{tender_id}/work?view=tasks&record={task['id']}",
                }
            )
        if conn.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='generated_outputs'"
        ).fetchone():
            for row in conn.execute(
                "SELECT record_json FROM generated_outputs WHERE tender_id=? ORDER BY rowid",
                (tender_id,),
            ):
                try:
                    output = json.loads(row["record_json"])
                except (TypeError, ValueError):
                    continue
                if output.get("metadata", {}).get("run_id") != run_id:
                    continue
                output_id = output.get("id")
                if not isinstance(output_id, str):
                    continue
                links.append(
                    {
                        "kind": "output",
                        "id": output_id,
                        "title": output.get("kind", "Prepared document"),
                        "target": f"/tenders/{tender_id}/submission?view=documents&record={output_id}",
                    }
                )
        from .result_links import saved_work_links

        return links + saved_work_links(conn, tender_id, run_id)

    def _message_record(self, conn, tender_id: str, row) -> dict:
        item = record(row)
        item["result_links"] = self._message_result_links(conn, tender_id, item.get("run_id"))
        return item

    def messages(self, tender_id):
        """Return the complete history for internal workers and legacy callers."""

        self.get_tender(tender_id)
        with self.db.connect() as conn:
            return [
                self._message_record(conn, tender_id, row)
                for row in conn.execute(
                    "SELECT * FROM messages WHERE tender_id=? ORDER BY rowid", (tender_id,)
                )
            ]

    def message_page(self, tender_id, *, limit=50, cursor=None):
        self.get_tender(tender_id)
        try:
            limit = int(limit)
        except (TypeError, ValueError):
            raise ValueError("Message history limit is invalid.") from None
        if not 1 <= limit <= 100:
            raise ValueError("Message history limit must be between 1 and 100.")
        anchor = self._message_anchor(cursor)
        with self.db.connect() as conn:
            before = None
            if anchor:
                boundary = conn.execute(
                    "SELECT rowid FROM messages WHERE id=? AND tender_id=? AND role IN ('engineer','manager')",
                    (anchor, tender_id),
                ).fetchone()
                if boundary is None:
                    raise ValueError("The message history cursor is invalid for this Tender.")
                before = boundary[0]
            rows = conn.execute(
                "SELECT * FROM messages WHERE tender_id=? AND role IN ('engineer','manager') "
                "AND (? IS NULL OR rowid<?) ORDER BY rowid DESC LIMIT ?",
                (tender_id, before, before, limit + 1),
            ).fetchall()
            selected = rows[:limit]
            # Initial loads show current dialogue; continuation walks strictly
            # before its oldest returned message. Concurrent appends cannot
            # move the older-page boundary as they would with an offset.
            items = [self._message_record(conn, tender_id, row) for row in reversed(selected)]
            next_cursor = self._message_cursor(selected[-1]["id"]) if len(rows) > limit else None
        return {"items": items, "next_cursor": next_cursor}

    def add_message(self, tender_id, role, content, source_ids=None, run_id=None):
        self.get_tender(tender_id)
        if role not in {"engineer", "manager", "system"}:
            raise ValueError("Unknown message author.")
        content = text(content, "a message", 100000)
        identifier = new_id()
        with self.db.connect(write=True) as conn:
            self._check_sources(conn, tender_id, source_ids or [])
            conn.execute(
                "INSERT INTO messages VALUES(?,?,?,?,?,?,?)",
                (identifier, tender_id, role, content, dump(source_ids or []), run_id, now()),
            )
            return record(
                conn.execute("SELECT * FROM messages WHERE id=?", (identifier,)).fetchone()
            )

    def create_plan(self, tender_id, title, tasks, run_id=None):
        self.get_tender(tender_id)
        title = text(title, "a plan title", 300)
        if not tasks or len(tasks) > 32:
            raise ValueError("A work plan needs between 1 and 32 tasks.")
        prepared = []
        for task in tasks:
            prepared.append(
                {
                    "title": text(task["title"], "a task title", 300),
                    "description": text(task["description"], "the task scope", 10000),
                    "role": text(task["role"], "a specialist role", 100),
                    "source_ids": task.get("source_ids", []),
                }
            )
        identifier, stamp = new_id(), now()
        with self.db.connect(write=True) as conn:
            for task in prepared:
                self._check_sources(conn, tender_id, task["source_ids"])
            version = conn.execute(
                "SELECT COALESCE(MAX(version),0)+1 FROM plans WHERE tender_id=?", (tender_id,)
            ).fetchone()[0]
            conn.execute(
                "UPDATE tasks SET status='superseded',updated_at=? WHERE plan_id IN (SELECT id FROM plans WHERE tender_id=? AND status='proposed')",
                (stamp, tender_id),
            )
            conn.execute(
                "UPDATE plans SET status='superseded',updated_at=? WHERE tender_id=? AND status='proposed'",
                (stamp, tender_id),
            )
            conn.execute(
                "INSERT INTO plans VALUES(?,?,?,?,?,?,?,?)",
                (identifier, tender_id, title, version, "proposed", run_id, stamp, stamp),
            )
            conn.execute(
                "INSERT INTO plan_basis(plan_id,tender_id,revision) SELECT ?,id,revision FROM tenders WHERE id=?",
                (identifier, tender_id),
            )
            for task in prepared:
                conn.execute(
                    "INSERT INTO tasks(id,tender_id,plan_id,title,description,role,status,source_ids_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)",
                    (
                        new_id(),
                        tender_id,
                        identifier,
                        task["title"],
                        task["description"],
                        task["role"],
                        "awaiting_approval",
                        dump(task["source_ids"]),
                        stamp,
                        stamp,
                    ),
                )
        return self.get_plan(tender_id, identifier)

    def get_plan(self, tender_id, plan_id):
        with self.db.connect() as conn:
            plan = record(
                conn.execute(
                    "SELECT * FROM plans WHERE tender_id=? AND id=?", (tender_id, plan_id)
                ).fetchone()
            )
            plan["tasks"] = [
                record(r)
                for r in conn.execute(
                    "SELECT * FROM tasks WHERE plan_id=? ORDER BY rowid", (plan_id,)
                )
            ]
            return plan

    def list_plans(self, tender_id):
        self.get_tender(tender_id)
        with self.db.connect() as conn:
            ids = [
                r[0]
                for r in conn.execute(
                    "SELECT id FROM plans WHERE tender_id=? ORDER BY version DESC", (tender_id,)
                )
            ]
        return [self.get_plan(tender_id, plan_id) for plan_id in ids]

    def approve_plan(self, tender_id, plan_id, rationale):
        rationale = text(rationale, "a reason for approving the plan", 4000)
        with self.db.connect(write=True) as conn:
            plan = record(
                conn.execute(
                    "SELECT * FROM plans WHERE tender_id=? AND id=?", (tender_id, plan_id)
                ).fetchone()
            )
            if plan["status"] != "proposed":
                raise ValueError("Only the current proposed plan can be approved.")
            basis = conn.execute(
                "SELECT b.revision,t.revision FROM plan_basis b JOIN tenders t ON t.id=b.tender_id WHERE b.plan_id=? AND b.tender_id=?",
                (plan_id, tender_id),
            ).fetchone()
            if basis is None or basis[0] != basis[1]:
                raise ValueError(
                    "The source package has changed since this plan was proposed. Request a revised plan."
                )
            for row in conn.execute(
                "SELECT source_ids_json FROM tasks WHERE plan_id=?", (plan_id,)
            ):
                self._check_sources(conn, tender_id, record(row)["source_ids"])
            if conn.execute(
                "SELECT 1 FROM tasks WHERE tender_id=? AND status='running'", (tender_id,)
            ).fetchone():
                raise ValueError(
                    "Stop or finish the current work before approving a replacement plan."
                )
            stamp = now()
            conn.execute(
                "UPDATE tasks SET status='superseded',updated_at=? WHERE tender_id=? AND status IN ('ready','awaiting_approval','needs_review') AND plan_id<>?",
                (stamp, tender_id, plan_id),
            )
            conn.execute(
                "UPDATE plans SET status='superseded',updated_at=? WHERE tender_id=? AND status='approved'",
                (stamp, tender_id),
            )
            conn.execute(
                "UPDATE plans SET status='approved',updated_at=? WHERE id=?", (stamp, plan_id)
            )
            conn.execute(
                "UPDATE tasks SET status='ready',updated_at=? WHERE plan_id=?", (stamp, plan_id)
            )
            conn.execute(
                "UPDATE tenders SET status='active',updated_at=? WHERE id=?", (stamp, tender_id)
            )
            conn.execute(
                "INSERT INTO decisions VALUES(?,?,?,?,?,?,?)",
                (new_id(), tender_id, "plan", plan_id, "approve", rationale, stamp),
            )
        return self.get_plan(tender_id, plan_id)

    def approved_scope(self, tender_id, plan_id):
        """Return binding engineer approval conditions for this Tender's approved plan."""
        with self.db.connect() as conn:
            plan = record(
                conn.execute(
                    "SELECT * FROM plans WHERE tender_id=? AND id=?", (tender_id, plan_id)
                ).fetchone()
            )
            if plan["status"] != "approved":
                raise ValueError("This work requires an approved current plan.")
            row = conn.execute(
                "SELECT d.rationale,b.revision FROM decisions d JOIN plan_basis b ON b.plan_id=d.target_id AND b.tender_id=d.tender_id WHERE d.tender_id=? AND d.target_type='plan' AND d.target_id=? AND d.decision='approve' ORDER BY d.rowid DESC LIMIT 1",
                (tender_id, plan_id),
            ).fetchone()
            if row is None:
                raise ValueError(
                    "The saved plan approval is incomplete. Request and approve a new plan."
                )
            return {
                "plan_id": plan_id,
                "title": plan["title"],
                "tender_revision": row["revision"],
                "rationale": row["rationale"],
            }

    def list_tasks(self, tender_id):
        self.get_tender(tender_id)
        with self.db.connect() as conn:
            return [
                record(r)
                for r in conn.execute(
                    "SELECT * FROM tasks WHERE tender_id=? ORDER BY rowid", (tender_id,)
                )
            ]

    def get_task(self, tender_id, task_id):
        with self.db.connect() as conn:
            return record(
                conn.execute(
                    "SELECT * FROM tasks WHERE tender_id=? AND id=?", (tender_id, task_id)
                ).fetchone()
            )

    def update_task(self, tender_id, task_id, *, status, run_id=None, result=None):
        with self.db.connect(write=True) as conn:
            task = record(
                conn.execute(
                    "SELECT * FROM tasks WHERE tender_id=? AND id=?", (tender_id, task_id)
                ).fetchone()
            )
            plan = conn.execute(
                "SELECT status FROM plans WHERE id=?", (task["plan_id"],)
            ).fetchone()[0]
            if status == "running" and (
                plan != "approved"
                or task["status"] not in {"ready", "failed", "cancelled", "interrupted"}
            ):
                raise ValueError(
                    "This task needs an approved current work plan before it can start."
                )
            if status in {"running", "completed"}:
                self.approved_scope(tender_id, task["plan_id"])
                dependencies = set(task["source_ids"])
                saved_result = result if result is not None else task["result"]
                dependencies.update(saved_result.get("source_ids", []))
                self._check_sources(conn, tender_id, list(dependencies))
            conn.execute(
                "UPDATE tasks SET status=?,run_id=?,result_json=?,updated_at=? WHERE id=?",
                (
                    status,
                    run_id or task["run_id"],
                    dump(result if result is not None else task["result"]),
                    now(),
                    task_id,
                ),
            )

    def create_run(self, tender_id, kind, instruction=""):
        self.get_tender(tender_id)
        if kind not in {"import", "manager", "index", "identify", "analysis"}:
            raise ValueError("Unknown work type.")
        identifier, stamp = new_id(), now()
        with self.db.connect(write=True) as conn:
            conn.execute(
                "INSERT INTO runs(id,tender_id,kind,instruction,status,created_at,updated_at) VALUES(?,?,?,?,?,?,?)",
                (identifier, tender_id, kind, instruction, "queued", stamp, stamp),
            )
        return self.get_run(identifier)

    def get_run(self, run_id):
        with self.db.connect() as conn:
            return record(conn.execute("SELECT * FROM runs WHERE id=?", (run_id,)).fetchone())

    def list_runs(self, tender_id):
        self.get_tender(tender_id)
        with self.db.connect() as conn:
            return [
                record(r)
                for r in conn.execute(
                    "SELECT * FROM runs WHERE tender_id=? ORDER BY created_at DESC,rowid DESC LIMIT 200",
                    (tender_id,),
                )
            ]

    def update_run(self, run_id, **fields):
        allowed = {"status", "progress", "detail", "result", "usage", "error"}
        if not fields or not fields.keys() <= allowed:
            raise ValueError("Unknown work update.")
        if "status" in fields and fields["status"] not in {
            "queued",
            "running",
            "completed",
            "failed",
            "cancelled",
            "interrupted",
        }:
            raise ValueError("Unknown work status.")
        if "progress" in fields and not 0 <= fields["progress"] <= 100:
            raise ValueError("Work progress must be between 0 and 100.")
        updates = {
            key + "_json" if key in {"result", "usage"} else key: dump(value)
            if key in {"result", "usage"}
            else value
            for key, value in fields.items()
        }
        updates["updated_at"] = now()
        with self.db.connect(write=True) as conn:
            old = record(conn.execute("SELECT * FROM runs WHERE id=?", (run_id,)).fetchone())
            if (
                old["status"] in {"completed", "failed", "cancelled", "interrupted"}
                and "status" in fields
                and fields["status"] != old["status"]
            ):
                raise ValueError("Finished work cannot be restarted in place. Create a new run.")
            columns = ",".join(f"{key}=?" for key in updates)
            conn.execute(f"UPDATE runs SET {columns} WHERE id=?", (*updates.values(), run_id))

    def update_active_run_detail(self, run_id, detail):
        """Project trusted live activity without touching a terminal run."""
        if not isinstance(detail, str) or not detail.strip() or len(detail) > 200:
            raise ValueError("Work activity must be a short nonblank description.")
        with self.db.connect(write=True) as conn:
            row = conn.execute("SELECT status FROM runs WHERE id=?", (run_id,)).fetchone()
            if row is None:
                raise KeyError("This item could not be found in the selected Tender.")
            if row["status"] not in {"queued", "running"}:
                return False
            conn.execute(
                "UPDATE runs SET detail=?,updated_at=? WHERE id=? AND status IN ('queued','running')",
                (detail, now(), run_id),
            )
            return True

    def event(self, run_id, kind, message, data=None):
        self.get_run(run_id)
        with self.db.connect(write=True) as conn:
            conn.execute(
                "INSERT INTO run_events(run_id,kind,message,data_json,created_at) VALUES(?,?,?,?,?)",
                (run_id, kind, message, dump(data or {}), now()),
            )

    def run_events(self, run_id):
        self.get_run(run_id)
        with self.db.connect() as conn:
            return [
                record(r)
                for r in conn.execute(
                    "SELECT * FROM run_events WHERE run_id=? ORDER BY id", (run_id,)
                )
            ]

    def recover_interrupted_runs(self):
        with self.db.connect(write=True) as conn:
            conn.execute(
                "UPDATE runs SET status='interrupted',detail='Quantix closed before this work finished. Review and resume when ready.',updated_at=? WHERE status IN ('running','queued')",
                (now(),),
            )
            conn.execute(
                "UPDATE tasks SET status='interrupted',updated_at=? WHERE status='running'",
                (now(),),
            )
            from .run_activity import ActivityRecordingError, settle_open_operations

            for row in conn.execute(
                "SELECT DISTINCT r.id FROM runs r JOIN run_activity_operations o ON o.run_id=r.id WHERE r.status IN ('failed','cancelled','interrupted')"
            ).fetchall():
                try:
                    settle_open_operations(
                        self,
                        row[0],
                        "interrupted",
                        "Quantix closed before this step had a confirmed completion.",
                    )
                except ActivityRecordingError:
                    pass  # Recovery must still revoke abandoned work authority.
        # Any pending instruction that survived a process restart must require
        # an explicit engineer confirmation.  This also covers a crash after
        # the prior run committed but before its follow-up could be scheduled.
        from .pending import PendingInstructionService

        PendingInstructionService(self).hold_for_interrupted_runs()

    def overview(self, tender_id):
        tender = self.get_tender(tender_id)
        artifacts = self.list_artifacts(tender_id)
        coverage = {
            "registered": len(artifacts),
            "extracted": 0,
            "needs_attention": 0,
            "unsupported": 0,
            "failed": 0,
        }
        for artifact in artifacts:
            if artifact["status"] in coverage and artifact["status"] != "registered":
                coverage[artifact["status"]] += 1
        with self.db.connect() as conn:
            evidence_count = conn.execute(
                "SELECT COUNT(*) FROM evidence e JOIN artifacts a ON a.id=e.artifact_id WHERE a.tender_id=? AND a.is_current=1 AND COALESCE(e.is_current,1)=1",
                (tender_id,),
            ).fetchone()[0]
            has_boq = conn.execute(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='boq_items'"
            ).fetchone()
            boq_count = (
                conn.execute(
                    "SELECT COUNT(*) FROM boq_items WHERE tender_id=? AND active=1", (tender_id,)
                ).fetchone()[0]
                if has_boq
                else 0
            )
        plans = self.list_plans(tender_id)
        return {
            "tender": tender,
            "artifact_count": len(artifacts),
            "evidence_count": evidence_count,
            "coverage": coverage,
            "areas": sorted({a["area"] for a in artifacts}),
            "findings": self.list_findings(tender_id),
            "plan": plans[0] if plans else None,
            "active_runs": [
                r for r in self.list_runs(tender_id) if r["status"] in {"queued", "running"}
            ],
            "boq_count": boq_count,
        }

    def setting(self, key, default=None):
        with self.db.connect() as conn:
            row = conn.execute("SELECT value_json FROM settings WHERE key=?", (key,)).fetchone()
            return record(row)["value"] if row else default

    def set_setting(self, key, value):
        with self.db.connect(write=True) as conn:
            conn.execute(
                "INSERT INTO settings VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",
                (key, dump(value)),
            )
