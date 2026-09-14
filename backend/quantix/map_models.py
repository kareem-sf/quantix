"""Source-backed project structure and explicitly scoped engineer reviews."""

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator


class MapModel(BaseModel):
    model_config = ConfigDict(extra="forbid", str_strip_whitespace=True)


class NodeInput(MapModel):
    kind: Literal["building", "area", "discipline", "work_item", "requirement"]
    title: str = Field(min_length=1, max_length=300)
    detail: str = Field(min_length=1, max_length=6000)
    source_ids: list[str] = Field(min_length=1, max_length=50)
    parent_id: str | None = None


class NodeDecision(MapModel):
    decision: Literal["approve", "withdraw"]
    engineer_confirmed: Literal[True]
    rationale: str = Field(min_length=1, max_length=4000)


class MapSource(MapModel):
    source_id: str
    artifact_id: str
    relative_path: str
    locator: str
    version: int
    content_hash: str
    evidence_hash: str


class MapDecisionRecord(MapModel):
    id: str
    decision: str
    rationale: str
    created_at: str


class NodeRecord(NodeInput):
    id: str
    tender_id: str
    origin: Literal["engineer", "agent"]
    run_id: str | None
    created_at: str
    state: Literal["proposed", "approved", "withdrawn"]
    is_current: bool
    approval_valid: bool
    stale_reasons: list[str]
    source_manifest: list[MapSource]
    related_artifact_ids: list[str]
    related_finding_ids: list[str]
    related_boq_item_ids: list[str]
    decisions: list[MapDecisionRecord]


class ReviewInput(MapModel):
    artifact_id: str
    scope_type: Literal["artifact", "page", "locator"]
    scope_label: str = Field(min_length=1, max_length=500)
    page: int | None = Field(default=None, ge=1)
    locator: str | None = Field(default=None, min_length=1, max_length=500)
    engineer_confirmed: Literal[True]
    rationale: str = Field(min_length=1, max_length=6000)
    whole_document_reviewed: bool = False

    @model_validator(mode="after")
    def exact_scope(self):
        if self.scope_type == "artifact":
            if (
                not self.whole_document_reviewed
                or self.page is not None
                or self.locator is not None
            ):
                raise ValueError(
                    "A whole-document review needs explicit confirmation and no partial-page or passage selection."
                )
        elif self.whole_document_reviewed:
            raise ValueError("A partial review cannot mark the whole document reviewed.")
        elif self.scope_type == "page" and (self.page is None or self.locator is not None):
            raise ValueError("Choose the single PDF page that was reviewed.")
        elif self.scope_type == "locator" and (self.locator is None or self.page is not None):
            raise ValueError("Choose the exact source passage that was reviewed.")
        return self


class ReviewRecord(ReviewInput):
    id: str
    tender_id: str
    created_at: str
    relative_path: str
    version: int
    content_hash: str
    evidence_hash: str | None
    source_ids: list[str]
    is_current: bool
    stale_reasons: list[str]


class MapCoverage(MapModel):
    registered_files: int
    extracted_files: int
    extracted_evidence: int
    evidence_cited_in_findings: int
    current_review_scopes: int
    reviewed_artifacts_in_full: int
    note: str


class ProjectMapView(MapModel):
    nodes: list[NodeRecord]
    review_scopes: list[ReviewRecord]
    coverage: MapCoverage
    directory_areas: list[str]
