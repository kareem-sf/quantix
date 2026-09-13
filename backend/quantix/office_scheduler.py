"""Bounded descendant spawning under one root budget owner."""

from __future__ import annotations

from pydantic import Field

from .execution_context import OfficeExecutionIdentity
from .staff_assignments import StaffAssignmentService
from .staff_models import IdentifierText, OfficeConflict, OfficeModel
from .staff_store import _key

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS office_execution_nodes (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL,
        root_id TEXT NOT NULL,
        parent_id TEXT,
        depth INTEGER NOT NULL,
        work_order_id TEXT NOT NULL,
        status TEXT NOT NULL,
        created_at TEXT NOT NULL
    )
    """,
)


class ChildAssignmentRequest(OfficeModel):
    work_order_id: IdentifierText
    parent_assignment_id: IdentifierText | None = None
    prerequisite_assignment_ids: list[IdentifierText] = Field(default_factory=list, max_length=50)
    requested_route_option_id: IdentifierText | None = None
    capability_ids: list[str] = Field(default_factory=list, max_length=100)
    max_depth: int = Field(default=2, ge=0, le=8)
    idempotency_key: IdentifierText


class AssignmentExecution(OfficeModel):
    id: IdentifierText
    parent_assignment_id: IdentifierText | None = None
    root_id: IdentifierText
    depth: int
    status: str


class OfficeScheduler:
    def __init__(self, repo):
        self.repo = repo
        self.assignments = StaffAssignmentService(repo)
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)

    def spawn_child(
        self, ctx: OfficeExecutionIdentity, request: ChildAssignmentRequest
    ) -> AssignmentExecution:
        """Queue one descendant as a real assignment under the same root budget.

        Depth is enforced against the reviewed delegation envelope held by the
        route binding, never against the model-supplied request cap alone: the
        effective cap is the smaller of the two.
        """

        if not ctx.tender_id or not ctx.root_run_id:
            raise ValueError("Descendant work requires a Tender root.")
        if ctx.actor_kind != "staff" or not ctx.assignment_id:
            raise ValueError("Only an active staff assignment can spawn a child.")
        key = _key(request.idempotency_key)
        from .office_tools import OfficeContext
        from .staff_models import ManagerCreationContext
        from .staff_routing import StaffRoutingService

        with self.repo.atomic() as conn:
            parent = conn.execute(
                "SELECT * FROM office_assignments WHERE tender_id=? AND id=?",
                (ctx.tender_id, ctx.assignment_id),
            ).fetchone()
        if parent is None or parent["root_run_id"] != ctx.root_run_id:
            raise OfficeConflict("The parent assignment does not belong to this work root.")
        if (
            request.parent_assignment_id is not None
            and request.parent_assignment_id != parent["id"]
        ):
            raise OfficeConflict("The child request names a different parent assignment.")
        routing = StaffRoutingService(self.repo)
        office = OfficeContext(
            self.repo,
            ctx.tender_id,
            ctx.root_run_id,
            actor_id=parent["staff_id"],
            staff_version=parent["staff_version"],
            assignment_id=parent["id"],
            route_binding_id=parent["route_binding_id"],
        )
        binding = routing.bind_child(
            office,
            request.work_order_id,
            request.requested_route_option_id,
            list(request.capability_ids),
            key,
        )
        grant = routing.approved_grant(ctx.tender_id, binding.plan_id)
        effective_cap = min(request.max_depth, grant.envelope.max_depth)
        if int(parent["depth"] or 0) + 1 > effective_cap:
            raise ValueError(
                f"Depth {int(parent['depth'] or 0) + 1} is outside the reviewed cap of {effective_cap}."
            )
        pin = routing.manager_runs.get(ctx.tender_id, binding.root_run_id)
        manager_context = ManagerCreationContext(
            ctx.tender_id, binding.root_run_id, pin.version, binding.plan_id
        )
        assignment = self.assignments.queue_child(
            manager_context,
            binding.id,
            parent["id"],
            list(request.prerequisite_assignment_ids),
            key,
        )
        return AssignmentExecution(
            id=assignment.id,
            parent_assignment_id=parent["id"],
            root_id=assignment.root_run_id,
            depth=assignment.depth,
            status=assignment.status,
        )

    def budget_owners(self, tender_id: str, root_id: str) -> int:
        """Count distinct budget owners under one root: always exactly one.

        Descendants inherit the reviewed root scope and spending allowance;
        they never mint a new budget. A count other than one means the root
        has no durable work and cannot admit children.
        """

        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT COUNT(DISTINCT root_run_id) FROM office_assignments WHERE tender_id=? AND root_run_id=?",
                (tender_id, root_id),
            ).fetchone()
        owners = int(row[0])
        if owners != 1:
            raise ValueError("Descendant work requires its parent work root.")
        return owners
