"""Typed engineer inputs and estimate/output API contracts."""

from datetime import date
from decimal import Decimal
from typing import Annotated, Any, Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator

from .client_boq import ClientBoqInput
from .submission_models import ConstructionProgramme

DecimalText = Annotated[str, Field(pattern=r"^[0-9]{1,12}(?:\.[0-9]{1,6})?$")]


class EstimateModel(BaseModel):
    model_config = ConfigDict(extra="forbid", str_strip_whitespace=True)


class SourceBoqProposal(EstimateModel):
    source_id: str = Field(min_length=1, max_length=100)
    row_reference: str = Field(min_length=1, max_length=100)
    source_excerpt: str = Field(min_length=1, max_length=6000)
    description: str = Field(min_length=1, max_length=6000)
    unit: str = Field(min_length=1, max_length=100)
    quantity: DecimalText
    replaces_item_id: str | None = Field(default=None, min_length=1, max_length=100)


class EngineerDecision(EstimateModel):
    engineer_confirmed: Literal[True]
    rationale: str = Field(min_length=1, max_length=4000)


class SourceRowExclusion(EngineerDecision):
    current_artifact_id: str | None


class RateComponent(EstimateModel):
    name: str = Field(min_length=1, max_length=200)
    quantity: DecimalText
    unit_rate: DecimalText
    unit: str = Field(min_length=1, max_length=100)


class RateSource(EstimateModel):
    basis: Literal["observed", "estimated"]
    observed_on: date
    source_ids: list[str] = Field(default_factory=list, max_length=30)
    urls: list[str] = Field(default_factory=list, max_length=20)
    geography: str = Field(min_length=1, max_length=200)
    conditions: str = Field(min_length=1, max_length=2000)


class ItemUpdate(EngineerDecision):
    confirm_source: bool = False
    quantity_cell: str | None = Field(default=None, max_length=20)
    unit_rate: DecimalText | None = None
    components: list[RateComponent] | None = Field(default=None, max_length=50)
    currency: str | None = Field(default=None, pattern=r"^[A-Z]{3}$")
    tax_basis: Literal["excluding_vat", "including_vat", "unknown"] | None = None
    vat_percent: DecimalText | None = None
    provenance: RateSource | None = None

    @model_validator(mode="after")
    def validate_price(self):
        if self.unit_rate is not None and self.components is not None:
            raise ValueError("Provide a direct rate or rate components, not both.")
        if self.unit_rate is not None or self.components:
            if not self.currency or not self.tax_basis or not self.provenance:
                raise ValueError("A rate needs currency, tax basis and dated source information.")
        return self


class UnitRateInput(EstimateModel):
    """A proposed commercial rate, with no quantity or approval authority."""

    unit_rate: DecimalText | None = None
    components: list[RateComponent] | None = Field(default=None, min_length=1, max_length=50)
    currency: str = Field(pattern=r"^[A-Z]{3}$")
    tax_basis: Literal["excluding_vat", "including_vat", "unknown"]
    vat_percent: DecimalText | None = None
    provenance: RateSource

    @model_validator(mode="after")
    def rate_required(self):
        if (self.unit_rate is None) == (self.components is None):
            raise ValueError("Propose exactly one direct rate or component build-up.")
        if self.vat_percent is not None and Decimal(self.vat_percent) > 100:
            raise ValueError("VAT percentage must be between zero and 100.")
        return self


class UnitRateProposalInput(UnitRateInput):
    item_id: str = Field(min_length=1, max_length=100)


class RateApproval(EngineerDecision):
    confirm_source: bool = False


class RateProposalRecord(EstimateModel):
    id: str
    tender_id: str
    item_id: str
    payload: UnitRateInput
    basis: dict[str, Any]
    basis_fingerprint: str
    approved_basis_fingerprint: str | None
    source_ids: list[str]
    run_id: str | None
    status: Literal["proposed", "approved"]
    is_current: bool
    created_at: str


class QuantityRequest(EngineerDecision):
    quantity: DecimalText
    calculation: str = Field(min_length=1, max_length=6000)
    source_ids: list[str] = Field(min_length=1, max_length=30)


class QuantityProposal(EstimateModel):
    id: str
    item_id: str
    quantity: str
    calculation: str
    source_ids: list[str]
    status: str
    created_at: str
    origin: Literal["engineer", "agent"] = "engineer"
    run_id: str | None = None
    basis_fingerprint: str | None = None


class EstimateItem(EstimateModel):
    id: str
    tender_id: str
    artifact_id: str
    source_id: str
    document: str
    sheet: str
    locator: str
    description: str
    unit: str
    unit_cell: str
    quantity_cell: str | None
    quantity_candidates: dict[str, str]
    supplied_quantity: str | None
    effective_quantity: str | None
    quantity_basis: str
    confirmed: bool
    issues: list[str]
    unit_rate: str | None
    rate_ex_vat: str | None
    components: list[RateComponent]
    currency: str | None
    tax_basis: str
    vat_percent: str | None
    provenance: RateSource | None
    line_ex_vat: str | None
    line_inc_vat: str | None
    quantity_proposals: list[QuantityProposal]
    source_excerpt: str | None = None
    row_reference: str | None = None
    origin: str | None = None
    run_id: str | None = None
    source_proposal: SourceBoqProposal | None = None


class CurrencyTotal(EstimateModel):
    currency: str
    priced_subtotal_ex_vat: str
    total_ex_vat: str | None
    total_inc_vat: str | None
    complete: bool


class RetiredSourceRow(EstimateModel):
    id: str
    description: str
    source_id: str
    artifact_id: str
    current_artifact_id: str | None = None
    row_reference: str
    unit: str
    supplied_quantity: str | None


class EstimateView(EstimateModel):
    tender_id: str
    items: list[EstimateItem]
    totals: list[CurrencyTotal]
    complete: bool
    refresh_required: bool
    unpriced_count: int
    unconfirmed_count: int
    unknown_vat_count: int
    unresolved_quantity_count: int
    blocking_reasons: list[str]
    coverage_note: str
    retired_source_rows: list[RetiredSourceRow] = Field(default_factory=list)


class DraftDocumentProposal(EstimateModel):
    """Routine draft content proposed inside an engineer-approved work plan."""

    kind: Literal[
        "boq_xlsx",
        "analysis_docx",
        "technical_docx",
        "registers_xlsx",
        "comparison_xlsx",
        "programme_xlsx",
    ]
    task_id: str | None = Field(default=None, min_length=1, max_length=100)
    programme: ConstructionProgramme | None = None

    @model_validator(mode="after")
    def selected_draft_content(self):
        if self.task_id is not None and self.kind != "technical_docx":
            raise ValueError("Select a saved task only for a technical document.")
        if (self.kind == "programme_xlsx") != (self.programme is not None):
            raise ValueError("A construction programme requires explicit activities and calendar.")
        return self


class OutputRequest(EngineerDecision):
    kind: Literal[
        "boq_xlsx",
        "analysis_docx",
        "technical_docx",
        "registers_xlsx",
        "comparison_xlsx",
        "programme_xlsx",
        "client_boq",
    ]
    task_id: str | None = Field(default=None, min_length=1, max_length=100)
    programme: ConstructionProgramme | None = None
    client_boq: ClientBoqInput | None = None

    @model_validator(mode="after")
    def selected_content(self):
        if (self.kind == "technical_docx") != (self.task_id is not None):
            raise ValueError("A technical document requires exactly one selected specialist task.")
        if (self.kind == "programme_xlsx") != (self.programme is not None):
            raise ValueError("A construction programme requires explicit activities and calendar.")
        if (self.kind == "client_boq") != (self.client_boq is not None):
            raise ValueError("A client-format BOQ requires its selected workbook and reviewed column mappings.")
        return self


class OutputRecord(EstimateModel):
    id: str
    tender_id: str
    kind: str
    filename: str
    status: Literal["draft"]
    created_at: str
    size: int
    sha256: str
    source_ids: list[str]
    pricing_complete: bool
    blocking_reasons: list[str]
    metadata: dict[str, Any] = Field(default_factory=dict)
