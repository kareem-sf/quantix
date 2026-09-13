"""Public, read-only execution history; payload pages never grant authority."""

from pydantic import Field

from .models import ApiModel


class ActivityArtifactReference(ApiModel):
    artifact_id: str
    page: int | None = None


class RunActivity(ApiModel):
    event_id: int
    run_id: str
    operation_id: str | None = None
    parent_operation_id: str | None = None
    actor_id: str | None = None
    actor_label: str = "Tender Manager"
    assignment_id: str | None = None
    category: str
    phase: str
    message: str
    created_at: str
    tool: str | None = None
    provider: str | None = None
    model: str | None = None
    provider_call_id: str | None = None
    preview: str = ""
    preview_truncated: bool = False
    detail_available: bool = False
    capture_status: str = "historical"
    elapsed_ms: int | None = None
    source_ids: list[str] = Field(default_factory=list)
    artifact_refs: list[ActivityArtifactReference] = Field(default_factory=list)


class RunActivityPage(ApiModel):
    items: list[RunActivity] = Field(default_factory=list)
    cursor: str | None = None
    before_cursor: str | None = None
    has_more: bool = False
    has_earlier: bool = False
    reset_required: bool = False
    run_status: str
    run_detail: str
    run_updated_at: str
    history_key: str


class RunActivityDetail(ApiModel):
    activity: RunActivity
    text: str
    offset: int = 0
    next_offset: int = 0
    total_chars: int
    has_more: bool = False
    redacted: bool = False
    unavailable_fields: list[str] = Field(default_factory=list)
    input_event_id: int | None = None
    output_event_id: int | None = None
