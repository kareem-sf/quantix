"""Typed, immutable records for reviewed dynamic-staff authority."""

from __future__ import annotations

import hashlib
import json
from typing import Any, Literal

from pydantic import (
    BaseModel,
    ConfigDict,
    Field,
    field_validator,
    model_serializer,
    model_validator,
)

from .ai_models import AIRoute
from .ai_native_tools import ReviewedNativeTools
from .code_runtime_models import ReviewedCodeRuntime


class RoutingModel(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True, str_strip_whitespace=True)

    @model_serializer(mode="wrap")
    def authority_shape(self, handler):
        value = handler(self)
        # New empty scopes confer nothing and preserve historical fingerprints.
        for field in ("native_tools", "code_runtimes"):
            if not value.get(field):
                value.pop(field, None)
        return value


class ArtifactBasis(RoutingModel):
    """The exact current artifact version admitted to a reviewed source scope."""

    artifact_id: str = Field(min_length=1, max_length=160)
    version: int = Field(ge=1)
    content_hash: str = Field(pattern=r"^[0-9a-fA-F]{64}$")

    @field_validator("content_hash")
    @classmethod
    def lower_hash(cls, value: str) -> str:
        return value.lower()


class ToolCapability(RoutingModel):
    """A closed, versioned product operation available to staff."""

    id: str = Field(min_length=1, max_length=500)
    version: int = Field(ge=1)
    description: str = Field(min_length=1, max_length=2000)
    read_only: bool


class DelegationRouteOption(RoutingModel):
    """One exact route alternative displayed during plan review."""

    id: str = Field(min_length=1, max_length=160)
    route: AIRoute
    connection_revision: int = Field(ge=1)
    model_revision: str = Field(pattern=r"^[0-9a-fA-F]{64}$")
    model: dict[str, Any]
    account_name: str = Field(min_length=1, max_length=150)
    provider: str = Field(min_length=1, max_length=100)
    data_destination: str = Field(min_length=1, max_length=2000)
    billing: Literal["metered", "subscription", "unknown", "local"]
    provider_managed_extras: bool = False
    readiness: Literal["ready", "missing", "unknown"] = "ready"

    @field_validator("model_revision")
    @classmethod
    def lower_model_revision(cls, value: str) -> str:
        return value.lower()

    @field_validator("model")
    @classmethod
    def public_model(cls, value: dict[str, Any]) -> dict[str, Any]:
        _assert_public(value)
        return value


_DRAFT_OUTPUT_KINDS = frozenset(
    {
        # Existing OfficeOutput field names.
        "summary",
        "findings",
        "plan",
        "web_findings",
        "price_proposals",
        "quote_drafts",
        "unit_rate_proposals",
        "project_map_nodes",
        "submission_requirements",
        "programme_proposal",
        "drawing_measurements",
        "draft_documents",
        "quantity_proposals",
        "boq_item_proposals",
    }
)


class DelegationEnvelope(RoutingModel):
    """Explicit scope and aggregate limits acquired by one plan approval."""

    version: Literal[1] = 1
    purpose: str = Field(min_length=1, max_length=4000)
    source_scope: Literal["reviewed_tender", "selected_sources"]
    artifacts: list[ArtifactBasis] = Field(default_factory=list, max_length=500)
    tools: list[ToolCapability] = Field(default_factory=list, max_length=100)
    native_tools: dict[str, ReviewedNativeTools] = Field(default_factory=dict, max_length=20)
    code_runtimes: list[ReviewedCodeRuntime] = Field(default_factory=list, max_length=2)
    allowed_draft_outputs: list[str] = Field(min_length=1, max_length=30)
    route_options: list[DelegationRouteOption] = Field(min_length=1, max_length=20)
    max_staff: int = Field(ge=1, le=100)
    max_assignments: int = Field(ge=1, le=1000)
    max_depth: int = Field(ge=1, le=8)
    max_concurrency: int = Field(ge=1, le=8)
    max_requests: int = Field(ge=1, le=10000)
    max_search_calls: int = Field(ge=0, le=10000)
    run_budget_usd: float | None = Field(default=None, gt=0, le=1000000, allow_inf_nan=False)
    tender_budget_usd: float | None = Field(default=None, gt=0, le=10000000, allow_inf_nan=False)

    @field_validator("allowed_draft_outputs")
    @classmethod
    def validate_output_kinds(cls, value: list[str]) -> list[str]:
        if len(set(value)) != len(value):
            raise ValueError("A delegation cannot repeat an allowed draft output kind.")
        unknown = [item for item in value if item not in _DRAFT_OUTPUT_KINDS]
        if unknown:
            raise ValueError(f"The draft output kind '{unknown[0]}' is unavailable.")
        forbidden = {"send", "send_email", "approve", "approval", "publish"}
        if forbidden.intersection(value):
            raise ValueError("Delegated staff cannot send externally or approve Tender records.")
        return value

    @model_validator(mode="after")
    def unique_scope_records(self):
        if len({item.artifact_id for item in self.artifacts}) != len(self.artifacts):
            raise ValueError("A delegation cannot repeat a source artifact.")
        if len({item.id for item in self.tools}) != len(self.tools):
            raise ValueError("A delegation cannot repeat a tool capability.")
        if len({item.id for item in self.route_options}) != len(self.route_options):
            raise ValueError("A delegation cannot repeat a route option.")
        return self


class DelegationGrant(RoutingModel):
    """Immutable main-database evidence of one explicit delegation approval."""

    id: str = Field(min_length=1, max_length=160)
    tender_id: str = Field(min_length=1, max_length=160)
    plan_id: str = Field(min_length=1, max_length=160)
    review_fingerprint: str = Field(pattern=r"^[0-9a-fA-F]{64}$")
    policy_revision: int = Field(ge=0)
    envelope: DelegationEnvelope
    created_at: str = Field(min_length=1, max_length=120)

    @field_validator("review_fingerprint")
    @classmethod
    def lower_review_fingerprint(cls, value: str) -> str:
        return value.lower()


class RouteBinding(RoutingModel):
    """Immutable route identity bound to a generated profile and work order."""

    id: str = Field(min_length=1, max_length=160)
    tender_id: str = Field(min_length=1, max_length=160)
    grant_id: str = Field(min_length=1, max_length=160)
    plan_id: str = Field(min_length=1, max_length=160)
    root_run_id: str = Field(min_length=1, max_length=160)
    staff_id: str = Field(min_length=1, max_length=160)
    staff_version: int = Field(ge=1)
    definition_id: str | None = Field(default=None, min_length=1, max_length=160)
    definition_version: int | None = Field(default=None, ge=1)
    work_order_id: str = Field(min_length=1, max_length=160)
    route_option_id: str = Field(min_length=1, max_length=160)
    route: AIRoute
    connection_revision: int = Field(ge=1)
    model_revision: str = Field(pattern=r"^[0-9a-fA-F]{64}$")
    artifacts: list[ArtifactBasis] = Field(default_factory=list, max_length=500)
    tools: list[ToolCapability] = Field(default_factory=list, max_length=100)
    code_runtimes: list[ReviewedCodeRuntime] = Field(default_factory=list, max_length=2)
    allowed_draft_outputs: list[str] = Field(min_length=1, max_length=30)
    created_at: str = Field(min_length=1, max_length=120)

    @model_validator(mode="after")
    def complete_definition_reference(self):
        if (self.definition_id is None) != (self.definition_version is None):
            raise ValueError(
                "A route binding definition identity and version must be supplied together."
            )
        return self


def canonical_json(value: Any) -> str:
    """Encode authority data deterministically for fingerprints and receipts."""

    def jsonable(item):
        if isinstance(item, BaseModel):
            return jsonable(item.model_dump(mode="json"))
        if isinstance(item, dict):
            return {key: jsonable(child) for key, child in item.items()}
        if isinstance(item, list):
            return [jsonable(child) for child in item]
        if isinstance(item, tuple):
            return [jsonable(child) for child in item]
        return item

    return json.dumps(
        jsonable(value), ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False
    )


def _stable(value: Any) -> Any:
    if isinstance(value, dict):
        return {
            key: _stable(item) for key, item in value.items() if key not in {"updated_at", "as_of"}
        }
    if isinstance(value, list):
        return [_stable(item) for item in value]
    return value


def model_revision_fingerprint(model: dict[str, Any] | BaseModel) -> str:
    """Return the same timestamp-insensitive model identity used by plan review."""

    payload = model.model_dump(mode="json") if isinstance(model, BaseModel) else model
    return hashlib.sha256(canonical_json(_stable(payload)).encode("utf-8")).hexdigest()


def route_option_id(option: DelegationRouteOption | dict[str, Any]) -> str:
    """Hash an option's route and public authority/model basis, excluding its ID."""

    payload = option.model_dump(mode="json") if isinstance(option, BaseModel) else dict(option)
    payload.pop("id", None)
    # Readiness is a checked observation revalidated at dispatch, rather than
    # part of the route/account/model identity hash.
    payload.pop("readiness", None)
    if isinstance(payload.get("model"), dict):
        payload["model"] = _stable(payload["model"])
    return hashlib.sha256(canonical_json(payload).encode("utf-8")).hexdigest()


def envelope_fingerprint(envelope: DelegationEnvelope | dict[str, Any]) -> str:
    """Fingerprint exact envelope contents for the plan-review receipt check."""

    parsed = (
        envelope
        if isinstance(envelope, DelegationEnvelope)
        else DelegationEnvelope.model_validate(envelope)
    )
    return hashlib.sha256(canonical_json(parsed).encode("utf-8")).hexdigest()


def _assert_public(value: Any, path: str = "model") -> None:
    """Reject secret-shaped fields in public route/model snapshots."""

    if isinstance(value, dict):
        for key, child in value.items():
            lowered = str(key).casefold()
            sensitive = lowered in {
                "token",
                "access_token",
                "refresh_token",
                "api_key",
                "password",
            } or any(part in lowered for part in ("secret", "credential"))
            if sensitive:
                raise ValueError(f"The public {path} snapshot cannot contain credentials.")
            _assert_public(child, f"{path}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            _assert_public(child, f"{path}[{index}]")


__all__ = [
    "ArtifactBasis",
    "DelegationEnvelope",
    "DelegationGrant",
    "DelegationRouteOption",
    "RouteBinding",
    "ToolCapability",
    "canonical_json",
    "envelope_fingerprint",
    "model_revision_fingerprint",
    "route_option_id",
]
