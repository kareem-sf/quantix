"""Durable Tender records and the narrow access surface used by the office."""

import re
from pathlib import Path, PurePosixPath

from .db import Database, dump, new_id, now, record


def text(value: str, label: str, limit: int = 20000) -> str:
    if not isinstance(value, str) or not value.strip() or len(value) > limit:
        raise ValueError(f"Enter {label} (up to {limit} characters).")
    return value.strip()


class Repository:
    def __init__(self, home: Path):
        self.db = Database(home)
        self.home = self.db.home
        self.objects = self.home / "objects"
        self.objects.mkdir(exist_ok=True)
        (self.home / "extractions").mkdir(exist_ok=True)

    def atomic(self):
        """Group validated synchronous domain operations in one SQLite transaction."""
        return self.db.connect(write=True)

    def list_tenders(self):
        with self.db.connect() as conn:
            return [
                record(r) for r in conn.execute("SELECT * FROM tenders ORDER BY updated_at DESC,id")
            ]

    def create_tender(self, name: str):
        name = text(name, "a Tender name", 200)
        identifier, stamp = new_id(), now()
        with self.db.connect(write=True) as conn:
            conn.execute(
                "INSERT INTO tenders(id,name,created_at,updated_at) VALUES(?,?,?,?)",
                (identifier, name, stamp, stamp),
            )
        return self.get_tender(identifier)

    def get_tender(self, tender_id):
        with self.db.connect() as conn:
            return record(conn.execute("SELECT * FROM tenders WHERE id=?", (tender_id,)).fetchone())

    def list_artifacts(self, tender_id, *, current_only=True):
        self.get_tender(tender_id)
        with self.db.connect() as conn:
            query = "SELECT * FROM artifacts WHERE tender_id=?"
            if current_only:
                query += " AND is_current=1"
            query += " ORDER BY relative_path,version DESC"
            return [record(r) for r in conn.execute(query, (tender_id,))]

    def get_artifact(self, tender_id, artifact_id):
        with self.db.connect() as conn:
            return record(
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
                    "INSERT INTO evidence(id,artifact_id,locator,text,page,sheet,cell_range,kind,metadata_json) VALUES(?,?,?,?,?,?,?,?,?)",
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
                    ),
                )
            conn.execute(
                "UPDATE tenders SET revision=revision+1,updated_at=? WHERE id=?", (stamp, tender_id)
            )
        return self.get_artifact(tender_id, identifier), True

    def artifact_evidence(self, tender_id, artifact_id, offset=0, limit=50):
        self.get_artifact(tender_id, artifact_id)
        with self.db.connect() as conn:
            rows = conn.execute(
                """SELECT e.*,a.name AS artifact_name,a.relative_path FROM evidence e JOIN artifacts a ON a.id=e.artifact_id
                WHERE a.tender_id=? AND a.id=? ORDER BY e.rowid LIMIT ? OFFSET ?""",
                (tender_id, artifact_id, max(1, min(int(limit), 200)), max(0, int(offset))),
            )
            return [record(r) | {"score": 0.0} for r in rows]

    def get_evidence(self, tender_id, evidence_id):
        with self.db.connect() as conn:
            row = conn.execute(
                """SELECT e.*,a.name AS artifact_name,a.relative_path FROM evidence e JOIN artifacts a ON a.id=e.artifact_id
                WHERE a.tender_id=? AND e.id=?""",
                (tender_id, evidence_id),
            ).fetchone()
            return record(row) | {"score": 0.0}

    def search(self, tender_id, query, limit=20, *, area=None, status=None):
        self.get_tender(tender_id)
        terms = re.findall(r"[^\W_]+(?:[-.][^\W_]+)*", str(query), flags=re.UNICODE)[:16]
        if not terms:
            return []
        expression = " OR ".join('"' + word.replace('"', '""') + '"' for word in terms)
        with self.db.connect() as conn:
            rows = conn.execute(
                """SELECT e.*,a.name AS artifact_name,a.relative_path,a.content_hash,bm25(evidence_fts) AS score
                FROM evidence_fts JOIN evidence e ON e.id=evidence_fts.evidence_id JOIN artifacts a ON a.id=e.artifact_id
                WHERE evidence_fts MATCH ? AND a.tender_id=? AND a.is_current=1
                AND (? IS NULL OR a.area=?) AND (? IS NULL OR a.status=?) ORDER BY score LIMIT ?""",
                (
                    expression,
                    tender_id,
                    area,
                    area,
                    status,
                    status,
                    min(max(int(limit), 1), 100) * 8,
                ),
            ).fetchall()
        results, seen = [], set()
        for row in rows:
            item = record(row)
            identity = (item.pop("content_hash"), item["locator"])
            if identity not in seen:
                results.append(item)
                seen.add(identity)
            if len(results) >= min(max(int(limit), 1), 100):
                break
        return results

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
        if kind not in {"requirement", "risk", "question", "assumption", "observation", "exclusion"}:
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

    def messages(self, tender_id):
        self.get_tender(tender_id)
        with self.db.connect() as conn:
            return [
                record(r)
                for r in conn.execute(
                    "SELECT * FROM messages WHERE tender_id=? ORDER BY rowid", (tender_id,)
                )
            ]

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
        if kind not in {"import", "manager", "task", "research", "index"}:
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
                "SELECT COUNT(*) FROM evidence e JOIN artifacts a ON a.id=e.artifact_id WHERE a.tender_id=? AND a.is_current=1",
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
