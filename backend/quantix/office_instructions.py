"""Status reads and instruction revisions without starting provider work."""

from __future__ import annotations

from .assignment_graph import AssignmentGraphService
from .db import new_id, now
from .execution_context import OfficeExecutionIdentity
from .instruction_models import InstructionAdmission, InstructionRevisionRequest
from .office_ownership import OfficeOwnershipService
from .staff_assignments import StaffAssignmentService
from .staff_models import OfficeConflict, OfficeModel
from .staff_store import _canonical_hash, _key

_STEERING_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS office_instruction_requests (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        root_id TEXT NOT NULL REFERENCES runs(id),
        kind TEXT NOT NULL CHECK (kind IN ('question','constraint','replace','urgent','cancel')),
        content TEXT NOT NULL,
        selection TEXT NOT NULL DEFAULT '',
        state TEXT NOT NULL DEFAULT 'admitted' CHECK (state IN ('admitted','applied')),
        idempotency_key TEXT NOT NULL,
        payload_hash TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE (tender_id, root_id, idempotency_key)
    )
    """,
)


class WorkStatusSnapshot(OfficeModel):
    tender_id: str
    root_run_id: str | None
    owners: list[str]
    blockers: list[str]
    provider_requests: int = 0


class OfficeInstructionService:
    def __init__(self, repo):
        self.repo = repo
        self.graphs = AssignmentGraphService(repo)
        self.ownership = OfficeOwnershipService(repo)
        self.assignments = StaffAssignmentService(repo)
        with repo.atomic() as conn:
            for statement in _STEERING_SCHEMA:
                conn.execute(statement)

    def admit(
        self, ctx: OfficeExecutionIdentity, request: InstructionRevisionRequest
    ) -> InstructionAdmission:
        """Record engineer steering for the next safe step. Admits nothing else.

        Admission never starts provider work and never edits running
        assignments: the controller applies admitted revisions at the next
        turn boundary, after the current step has fully completed.
        """

        if not ctx.tender_id or not ctx.root_run_id:
            raise ValueError("Steering requires the selected Tender and work root.")
        content = request.content.strip()
        if not content:
            raise ValueError("Steering needs actual instruction text.")
        key = _key(request.idempotency_key)
        payload_hash = _canonical_hash(request.model_dump(mode="json"))
        with self.repo.atomic() as conn:
            run = conn.execute(
                "SELECT status FROM runs WHERE tender_id=? AND id=?",
                (ctx.tender_id, ctx.root_run_id),
            ).fetchone()
            if run is None:
                raise KeyError("This item could not be found in the selected Tender.")
            if run["status"] not in {"queued", "running"}:
                raise OfficeConflict("Steer only while its work is still running.")
            existing = conn.execute(
                """
                SELECT * FROM office_instruction_requests
                WHERE tender_id=? AND root_id=? AND idempotency_key=?
                """,
                (ctx.tender_id, ctx.root_run_id, key),
            ).fetchone()
            if existing is not None:
                if existing["payload_hash"] != payload_hash:
                    raise OfficeConflict(
                        "This steering key was already used for different content."
                    )
                return InstructionAdmission(
                    id=existing["id"],
                    root_id=existing["root_id"],
                    kind=existing["kind"],
                    state=existing["state"],
                    replayed=True,
                    created_at=existing["created_at"],
                )
            identifier, stamp = new_id(), now()
            conn.execute(
                """
                INSERT INTO office_instruction_requests(
                    id,tender_id,root_id,kind,content,selection,state,
                    idempotency_key,payload_hash,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    identifier,
                    ctx.tender_id,
                    ctx.root_run_id,
                    request.kind,
                    content,
                    request.selection.strip(),
                    "admitted",
                    key,
                    payload_hash,
                    stamp,
                ),
            )
            return InstructionAdmission(
                id=identifier,
                root_id=ctx.root_run_id,
                kind=request.kind,
                state="admitted",
                replayed=False,
                created_at=stamp,
            )

    def pending_for_turn(self, tender_id: str, root_run_id: str) -> list[InstructionAdmission]:
        """Return admitted steering in application order without changing state."""

        with self.repo.db.connect() as conn:
            rows = conn.execute(
                """
                SELECT * FROM office_instruction_requests
                WHERE tender_id=? AND root_id=? AND state='admitted'
                ORDER BY
                    CASE kind WHEN 'cancel' THEN 0 WHEN 'urgent' THEN 1 ELSE 2 END,
                    created_at,id
                """,
                (tender_id, root_run_id),
            ).fetchall()
            return [
                InstructionAdmission(
                    id=row["id"],
                    root_id=row["root_id"],
                    kind=row["kind"],
                    state=row["state"],
                    replayed=False,
                    created_at=row["created_at"],
                )
                for row in rows
            ]

    def admission_text(self, tender_id: str, admission_id: str) -> str:
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT content FROM office_instruction_requests WHERE tender_id=? AND id=?",
                (tender_id, admission_id),
            ).fetchone()
            if row is None:
                raise KeyError("This item could not be found in the selected Tender.")
            return row["content"]

    def mark_applied(self, tender_id: str, admission_ids: list[str]) -> None:
        if not admission_ids:
            return
        with self.repo.atomic() as conn:
            for admission_id in admission_ids:
                conn.execute(
                    """
                    UPDATE office_instruction_requests SET state='applied'
                    WHERE tender_id=? AND id=? AND state='admitted'
                    """,
                    (tender_id, admission_id),
                )

    def list(self, tender_id: str, root_run_id: str) -> list[InstructionAdmission]:
        with self.repo.db.connect() as conn:
            rows = conn.execute(
                """
                SELECT * FROM office_instruction_requests
                WHERE tender_id=? AND root_id=?
                ORDER BY created_at,id
                """,
                (tender_id, root_run_id),
            ).fetchall()
            return [
                InstructionAdmission(
                    id=row["id"],
                    root_id=row["root_id"],
                    kind=row["kind"],
                    state=row["state"],
                    replayed=False,
                    created_at=row["created_at"],
                )
                for row in rows
            ]

    def status(self, ctx: OfficeExecutionIdentity) -> WorkStatusSnapshot:
        if not ctx.tender_id:
            raise ValueError("Status requires a selected Tender.")
        blockers = []
        owners = []
        if ctx.root_run_id:
            graph = self.graphs.latest(ctx.tender_id, ctx.root_run_id)
            if graph:
                for node in graph.nodes:
                    if node.state == "blocked":
                        blockers.append(f"{node.key} is waiting on a prerequisite.")
                    if node.owner_staff_id:
                        owners.append(node.owner_staff_id)
            for assignment in self.assignments.list(
                ctx.tender_id, root_run_id=ctx.root_run_id, limit=200
            ):
                if assignment.status in {"queued", "running"}:
                    if assignment.staff_id not in owners:
                        owners.append(assignment.staff_id)
                elif assignment.status == "waiting":
                    if assignment.staff_id not in owners:
                        owners.append(assignment.staff_id)
                    detail = (assignment.detail or "").strip()
                    blockers.append(
                        f"{assignment.staff_id} is waiting: {detail[:200]}"
                        if detail
                        else f"{assignment.staff_id} is waiting for a reply."
                    )
            run = self.repo.get_run(ctx.root_run_id)
            if run["status"] in {"queued", "running"}:
                owners.append(run["id"])
        return WorkStatusSnapshot(
            tender_id=ctx.tender_id,
            root_run_id=ctx.root_run_id,
            owners=owners,
            blockers=blockers,
            provider_requests=0,
        )
