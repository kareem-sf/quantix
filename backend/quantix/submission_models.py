"""Explicit construction calendars and engineer-reviewed local export contracts."""

from datetime import date
from typing import Annotated, Any, Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator


class SubmissionModel(BaseModel):
    model_config = ConfigDict(extra="forbid", str_strip_whitespace=True)


class ProgrammeActivity(SubmissionModel):
    id: str = Field(min_length=1, max_length=80, pattern=r"^[A-Za-z0-9_-]+$")
    title: str = Field(min_length=1, max_length=300)
    duration_days: int = Field(strict=True, ge=1, le=10000)
    predecessor_ids: list[str] = Field(default_factory=list, max_length=200)
    source_ids: list[str] = Field(default_factory=list, max_length=50)
    assumptions: list[Annotated[str, Field(min_length=1, max_length=2000)]] = Field(
        default_factory=list, max_length=30
    )


class ConstructionProgramme(SubmissionModel):
    title: str = Field(min_length=1, max_length=300)
    start_date: date
    working_week: list[Annotated[int, Field(strict=True, ge=0, le=6)]] = Field(
        min_length=1, max_length=7
    )
    holidays: list[date] = Field(default_factory=list, max_length=1000)
    activities: list[ProgrammeActivity] = Field(min_length=1, max_length=500)
    assumptions: list[Annotated[str, Field(min_length=1, max_length=2000)]] = Field(
        default_factory=list, max_length=100
    )

    @model_validator(mode="after")
    def unique_calendar_and_ids(self):
        if len(set(self.working_week)) != len(self.working_week):
            raise ValueError("Working weekdays must be unique.")
        ids = [activity.id for activity in self.activities]
        if len(set(ids)) != len(ids):
            raise ValueError("Construction activity IDs must be unique.")
        return self


class SubmissionSelection(SubmissionModel):
    output_ids: list[str] = Field(min_length=1, max_length=30)
    requirement_ids: list[str] | None = Field(default=None, max_length=1000)

    @model_validator(mode="after")
    def unique_outputs(self):
        if len(set(self.output_ids)) != len(self.output_ids):
            raise ValueError("Select each output once.")
        if self.requirement_ids is not None and len(set(self.requirement_ids)) != len(self.requirement_ids):
            raise ValueError("Select each submission requirement once.")
        return self


class ProgrammeProposalRecord(SubmissionModel):
    run_id: str
    created_at: str
    source_ids: list[str]
    is_current: bool
    programme: ConstructionProgramme


class SubmissionApproval(SubmissionSelection):
    fingerprint: str = Field(pattern=r"^[a-f0-9]{64}$")
    engineer_confirmed: Literal[True]
    final_review_confirmed: Literal[True]
    acknowledged_scope: str = Field(min_length=1, max_length=6000)
    acknowledged_gaps: list[str] = Field(max_length=1000)
    rationale: str = Field(min_length=1, max_length=4000)


class SubmissionRepairTarget(SubmissionModel):
    kind: Literal["requirement", "output", "package"]
    record_id: str
    output_ids: list[str] = Field(default_factory=list)


class SubmissionBlocker(SubmissionModel):
    code: Literal["requirement_approval", "requirement_source", "requirement_review", "requirement_document", "missing_output", "output_changed"]
    message: str
    target: SubmissionRepairTarget


class SubmissionPreview(SubmissionModel):
    fingerprint: str
    outputs: list[dict[str, Any]]
    blocking_reasons: list[str]
    blockers: list[SubmissionBlocker] = Field(default_factory=list)
    warnings: list[str]
    requirements: list[dict[str, Any]]
    requirement_ids: list[str]


class SubmissionRecord(SubmissionModel):
    id: str
    tender_id: str
    status: Literal["approved_export"]
    filename: str
    created_at: str
    size: int
    sha256: str
    fingerprint: str
    output_ids: list[str]
    outputs: list[dict[str, Any]]
    acknowledged_scope: str
    acknowledged_gaps: list[str]
    rationale: str
    final_review_confirmed: Literal[True]
    external_transmission: Literal[False]
    requirements: list[dict[str, Any]]
    requirement_ids: list[str]
