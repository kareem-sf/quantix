"""Record an engineer's explicit Resume action without duplicating dialogue."""

import json

from .db import new_id, now
from .manager_runtime import ManagerRunProfiles


def _direct_origin(conn, tender_id, run, grant):
    if run["kind"] not in {"manager", "conversation"}:
        return False
    receipt = conn.execute(
        "SELECT work_intents_json FROM plan_review_approvals WHERE tender_id=? AND plan_id=? AND fingerprint=?",
        (tender_id, grant.plan_id, grant.review_fingerprint),
    ).fetchone()
    if receipt and any(
        item.get("run_id") == run["id"] and item.get("tender_id") == tender_id
        and item.get("plan_id") == grant.plan_id and item.get("kind") == run["kind"]
        for item in json.loads(receipt[0]) if isinstance(item, dict)
    ):
        return True
    return conn.execute(
        "SELECT 1 FROM messages WHERE tender_id=? AND run_id=? AND role='engineer' AND content=? AND created_at>?",
        (tender_id, run["id"], run["instruction"], grant.created_at),
    ).fetchone() is not None


def _receipt(conn, tender_id, run_id, grant):
    return conn.execute(
        """SELECT o.* FROM office_resume_origins o JOIN decisions d ON d.id=o.decision_id
        WHERE o.tender_id=? AND o.root_run_id=? AND o.grant_id=? AND o.plan_id=?
          AND d.tender_id=o.tender_id AND d.target_type='office_run'
          AND d.target_id=o.root_run_id AND d.decision='resume'""",
        (tender_id, run_id, grant.id, grant.plan_id),
    ).fetchone()


def valid_resume_origin(conn, tender_id, run, grant):
    """Read a committed action and its original explicit instruction basis."""
    if conn.execute("SELECT 1 FROM sqlite_master WHERE name='office_resume_origins' AND type='table'").fetchone() is None:
        return False
    saved = _receipt(conn, tender_id, run["id"], grant)
    if saved is None:
        return False
    original = conn.execute("SELECT * FROM runs WHERE tender_id=? AND id=?", (tender_id, saved["origin_run_id"])).fetchone()
    previous = conn.execute("SELECT * FROM runs WHERE tender_id=? AND id=?", (tender_id, saved["previous_run_id"])).fetchone()
    if (original is None or previous is None or previous["status"] not in {"failed", "cancelled", "interrupted"}
            or run["kind"] != previous["kind"] or run["kind"] != original["kind"]
            or run["instruction"] != previous["instruction"] or run["instruction"] != original["instruction"]
            or not _direct_origin(conn, tender_id, original, grant)):
        return False
    if previous["id"] != original["id"]:
        prior = _receipt(conn, tender_id, previous["id"], grant)
        if prior is None or prior["origin_run_id"] != original["id"]:
            return False
    return True


class OfficeResumeService:
    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            conn.execute("""CREATE TABLE IF NOT EXISTS office_resume_origins (
                root_run_id TEXT PRIMARY KEY REFERENCES runs(id),
                tender_id TEXT NOT NULL REFERENCES tenders(id),
                plan_id TEXT NOT NULL REFERENCES plans(id),
                grant_id TEXT NOT NULL REFERENCES office_delegation_grants(id),
                previous_run_id TEXT NOT NULL REFERENCES runs(id),
                origin_run_id TEXT NOT NULL REFERENCES runs(id),
                decision_id TEXT NOT NULL REFERENCES decisions(id),
                created_at TEXT NOT NULL)""")
            conn.execute("""CREATE TRIGGER IF NOT EXISTS office_resume_immutable_update
                BEFORE UPDATE ON office_resume_origins BEGIN SELECT RAISE(ABORT,'Resume actions are immutable'); END""")
            conn.execute("""CREATE TRIGGER IF NOT EXISTS office_resume_immutable_delete
                BEFORE DELETE ON office_resume_origins BEGIN SELECT RAISE(ABORT,'Resume actions are immutable'); END""")

    def record(self, previous, resumed, plan_id):
        """Called only by the engineer Resume admission, inside its queue transaction."""
        from .staff_routing import StaffRoutingService

        tender_id = previous["tender_id"]
        routing = StaffRoutingService(self.repo)
        with routing.policy.connections.authority_guard(), self.repo.atomic() as conn:
            grant = routing.approved_grant(tender_id, plan_id)
            actual_previous = self.repo.get_run(previous["id"])
            actual_resumed = self.repo.get_run(resumed["id"])
            if (actual_previous["status"] not in {"failed", "cancelled", "interrupted"}
                    or actual_resumed["status"] != "queued" or actual_resumed["tender_id"] != tender_id
                    or actual_previous["instruction"] != actual_resumed["instruction"]
                    or actual_previous["kind"] != actual_resumed["kind"]):
                raise ValueError("Resume requires the same stopped Tender instruction.")
            ManagerRunProfiles(self.repo).get(tender_id, actual_previous["id"])
            if _direct_origin(conn, tender_id, actual_previous, grant):
                origin_id = actual_previous["id"]
            elif valid_resume_origin(conn, tender_id, actual_previous, grant):
                origin_id = _receipt(conn, tender_id, actual_previous["id"], grant)["origin_run_id"]
            else:
                raise ValueError("This stopped run has no instruction under the current approved work. Send a new request to the Tender Manager.")
            decision_id, stamp = new_id(), now()
            conn.execute("INSERT INTO decisions VALUES(?,?,?,?,?,?,?)", (
                decision_id, tender_id, "office_run", actual_resumed["id"], "resume",
                "The engineer selected Resume for stopped work " + actual_previous["id"] + ". A new attempt shares the current Tender allowance.", stamp,
            ))
            conn.execute("INSERT INTO office_resume_origins VALUES(?,?,?,?,?,?,?,?)", (
                actual_resumed["id"], tender_id, plan_id, grant.id, actual_previous["id"], origin_id, decision_id, stamp,
            ))
            routing.validate_root(tender_id, actual_resumed["id"], plan_id)
