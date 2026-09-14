"""Shared retrieval request and result shapes. Ranking scores are not confidence."""

from __future__ import annotations

from typing import Literal

from pydantic import Field

from .models import ApiModel, Evidence

RetrievalMode = Literal["auto", "words", "meaning", "combined"]
ActualMode = Literal["words", "meaning", "combined"]
CollectionName = Literal["tender_evidence"]


class RetrievalSpan(ApiModel):
    start: int = Field(ge=0)
    end: int = Field(ge=0)
    found_by: Literal["words", "meaning"]
    heading: str | None = None


class RetrievalOpenTarget(ApiModel):
    evidence_id: str
    artifact_id: str
    locator: str
    start: int | None = None
    end: int | None = None


class RetrievalOccurrence(ApiModel):
    evidence_id: str
    artifact_id: str
    relative_path: str
    locator: str


class RetrievalHit(Evidence):
    collection: CollectionName = "tender_evidence"
    found_by: str = "words"
    weak_match: bool = False
    content_hash: str | None = None
    source_version: int | None = None
    document_kind: str | None = None
    spans: list[RetrievalSpan] = Field(default_factory=list)
    duplicate_occurrences: list[RetrievalOccurrence] = Field(default_factory=list)
    open_target: RetrievalOpenTarget | None = None


class RetrievalCoverage(ApiModel):
    truncated: bool = False
    scanned: int = 0
    ceiling: int = 0
    meaning_status: str | None = None
    unsupported_answer: bool = False
    unsupported_reason: str | None = None


class RetrievalResponse(ApiModel):
    hits: list[RetrievalHit]
    requested_mode: RetrievalMode
    actual_mode: ActualMode
    ranking_version: str
    generation: str | None = None
    coverage: RetrievalCoverage
    limitations: list[str] = Field(default_factory=list)
    continuation: str | None = None
