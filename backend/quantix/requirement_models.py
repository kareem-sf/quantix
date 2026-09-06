"""Source-backed submission requirements and explicit engineer review decisions."""

from datetime import date
from typing import Literal

from pydantic import Field, model_validator

from .estimate_models import EngineerDecision, EstimateModel

DeliverableKind = Literal[
    "boq_xlsx", "analysis_docx", "technical_docx", "registers_xlsx",
    "comparison_xlsx", "programme_xlsx", "client_boq",
]
RequirementAction = Literal["approve", "withdraw", "satisfied", "exception", "reopen"]


class RequirementProposal(EstimateModel):
    title: str = Field(min_length=1, max_length=300)
    detail: str = Field(min_length=1, max_length=6000)
    source_ids: list[str] = Field(min_length=1, max_length=50)
    deliverable_kind: DeliverableKind
    due_date: date | None = None

    @model_validator(mode="after")
    def distinct_sources(self):
        if any(not source.strip() or len(source) > 100 for source in self.source_ids):
            raise ValueError("Choose valid supporting source references.")
        if len(set(self.source_ids)) != len(self.source_ids):
            raise ValueError("Select each supporting source once.")
        return self


class RequirementDecision(EngineerDecision):
    decision: RequirementAction


class RequirementOutputLink(EngineerDecision):
    output_id: str = Field(min_length=1, max_length=100)


class RequirementSource(EstimateModel):
    source_id: str
    artifact_id: str
    artifact_name: str
    relative_path: str
    locator: str
    version: int
    content_hash: str
    evidence_hash: str
    kind: str
    is_current: bool
    available: bool
    recheck_reasons: list[str]


class RequirementLinkedOutput(EstimateModel):
    output_id: str
    filename: str
    kind: str
    sha256: str
    basis_fingerprint: str
    linked_at: str
    link_rationale: str
    is_current: bool
    available: bool
    recheck_reasons: list[str]


class RequirementAudit(EstimateModel):
    id: str
    action: Literal["approve", "withdraw", "satisfied", "exception", "reopen", "link_output", "unlink_output"]
    rationale: str
    created_at: str
    output_id: str | None = None


class RequirementRecord(RequirementProposal):
    id: str
    tender_id: str
    origin: Literal["engineer", "manager"]
    run_id: str | None
    created_at: str
    status: Literal["proposed", "approved", "withdrawn"]
    sources: list[RequirementSource]
    is_current: bool
    recheck_reasons: list[str]
    linked_outputs: list[RequirementLinkedOutput]
    review_status: Literal["pending", "satisfied", "exception"]
    review_is_current: bool
    reviewed_at: str | None
    review_rationale: str | None
    overdue: bool
    warnings: list[str]
    audit: list[RequirementAudit]


class RequirementSubmissionBasis(EstimateModel):
    requirements: list[RequirementRecord]
    blocking_reasons: list[str]
    warnings: list[str]
    fingerprint: str
