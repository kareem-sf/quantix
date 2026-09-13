"""Typed records used by isolated generated-staff execution contexts."""

from __future__ import annotations

from typing import Annotated, Any

from pydantic import Field

from .staff_models import IdentifierText, OfficeModel, TimestampText
from .staff_routing_models import ArtifactBasis, ToolCapability


class StaffScope(OfficeModel):
    """The exact immutable source and native-tool scope of an assignment."""

    artifacts: list[ArtifactBasis] = Field(max_length=500)
    tools: list[ToolCapability] = Field(max_length=100)
    allowed_draft_outputs: list[Annotated[str, Field(min_length=1, max_length=80)]] = Field(
        min_length=1, max_length=30
    )
    source_scope: Annotated[str, Field(min_length=1, max_length=40)] = "selected_sources"


class StaffAuthoredMaterial(OfficeModel):
    """Material authored by this actor and assignment only."""

    notes: list[dict[str, Any]] = Field(default_factory=list, max_length=200)
    history: list[dict[str, Any]] = Field(default_factory=list, max_length=200)


class StaffSourceReceipt(OfficeModel):
    """Attributable extent actually returned by one successful staff read."""

    id: IdentifierText
    tender_id: IdentifierText
    actor_id: IdentifierText
    profile_id: IdentifierText
    profile_version: int = Field(ge=1)
    assignment_id: IdentifierText
    route_binding_id: IdentifierText
    root_run_id: IdentifierText
    source_id: IdentifierText
    artifact_id: IdentifierText
    artifact_version: int = Field(ge=1)
    content_hash: Annotated[str, Field(pattern=r"^[0-9a-fA-F]{64}$")]
    locator: Annotated[str, Field(min_length=1, max_length=1000)]
    text_offset: int | None = Field(default=None, ge=0)
    text_length: int | None = Field(default=None, ge=0)
    page: int | None = Field(default=None, ge=1)
    region: list[float] | None = Field(default=None, min_length=4, max_length=4)
    visible_cells: list[str] = Field(default_factory=list, max_length=200)
    method: Annotated[str, Field(min_length=1, max_length=80)]
    created_at: TimestampText

__all__ = ["StaffAuthoredMaterial", "StaffScope", "StaffSourceReceipt"]
