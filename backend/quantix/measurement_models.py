"""Typed, engineer-controlled calibrated PDF measurement contracts."""

from typing import Annotated, Literal

from pydantic import Field, model_validator

from .estimate_models import DecimalText, EngineerDecision, EstimateModel

Coordinate = Annotated[float, Field(ge=0, le=1, allow_inf_nan=False, strict=True)]
Point = tuple[Coordinate, Coordinate]
Mode = Literal["length", "area", "count"]


class MeasurementInput(EstimateModel):
    artifact_id: str = Field(min_length=1, max_length=100)
    page: int = Field(ge=1, strict=True)
    mode: Mode
    points: list[Point] = Field(min_length=1, max_length=500)
    calibration_points: list[Point] | None = Field(default=None, min_length=2, max_length=2)
    calibration_metres: DecimalText | None = None


class MeasurementCreate(MeasurementInput, EngineerDecision):
    scope_label: str = Field(min_length=1, max_length=300)


class AgentMeasurementProposal(MeasurementInput):
    scope_label: str = Field(min_length=1, max_length=300)
    source_ids: list[str] = Field(min_length=1, max_length=28)

    @model_validator(mode="after")
    def distinct_supporting_sources(self):
        if any(not value.strip() or len(value) > 100 for value in self.source_ids):
            raise ValueError("Use valid supporting drawing and dimension references.")
        if len(set(self.source_ids)) != len(self.source_ids):
            raise ValueError("Select each supporting source once.")
        return self


class MeasurementSupportingSource(EstimateModel):
    source_id: str
    artifact_id: str
    artifact_name: str
    relative_path: str
    locator: str
    page: int | None
    version: int
    content_hash: str
    evidence_hash: str
    kind: str


class MeasurementLink(EngineerDecision):
    item_id: str = Field(min_length=1, max_length=100)


class MeasurementPage(EstimateModel):
    artifact_id: str
    version: int
    content_hash: str
    name: str
    relative_path: str
    page: int
    page_count: int
    page_size: tuple[float, float]
    is_current: bool


class MeasurementCalculation(EstimateModel):
    source: MeasurementPage
    mode: Mode
    points: list[Point]
    calibration_points: list[Point] | None
    calibration_metres: str | None
    quantity: str
    unit: Literal["m", "m2", "nr"]
    calculation: str
    calculation_version: str
    coordinate_system: Literal["normalized_top_left"]
    precision_note: str


class MeasurementLinkRecord(EstimateModel):
    item_id: str
    proposal_id: str
    rationale: str
    created_at: str


class MeasurementRecord(MeasurementCalculation):
    id: str
    tender_id: str
    source_id: str
    scope_label: str
    status: Literal["proposed"]
    origin: Literal["engineer", "agent"]
    reviewed_at: str | None
    review_rationale: str | None
    run_id: str | None = None
    supporting_source_ids: list[str] = Field(default_factory=list)
    supporting_sources: list[MeasurementSupportingSource] = Field(default_factory=list)
    is_current: bool
    created_at: str
    links: list[MeasurementLinkRecord]
    source_available: bool
    stale_reasons: list[str]
