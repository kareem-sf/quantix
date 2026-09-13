"""Tender working-memory views and explicit reusable-knowledge promotion."""

from __future__ import annotations

from typing import Literal

from pydantic import Field, field_validator

from .knowledge_models import KnowledgeCategory
from .staff_models import IdentifierText, OfficeModel, TimestampText


class WorkingMemoryDraft(OfficeModel):
    kind: Literal["scratch", "assumption"]
    title: str = Field(min_length=1, max_length=300)
    content: str = Field(min_length=1, max_length=20_000)
    source_ids: list[IdentifierText] = Field(default_factory=list, max_length=100)
    valid_until: str | None = Field(default=None, pattern=r"^\d{4}-\d{2}-\d{2}$")

    @field_validator("source_ids")
    @classmethod
    def unique_sources(cls, values: list[str]) -> list[str]:
        return list(dict.fromkeys(values))

    @field_validator("valid_until")
    @classmethod
    def real_validity_date(cls, value: str | None) -> str | None:
        if value is not None:
            from datetime import date

            date.fromisoformat(value)
        return value


class WorkingMemoryCommand(WorkingMemoryDraft):
    idempotency_key: IdentifierText


class MemoryDependency(OfficeModel):
    source_id: IdentifierText
    artifact_id: IdentifierText
    artifact_version: int = Field(ge=1)
    content_hash: str = Field(pattern=r"^[a-f0-9]{64}$")
    evidence_hash: str = Field(pattern=r"^[a-f0-9]{64}$")
    current: bool
    reason: str | None = None


class WorkingMemoryRecord(WorkingMemoryDraft):
    id: IdentifierText
    tender_id: IdentifierText
    run_id: IdentifierText | None = None
    actor_id: IdentifierText
    created_at: TimestampText
    dependencies: list[MemoryDependency]
    state: Literal["current", "needs_review"]
    review_reasons: list[str]


class DependencyImpact(OfficeModel):
    owner_kind: str
    owner_id: IdentifierText
    state: Literal["current", "needs_review"]
    review_reasons: list[str]
    dependencies: list[MemoryDependency]


class MemoryPromotionRequest(OfficeModel):
    category: KnowledgeCategory
    engineer_confirmed: Literal[True]
    rationale: str = Field(min_length=1, max_length=4000)
    verified_on: str | None = Field(default=None, pattern=r"^\d{4}-\d{2}-\d{2}$")
    recheck_after: str | None = Field(default=None, pattern=r"^\d{4}-\d{2}-\d{2}$")


class MemoryOverview(OfficeModel):
    working_memory: list[WorkingMemoryRecord]
    approved_decisions: list[dict]
    company_knowledge: list[dict]


__all__ = [
    "DependencyImpact",
    "MemoryDependency",
    "MemoryOverview",
    "MemoryPromotionRequest",
    "WorkingMemoryCommand",
    "WorkingMemoryDraft",
    "WorkingMemoryRecord",
]
