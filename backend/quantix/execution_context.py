"""Server-only execution identity. Never a public HTTP/Pydantic request model."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

ActorKind = Literal["engineer", "manager", "staff", "worker"]


@dataclass(frozen=True)
class OfficeExecutionIdentity:
    tender_id: str | None
    actor_kind: ActorKind
    actor_id: str
    root_run_id: str | None
    budget_scope_id: str | None
    assignment_id: str | None
    profile_version: int | None
    route_binding_id: str | None
    instruction_revision_id: str | None
    grant_fingerprint: str | None
    ownership_epoch: int | None
    trusted_invocation_id: str | None


def identity_from_office_context(context, *, invocation_id: str | None = None) -> OfficeExecutionIdentity:
    """Build identity from the live controller context. Payload fields cannot supply it."""

    is_staff = bool(getattr(context, "is_staff", False))
    actor_id = getattr(context, "actor_id", None) or getattr(context, "staff_id", None)
    if not actor_id:
        actor_id = "manager"
    binding = getattr(context, "route_binding", None)
    grant = getattr(binding, "grant_id", None) if binding is not None else None
    return OfficeExecutionIdentity(
        tender_id=getattr(context, "tender_id", None),
        actor_kind="staff" if is_staff else "manager",
        actor_id=str(actor_id),
        root_run_id=getattr(context, "run_id", None),
        budget_scope_id=getattr(context, "run_id", None),
        assignment_id=getattr(context, "assignment_id", None),
        profile_version=getattr(context, "staff_version", None),
        route_binding_id=getattr(context, "route_binding_id", None),
        instruction_revision_id=None,
        grant_fingerprint=str(grant) if grant else None,
        ownership_epoch=None,
        trusted_invocation_id=invocation_id,
    )


def engineer_identity(tender_id: str, **overrides) -> OfficeExecutionIdentity:
    """Build the server-only engineer identity used by authenticated office routes."""

    fields = {
        "tender_id": tender_id,
        "actor_kind": "engineer",
        "actor_id": "engineer",
        "root_run_id": None,
        "budget_scope_id": None,
        "assignment_id": None,
        "profile_version": None,
        "route_binding_id": None,
        "instruction_revision_id": None,
        "grant_fingerprint": None,
        "ownership_epoch": None,
        "trusted_invocation_id": None,
    }
    fields.update(overrides)
    return OfficeExecutionIdentity(**fields)
