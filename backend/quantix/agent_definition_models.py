"""Strict public contracts for reusable professional definitions.

Definitions describe professional conduct and generation preferences. They do
not contain Tender sources, connection credentials, route bindings, spending
authority or execution grants.
"""

from __future__ import annotations

from typing import Literal

from pydantic import Field

from .ai_generation_models import GenerationSettings
from .staff_models import IdentifierText, NameText, OfficeModel, StaffProfileDraft, TimestampText


class AgentDefinitionCreate(OfficeModel):
    profile: StaffProfileDraft
    generation_settings: GenerationSettings = Field(default_factory=GenerationSettings)
    idempotency_key: IdentifierText


class AgentDefinitionEdit(OfficeModel):
    expected_version: int = Field(ge=1)
    profile: StaffProfileDraft
    generation_settings: GenerationSettings = Field(default_factory=GenerationSettings)
    idempotency_key: IdentifierText


class AgentDefinitionDuplicate(OfficeModel):
    source_version: int = Field(ge=1)
    display_name: NameText
    idempotency_key: IdentifierText


class AgentDefinitionRetire(OfficeModel):
    expected_version: int = Field(ge=1)
    idempotency_key: IdentifierText


class AgentDefinitionRecord(OfficeModel):
    id: IdentifierText
    version: int = Field(ge=1)
    lifecycle: Literal["active", "retired"]
    profile: StaffProfileDraft
    generation_settings: GenerationSettings
    fingerprint: str = Field(pattern=r"^[a-f0-9]{64}$")
    source_definition_id: IdentifierText | None = None
    source_definition_version: int | None = Field(default=None, ge=1)
    created_at: TimestampText
    updated_at: TimestampText


class AgentDefinitionExport(OfficeModel):
    """Portable provider-independent fields from one immutable version."""

    schema_version: Literal[1] = 1
    definition_id: IdentifierText
    version: int = Field(ge=1)
    fingerprint: str = Field(pattern=r"^[a-f0-9]{64}$")
    profile: StaffProfileDraft
    generation_settings: GenerationSettings


__all__ = [
    "AgentDefinitionCreate",
    "AgentDefinitionDuplicate",
    "AgentDefinitionEdit",
    "AgentDefinitionExport",
    "AgentDefinitionRecord",
    "AgentDefinitionRetire",
]
