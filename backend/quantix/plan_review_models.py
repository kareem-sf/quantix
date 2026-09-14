"""Immutable, fingerprinted contracts for reviewing and approving a work plan."""

from typing import Any, Literal

from pydantic import Field, field_validator

from .ai_models import AIModel, AIRoute
from .ai_native_tools import ReviewedNativeTools
from .code_runtime_models import ReviewedCodeRuntime
from .staff_routing_models import (
    DelegationEnvelope,
    DelegationRouteOption,
    ToolCapability,
)

BlockerCode = Literal[
    "configuration",
    "sign_in",
    "model_check",
    "spending",
    "sources",
    "running_work",
    "plan",
    "connection",
]


class PlanReviewScope(AIModel):
    """The exact engineer scope used when the review was generated."""

    title: str
    plan_version: int
    tender_revision: int
    source_ids: list[str] = Field(default_factory=list)
    source_revision: int


class PlanReviewTask(AIModel):
    id: str
    title: str
    description: str
    role: str
    status: str
    source_ids: list[str] = Field(default_factory=list)
    route: AIRoute | None = None


class PlanReviewRoute(AIModel):
    """A displayed route and its immutable account/model evidence."""

    kind: Literal["manager", "specialist", "fallback"]
    task_id: str | None = None
    role: str | None = None
    route: AIRoute
    connection_revision: int
    model: dict[str, Any]
    readiness: Literal["ready", "missing", "unknown"]
    account_name: str
    provider: str
    data_destination: str
    billing: Literal["metered", "subscription", "unknown"]
    provider_managed_extras: bool = False
    spending_detail: str


class PlanReviewChange(AIModel):
    code: str = Field(min_length=1, max_length=80)
    detail: str = Field(min_length=1, max_length=1200)
    before: Any = None
    after: Any = None


class PlanReviewBlocker(AIModel):
    code: BlockerCode
    detail: str = Field(min_length=1, max_length=1200)
    repair_target: str = Field(min_length=1, max_length=500)
    connection_id: str | None = None
    model_id: str | None = None
    task_id: str | None = None


class PlanReviewSnapshot(AIModel):
    """All mutable authority inputs bound by the approval fingerprint."""

    tender_revision: int
    source_revision: int
    policy_revision: int
    allowed_connection_ids: list[str] = Field(default_factory=list)
    connection_revisions: dict[str, int] = Field(default_factory=dict)
    model_revisions: dict[str, str] = Field(default_factory=dict)
    route_intents: list[dict[str, Any]] = Field(default_factory=list)
    run_budget_usd: float | None = None
    tender_budget_usd: float | None = None
    max_requests: int
    spent_usd: float
    reserved_usd: float
    usage_fingerprint: str
    provider_managed_extras: dict[str, int] = Field(default_factory=dict, description="Current displayed Grok account revisions whose enabled extras are proposed for explicit approval.")
    allowed_destinations: list[dict[str, Any]] = Field(default_factory=list)
    restore_reconciliation_required: bool
    spend_history_may_be_incomplete: bool
    active_run_ids: list[str] = Field(default_factory=list)


class DelegationArtifactOption(AIModel):
    """A current source artifact available to the delegation editor."""

    artifact_id: str = Field(min_length=1, max_length=160)
    name: str = Field(min_length=1, max_length=300)
    display_name: str = Field(min_length=1, max_length=300)
    relative_path: str = Field(min_length=1, max_length=2000)
    version: int = Field(ge=1)
    content_hash: str = Field(pattern=r"^[0-9a-fA-F]{64}$")
    status: str = Field(min_length=1, max_length=80)


class DelegationProposalEdit(AIModel):
    """Engineer-controlled delegation selections, without authority fields."""

    expected_version: int = Field(ge=0)
    source_scope: Literal["reviewed_tender", "selected_sources"]
    artifact_ids: list[str] = Field(default_factory=list, max_length=500)
    tool_ids: list[str] = Field(default_factory=list, max_length=100)
    native_tools: dict[str, ReviewedNativeTools] = Field(default_factory=dict, max_length=20)
    code_runtimes: list[ReviewedCodeRuntime] = Field(default_factory=list, max_length=2)
    allowed_draft_outputs: list[str] = Field(min_length=1, max_length=30)
    route_option_ids: list[str] = Field(min_length=1, max_length=20)
    max_staff: int = Field(ge=1, le=100)
    max_assignments: int = Field(ge=1, le=1000)
    max_depth: int = Field(default=1, ge=1, le=8)
    max_concurrency: int = Field(default=1, ge=1, le=8)
    max_requests: int = Field(ge=1, le=10000)
    max_search_calls: int = Field(ge=0, le=10000)

    @field_validator("artifact_ids", "tool_ids", "allowed_draft_outputs", "route_option_ids")
    @classmethod
    def unique_selection(cls, value: list[str]) -> list[str]:
        if len(set(value)) != len(value):
            raise ValueError("Delegation selections cannot repeat an item.")
        return value


class DelegationProposal(AIModel):
    """Persisted plan-scoped selection with a compare-and-swap version."""

    tender_id: str = Field(min_length=1, max_length=160)
    plan_id: str = Field(min_length=1, max_length=160)
    version: int = Field(ge=1)
    source_scope: Literal["reviewed_tender", "selected_sources"]
    artifact_ids: list[str] = Field(default_factory=list, max_length=500)
    tool_ids: list[str] = Field(default_factory=list, max_length=100)
    native_tools: dict[str, ReviewedNativeTools] = Field(default_factory=dict, max_length=20)
    code_runtimes: list[ReviewedCodeRuntime] = Field(default_factory=list, max_length=2)
    allowed_draft_outputs: list[str] = Field(min_length=1, max_length=30)
    route_option_ids: list[str] = Field(min_length=1, max_length=20)
    max_staff: int = Field(ge=1, le=100)
    max_assignments: int = Field(ge=1, le=1000)
    max_depth: int = Field(default=1, ge=1, le=8)
    max_concurrency: int = Field(default=1, ge=1, le=8)
    max_requests: int = Field(ge=1, le=10000)
    max_search_calls: int = Field(ge=0, le=10000)
    updated_at: str = Field(min_length=1, max_length=120)

    @field_validator("artifact_ids", "tool_ids", "allowed_draft_outputs", "route_option_ids")
    @classmethod
    def unique_selection(cls, value: list[str]) -> list[str]:
        if len(set(value)) != len(value):
            raise ValueError("Delegation selections cannot repeat an item.")
        return value


class DelegationOptions(AIModel):
    """Bounded current choices and the saved selections for a future editor."""

    artifacts: list[DelegationArtifactOption] = Field(default_factory=list, max_length=50)
    tools: list[ToolCapability] = Field(default_factory=list, max_length=100)
    code_runtimes: list[ReviewedCodeRuntime] = Field(default_factory=list, max_length=2)
    route_options: list[DelegationRouteOption] = Field(default_factory=list, max_length=20)
    supported_draft_outputs: list[str] = Field(default_factory=list, max_length=30)
    selected: DelegationProposal | None = None


class DelegationArtifactOptionPage(AIModel):
    """A bounded source-choice page for large Tender artifact catalogs."""

    items: list[DelegationArtifactOption] = Field(default_factory=list, max_length=100)
    next_offset: int | None = Field(default=None, ge=0)
    total: int = Field(ge=0)


class PlanReview(AIModel):
    tender_id: str
    plan_id: str
    plan_version: int
    plan_status: Literal["proposed", "approved", "superseded"] = "proposed"
    scope: PlanReviewScope
    tasks: list[PlanReviewTask]
    routes: list[PlanReviewRoute]
    delegation: DelegationEnvelope | None = None
    delegation_proposal_version: int = Field(default=0, ge=0)
    delegation_options: DelegationOptions | None = None
    ai_summary: str = Field(min_length=1, max_length=2000)
    meaningful_changes: list[PlanReviewChange] = Field(default_factory=list)
    blockers: list[PlanReviewBlocker] = Field(default_factory=list)
    snapshot: PlanReviewSnapshot
    fingerprint: str = Field(pattern=r"^[0-9a-f]{64}$")
    can_approve: bool
    reviewed_at: str


class PlanReviewApproval(AIModel):
    fingerprint: str = Field(pattern=r"^[0-9a-f]{64}$")
    engineer_confirmed: Literal[True]
    rationale: str = Field(default="Engineer approved the displayed work plan and AI team.", max_length=4000)

    @field_validator("rationale")
    @classmethod
    def default_blank_rationale(cls, value: str) -> str:
        return value.strip() or "Engineer approved the displayed work plan and AI team."


class WorkIntent(AIModel):
    id: str
    run_id: str
    tender_id: str
    plan_id: str
    task_id: str | None = None
    kind: Literal["task", "manager"]
    instruction: str
    status: Literal["queued"]
    created_at: str


class PlanApprovalResult(AIModel):
    plan: dict[str, Any]
    review: PlanReview
    work_intents: list[WorkIntent]
