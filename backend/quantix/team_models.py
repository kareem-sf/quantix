"""Tender team members and the work the Tender Manager assigns to them."""

from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field

from .office_types import FindingProposal


class TeamModel(BaseModel):
    model_config = ConfigDict(extra="forbid", str_strip_whitespace=True)


class StaffDraft(TeamModel):
    """A colleague the Manager creates for this tender's actual needs."""

    name: str = Field(min_length=1, max_length=120)
    role: str = Field(min_length=1, max_length=200)
    specialisms: list[str] = Field(min_length=1, max_length=8)
    background: str = Field(min_length=1, max_length=1500)
    working_style: str = Field(min_length=1, max_length=800)


class StaffMember(StaffDraft):
    id: str
    tender_id: str
    status: Literal["active", "retired"]
    portrait_seed: str
    created_run_id: str
    created_at: str
    updated_at: str


class AssignmentResult(TeamModel):
    summary: str
    findings: list[FindingProposal] = Field(default_factory=list)
    source_ids: list[str] = Field(default_factory=list)
    # How many records of each kind this assignment saved for the engineer's review.
    saved_records: dict[str, int] = Field(default_factory=dict)


AssignmentStatus = Literal["queued", "running", "waiting", "completed", "failed", "cancelled"]


class Assignment(TeamModel):
    id: str
    tender_id: str
    run_id: str
    staff_id: str
    title: str
    brief: str
    expected_result: str
    source_ids: list[str]
    connection_id: str
    model_id: str
    status: AssignmentStatus
    question: str | None = None
    answer: str | None = None
    result: AssignmentResult | None = None
    detail: str = ""
    usage: dict = Field(default_factory=dict)
    created_at: str
    updated_at: str


class StaffOutput(TeamModel):
    """What a staff turn returns: finished work, or one question for the Manager."""

    kind: Literal["completed", "question"]
    summary: str = Field(default="", max_length=6000)
    findings: list[FindingProposal] = Field(default_factory=list, max_length=40)
    source_ids: list[str] = Field(default_factory=list, max_length=100)
    question: str = Field(default="", max_length=2000)


class TeamView(TeamModel):
    staff: list[StaffMember]
    assignments: list[Assignment]
