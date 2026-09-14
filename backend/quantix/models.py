"""The public API contract. Frontend declarations are generated from OpenAPI."""

from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator

from .pending import PendingInstruction


class ApiModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class Tender(ApiModel):
    id: str
    name: str
    status: str
    revision: int
    created_at: str
    updated_at: str
    # engineer: typed by the engineer; pending: the package is still being
    # analysed (shown as "Analyzing tender package"); package: a provisional
    # folder name after analysis could not name the project; ai: named by analysis.
    name_source: Literal["engineer", "pending", "package", "ai"] = "engineer"


class CreateTender(ApiModel):
    # Omitted when the package is added straight away: Quantix names the Tender
    # from the package and then identifies the project from its documents.
    name: str | None = Field(default=None, max_length=200)

    @field_validator("name")
    @classmethod
    def valid_name(cls, value: str | None) -> str | None:
        if value is None:
            return None
        value = value.strip()
        if not value:
            raise ValueError("Enter a Tender name.")
        return value


class RenameTender(ApiModel):
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
    extraction_id: str | None = None
    extraction_current: bool | None = None


class ResultLink(ApiModel):
    kind: Literal["finding", "plan", "task", "output", "requirement", "boq_item", "work_product", "calculation"]
    id: str
    title: str
    target: str


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
    result_links: list[ResultLink] = Field(default_factory=list)


class MessagePage(ApiModel):
    items: list[Message]
    next_cursor: str | None = None


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


class MessageSubmission(ApiModel):
    outcome: Literal["immediate", "pending"]
    run: Run | None = None
    pending: PendingInstruction | None = None

    @model_validator(mode="after")
    def matching_result(self):
        if self.outcome == "immediate" and (self.run is None or self.pending is not None):
            raise ValueError("An immediate message submission must include its run.")
        if self.outcome == "pending" and (self.pending is None or self.run is not None):
            raise ValueError("A pending message submission must include its pending instruction.")
        return self


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
    idempotency_key: str | None = Field(default=None, min_length=1, max_length=160)
    action: Literal["review_documents"] | None = None


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
    default_currency: str | None = Field(default=None, pattern=r"^[A-Z]{3}$")
    preferences: str | None = Field(default=None, max_length=10000)


class Health(ApiModel):
    version: str
    ai_setup_revision: int = 7
    workspace_revision: int = 1
    reset_pending: bool = False
    provider_ready: bool
    model: str
    home: str
    capabilities: list[str] = Field(default_factory=list)


class MutationReceipt(ApiModel):
    ok: bool
