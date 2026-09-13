"""Engineer-visible, server-verified benchmark adoption decisions."""

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field


class BenchmarkAdoptionDecision(BaseModel):
    model_config = ConfigDict(extra="forbid")
    configuration_hash: str
    report_id: str
    report_hash: str
    baseline_report_id: str | None = None
    baseline_report_hash: str | None = None
    state: Literal["critical_block", "review_required", "accepted"]
    connection_id: str
    model_id: str
    manager_profile_version: int
    reasons: list[str]
    updated_at: str
    review_rationale: str | None = None


class BenchmarkAdoptionReview(BaseModel):
    model_config = ConfigDict(extra="forbid")
    configuration_hash: str = Field(pattern=r"^[a-f0-9]{64}$")
    report_id: str
    report_hash: str = Field(pattern=r"^[a-f0-9]{64}$")
    baseline_report_id: str
    baseline_report_hash: str = Field(pattern=r"^[a-f0-9]{64}$")
    engineer_confirmed: Literal[True]
    rationale: str = Field(min_length=10, max_length=4000)
