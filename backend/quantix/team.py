"""Saved tender staff and assignments. Running an assignment lives in team_runtime."""

from __future__ import annotations

import json

from .db import dump, new_id, now
from .team_models import Assignment, AssignmentResult, StaffDraft, StaffMember

_SCHEMA = (
    """CREATE TABLE IF NOT EXISTS team_staff (
        id TEXT PRIMARY KEY, tender_id TEXT NOT NULL REFERENCES tenders(id),
        status TEXT NOT NULL, data_json TEXT NOT NULL, created_run_id TEXT NOT NULL,
        created_at TEXT NOT NULL, updated_at TEXT NOT NULL)""",
    "CREATE INDEX IF NOT EXISTS team_staff_tender ON team_staff(tender_id, created_at)",
    """CREATE TABLE IF NOT EXISTS team_assignments (
        id TEXT PRIMARY KEY, tender_id TEXT NOT NULL REFERENCES tenders(id),
        run_id TEXT NOT NULL, staff_id TEXT NOT NULL REFERENCES team_staff(id),
        status TEXT NOT NULL, data_json TEXT NOT NULL,
        created_at TEXT NOT NULL, updated_at TEXT NOT NULL)""",
    "CREATE INDEX IF NOT EXISTS team_assignments_tender ON team_assignments(tender_id, created_at)",
    "CREATE INDEX IF NOT EXISTS team_assignments_run ON team_assignments(run_id, status)",
)
_OPEN = ("queued", "running", "waiting")


class TeamService:
    def __init__(self, repo):
        self.repo = repo
        with repo.db.connect(write=True) as conn:
            for statement in _SCHEMA:
                conn.execute(statement)

    # Staff -----------------------------------------------------------------

    def hire(self, tender_id: str, run_id: str, draft: StaffDraft) -> StaffMember:
        self.repo.get_tender(tender_id)
        with self.repo.db.connect() as conn:
            names = {
                json.loads(row[0])["name"].casefold()
                for row in conn.execute(
                    "SELECT data_json FROM team_staff WHERE tender_id=? AND status='active'",
                    (tender_id,),
                )
            }
        if draft.name.casefold() in names:
            raise ValueError(
                f"{draft.name} is already on this tender's team. Assign them work instead of hiring again."
            )
        identifier, stamp = new_id(), now()
        with self.repo.db.connect(write=True) as conn:
            conn.execute(
                "INSERT INTO team_staff VALUES(?,?,?,?,?,?,?)",
                (identifier, tender_id, "active", dump(draft.model_dump()), run_id, stamp, stamp),
            )
        return self.get_staff(tender_id, identifier)

    def retire(self, tender_id: str, staff_id: str) -> StaffMember:
        self.get_staff(tender_id, staff_id)
        with self.repo.db.connect(write=True) as conn:
            conn.execute(
                "UPDATE team_staff SET status='retired', updated_at=? WHERE id=? AND tender_id=?",
                (now(), staff_id, tender_id),
            )
        return self.get_staff(tender_id, staff_id)

    def get_staff(self, tender_id: str, staff_id: str) -> StaffMember:
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT * FROM team_staff WHERE id=? AND tender_id=?", (staff_id, tender_id)
            ).fetchone()
        if row is None:
            raise KeyError("This staff member is not on this tender's team.")
        return self._staff(row)

    def list_staff(self, tender_id: str) -> list[StaffMember]:
        with self.repo.db.connect() as conn:
            rows = conn.execute(
                "SELECT * FROM team_staff WHERE tender_id=? ORDER BY created_at, id", (tender_id,)
            ).fetchall()
        return [self._staff(row) for row in rows]

    @staticmethod
    def _staff(row) -> StaffMember:
        return StaffMember(
            **json.loads(row["data_json"]),
            id=row["id"],
            tender_id=row["tender_id"],
            status=row["status"],
            portrait_seed=row["id"],
            created_run_id=row["created_run_id"],
            created_at=row["created_at"],
            updated_at=row["updated_at"],
        )

    # Assignments -----------------------------------------------------------

    def assign(
        self,
        tender_id: str,
        run_id: str,
        staff_id: str,
        *,
        title: str,
        brief: str,
        expected_result: str,
        source_ids: list[str],
        route: dict,
    ) -> Assignment:
        staff = self.get_staff(tender_id, staff_id)
        if staff.status != "active":
            raise ValueError(f"{staff.name} is retired. Hire or choose an active colleague.")
        for source_id in source_ids:
            self.repo.get_evidence(tender_id, source_id)
        identifier, stamp = new_id(), now()
        data = {
            "title": title,
            "brief": brief,
            "expected_result": expected_result,
            "source_ids": source_ids,
            "connection_id": route["connection_id"],
            "model_id": route["model_id"],
            "question": None,
            "answer": None,
            "result": None,
            "detail": "",
            "usage": {},
        }
        with self.repo.db.connect(write=True) as conn:
            conn.execute(
                "INSERT INTO team_assignments VALUES(?,?,?,?,?,?,?,?)",
                (identifier, tender_id, run_id, staff_id, "queued", dump(data), stamp, stamp),
            )
        return self.get(tender_id, identifier)

    def get(self, tender_id: str, assignment_id: str) -> Assignment:
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT * FROM team_assignments WHERE id=? AND tender_id=?",
                (assignment_id, tender_id),
            ).fetchone()
        if row is None:
            raise KeyError("This assignment does not belong to this tender.")
        return self._assignment(row)

    def list(
        self,
        tender_id: str,
        *,
        run_id: str | None = None,
        staff_id: str | None = None,
        limit: int = 200,
    ) -> list[Assignment]:
        query, values = "SELECT * FROM team_assignments WHERE tender_id=?", [tender_id]
        if run_id:
            query, values = query + " AND run_id=?", [*values, run_id]
        if staff_id:
            query, values = query + " AND staff_id=?", [*values, staff_id]
        with self.repo.db.connect() as conn:
            rows = conn.execute(
                query + " ORDER BY created_at DESC, id LIMIT ?", (*values, limit)
            ).fetchall()
        return [self._assignment(row) for row in rows]

    def queued(self, tender_id: str, run_id: str) -> list[Assignment]:
        with self.repo.db.connect() as conn:
            rows = conn.execute(
                "SELECT * FROM team_assignments WHERE tender_id=? AND run_id=? AND status='queued' ORDER BY created_at, id",
                (tender_id, run_id),
            ).fetchall()
        return [self._assignment(row) for row in rows]

    def answer(self, tender_id: str, assignment_id: str, text: str) -> Assignment:
        assignment = self.get(tender_id, assignment_id)
        if assignment.status != "waiting":
            raise ValueError("This assignment is not waiting for an answer.")
        return self._update(assignment, "queued", answer=text)

    def start(self, tender_id: str, assignment_id: str) -> Assignment:
        assignment = self.get(tender_id, assignment_id)
        if assignment.status != "queued":
            raise ValueError("Only queued work can start.")
        return self._update(assignment, "running", detail="")

    def complete(self, assignment: Assignment, result: AssignmentResult, usage: dict) -> Assignment:
        return self._update(
            assignment,
            "completed",
            result=result.model_dump(),
            usage=_add_usage(assignment.usage, usage),
        )

    def ask(self, assignment: Assignment, question: str, usage: dict) -> Assignment:
        return self._update(
            assignment, "waiting", question=question, usage=_add_usage(assignment.usage, usage)
        )

    def fail(self, assignment: Assignment, detail: str, *, status: str = "failed") -> Assignment:
        return self._update(assignment, status, detail=detail[:1200])

    def cancel_run(self, tender_id: str, run_id: str, detail: str) -> int:
        stopped = [a for a in self.list(tender_id, run_id=run_id) if a.status in _OPEN]
        for assignment in stopped:
            self._update(assignment, "cancelled", detail=detail)
        return len(stopped)

    def recover_interrupted(self) -> int:
        """Work that was running when the service stopped cannot resume mid-turn."""
        with self.repo.db.connect() as conn:
            rows = conn.execute(
                "SELECT * FROM team_assignments WHERE status IN ('queued','running')"
            ).fetchall()
        for row in rows:
            self._update(
                self._assignment(row),
                "cancelled",
                detail="Quantix stopped before this work finished.",
            )
        return len(rows)

    def _update(self, assignment: Assignment, status: str, **changes) -> Assignment:
        data = assignment.model_dump(
            include={
                "title",
                "brief",
                "expected_result",
                "source_ids",
                "connection_id",
                "model_id",
                "question",
                "answer",
                "result",
                "detail",
                "usage",
            }
        )
        data.update(changes)
        with self.repo.db.connect(write=True) as conn:
            conn.execute(
                "UPDATE team_assignments SET status=?, data_json=?, updated_at=? WHERE id=?",
                (status, dump(data), now(), assignment.id),
            )
        return self.get(assignment.tender_id, assignment.id)

    @staticmethod
    def _assignment(row) -> Assignment:
        return Assignment(
            **json.loads(row["data_json"]),
            id=row["id"],
            tender_id=row["tender_id"],
            run_id=row["run_id"],
            staff_id=row["staff_id"],
            status=row["status"],
            created_at=row["created_at"],
            updated_at=row["updated_at"],
        )


def _add_usage(current: dict, extra: dict) -> dict:
    counters = (
        "requests",
        "input_tokens",
        "output_tokens",
        "cached_input_tokens",
        "reasoning_tokens",
        "web_search_calls",
    )
    return {key: (current.get(key) or 0) + (extra.get(key) or 0) for key in counters}
