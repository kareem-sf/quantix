"""Immutable approved notes with their own append-only engineer decision ledger."""

import hashlib
import json
import re
from datetime import UTC, date, datetime

from .db import dump, new_id, now, record
from .knowledge_models import KnowledgeCreate, KnowledgeDecision

SCHEMA = (
    """CREATE TABLE IF NOT EXISTS reusable_knowledge (
        id TEXT PRIMARY KEY, payload_json TEXT NOT NULL, created_at TEXT NOT NULL
    )""",
    """CREATE TABLE IF NOT EXISTS knowledge_audit (
        id TEXT PRIMARY KEY, knowledge_id TEXT NOT NULL REFERENCES reusable_knowledge(id),
        action TEXT NOT NULL CHECK(action IN ('approve','withdraw')),
        engineer_confirmed INTEGER NOT NULL CHECK(engineer_confirmed=1),
        rationale TEXT NOT NULL, created_at TEXT NOT NULL,
        UNIQUE(knowledge_id, action)
    )""",
    """CREATE TRIGGER IF NOT EXISTS knowledge_payload_no_update
        BEFORE UPDATE ON reusable_knowledge BEGIN
        SELECT RAISE(ABORT,'Approved reusable knowledge is immutable.'); END""",
    """CREATE TRIGGER IF NOT EXISTS knowledge_payload_no_delete
        BEFORE DELETE ON reusable_knowledge BEGIN
        SELECT RAISE(ABORT,'Approved reusable knowledge is immutable.'); END""",
    """CREATE TRIGGER IF NOT EXISTS knowledge_audit_no_update
        BEFORE UPDATE ON knowledge_audit BEGIN
        SELECT RAISE(ABORT,'Reusable knowledge decisions are immutable.'); END""",
    """CREATE TRIGGER IF NOT EXISTS knowledge_audit_no_delete
        BEFORE DELETE ON knowledge_audit BEGIN
        SELECT RAISE(ABORT,'Reusable knowledge decisions are immutable.'); END""",
)

USE_LIMITATIONS = (
    "Engineer-approved reusable note, not current Tender evidence. Approval permits reuse "
    "of the note but does not independently verify its content or establish that its sources "
    "support every statement. Check applicability in each Tender. Verification dates are "
    "engineer-entered. Price and tax information always needs fresh validation before "
    "commercial use."
)


def _evidence_hash(evidence):
    content = {
        key: evidence.get(key)
        for key in (
            "id",
            "artifact_id",
            "locator",
            "text",
            "page",
            "sheet",
            "cell_range",
            "kind",
            "metadata",
        )
    }
    canonical = json.dumps(content, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


class KnowledgeService:
    def __init__(self, repo):
        self.repo = repo
        with repo.db.connect(write=True) as conn:
            for statement in SCHEMA:
                conn.execute(statement)

    def _capture_source(self, tender_id, source_id):
        evidence = self.repo.get_evidence(tender_id, source_id)
        artifact = self.repo.get_artifact(tender_id, evidence["artifact_id"])
        if not artifact["is_current"]:
            raise ValueError("Approve reusable notes using current source revisions.")
        if not re.fullmatch(r"[a-f0-9]{64}", artifact["content_hash"]):
            raise ValueError("The supporting source has no valid saved content hash.")
        return {
            "source_id": source_id,
            "tender_id": tender_id,
            "artifact_id": artifact["id"],
            "artifact_name": artifact["name"],
            "relative_path": artifact["relative_path"],
            "locator": evidence["locator"],
            "content_hash": artifact["content_hash"],
            "version": artifact["version"],
            "evidence_hash": _evidence_hash(evidence),
        }

    def _audit(self, conn, knowledge_id, action, rationale, stamp):
        conn.execute(
            "INSERT INTO knowledge_audit(id,knowledge_id,action,engineer_confirmed,rationale,created_at) VALUES(?,?,?,1,?,?)",
            (new_id(), knowledge_id, action, rationale, stamp),
        )

    def create(self, values):
        request = KnowledgeCreate.model_validate(values)
        with self.repo.atomic() as conn:
            source_tender = (
                self.repo.get_tender(request.source_tender_id) if request.source_tender_id else None
            )
            sources = [
                self._capture_source(request.source_tender_id, source_id)
                for source_id in request.source_ids
            ]
            identifier, stamp = new_id(), now()
            payload = request.model_dump(mode="json", exclude={"engineer_confirmed", "rationale"})
            payload.update(
                sources=sources,
                source_tender_name=source_tender["name"] if source_tender else None,
            )
            conn.execute(
                "INSERT INTO reusable_knowledge(id,payload_json,created_at) VALUES(?,?,?)",
                (identifier, dump(payload), stamp),
            )
            self._audit(conn, identifier, "approve", request.rationale, stamp)
            return self.get(identifier)

    def _check_source(self, source):
        reasons = []
        try:
            evidence = self.repo.get_evidence(source["tender_id"], source["source_id"])
            artifact = self.repo.get_artifact(source["tender_id"], evidence["artifact_id"])
        except KeyError:
            return source | {"available": False, "is_current": False}, ["source_unavailable"]
        if (
            artifact["id"] != source["artifact_id"]
            or not artifact["is_current"]
            or artifact["content_hash"] != source["content_hash"]
            or artifact["version"] != source["version"]
        ):
            reasons.append("source_revision_changed")
        if _evidence_hash(evidence) != source["evidence_hash"]:
            reasons.append("source_evidence_changed")
        return source | {"available": True, "is_current": not reasons}, reasons

    def get(self, knowledge_id):
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT * FROM reusable_knowledge WHERE id=?", (knowledge_id,)
            ).fetchone()
            if row is None:
                raise KeyError("This approved reusable note could not be found.")
            saved = record(row)
            audit = [
                dict(event) | {"engineer_confirmed": bool(event["engineer_confirmed"])}
                for event in conn.execute(
                    "SELECT * FROM knowledge_audit WHERE knowledge_id=? ORDER BY rowid",
                    (knowledge_id,),
                )
            ]
            approval = next((event for event in audit if event["action"] == "approve"), None)
            if approval is None:
                raise ValueError("This reusable note has no recorded engineer approval.")
            withdrawal = next((event for event in audit if event["action"] == "withdraw"), None)
            payload = saved["payload"]
            sources, reasons = [], []
            for source in payload["sources"]:
                checked, source_reasons = self._check_source(source)
                sources.append(checked)
                reasons.extend(source_reasons)
            due = payload["recheck_after"]
            if due and date.fromisoformat(due) <= datetime.now(UTC).date():
                reasons.append("recheck_date_reached")
            commercial = payload["category"] in {"price", "tax"}
            if commercial:
                reasons.append("commercial_use_requires_fresh_validation")
            if withdrawal:
                reasons.append("withdrawn")
            return payload | {
                "id": saved["id"],
                "sources": sources,
                "status": "withdrawn" if withdrawal else "approved",
                "approved_at": approval["created_at"],
                "approval_rationale": approval["rationale"],
                "withdrawn_at": withdrawal["created_at"] if withdrawal else None,
                "withdrawal_rationale": withdrawal["rationale"] if withdrawal else None,
                "sources_current": all(source["is_current"] for source in sources)
                if sources
                else None,
                "needs_recheck": bool(reasons),
                "commercial_revalidation_required": commercial,
                "revalidation_reasons": list(dict.fromkeys(reasons)),
                "use_limitations": USE_LIMITATIONS,
                "audit": audit,
            }

    def list(self, *, include_withdrawn=False, category=None, offset=0, limit=100):
        if category not in {None, "preference", "method", "reference", "price", "tax"}:
            raise ValueError("Choose a supported reusable-note category.")
        if (
            not isinstance(limit, int)
            or not 1 <= limit <= 100
            or not isinstance(offset, int)
            or offset < 0
        ):
            raise ValueError("Request 1 to 100 reusable notes with a nonnegative offset.")
        conditions, parameters = [], []
        if not include_withdrawn:
            conditions.append(
                "NOT EXISTS (SELECT 1 FROM knowledge_audit a WHERE a.knowledge_id=k.id AND a.action='withdraw')"
            )
        if category:
            conditions.append("json_extract(k.payload_json,'$.category')=?")
            parameters.append(category)
        query = "SELECT k.id FROM reusable_knowledge k"
        if conditions:
            query += " WHERE " + " AND ".join(conditions)
        query += " ORDER BY k.rowid DESC LIMIT ? OFFSET ?"
        with self.repo.db.connect() as conn:
            ids = [row[0] for row in conn.execute(query, [*parameters, limit, offset])]
            return [self.get(identifier) for identifier in ids]

    def withdraw(self, knowledge_id, values):
        decision = KnowledgeDecision.model_validate(values)
        with self.repo.atomic() as conn:
            saved = self.get(knowledge_id)
            if saved["status"] == "withdrawn":
                raise ValueError("This reusable note has already been withdrawn.")
            self._audit(conn, knowledge_id, "withdraw", decision.rationale, now())
            return self.get(knowledge_id)
