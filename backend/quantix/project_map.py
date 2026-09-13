"""Durable project structure with source-linked, scoped engineer review history."""

import hashlib

from .db import dump, new_id, now, record
from .map_models import NodeDecision, NodeInput, ReviewInput


def evidence_hash(evidence):
    return hashlib.sha256(dump({key: evidence.get(key) for key in (
        "id", "artifact_id", "locator", "text", "page", "sheet", "cell_range", "kind", "metadata",
    )}).encode()).hexdigest()


class ProjectMapService:
    def __init__(self, repo):
        self.repo = repo
        with repo.db.connect(write=True) as conn:
            for sql in (
                "CREATE TABLE IF NOT EXISTS project_nodes(id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),payload_json TEXT NOT NULL,created_at TEXT NOT NULL)",
                "CREATE TABLE IF NOT EXISTS project_node_decisions(id TEXT PRIMARY KEY,node_id TEXT NOT NULL REFERENCES project_nodes(id),decision TEXT NOT NULL,rationale TEXT NOT NULL,created_at TEXT NOT NULL)",
                "CREATE TABLE IF NOT EXISTS project_reviews(id TEXT PRIMARY KEY,tender_id TEXT NOT NULL REFERENCES tenders(id),payload_json TEXT NOT NULL,created_at TEXT NOT NULL)",
                "CREATE INDEX IF NOT EXISTS project_nodes_tender ON project_nodes(tender_id)",
                "CREATE INDEX IF NOT EXISTS project_reviews_tender ON project_reviews(tender_id)",
            ):
                conn.execute(sql)
            for table in ("project_nodes", "project_node_decisions", "project_reviews"):
                for action in ("UPDATE", "DELETE"):
                    conn.execute(f"CREATE TRIGGER IF NOT EXISTS {table}_no_{action.lower()} BEFORE {action} ON {table} BEGIN SELECT RAISE(ABORT,'Project structure and review history are immutable'); END")

    def _original_current(self, tender_id, artifact, cache):
        key = artifact["content_hash"]
        if key not in cache:
            try:
                path = self.repo.object_path(tender_id, artifact["id"])
                with path.open("rb") as stream:
                    cache[key] = hashlib.file_digest(stream, "sha256").hexdigest() == key
            except (OSError, ValueError):
                cache[key] = False
        return cache[key]

    def _capture(self, tender_id, source_id, cache):
        source = self.repo.get_evidence(tender_id, source_id)
        artifact = self.repo.get_artifact(tender_id, source["artifact_id"])
        if not artifact["is_current"] or not self._original_current(tender_id, artifact, cache):
            raise ValueError("Use an available current source for the project map.")
        return {
            "source_id": source_id, "artifact_id": artifact["id"],
            "relative_path": artifact["relative_path"], "locator": source["locator"],
            "version": artifact["version"], "content_hash": artifact["content_hash"],
            "evidence_hash": evidence_hash(source),
        }

    def _source_reasons(self, tender_id, sources, cache):
        reasons = []
        for saved in sources:
            try:
                current = self._capture(tender_id, saved["source_id"], cache)
                if current != saved:
                    reasons.append("Supporting source content or version changed.")
            except (ValueError, KeyError):
                reasons.append("A supporting source is superseded, missing or changed.")
        return list(dict.fromkeys(reasons))

    def propose(self, tender_id, values, *, origin="engineer", run_id=None):
        request = NodeInput.model_validate(values)
        if origin not in {"engineer", "agent"}:
            raise ValueError("Choose a supported project-map author.")
        with self.repo.atomic() as conn:
            self.repo.get_tender(tender_id)
            if run_id and self.repo.get_run(run_id)["tender_id"] != tender_id:
                raise ValueError("The proposing run belongs to another Tender.")
            if request.parent_id:
                parent = self.get(tender_id, request.parent_id)
                if not parent["is_current"] or parent["state"] == "withdrawn":
                    raise ValueError("Choose a current project-map parent.")
            identifier, stamp, cache = new_id(), now(), {}
            source_ids = list(dict.fromkeys(request.source_ids))
            payload = request.model_dump() | {
                "source_ids": source_ids, "origin": origin, "run_id": run_id,
                "source_manifest": [self._capture(tender_id, sid, cache) for sid in source_ids],
            }
            conn.execute("INSERT INTO project_nodes VALUES(?,?,?,?)", (identifier, tender_id, dump(payload), stamp))
            return self.get(tender_id, identifier)

    def _records(self, tender_id):
        self.repo.get_tender(tender_id)
        with self.repo.db.connect() as conn:
            nodes = [record(row) for row in conn.execute("SELECT * FROM project_nodes WHERE tender_id=? ORDER BY created_at,id", (tender_id,))]
            decisions = [dict(row) for row in conn.execute("SELECT d.* FROM project_node_decisions d JOIN project_nodes n ON n.id=d.node_id WHERE n.tender_id=? ORDER BY d.rowid", (tender_id,))]
        by_id, cache = {}, {}
        findings = self.repo.list_findings(tender_id)
        with self.repo.db.connect() as conn:
            has_boq = conn.execute("SELECT 1 FROM sqlite_master WHERE name='boq_items' AND type='table'").fetchone()
            boq = list(conn.execute("SELECT id,source_id FROM boq_items WHERE tender_id=? AND active=1", (tender_id,))) if has_boq else []
        for saved in nodes:
            data = saved["payload"]
            audit = [{k: d[k] for k in ("id", "decision", "rationale", "created_at")} for d in decisions if d["node_id"] == saved["id"]]
            state = {"approve": "approved", "withdraw": "withdrawn"}.get(audit[-1]["decision"], "proposed") if audit else "proposed"
            source_ids = set(data["source_ids"])
            reasons = self._source_reasons(tender_id, data["source_manifest"], cache)
            by_id[saved["id"]] = data | {
                "id": saved["id"], "tender_id": tender_id, "created_at": saved["created_at"],
                "state": state, "decisions": audit, "stale_reasons": reasons,
                "related_artifact_ids": list(dict.fromkeys(s["artifact_id"] for s in data["source_manifest"])),
                "related_finding_ids": [f["id"] for f in findings if source_ids.intersection(f["source_ids"])],
                "related_boq_item_ids": [b["id"] for b in boq if b["source_id"] in source_ids],
            }
        for node in by_id.values():
            parent_id, visited = node["parent_id"], {node["id"]}
            while parent_id:
                if parent_id in visited or parent_id not in by_id:
                    node["stale_reasons"].append("The parent structure is unavailable or cyclic.")
                    break
                visited.add(parent_id)
                parent = by_id[parent_id]
                if parent["stale_reasons"] or parent["state"] == "withdrawn":
                    node["stale_reasons"].append("A parent needs review or was withdrawn.")
                    break
                parent_id = parent["parent_id"]
            node["is_current"] = not node["stale_reasons"]
            node["approval_valid"] = node["is_current"] and node["state"] == "approved"
        return list(by_id.values())

    def get(self, tender_id, node_id):
        result = next((row for row in self._records(tender_id) if row["id"] == node_id), None)
        if result is None:
            raise KeyError("This project-map item is not in the selected Tender.")
        return result

    def decide(self, tender_id, node_id, values):
        request = NodeDecision.model_validate(values)
        with self.repo.atomic() as conn:
            node = self.get(tender_id, node_id)
            if request.decision == "approve" and (not node["is_current"] or node["state"] != "proposed"):
                raise ValueError("Only a current proposed map item can be approved. Create a new source-backed proposal for revised content.")
            if request.decision == "withdraw" and node["state"] == "withdrawn":
                raise ValueError("This map item is already withdrawn.")
            identifier, stamp = new_id(), now()
            conn.execute("INSERT INTO project_node_decisions VALUES(?,?,?,?,?)", (identifier, node_id, request.decision, request.rationale, stamp))
            conn.execute("INSERT INTO decisions VALUES(?,?,?,?,?,?,?)", (new_id(), tender_id, "project_node", node_id, request.decision, request.rationale, stamp))
            return self.get(tender_id, node_id)

    def review(self, tender_id, values):
        request = ReviewInput.model_validate(values)
        with self.repo.atomic() as conn:
            artifact = self.repo.get_artifact(tender_id, request.artifact_id)
            if not artifact["is_current"] or not self._original_current(tender_id, artifact, {}):
                raise ValueError("Review the current available original document.")
            source_ids, fingerprint = [], None
            if request.scope_type == "page":
                from .measurements import MeasurementService
                MeasurementService(self.repo).page(tender_id, artifact["id"], request.page)
                source_ids = [row[0] for row in conn.execute("SELECT id FROM evidence WHERE artifact_id=? AND page=?", (artifact["id"], request.page))]
            elif request.scope_type == "locator":
                sources = [record(row) for row in conn.execute("SELECT * FROM evidence WHERE artifact_id=? AND locator=?", (artifact["id"], request.locator))]
                if len(sources) != 1:
                    raise ValueError("Choose one exact source passage from this document.")
                source_ids = [sources[0]["id"]]
                fingerprint = evidence_hash(self.repo.get_evidence(tender_id, source_ids[0]))
            identifier, stamp = new_id(), now()
            data = request.model_dump() | {
                "relative_path": artifact["relative_path"], "version": artifact["version"],
                "content_hash": artifact["content_hash"], "evidence_hash": fingerprint, "source_ids": source_ids,
            }
            conn.execute("INSERT INTO project_reviews VALUES(?,?,?,?)", (identifier, tender_id, dump(data), stamp))
            conn.execute("INSERT INTO decisions VALUES(?,?,?,?,?,?,?)", (new_id(), tender_id, "source_review", identifier, "review", request.rationale, stamp))
            return data | {"id": identifier, "tender_id": tender_id, "created_at": stamp, "is_current": True, "stale_reasons": []}

    def view(self, tender_id):
        nodes = self._records(tender_id)
        artifacts = self.repo.list_artifacts(tender_id)
        by_id = {a["id"]: a for a in artifacts}
        reviews, cache = [], {}
        with self.repo.db.connect() as conn:
            saved_reviews = [record(row) for row in conn.execute("SELECT * FROM project_reviews WHERE tender_id=? ORDER BY created_at DESC,id", (tender_id,))]
            extracted = conn.execute("SELECT COUNT(*) FROM evidence e JOIN artifacts a ON a.id=e.artifact_id WHERE a.tender_id=? AND a.is_current=1 AND e.kind!='measurement' AND length(e.text)>0", (tender_id,)).fetchone()[0]
        for row in saved_reviews:
            data, reasons = row["payload"], []
            artifact = by_id.get(data["artifact_id"])
            if artifact is None or artifact["version"] != data["version"] or artifact["content_hash"] != data["content_hash"]:
                reasons.append("The reviewed document has a newer revision.")
            elif not self._original_current(tender_id, artifact, cache):
                reasons.append("The reviewed original is unavailable or changed.")
            if data["evidence_hash"]:
                try:
                    if evidence_hash(self.repo.get_evidence(tender_id, data["source_ids"][0])) != data["evidence_hash"]:
                        reasons.append("The reviewed passage changed.")
                except KeyError:
                    reasons.append("The reviewed passage is unavailable.")
            reviews.append(data | {"id": row["id"], "tender_id": tender_id, "created_at": row["created_at"], "is_current": not reasons, "stale_reasons": reasons})
        cited = set()
        for finding in self.repo.list_findings(tender_id):
            for sid in finding["source_ids"]:
                try:
                    if self.repo.get_evidence(tender_id, sid)["artifact_id"] in by_id:
                        cited.add(sid)
                except KeyError:
                    pass
        return {
            "nodes": nodes, "review_scopes": reviews,
            "directory_areas": sorted(set(a["area"] for a in artifacts if a["area"])),
            "coverage": {
                "registered_files": len(artifacts), "extracted_files": sum(a["status"] == "extracted" for a in artifacts),
                "extracted_evidence": extracted, "evidence_cited_in_findings": len(cited),
                "current_review_scopes": sum(r["is_current"] for r in reviews),
                "reviewed_artifacts_in_full": len({r["artifact_id"] for r in reviews if r["is_current"] and r["scope_type"] == "artifact"}),
                "note": "Cited passages indicate recorded analysis, not complete analysis. Review coverage records only the exact scopes explicitly checked by the engineer.",
            },
        }
