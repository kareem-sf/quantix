"""Validated proposals returned by the Tender Manager and task specialists."""

from dataclasses import dataclass
from datetime import date
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field

from .correspondence_models import DraftInput
from .estimate_models import DraftDocumentProposal, SourceBoqProposal, UnitRateProposalInput
from .map_models import NodeInput
from .measurement_models import AgentMeasurementProposal
from .quantity_models import AgentQuantityProposal
from .requirement_models import RequirementProposal
from .submission_models import ConstructionProgramme


class Proposal(BaseModel):
    model_config = ConfigDict(extra="forbid", str_strip_whitespace=True)


class FindingProposal(Proposal):
    title: str = Field(min_length=1, max_length=200)
    detail: str = Field(min_length=1, max_length=6000)
    kind: Literal["requirement", "risk", "question", "assumption", "observation", "exclusion"]
    source_ids: list[str] = Field(default_factory=list, max_length=50)


class TaskProposal(Proposal):
    title: str = Field(min_length=1, max_length=200)
    description: str = Field(min_length=1, max_length=4000)
    role: str = Field(min_length=1, max_length=100)
    source_ids: list[str] = Field(default_factory=list, max_length=50)


class PlanProposal(Proposal):
    title: str = Field(min_length=1, max_length=200)
    tasks: list[TaskProposal] = Field(min_length=1, max_length=12)


class WebFinding(Proposal):
    title: str = Field(min_length=1, max_length=200)
    detail: str = Field(min_length=1, max_length=6000)
    urls: list[str] = Field(min_length=1, max_length=10)


class PriceProposal(Proposal):
    item: str = Field(min_length=1, max_length=300)
    amount: str = Field(pattern=r"^[0-9]+(?:\.[0-9]+)?$", max_length=30)
    currency: str = Field(pattern=r"^[A-Z]{3}$")
    unit: str = Field(min_length=1, max_length=100)
    location: str = Field(min_length=1, max_length=200)
    tax_basis: str = Field(min_length=1, max_length=500)
    basis: Literal["observed", "estimated"]
    observed_on: date | None
    valid_until: date | None
    conditions: str = Field(min_length=1, max_length=2000)
    urls: list[str] = Field(min_length=1, max_length=10)


class SpecialistRequest(Proposal):
    role: str = Field(min_length=1, max_length=120)
    brief: str = Field(min_length=1, max_length=4000)
    source_ids: list[str] = Field(max_length=30)


class OfficeOutput(Proposal):
    summary: str = Field(min_length=1, max_length=18000)
    source_ids: list[str] = Field(default_factory=list, max_length=100)
    findings: list[FindingProposal] = Field(default_factory=list, max_length=30)
    plan: PlanProposal | None = None
    web_findings: list[WebFinding] = Field(default_factory=list, max_length=20)
    price_proposals: list[PriceProposal] = Field(default_factory=list, max_length=20)
    quote_drafts: list[DraftInput] = Field(default_factory=list, max_length=8)
    unit_rate_proposals: list[UnitRateProposalInput] = Field(default_factory=list, max_length=20)
    project_map_nodes: list[NodeInput] = Field(default_factory=list, max_length=30)
    submission_requirements: list[RequirementProposal] = Field(default_factory=list, max_length=30)
    programme_proposal: ConstructionProgramme | None = None
    drawing_measurements: list[AgentMeasurementProposal] = Field(default_factory=list, max_length=30)
    draft_documents: list[DraftDocumentProposal] = Field(default_factory=list, max_length=8)
    quantity_proposals: list[AgentQuantityProposal] = Field(default_factory=list, max_length=30)
    boq_item_proposals: list[SourceBoqProposal] = Field(default_factory=list, max_length=50)


@dataclass(frozen=True)
class PreparedOfficeResult:
    """Validated SDK result awaiting the job owner's atomic publication."""

    tender_id: str
    run_id: str
    output: OfficeOutput
    usage: dict
    source_ids_read: tuple[str, ...]
    web_sources: tuple[dict, ...]
    item_bases: tuple[tuple[str, str], ...] = ()
    trusted_recipients: tuple[str, ...] = ()
    source_recipients: tuple[tuple[str, tuple[str, ...]], ...] = ()
    approved_plan_id: str | None = None
    actor_id: str | None = None
