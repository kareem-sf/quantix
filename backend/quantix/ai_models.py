"""Public contracts for user-owned AI routes, capabilities and work authority."""

from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, SecretStr, model_serializer, model_validator

from .ai_generation_models import GenerationSettings

Protocol = Literal["openai_responses", "openai_chat", "anthropic", "google", "bedrock", "mistral", "cohere", "codex", "copilot", "gemini_cli", "grok_build", "claude_agent", "claude_code"]
Authentication = Literal["api_key", "environment", "azure_identity", "aws_identity", "google_identity", "client_login", "none"]
Billing = Literal["metered", "subscription", "local", "unknown"]


class AIModel(BaseModel):
    model_config = ConfigDict(extra="forbid", str_strip_whitespace=True)


class PriceCard(AIModel):
    input_per_million: float = Field(ge=0, allow_inf_nan=False)
    output_per_million: float = Field(ge=0, allow_inf_nan=False)
    cached_input_per_million: float | None = Field(default=None, ge=0, allow_inf_nan=False)
    source: str = Field(min_length=1, max_length=1000)
    as_of: str = Field(min_length=1, max_length=50)
    web_search_per_call: float | None = Field(default=None, ge=0, allow_inf_nan=False)
    code_execution_per_session: float | None = Field(default=None, gt=0, allow_inf_nan=False)
    code_execution_source: str | None = Field(default=None, max_length=2000)
    code_execution_as_of: str | None = Field(default=None, max_length=50)

    @model_validator(mode="after")
    def documented_code_price(self):
        values = (self.code_execution_per_session, self.code_execution_source, self.code_execution_as_of)
        if any(value is not None for value in values):
            from .ai_native_tools import NativeCodePrice
            NativeCodePrice(per_session_usd=values[0], source=values[1], as_of=values[2])
        return self

    @model_serializer(mode="wrap")
    def preserve_saved_price_identity(self, handler):
        data = handler(self)
        for key in ("code_execution_per_session", "code_execution_source", "code_execution_as_of"):
            if data.get(key) is None:
                data.pop(key, None)
        return data


class ModelCapabilities(AIModel):
    streaming: bool | None = None
    temperature: bool | None = None
    top_p: bool | None = None
    web_fetch: bool | None = None
    code_execution: bool | None = None
    tools: bool | None = None
    structured_output: bool | None = None
    images: bool | None = None
    pdf: bool | None = None
    web_search: bool | None = None
    reasoning: list[str] = Field(default_factory=list)
    context_window: int | None = Field(default=None, ge=1)
    max_output_tokens: int | None = Field(default=None, ge=1)

    @model_serializer(mode="wrap")
    def preserve_saved_capability_identity(self, handler):
        data = handler(self)
        for key in ("temperature", "top_p", "web_fetch", "code_execution", "streaming"):
            if data.get(key) is None:
                data.pop(key, None)
        return data


class ProviderPreset(AIModel):
    id: str
    name: str
    kind: Literal["api", "runtime"]
    protocols: list[Protocol]
    default_protocol: Protocol
    base_url: str | None = None
    auth_methods: list[Authentication]
    billing: Billing
    docs_url: str
    notes: list[str] = Field(default_factory=list)
    settings_fields: list[str] = Field(default_factory=list)


class ConnectionInput(AIModel):
    name: str = Field(min_length=1, max_length=150)
    provider_id: str = Field(min_length=1, max_length=100)
    protocol: Protocol
    base_url: str | None = Field(default=None, max_length=2000)
    auth_type: Authentication = "api_key"
    billing: Billing = "metered"
    enabled: bool = True
    environment_key: str | None = Field(default=None, pattern=r"^[A-Za-z_][A-Za-z0-9_]*$", max_length=150)
    settings: dict[str, Any] = Field(default_factory=dict)
    credentials: dict[str, SecretStr] | None = None
    session_only: bool = False
    allow_insecure_http: bool = False


class ConnectionRecord(AIModel):
    id: str
    name: str
    provider_id: str
    protocol: Protocol
    base_url: str | None
    auth_type: Authentication
    billing: Billing
    enabled: bool
    environment_key: str | None
    settings: dict[str, Any]
    revision: int
    created_at: str
    updated_at: str
    credential_state: Literal["stored", "session", "environment", "runtime", "missing", "not_required"]
    status: Literal["configured", "models_discovered", "used", "needs_attention"]
    last_error: str | None = None
    allow_insecure_http: bool = False


class ModelInput(AIModel):
    model_id: str = Field(min_length=1, max_length=300)
    display_name: str = Field(min_length=1, max_length=300)
    capabilities: ModelCapabilities = Field(default_factory=ModelCapabilities)
    pricing: PriceCard | None = None


class ModelRecord(ModelInput):
    connection_id: str
    source: Literal["provider", "manual", "catalog"]
    updated_at: str


def _route_schema(schema):
    # New default controls are intentionally absent from serialized old routes.
    for name in ("temperature", "top_p", "output_mode", "native_tools", "max_native_tool_calls"):
        schema.get("properties", {}).get(name, {}).pop("default", None)


class AIRoute(GenerationSettings):
    model_config = ConfigDict(json_schema_extra=_route_schema)
    connection_id: str
    model_id: str = Field(min_length=1, max_length=300)
    web_search: bool = False

    @model_validator(mode="after")
    def search_budget_flag(self):
        if "web_search" in self.native_tools:
            self.web_search = True
        return self

    @model_serializer(mode="wrap")
    def preserve_saved_route_identity(self, handler):
        data = handler(self)
        # Old approvals retain precisely their settings. An absent new control
        # means provider default, never a new capability or a changed route ID.
        for key, default in (("temperature", None), ("top_p", None), ("output_mode", "auto"),
                             ("native_tools", []), ("max_native_tool_calls", 3)):
            if data.get(key) == default:
                data.pop(key, None)
        legacy_order = ("connection_id", "model_id", "reasoning", "max_output_tokens", "web_search", "max_search_calls")
        return {**{key: data[key] for key in legacy_order if key in data},
                **{key: value for key, value in data.items() if key not in legacy_order}}


class TenderAIInput(AIModel):
    allowed_connection_ids: list[str] = Field(default_factory=list, max_length=100)
    manager: AIRoute | None = None
    specialist: AIRoute | None = None
    role_routes: dict[str, AIRoute] = Field(default_factory=dict)
    fallback_routes: list[AIRoute] = Field(default_factory=list, max_length=5)
    run_budget_usd: float | None = Field(default=None, gt=0, le=1000000, allow_inf_nan=False)
    tender_budget_usd: float | None = Field(default=None, gt=0, le=10000000, allow_inf_nan=False)
    max_requests: int = Field(default=12, ge=1, le=100)
    restore_budget_reviewed: bool = False
    provider_managed_extras: dict[str, int] = Field(default_factory=dict, max_length=100)
    engineer_confirmed: Literal[True]
    rationale: str = Field(min_length=1, max_length=4000)

    @model_validator(mode="after")
    def approved_connections(self):
        routes = [self.manager, self.specialist, *self.role_routes.values(), *self.fallback_routes]
        if any(route and route.connection_id not in self.allowed_connection_ids for route in routes):
            raise ValueError("Every model route must use a connection approved for this tender.")
        return self


class TenderAIRecord(AIModel):
    tender_id: str
    revision: int
    allowed_connection_ids: list[str]
    manager: AIRoute | None
    specialist: AIRoute | None
    role_routes: dict[str, AIRoute]
    fallback_routes: list[AIRoute]
    run_budget_usd: float | None
    tender_budget_usd: float | None
    max_requests: int
    rationale: str
    updated_at: str | None
    spent_usd: float
    reserved_usd: float
    restore_reconciliation_required: bool = False
    spend_history_may_be_incomplete: bool = False
    provider_managed_extras: dict[str, int] = Field(default_factory=dict)


class AITeamMember(AIModel):
    task_id: str
    role: str
    route: AIRoute
    rationale: str


class AITeam(AIModel):
    plan_id: str
    policy_revision: int
    manager: AIRoute | None
    specialists: list[AITeamMember]
    fallback_routes: list[AIRoute]
    status: Literal["proposed", "approved"]
    warnings: list[str]
    fingerprint: str


class AIReconcile(AIModel):
    estimated_cost_usd: float = Field(ge=0, allow_inf_nan=False)
    engineer_confirmed: Literal[True]
    rationale: str = Field(min_length=1, max_length=4000)


class AITeamApproval(AIModel):
    fingerprint: str
    engineer_confirmed: Literal[True]
    rationale: str = Field(min_length=1, max_length=4000)


class AIUsageRecord(AIModel):
    id: str
    run_id: str
    tender_id: str
    connection_id: str
    model_id: str
    actual_model: str | None
    billing: Billing
    status: str
    requests: int
    input_tokens: int
    output_tokens: int
    estimated_cost_usd: float | None
    reserved_usd: float
    detail: str
    created_at: str
    provider_reported_cost_usd: str | None = None
    provider_cost_is_partial: bool | None = None
    provider_usage_is_incomplete: bool | None = None
    cached_input_tokens: int | None = None
    reasoning_tokens: int | None = None


class SubscriptionUsage(AIModel):
    """Dated account-wide metadata, never a Tender invoice or spending authority."""
    fetched_at: str
    subscription_tier: str | None = Field(default=None, max_length=150)
    used_percent: float | None = Field(default=None, ge=0, le=100, allow_inf_nan=False)
    period_type: str | None = Field(default=None, max_length=100)
    period_start: str | None = None
    period_end: str | None = None
    prepaid_balance_usd: str | None = None
    on_demand_cap_usd: str | None = None
    on_demand_used_usd: str | None = None
    auto_topup_enabled: bool | None = None
    included_only_allowed: bool
    detail: str = Field(max_length=1200)


class RuntimeStatus(AIModel):
    connection_id: str
    installed: bool
    state: str
    detail: str
    login_url: str | None = None
    user_code: str | None = None
    docs_url: str | None = None
    subscription_usage: SubscriptionUsage | None = None
