"""The public API contract. Frontend declarations are generated from OpenAPI."""

from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, SecretStr, field_validator


class ApiModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class Tender(ApiModel):
    id: str
    name: str
    status: str
    revision: int
    created_at: str
    updated_at: str


class CreateTender(ApiModel):
    name: str = Field(min_length=1, max_length=200)

    @field_validator("name")
    @classmethod
    def valid_name(cls, value: str) -> str:
        value = value.strip()
        if not value:
            raise ValueError("Enter a Tender name.")
        return value


class Artifact(ApiModel):
    id: str
    tender_id: str
    relative_path: str
    name: str
    version: int
    content_hash: str
    size: int
    kind: str
    status: str
    area: str
    metadata: dict[str, Any] = Field(default_factory=dict)
    warnings: list[dict[str, Any]] = Field(default_factory=list)
    is_current: bool
    created_at: str


class Evidence(ApiModel):
    id: str
    artifact_id: str
    artifact_name: str
    relative_path: str
    locator: str
    text: str
    page: int | None = None
    sheet: str | None = None
    cell_range: str | None = None
    kind: str
    metadata: dict[str, Any] = Field(default_factory=dict)
    score: float = 0


class Finding(ApiModel):
    id: str
    tender_id: str
    title: str
    detail: str
    kind: str
    state: str
    source_ids: list[str]
    origin: str
    run_id: str | None = None
    is_stale: bool = False
    created_at: str
    updated_at: str


class Message(ApiModel):
    id: str
    tender_id: str
    role: Literal["engineer", "manager", "system"]
    content: str
    source_ids: list[str]
    run_id: str | None = None
    created_at: str


class Task(ApiModel):
    id: str
    tender_id: str
    plan_id: str
    title: str
    description: str
    role: str
    status: str
    source_ids: list[str]
    result: dict[str, Any] = Field(default_factory=dict)
    run_id: str | None = None
    created_at: str
    updated_at: str


class WorkPlan(ApiModel):
    id: str
    tender_id: str
    title: str
    version: int
    status: str
    run_id: str | None = None
    tasks: list[Task]
    created_at: str
    updated_at: str


class Run(ApiModel):
    id: str
    tender_id: str
    kind: str
    instruction: str
    status: str
    progress: int
    detail: str
    result: dict[str, Any] = Field(default_factory=dict)
    usage: dict[str, Any] = Field(default_factory=dict)
    error: str | None = None
    created_at: str
    updated_at: str


class RunEvent(ApiModel):
    id: int
    run_id: str
    kind: str
    message: str
    data: dict[str, Any]
    created_at: str


class Coverage(ApiModel):
    registered: int = 0
    extracted: int = 0
    needs_attention: int = 0
    unsupported: int = 0
    failed: int = 0


class Overview(ApiModel):
    tender: Tender
    artifact_count: int
    evidence_count: int
    coverage: Coverage
    areas: list[str]
    findings: list[Finding]
    plan: WorkPlan | None
    active_runs: list[Run]
    boq_count: int = 0


class ImportRequest(ApiModel):
    source_path: str = Field(min_length=1, max_length=4096)


class MessageRequest(ApiModel):
    content: str = Field(min_length=1, max_length=20000)


class DecisionRequest(ApiModel):
    decision: Literal["accept", "reject", "resolve"]
    rationale: str = Field(min_length=1, max_length=4000)


class ApprovalRequest(ApiModel):
    rationale: str = Field(min_length=1, max_length=4000)


class Settings(ApiModel):
    provider_ready: bool
    model: str
    default_currency: str
    home: str
    preferences: str = ""
    provider_detail: str = ""


class SettingsPatch(ApiModel):
    api_key: SecretStr | None = None
    model: Literal["gpt-6-astra"] | None = None
    default_currency: str | None = Field(default=None, pattern=r"^[A-Z]{3}$")
    preferences: str | None = Field(default=None, max_length=10000)


class Health(ApiModel):
    version: str
    provider_ready: bool
    model: str
    home: str
    capabilities: list[str] = Field(default_factory=list)


class MutationReceipt(ApiModel):
    ok: bool
