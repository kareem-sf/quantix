"""Company assets remain distinct from current-Tender evidence."""
from __future__ import annotations

from typing import Literal

from pydantic import Field

from .staff_models import IdentifierText, OfficeModel


class CompanyAssetDraft(OfficeModel):
    kind: Literal["project_sheet","cv","certificate","equipment","financial","template","method"]
    title: str
    valid_from: str | None = None
    valid_until: str | None = None
    verified: bool = False
    sensitive: bool = False
    permitted_reuse: str = "tender"
    payload: dict = Field(default_factory=dict)
    idempotency_key: IdentifierText

class CompanyAsset(OfficeModel):
    id: IdentifierText
    kind: str
    title: str
    valid_from: str | None
    valid_until: str | None
    verified: bool
    revoked: bool = False
    sensitive: bool
    permitted_reuse: str
    fingerprint: IdentifierText
