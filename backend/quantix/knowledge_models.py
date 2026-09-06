"""Explicit engineer decisions and approved reusable-note read contracts."""

from datetime import UTC, date, datetime
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator

KnowledgeCategory = Literal["preference", "method", "reference", "price", "tax"]
RevalidationReason = Literal[
    "source_revision_changed",
    "source_evidence_changed",
    "source_unavailable",
    "recheck_date_reached",
    "commercial_use_requires_fresh_validation",
    "withdrawn",
]


class KnowledgeModel(BaseModel):
    model_config = ConfigDict(extra="forbid", str_strip_whitespace=True)

    @field_validator("*", mode="before")
    @classmethod
    def no_null_characters(cls, value):
        if isinstance(value, str) and "\x00" in value:
            raise ValueError("Reusable note fields must not contain null characters.")
        return value


class KnowledgeDecision(KnowledgeModel):
    engineer_confirmed: Literal[True]
    rationale: str = Field(min_length=1, max_length=4000)

    @field_validator("engineer_confirmed", mode="before")
    @classmethod
    def explicit_confirmation(cls, value):
        if value is not True:
            raise ValueError("An explicit engineer confirmation is required.")
        return value


class KnowledgeNote(KnowledgeModel):
    title: str = Field(min_length=1, max_length=200)
    content: str = Field(min_length=1, max_length=20000)
    category: KnowledgeCategory
    source_tender_id: str | None = Field(default=None, min_length=1, max_length=100)
    source_ids: list[str] = Field(default_factory=list, max_length=100)
    verified_on: date | None = None
    recheck_after: date | None = None

    @field_validator("source_ids")
    @classmethod
    def unique_sources(cls, values):
        if any(not value.strip() or len(value) > 100 or "\x00" in value for value in values):
            raise ValueError("Use valid saved source IDs from the supporting Tender.")
        return list(dict.fromkeys(value.strip() for value in values))


class KnowledgeCreate(KnowledgeNote, KnowledgeDecision):
    @model_validator(mode="after")
    def check_scope_and_dates(self):
        if self.source_ids and not self.source_tender_id:
            raise ValueError("Select the original Tender for the supporting source references.")
        if self.verified_on and self.verified_on > datetime.now(UTC).date():
            raise ValueError("The engineer's verification date cannot be in the future.")
        if self.verified_on and self.recheck_after and self.recheck_after < self.verified_on:
            raise ValueError("The recheck date cannot precede the recorded verification date.")
        return self


class KnowledgeSource(KnowledgeModel):
    model_config = ConfigDict(extra="forbid", str_strip_whitespace=False)

    source_id: str
    tender_id: str
    artifact_id: str
    artifact_name: str
    relative_path: str
    locator: str
    content_hash: str
    version: int
    evidence_hash: str
    available: bool
    is_current: bool


class KnowledgeAudit(KnowledgeModel):
    id: str
    knowledge_id: str
    action: Literal["approve", "withdraw"]
    engineer_confirmed: Literal[True]
    rationale: str
    created_at: str


class KnowledgeRecord(KnowledgeNote):
    id: str
    source_tender_name: str | None
    sources: list[KnowledgeSource]
    status: Literal["approved", "withdrawn"]
    approved_at: str
    approval_rationale: str
    withdrawn_at: str | None
    withdrawal_rationale: str | None
    sources_current: bool | None
    needs_recheck: bool
    commercial_revalidation_required: bool
    revalidation_reasons: list[RevalidationReason]
    use_limitations: str
    audit: list[KnowledgeAudit]
