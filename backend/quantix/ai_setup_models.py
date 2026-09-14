"""Plain-language AI onboarding contracts; internal routes remain explicit."""

from typing import Literal

from pydantic import Field

from .ai_models import AIModel, ConnectionInput, ConnectionRecord, ModelRecord, SubscriptionUsage


class SetupMethod(AIModel):
    id: str
    title: str
    detail: str
    provider_id: str
    requires_key: bool
    requires_details: bool = False
    billing: Literal["metered", "subscription", "unknown"]
    access_kind: Literal["api_key", "subscription"] = "api_key"
    available: bool = True
    unavailable_reason: str | None = None
    docs_url: str


class SetupService(AIModel):
    id: str
    title: str
    detail: str
    featured: bool
    methods: list[SetupMethod]
    subscription_note: str | None = None
    subscription_docs_url: str | None = None


class SetupSoftware(AIModel):
    component_id: str
    state: Literal["missing", "preparing", "ready", "attention"]
    detail: str
    progress: int | None = None
    version: str | None = None


class SetupCheckPreview(AIModel):
    allowed: bool
    detail: str
    model_id: str | None
    maximum_cost_usd: float | None
    pricing_source: str | None = None
    max_requests: int | None = 2
    max_output_tokens: int | None = 1024
    requires_unknown_cost_consent: bool = False
    max_input_tokens: int | None = 16384
    limit_description: str | None = None
    subscription_check: bool = False
    fingerprint: str


class SetupCheckResult(AIModel):
    status: Literal["not_checked", "checking", "passed", "failed", "interrupted"]
    detail: str
    model_id: str | None = None
    checked_at: str | None = None
    estimated_cost_usd: float | None = None
    reserved_usd: float = 0


class SetupCheckCharge(AIModel):
    id: str
    attempt_id: str | None = None
    model_id: str
    created_at: str
    status: str
    requests: int
    input_tokens: int
    output_tokens: int
    estimated_cost_usd: float | None
    reserved_usd: float
    actual_model: str | None = None
    provider_reported_cost_usd: str | None = None
    provider_cost_is_partial: bool | None = None
    provider_usage_is_incomplete: bool | None = None
    cached_input_tokens: int | None = None
    max_requests: int | None = 2
    max_input_tokens: int | None = 16384
    max_output_tokens: int | None = 1024
    unknown_cost_consent: bool = False


class SetupAccount(AIModel):
    id: str
    service_id: str
    service_title: str
    method_id: str
    method_title: str
    connection: ConnectionRecord
    supported: bool = True
    access_kind: Literal["api_key", "subscription"] = "api_key"
    stage: Literal[
        "needs_preparation",
        "preparing",
        "needs_credentials",
        "needs_sign_in",
        "discovering",
        "choose_model",
        "ready_to_check",
        "checking",
        "ready",
        "attention",
        "cancelled",
    ]
    detail: str
    active: bool = False
    progress: int | None = None
    software: SetupSoftware
    models: list[ModelRecord]
    selected_model_id: str | None = None
    recommended_model_id: str | None = None
    recommendation: str
    login_url: str | None = None
    user_code: str | None = None
    check: SetupCheckResult
    subscription_usage: SubscriptionUsage | None = None


class SetupStart(AIModel):
    service_id: str
    method_id: str
    name: str | None = Field(default=None, max_length=150)
    connection: ConnectionInput | None = None


class SetupConfigure(AIModel):
    # The advanced editor uses the existing write-only credential contract.
    connection: ConnectionInput


class SetupAction(AIModel):
    action: Literal[
        "prepare",
        "repair",
        "cancel",
        "sign_in",
        "sign_out",
        "refresh",
        "refresh_usage",
        "set_subscription_extras",
        "check",
        "select_model",
        "rename",
        "remove_software",
    ]
    model_id: str | None = Field(default=None, max_length=300)
    name: str | None = Field(default=None, min_length=1, max_length=150)
    check_fingerprint: str | None = None
    maximum_cost_usd: float | None = Field(default=None, ge=0, allow_inf_nan=False)
    allow_provider_managed_extras: bool | None = Field(default=None, strict=True)
    sign_in_method: Literal["browser", "device_code"] | None = None
    accept_unknown_cost: bool = Field(default=False, strict=True)


class SimpleTenderAIInput(AIModel):
    account_id: str
    model_id: str
    account_revision: int | None = Field(default=None, ge=1)
    budget_usd: float | None = Field(default=None, gt=0, le=10000000, allow_inf_nan=False)
    restore_budget_reviewed: bool = False
    engineer_confirmed: Literal[True]


class ThinkingOption(AIModel):
    value: str | None
    label: str
    detail: str
    off: bool = False


class TenderThinking(AIModel):
    """How much the Tender Manager's AI thinks before answering."""

    connection_id: str | None
    model_id: str | None
    current: ThinkingOption | None
    options: list[ThinkingOption]
    busy: bool


class TenderThinkingInput(AIModel):
    reasoning: str | None = Field(default=None, min_length=1, max_length=40)
    engineer_confirmed: Literal[True]
