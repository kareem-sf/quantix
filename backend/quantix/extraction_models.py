"""Extraction versions stay separate from immutable original bytes."""

from __future__ import annotations

from .staff_models import IdentifierText, OfficeModel


class ReprocessRequest(OfficeModel):
    original_hash: IdentifierText
    page_limit: int | None = None
    reader_id: str = "pdfium-embedded-text"
    reader_version: str = "pypdfium2-5.13.0"
