"""Versioned work-product contracts. View specs cannot mint approvals."""

from __future__ import annotations

from typing import Literal

from pydantic import Field, model_validator

from .staff_models import IdentifierText, OfficeModel

WorkProductKind = Literal[
    "table", "chart", "calculation_sheet", "note", "comparison", "timeline", "document"
]


class WorkProductDraft(OfficeModel):
    product_id: IdentifierText | None = None
    expected_version: int | None = Field(default=None, ge=1)
    kind: WorkProductKind
    title: str = Field(min_length=1, max_length=200)
    view_schema: dict = Field(default_factory=dict)
    rows: list[dict] = Field(default_factory=list)
    content: str = ""
    source_refs: list[str] = Field(default_factory=list)
    method_refs: list[str] = Field(default_factory=list)
    idempotency_key: IdentifierText

    @model_validator(mode="after")
    def explicit_revision(self):
        if (self.product_id is None) != (self.expected_version is None):
            raise ValueError("Revising a work product requires its identity and expected version.")
        return self


class WorkProductVersion(OfficeModel):
    id: IdentifierText
    product_id: IdentifierText
    version: int
    kind: WorkProductKind
    title: str
    view_schema: dict
    rows: list[dict]
    content: str
    sanitized_content: str
    source_refs: list[str]
    method_refs: list[str]
    author: str
    basis: str
    status: str
    dependency_state: Literal["current", "needs_review"] = "current"
    review_reasons: list[str] = Field(default_factory=list)
    sha256: IdentifierText
    executed_scripts: int = 0
    created_at: str


class WorkProductVersionSummary(OfficeModel):
    """Bounded metadata for a saved version; large rows and content stay out."""

    id: IdentifierText
    product_id: IdentifierText
    version: int
    kind: WorkProductKind
    title: str
    author: str
    basis: str
    status: str
    dependency_state: Literal["current", "needs_review"] = "current"
    review_reasons: list[str] = Field(default_factory=list)
    source_refs: list[str] = Field(default_factory=list)
    method_refs: list[str] = Field(default_factory=list)
    sha256: IdentifierText
    row_count: int = Field(ge=0)
    created_at: str


class WorkProductPage(OfficeModel):
    items: list[WorkProductVersionSummary] = Field(default_factory=list, max_length=100)
    next_offset: int | None = Field(default=None, ge=0)
    total: int = Field(ge=0)


class WorkProductRowPage(OfficeModel):
    items: list[dict] = Field(default_factory=list, max_length=100)
    total: int = Field(ge=0)
    missing: int = Field(default=0, ge=0)
