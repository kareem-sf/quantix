"""Manager-only tools for creating and reusing generated Tender staff.

These tools persist planning records.  They do not execute staff, grant tools,
read Tender sources or select an AI route.  The active Manager controller owns
the repository and run lineage supplied to this module.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass

from .agent_definition_models import AgentDefinitionCreate
from .agent_definitions import AgentDefinitionService
from .ai_generation_models import GenerationSettings
from .ai_tools import ToolContext, tool
from .manager_runtime import ManagerRunProfiles
from .office_events import OfficeEventService
from .staff_models import ManagerCreationContext, StaffProfileDraft, StaffWorkOrder
from .staff_store import StaffStore
from .staff_store_models import (
    StaffCreationReceipt,
    StaffProfileRecord,
    StaffRevisionReceipt,
    StaffWorkOrderReceipt,
)

_LIST_LIMIT = 50
_WORK_ORDER_SUMMARY_LIMIT = 20


@dataclass(frozen=True)
class _ManagerGenerationBoundary:
    repo: object
    tender_id: str
    run_id: str
    context: ManagerCreationContext
    manager_id: str
    store: StaffStore
    definitions: AgentDefinitionService
    events: OfficeEventService

    def validate(self, runtime_context) -> None:
        """Require the exact controller repository and Manager run lineage."""

        if runtime_context is None:
            raise ValueError("The Manager tool context is missing.")
        if getattr(runtime_context, "repo", None) is not self.repo:
            raise ValueError("The Manager tool context does not match its active repository.")
        if getattr(runtime_context, "tender_id", None) != self.tender_id:
            raise ValueError("The Manager tool context does not match its active Tender.")
        if getattr(runtime_context, "run_id", None) != self.run_id:
            raise ValueError("The Manager tool context does not match its active run.")
        actor_id = getattr(runtime_context, "actor_id", None)
        if actor_id is not None and actor_id != self.manager_id:
            raise ValueError("Only the active Tender Manager can plan staff work.")


def _idempotency_key(operation: str, invocation_id: str) -> str:
    """Derive a bounded operation key only from trusted invocation identity."""

    digest = hashlib.sha256(f"{operation}\x00{invocation_id}".encode("utf-8")).hexdigest()
    return f"staff-{digest}"


def _json(value) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def _capability_status(profile: StaffProfileRecord | StaffProfileDraft, repo=None, tender_id=None) -> dict:
    """Describe requests without turning profile text into execution authority."""

    # Resolve the installed source catalog at call time.  This keeps status
    # factual if the bundled tool set changes and avoids a role/title map.
    from .office_tools import source_tools

    known_tools = {definition.name for definition in source_tools()}
    tools = []
    for identifier in profile.requested_tool_ids:
        if identifier in known_tools:
            status = "Needs an approved work scope"
        else:
            status = "Unavailable"
        tools.append({"id": identifier, "status": status})
    return {
        "tools": tools,
        "has_unavailable": any(item["status"] == "Unavailable" for item in tools),
        "needs_approved_work_scope": any(
            item["status"] == "Needs an approved work scope" for item in tools
        ),
    }


def _work_order_summary(order) -> dict:
    return {
        "id": order.id,
        "staff_id": order.staff_id,
        "staff_version": order.staff_version,
        "scope_id": order.scope_id,
        "creator_run_id": order.creator_run_id,
        "brief": order.work_order.brief,
        "created_at": order.created_at,
    }


def _staff_item(boundary: _ManagerGenerationBoundary, profile: StaffProfileRecord) -> dict:
    orders = boundary.store.list_work_orders(
        boundary.tender_id,
        profile.id,
        limit=_WORK_ORDER_SUMMARY_LIMIT + 1,
    )
    partial = len(orders) > _WORK_ORDER_SUMMARY_LIMIT
    return {
        "staff": profile.model_dump(mode="json"),
        "work_orders": [_work_order_summary(order) for order in orders[:_WORK_ORDER_SUMMARY_LIMIT]],
        "work_orders_partial": partial,
    }


def _creation_result(receipt: StaffCreationReceipt, capabilities: dict, manager_id: str) -> str:
    return _json(
        {
            "staff": receipt.staff.model_dump(mode="json"),
            "work_order": receipt.work_order.model_dump(mode="json"),
            "lifecycle": receipt.staff.lifecycle,
            "requested_capabilities": capabilities,
            "manager_id": manager_id,
            "replayed": receipt.replayed,
        }
    )


def _revision_result(receipt: StaffRevisionReceipt, manager_id: str, capabilities: dict) -> str:
    return _json(
        {
            "staff": receipt.staff.model_dump(mode="json"),
            "lifecycle": receipt.staff.lifecycle,
            "requested_capabilities": capabilities,
            "manager_id": manager_id,
            "replayed": receipt.replayed,
        }
    )


def _work_order_result(receipt: StaffWorkOrderReceipt) -> str:
    return _json(
        {
            "work_order": receipt.work_order.model_dump(mode="json"),
            "lifecycle": "available",
            "replayed": receipt.replayed,
        }
    )


def staff_generation_tools(repo, tender_id: str, run_id: str, scope_id: str) -> list:
    """Build the Manager's generated-staff tools for one trusted run.

    The Manager profile is read once here.  The returned definitions never
    accept sender, profile-version, run, scope, timestamp or grant arguments
    from the model.
    """

    manager_profile = ManagerRunProfiles(repo).get(tender_id, run_id)
    boundary = _ManagerGenerationBoundary(
        repo=repo,
        tender_id=tender_id,
        run_id=run_id,
        context=ManagerCreationContext(
            tender_id=tender_id,
            run_id=run_id,
            manager_profile_version=manager_profile.version,
            scope_id=scope_id,
        ),
        manager_id=manager_profile.id,
        store=StaffStore(repo),
        definitions=AgentDefinitionService(repo),
        events=OfficeEventService(repo),
    )

    @tool(read_only=False, idempotent=True, requires_invocation_id=True)
    async def create_staff(
        ctx: ToolContext[object], profile: StaffProfileDraft, work_order: StaffWorkOrder
    ) -> str:
        """Create one complete generated colleague and its planned work order."""

        boundary.validate(ctx.context)
        # ToolDefinition passes nested DTOs as dictionaries after validation;
        # validate them explicitly before handing them to the persistence API.
        prepared_profile = StaffProfileDraft.model_validate(profile)
        prepared_order = StaffWorkOrder.model_validate(work_order)
        key = _idempotency_key("create_staff", ctx.invocation_id)
        with repo.atomic():
            receipt = boundary.store.create_generated(
                boundary.context, prepared_profile, prepared_order, key
            )
            if not receipt.replayed:
                boundary.events.append(
                    tender_id,
                    "staff_created",
                    actor_id=boundary.manager_id,
                    record_ref={
                        "kind": "staff",
                        "id": receipt.staff.id,
                        "version": receipt.staff.version,
                    },
                    payload={"version": receipt.staff.version, "work_order_id": receipt.work_order.id},
                    idempotency_key=key,
                )
        return _creation_result(receipt, _capability_status(receipt.staff, repo, tender_id), boundary.manager_id)

    @tool(read_only=False, idempotent=True, requires_invocation_id=True)
    async def revise_staff(
        ctx: ToolContext[object], staff_id: str, expected_version: int, profile: StaffProfileDraft
    ) -> str:
        """Save a new immutable profile version without changing existing work orders."""

        boundary.validate(ctx.context)
        prepared_profile = StaffProfileDraft.model_validate(profile)
        key = _idempotency_key("revise_staff", ctx.invocation_id)
        with repo.atomic():
            receipt = boundary.store.revise_generated(
                boundary.context, staff_id, expected_version, prepared_profile, key
            )
            if not receipt.replayed:
                boundary.events.append(
                    tender_id,
                    "profile_updated",
                    actor_id=boundary.manager_id,
                    record_ref={
                        "kind": "staff",
                        "id": receipt.staff.id,
                        "version": receipt.staff.version,
                    },
                    payload={"version": receipt.staff.version},
                    idempotency_key=key,
                )
        return _revision_result(receipt, boundary.manager_id, _capability_status(receipt.staff, repo, tender_id))

    @tool
    async def list_office_staff(
        ctx: ToolContext[object], offset: int = 0, limit: int = 20
    ) -> str:
        """List saved staff in this Tender with bounded work-order summaries."""

        boundary.validate(ctx.context)
        if type(offset) is not int or offset < 0:
            raise ValueError("The staff offset must be nonnegative.")
        if type(limit) is not int or not 1 <= limit <= _LIST_LIMIT:
            raise ValueError("The staff limit must be between 1 and 50.")
        profiles = boundary.store.list_staff(tender_id)
        selected = profiles[offset : offset + limit]
        return _json(
            {
                "items": [_staff_item(boundary, profile) for profile in selected],
                "offset": offset,
                "limit": limit,
                "has_more": offset + len(selected) < len(profiles),
                "partial": offset > 0 or offset + len(selected) < len(profiles),
                "total": len(profiles),
            }
        )

    @tool
    async def read_staff(ctx: ToolContext[object], staff_id: str) -> str:
        """Read one saved staff profile and its bounded work-order history."""

        boundary.validate(ctx.context)
        profile = boundary.store.get_staff(tender_id, staff_id)
        return _json(_staff_item(boundary, profile))

    @tool(read_only=False, idempotent=True, requires_invocation_id=True)
    async def plan_staff_work(
        ctx: ToolContext[object],
        staff_id: str,
        expected_version: int,
        work_order: StaffWorkOrder,
    ) -> str:
        """Create another planned order fixed to the current profile version."""

        boundary.validate(ctx.context)
        prepared_order = StaffWorkOrder.model_validate(work_order)
        key = _idempotency_key("plan_staff_work", ctx.invocation_id)
        with repo.atomic():
            receipt = boundary.store.create_work_order(
                boundary.context, staff_id, expected_version, prepared_order, key
            )
            if not receipt.replayed:
                boundary.events.append(
                    tender_id,
                    "scope_changed",
                    actor_id=boundary.manager_id,
                    record_ref={
                        "kind": "work_order",
                        "id": receipt.work_order.id,
                        "version": receipt.work_order.staff_version,
                    },
                    payload={
                        "work_order_id": receipt.work_order.id,
                        "staff_id": receipt.work_order.staff_id,
                        "staff_version": receipt.work_order.staff_version,
                    },
                    idempotency_key=key,
                )
        return _work_order_result(receipt)

    @tool(read_only=False, idempotent=True, requires_invocation_id=True)
    async def save_agent_definition(
        ctx: ToolContext[object],
        profile: StaffProfileDraft,
        generation_settings: GenerationSettings,
    ) -> str:
        """Save a complete reusable professional definition from the engineer's brief."""

        boundary.validate(ctx.context)
        prepared_profile = StaffProfileDraft.model_validate(profile)
        prepared_settings = GenerationSettings.model_validate(generation_settings)
        record = boundary.definitions.create(
            AgentDefinitionCreate(
                profile=prepared_profile,
                generation_settings=prepared_settings,
                idempotency_key=_idempotency_key(
                    "save_agent_definition", ctx.invocation_id
                ),
            )
        )
        return _json({"definition": record.model_dump(mode="json")})

    @tool
    async def list_agent_definitions(
        ctx: ToolContext[object], offset: int = 0, limit: int = 20
    ) -> str:
        """List active reusable professional definitions before choosing a colleague."""

        boundary.validate(ctx.context)
        if type(offset) is not int or offset < 0:
            raise ValueError("The definition offset must be nonnegative.")
        if type(limit) is not int or not 1 <= limit <= _LIST_LIMIT:
            raise ValueError("The definition limit must be between 1 and 50.")
        records = boundary.definitions.list()
        selected = records[offset : offset + limit]
        return _json(
            {
                "items": [record.model_dump(mode="json") for record in selected],
                "offset": offset,
                "limit": limit,
                "has_more": offset + len(selected) < len(records),
                "total": len(records),
            }
        )

    @tool
    async def read_agent_definition(
        ctx: ToolContext[object], definition_id: str, version: int | None = None
    ) -> str:
        """Read one exact reusable professional definition and its generation preferences."""

        boundary.validate(ctx.context)
        return _json(
            boundary.definitions.get(definition_id, version).model_dump(mode="json")
        )

    @tool(read_only=False, idempotent=True, requires_invocation_id=True)
    async def create_staff_from_definition(
        ctx: ToolContext[object],
        definition_id: str,
        definition_version: int,
        work_order: StaffWorkOrder,
    ) -> str:
        """Reuse one exact definition as planned Tender staff with no execution grant."""

        boundary.validate(ctx.context)
        prepared_order = StaffWorkOrder.model_validate(work_order)
        key = _idempotency_key("create_staff_from_definition", ctx.invocation_id)
        with repo.atomic():
            receipt = boundary.store.create_from_definition(
                boundary.context,
                definition_id,
                definition_version,
                prepared_order,
                key,
            )
            if not receipt.replayed:
                boundary.events.append(
                    tender_id,
                    "staff_created",
                    actor_id=boundary.manager_id,
                    record_ref={
                        "kind": "staff",
                        "id": receipt.staff.id,
                        "version": receipt.staff.version,
                    },
                    payload={
                        "version": receipt.staff.version,
                        "work_order_id": receipt.work_order.id,
                        "definition_id": receipt.staff.definition_id,
                        "definition_version": receipt.staff.definition_version,
                    },
                    idempotency_key=key,
                )
        return _creation_result(
            receipt,
            _capability_status(receipt.staff, repo, tender_id),
            boundary.manager_id,
        )

    return [
        create_staff,
        revise_staff,
        list_office_staff,
        read_staff,
        plan_staff_work,
        save_agent_definition,
        list_agent_definitions,
        read_agent_definition,
        create_staff_from_definition,
    ]


__all__ = ["staff_generation_tools"]
