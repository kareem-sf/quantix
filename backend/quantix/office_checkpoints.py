"""Durable step checkpoints and uncertain external-effect receipts."""

from __future__ import annotations

import hashlib
import json
from typing import Literal

from .db import dump, new_id, now
from .execution_context import OfficeExecutionIdentity
from .staff_models import IdentifierText, OfficeConflict, OfficeModel, TimestampText
from .staff_store import _key

EffectState = Literal["prepared", "attempted", "confirmed", "uncertain"]


class CheckpointImplementationUnavailable(ValueError):
    pass


def tool_implementation_fingerprint():
    """Conservatively bind bundled Python helpers and installed library versions."""
    import sys
    from importlib.metadata import distributions
    from pathlib import Path

    root = Path(__file__).parent
    sources = sorted(root.rglob("*.py"))
    if not sources or not (root / "office_tools.py").is_file():
        raise CheckpointImplementationUnavailable("Bundled tool source proof is unavailable; completed steps must be verified again.")
    try:
        digest = hashlib.sha256()
        for path in sources:
            digest.update(path.relative_to(root).as_posix().encode())
            digest.update(hashlib.sha256(path.read_bytes()).digest())
        packages = sorted((item.metadata["Name"], item.version) for item in distributions()
                          if item.metadata["Name"])
        if not packages:
            raise CheckpointImplementationUnavailable("Installed library proof is unavailable; completed steps must be verified again.")
        digest.update(dump(packages).encode())
        digest.update(dump({"python": list(sys.version_info[:3]), "implementation": sys.implementation.name}).encode())
        return digest.hexdigest()
    except OSError as error:
        raise CheckpointImplementationUnavailable("Bundled tool implementation could not be verified; completed steps must be verified again.") from error


class CheckpointDraft(OfficeModel):
    step_id: IdentifierText
    assignment_id: IdentifierText | None = None
    input_fingerprint: str
    outputs: dict
    remaining_dependencies: list[str] = []


class StepCheckpoint(OfficeModel):
    id: IdentifierText
    step_id: IdentifierText
    assignment_id: IdentifierText | None = None
    input_fingerprint: str
    outputs: dict
    remaining_dependencies: list[str]
    created_at: TimestampText


class EffectReceipt(OfficeModel):
    operation_key: str
    state: EffectState
    result_ref: str | None = None


class ResumeCheckpointRequest(OfficeModel):
    checkpoint_id: IdentifierText
    expected_basis_fingerprint: str
    idempotency_key: IdentifierText


class CheckpointReuse(OfficeModel):
    checkpoint_id: IdentifierText
    step_id: str
    from_root_run_id: str | None
    to_root_run_id: str
    created_at: TimestampText


_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS office_checkpoint_invocations (
        root_id TEXT NOT NULL REFERENCES runs(id),
        invocation_id TEXT NOT NULL,
        request_hash TEXT NOT NULL,
        checkpoint_id TEXT NOT NULL REFERENCES office_step_checkpoints(id),
        created_at TEXT NOT NULL,
        PRIMARY KEY(root_id, invocation_id)
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS office_step_checkpoints (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL,
        root_id TEXT,
        step_id TEXT NOT NULL,
        assignment_id TEXT,
        input_fingerprint TEXT NOT NULL,
        outputs_json TEXT NOT NULL,
        remaining_json TEXT NOT NULL,
        created_at TEXT NOT NULL
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS office_effect_receipts (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL,
        operation_key TEXT NOT NULL,
        state TEXT NOT NULL,
        result_ref TEXT,
        created_at TEXT NOT NULL
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS office_checkpoint_reuses (
        checkpoint_id TEXT NOT NULL,
        tender_id TEXT NOT NULL REFERENCES tenders(id),
        from_root_run_id TEXT,
        to_root_run_id TEXT NOT NULL REFERENCES runs(id),
        created_at TEXT NOT NULL,
        UNIQUE (checkpoint_id, to_root_run_id)
    )
    """,
)


class OfficeCheckpointService:
    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)

    def invocation(self, root_id, invocation_id, request, *, checkpoint_id=None):
        """Bind a trusted tool invocation to one verified historical result."""
        from .staff_routing_models import canonical_json

        request_hash = hashlib.sha256(canonical_json(request).encode()).hexdigest()
        with self.repo.atomic() as conn:
            previous = conn.execute(
                "SELECT request_hash,checkpoint_id FROM office_checkpoint_invocations WHERE root_id=? AND invocation_id=?",
                (root_id, invocation_id),
            ).fetchone()
            if previous:
                if (previous["request_hash"] != request_hash
                        or checkpoint_id is not None and previous["checkpoint_id"] != checkpoint_id):
                    raise OfficeConflict("This staff invocation already selected different completed work.")
                return previous["checkpoint_id"]
            if checkpoint_id is not None:
                conn.execute("INSERT INTO office_checkpoint_invocations VALUES(?,?,?,?,?)",
                             (root_id, invocation_id, request_hash, checkpoint_id, now()))
            return checkpoint_id

    def checkpoint(self, ctx: OfficeExecutionIdentity, draft: CheckpointDraft) -> StepCheckpoint:
        if not ctx.tender_id:
            raise ValueError("Checkpoints require a selected Tender.")
        identifier, stamp = new_id(), now()
        with self.repo.atomic() as conn:
            conn.execute(
                """
                INSERT INTO office_step_checkpoints(
                    id,tender_id,root_id,step_id,assignment_id,input_fingerprint,outputs_json,remaining_json,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?)
                """,
                (
                    identifier,
                    ctx.tender_id,
                    ctx.root_run_id,
                    draft.step_id,
                    draft.assignment_id,
                    draft.input_fingerprint,
                    dump(draft.outputs),
                    dump(draft.remaining_dependencies),
                    stamp,
                ),
            )
        return StepCheckpoint(
            id=identifier,
            step_id=draft.step_id,
            assignment_id=draft.assignment_id,
            input_fingerprint=draft.input_fingerprint,
            outputs=draft.outputs,
            remaining_dependencies=draft.remaining_dependencies,
            created_at=stamp,
        )

    def resume_checkpoint(
        self, ctx: OfficeExecutionIdentity, request: ResumeCheckpointRequest
    ) -> StepCheckpoint:
        _key(request.idempotency_key)
        if not ctx.tender_id or not ctx.root_run_id:
            raise ValueError("Checkpoint continuation requires the selected Tender and work root.")
        with self.repo.atomic() as conn:
            run = self.repo.get_run(ctx.root_run_id)
            if run["tender_id"] != ctx.tender_id:
                raise KeyError("This work is outside the selected Tender.")
            if run["status"] not in {"queued", "running"}:
                raise ValueError("Only active work can reuse a completed checkpoint.")
            row = conn.execute(
                "SELECT * FROM office_step_checkpoints WHERE id=? AND tender_id=?",
                (request.checkpoint_id, ctx.tender_id),
            ).fetchone()
            if row is None:
                raise KeyError("This item could not be found in the selected Tender.")
            if row["input_fingerprint"] != request.expected_basis_fingerprint:
                raise ValueError("The checkpoint basis changed. Re-run the affected step.")
            if row["root_id"] != ctx.root_run_id and not self._shares_resume_lineage(
                conn, ctx.tender_id, row["root_id"], ctx.root_run_id
            ):
                raise ValueError(
                    "This checkpoint belongs to unrelated work. Resume that work before reusing its steps."
                )
            stamp = now()
            inserted = conn.execute(
                """
                INSERT OR IGNORE INTO office_checkpoint_reuses(
                    checkpoint_id,tender_id,from_root_run_id,to_root_run_id,created_at
                ) VALUES(?,?,?,?,?)
                """,
                (row["id"], ctx.tender_id, row["root_id"], ctx.root_run_id, stamp),
            )
            if inserted.rowcount:
                self.repo.event(ctx.root_run_id, "step_checkpoint_reused", "A saved step passed current continuation checks.",
                                {"checkpoint_id": row["id"], "from_root_run_id": row["root_id"], "step_id": row["step_id"]})

            return StepCheckpoint(
                id=row["id"],
                step_id=row["step_id"],
                assignment_id=row["assignment_id"],
                input_fingerprint=row["input_fingerprint"],
                outputs=json.loads(row["outputs_json"]),
                remaining_dependencies=json.loads(row["remaining_json"]),
                created_at=row["created_at"],
            )

    def _shares_resume_lineage(
        self, conn, tender_id: str, from_root: str | None, to_root: str
    ) -> bool:
        """Only an actual ancestor is eligible; sibling/future roots confer nothing."""
        return bool(from_root and from_root in self._ancestors(conn, tender_id, to_root))

    def _ancestors(self, conn, tender_id, root_id):
        roots = []
        has_origins = conn.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='office_resume_origins'"
        ).fetchone()
        while root_id:
            if root_id in roots or len(roots) >= 1000:
                raise ValueError("The saved work continuation lineage is invalid.")
            roots.append(root_id)
            row = conn.execute(
                "SELECT previous_run_id FROM office_resume_origins WHERE tender_id=? AND root_run_id=?",
                (tender_id, root_id),
            ).fetchone() if has_origins else None
            root_id = row[0] if row else None
        return roots

    def _staff_basis(self, conn, result, binding, root_id, grant, *, implementation=None):
        """Derive reusable input proof from trusted records, never model hashes."""
        from .ai_connections import AIConnectionService
        from .ai_readiness import require_ready
        from .manager_runtime import ManagerRunProfiles
        from .office_messages import OfficeMessageService
        from .office_tools import source_tools
        from .staff_routing_models import canonical_json
        from .staff_store import StaffStore

        staff = StaffStore(self.repo)
        profile = staff.get_staff(result.tender_id, result.staff_id, result.staff_version)
        if grant.id != binding.grant_id:
            raise ValueError("The completed step's permission or source basis changed.")
        roots = self._ancestors(conn, result.tender_id, root_id)
        steering = []
        if conn.execute("SELECT 1 FROM sqlite_master WHERE name='office_instruction_requests'").fetchone():
            steering = [dict(row) for row in conn.execute(
                f"SELECT kind,content,selection FROM office_instruction_requests WHERE tender_id=? AND root_id IN ({','.join('?' for _ in roots)}) ORDER BY created_at,id",
                (result.tender_id, *roots),
            )]
        wanted = {item.id for item in binding.tools}
        definitions = [item for item in source_tools() if item.name in wanted]
        if {item.name for item in definitions} != wanted:
            raise ValueError("The completed step's tool implementation is unavailable.")
        tool_basis = [{"name": item.name, "parameters": item.parameters,
                       "description": item.description, "read_only": item.read_only,
                       "idempotent": item.idempotent,
                       "requires_invocation_id": item.requires_invocation_id}
                      for item in definitions]
        OfficeMessageService(self.repo)  # Ensure the existing immutable message schema.
        messages = [dict(row) for row in conn.execute(
            """SELECT m.id,m.kind,m.text,m.sender_id,m.sender_version,m.artifact_refs_json
            FROM office_messages m WHERE m.tender_id=? AND m.root_run_id=?
            AND NOT (m.kind='finding' AND m.sender_id=? AND m.sender_assignment_id=?)
            AND (m.sender_assignment_id=? OR EXISTS(
                SELECT 1 FROM office_message_recipients r WHERE r.message_id=m.id AND r.assignment_id=?))
            ORDER BY m.rowid""",
            (result.tender_id, result.root_run_id, result.staff_id, result.assignment_id,
             result.assignment_id, result.assignment_id),
        )]
        notebook = []
        if conn.execute("SELECT 1 FROM sqlite_master WHERE name='office_staff_notebook'").fetchone():
            notebook = [dict(row) for row in conn.execute(
                "SELECT * FROM office_staff_notebook WHERE tender_id=? AND staff_id=? AND assignment_id=? AND current=1 ORDER BY created_at,id",
                (result.tender_id, result.staff_id, result.assignment_id),
            )]
        basis = {
            "version": 1,
            "instruction": self.repo.get_run(root_id)["instruction"],
            "steering": steering,
            "preferences": self.repo.setting("preferences", ""),
            "manager": ManagerRunProfiles(self.repo).get(result.tender_id, root_id).model_dump(mode="json"),
            "profile": profile.model_dump(mode="json"),
            "order": staff.get_work_order(result.tender_id, result.work_order_id).model_dump(mode="json"),
            "approved_scope": self.repo.approved_scope(result.tender_id, binding.plan_id),
            "input_messages": messages,
            "notebook": notebook,
            "permissions": grant.envelope.model_dump(mode="json"),
            "binding": binding.model_dump(mode="json", exclude={"id", "root_run_id", "created_at"}),
            "checked_component_version": require_ready(
                self.repo, AIConnectionService(self.repo).get(binding.route.connection_id), binding.route.model_id
            ),
            "tools": tool_basis,
            "implementation": implementation or tool_implementation_fingerprint(),
        }
        return hashlib.sha256(canonical_json(basis).encode()).hexdigest()

    def capture_staff_inputs(self, tender_id, assignment_id, binding):
        """Pin the actual input basis before the provider is allowed to start."""
        from types import SimpleNamespace

        from .staff_assignments import StaffAssignmentService
        from .staff_routing import StaffRoutingService

        routing = StaffRoutingService(self.repo)
        with routing.policy.connections.authority_guard(), self.repo.atomic() as conn:
            assignment = StaffAssignmentService(self.repo).get(tender_id, assignment_id)
            if assignment.status != "running":
                raise ValueError("Only a running assignment can capture step inputs.")
            routing.validate_binding(tender_id, binding.id)
            grant = routing.validate_root(tender_id, assignment.root_run_id, binding.plan_id)
            identity = SimpleNamespace(**assignment.model_dump(), assignment_id=assignment.id)
            try:
                return self._staff_basis(conn, identity, binding, assignment.root_run_id, grant)
            except CheckpointImplementationUnavailable as error:
                self.repo.event(assignment.root_run_id, "step_checkpoint_unavailable", str(error),
                                {"assignment_id": assignment.id})
                return None

    @staticmethod
    def _result_hash(conn, tender_id, result_id):
        original = conn.execute(
            "SELECT payload_json FROM office_staff_results WHERE tender_id=? AND id=?",
            (tender_id, result_id),
        ).fetchone()
        return hashlib.sha256(original[0].encode()).hexdigest()

    def checkpoint_staff_result(self, tender_id, result_id, binding, input_fingerprint):
        """Called in the staff completion transaction after its immutable result exists."""
        from .execution_context import engineer_identity
        from .staff_assignments import StaffAssignmentService
        from .staff_routing import StaffRoutingService

        routing = StaffRoutingService(self.repo)
        with routing.policy.connections.authority_guard(), self.repo.atomic() as conn:
            result = StaffAssignmentService(self.repo).get_result(tender_id, result_id)
            assignment = StaffAssignmentService(self.repo).get(tender_id, result.assignment_id)
            if assignment.status != "completed" or assignment.result_id != result_id:
                raise ValueError("Only a committed completed staff result can be checkpointed.")
            grant = routing.validate_root(tender_id, result.root_run_id, binding.plan_id)
            try:
                fingerprint = self._staff_basis(conn, result, binding, result.root_run_id, grant)
            except CheckpointImplementationUnavailable as error:
                self.repo.event(result.root_run_id, "step_checkpoint_unavailable", str(error),
                                {"assignment_id": assignment.id})
                return None
            # New steering/material during the turn does not retroactively
            # become an input to its completed result. Save the draft, but
            # require a fresh step instead of claiming reusable completion.
            if fingerprint != input_fingerprint:
                self.repo.event(result.root_run_id, "step_checkpoint_unavailable",
                                "The draft is saved, but its inputs changed during the turn. Verify the step again before continuing.",
                                {"assignment_id": assignment.id, "result_id": result.id})
                return None
            result_hash = self._result_hash(conn, tender_id, result.id)
            pending = [row[0] for row in conn.execute(
                "SELECT id FROM office_assignments WHERE tender_id=? AND parent_assignment_id=? AND status!='completed' ORDER BY created_at,id",
                (tender_id, assignment.id),
            )]
            checkpoint = self.checkpoint(
                engineer_identity(tender_id, actor_kind="staff", actor_id=result.staff_id,
                                  root_run_id=result.root_run_id, assignment_id=result.assignment_id,
                                  profile_version=result.staff_version, route_binding_id=binding.id),
                CheckpointDraft(step_id="staff-result:" + result.id, assignment_id=result.assignment_id,
                    input_fingerprint=fingerprint, outputs={"kind": "staff_result", "result_id": result.id,
                    "result_sha256": result_hash, "source_bases": [item.model_dump(mode="json") for item in result.source_bases]},
                    remaining_dependencies=pending),
            )
            self.repo.event(result.root_run_id, "step_checkpoint_saved", "A completed staff draft is saved for verified continuation.",
                            {"checkpoint_id": checkpoint.id, "assignment_id": assignment.id, "result_id": result.id})
            return checkpoint

    def verified_staff_for_root(self, tender_id, root_id, plan_id):
        """Reuse immutable results only after validating this active root and current basis."""
        from .execution_context import engineer_identity
        from .manager_runtime import ManagerRunProfiles
        from .staff_assignments import StaffAssignmentService
        from .staff_routing import StaffRoutingService

        routing = StaffRoutingService(self.repo)
        with routing.policy.connections.authority_guard(), self.repo.atomic() as conn:
            grant = routing.validate_root(tender_id, root_id, plan_id)
            roots = self._ancestors(conn, tender_id, root_id)
            if len(roots) < 2:
                return []
            try:
                implementation = tool_implementation_fingerprint()
            except CheckpointImplementationUnavailable as error:
                self.repo.event(root_id, "step_checkpoint_unavailable", str(error))
                return []
            rows = conn.execute(
                f"SELECT * FROM office_step_checkpoints WHERE tender_id=? AND root_id IN ({','.join('?' for _ in roots[1:])}) ORDER BY created_at,id",
                (tender_id, *roots[1:]),
            ).fetchall()
            assignments = StaffAssignmentService(self.repo)
            manager = ManagerRunProfiles(self.repo).get(tender_id, root_id)
            verified = []
            for row in rows:
                outputs = json.loads(row["outputs_json"])
                if outputs.get("kind") != "staff_result":
                    continue
                try:
                    result = assignments.get_result(tender_id, outputs["result_id"])
                    assignment = assignments.get(tender_id, result.assignment_id)
                    if routing.staff.get_staff(tender_id, result.staff_id).version != result.staff_version:
                        continue
                    if (assignment.status != "completed" or assignment.result_id != result.id
                            or result.currentness != "current"
                            or result.root_run_id != row["root_id"] or result.assignment_id != row["assignment_id"]):
                        continue
                    binding_row = conn.execute("SELECT * FROM office_route_bindings WHERE tender_id=? AND id=?",
                                               (tender_id, result.route_binding_id)).fetchone()
                    if binding_row is None:
                        continue
                    binding = routing._parse_binding(binding_row)
                    fingerprint = self._staff_basis(conn, result, binding, root_id, grant, implementation=implementation)
                    result_hash = self._result_hash(conn, tender_id, result.id)
                    if fingerprint != row["input_fingerprint"] or result_hash != outputs.get("result_sha256"):
                        continue
                    checkpoint = self.resume_checkpoint(
                        engineer_identity(tender_id, actor_kind="manager", actor_id=manager.id, root_run_id=root_id),
                        ResumeCheckpointRequest(checkpoint_id=row["id"], expected_basis_fingerprint=fingerprint,
                                                idempotency_key="staff-" + result.id),
                    )
                    verified.append({"assignment": assignment.model_dump(mode="json"), "result_id": result.id,
                                     "checkpoint_id": checkpoint.id, "question_message_id": None,
                                     "reused_prior_work": True, "remaining_dependencies": checkpoint.remaining_dependencies})
                except (KeyError, ValueError):
                    continue
            return verified

    def reusable_for_root(self, tender_id: str, root_run_id: str) -> list[CheckpointReuse]:
        """List checkpoints a resumed attempt may continue from, newest first."""

        with self.repo.db.connect() as conn:
            origins_table = conn.execute(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='office_resume_origins'"
            ).fetchone()
            if origins_table is None:
                return []
            origins = conn.execute(
                "SELECT previous_run_id,origin_run_id FROM office_resume_origins WHERE tender_id=? AND root_run_id=?",
                (tender_id, root_run_id),
            ).fetchone()
            if origins is None:
                return []
            roots = self._ancestors(conn, tender_id, root_run_id)
            rows = conn.execute(
                f"""
                SELECT id,step_id,root_id,created_at FROM office_step_checkpoints
                WHERE tender_id=? AND root_id IN ({",".join("?" for _ in roots)})
                ORDER BY created_at DESC,id DESC LIMIT 50
                """,
                (tender_id, *roots),
            ).fetchall()
            return [
                CheckpointReuse(
                    checkpoint_id=row["id"],
                    step_id=row["step_id"],
                    from_root_run_id=row["root_id"],
                    to_root_run_id=root_run_id,
                    created_at=row["created_at"],
                )
                for row in rows
            ]

    def record_effect(
        self, ctx: OfficeExecutionIdentity, operation_key: str, state: EffectState
    ) -> EffectReceipt:
        if not operation_key or not operation_key.strip():
            raise ValueError("Effect receipts require an operation key.")
        identifier, stamp = new_id(), now()
        with self.repo.atomic() as conn:
            latest = conn.execute(
                """
                SELECT state FROM office_effect_receipts
                WHERE tender_id=? AND operation_key=?
                ORDER BY created_at DESC,rowid DESC LIMIT 1
                """,
                (ctx.tender_id, operation_key.strip()),
            ).fetchone()
            previous = latest["state"] if latest is not None else None
            allowed = {
                None: {"prepared", "attempted", "uncertain"},
                "prepared": {"attempted", "uncertain"},
                "attempted": {"confirmed", "uncertain"},
                "uncertain": {"attempted", "confirmed"},
                "confirmed": set(),
            }
            if state not in allowed[previous]:
                raise ValueError(f"An effect cannot move from {previous or 'nothing'} to {state}.")
            conn.execute(
                """
                INSERT INTO office_effect_receipts(id,tender_id,operation_key,state,result_ref,created_at)
                VALUES(?,?,?,?,?,?)
                """,
                (identifier, ctx.tender_id, operation_key.strip(), state, None, stamp),
            )
        return EffectReceipt(operation_key=operation_key.strip(), state=state)

    def effect_state(self, tender_id: str, operation_key: str) -> EffectState | None:
        with self.repo.db.connect() as conn:
            row = conn.execute(
                """
                SELECT state FROM office_effect_receipts
                WHERE tender_id=? AND operation_key=?
                ORDER BY created_at DESC,rowid DESC LIMIT 1
                """,
                (tender_id, operation_key),
            ).fetchone()
            return row["state"] if row is not None else None
