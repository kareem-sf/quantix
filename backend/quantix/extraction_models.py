"""Extraction versions stay separate from immutable original bytes."""

from __future__ import annotations

from pydantic import Field

from .staff_models import IdentifierText, OfficeModel


class ReprocessRequest(OfficeModel):
    original_hash: IdentifierText
    page_limit: int | None = None
    reader_id: str = "pdfium-embedded-text"
    reader_version: str = "pypdfium2-5.13.0"
    artifact_id: str | None = None


class ReprocessResult(OfficeModel):
    id: str
    original_hash_unchanged: bool
    extracted_pages: int
    exception_pages: int
    reader_id: str
    reader_version: str
    published_artifact_ids: list[str] = Field(default_factory=list)
    published_evidence_ids: list[str] = Field(default_factory=list)
    retained_locators: list[str] = Field(default_factory=list)
    retrieval_generation: int | None = None
