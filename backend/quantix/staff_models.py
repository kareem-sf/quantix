"""Generic, provider-neutral DTOs for the adaptive Tender Office.

The profile and work-order models deliberately describe professional work in
free text.  Display names, roles and personas are data, never authority keys.
Server-owned identifiers, timestamps, grants and creator lineage are omitted
from input models and rejected by ``extra='forbid'``.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Annotated

from pydantic import BaseModel, ConfigDict, Field, model_validator


class OfficeModel(BaseModel):
    """Base contract for public office payloads."""

    model_config = ConfigDict(extra="forbid", str_strip_whitespace=True)


class OfficeConflict(ValueError):
    """Raised when an optimistic or idempotent office operation is stale."""


NonEmptyText = Annotated[str, Field(min_length=1)]
NameText = Annotated[str, Field(min_length=1, max_length=120)]
RoleText = Annotated[str, Field(min_length=1, max_length=200)]
LongText = Annotated[str, Field(min_length=1, max_length=2000)]
ListItemText = Annotated[str, Field(min_length=1, max_length=500)]
IdentifierText = Annotated[str, Field(min_length=1, max_length=500)]
TimestampText = Annotated[str, Field(min_length=1, max_length=120)]

OptionalList = Annotated[list[ListItemText], Field(max_length=20)]
ProfessionalList = Annotated[list[ListItemText], Field(min_length=1, max_length=20)]
CapabilityList = Annotated[list[IdentifierText], Field(max_length=20)]
SourceList = Annotated[list[IdentifierText], Field(max_length=50)]


class Personality(OfficeModel):
    """Freeform working style used to guide an agent's communication."""

    description: LongText
    traits: OptionalList
    communication_style: LongText
    problem_solving_style: LongText
    collaboration_style: LongText
    uncertainty_handling: LongText
    initiative: LongText
    explanation_style: LongText
    language_preferences: OptionalList
    working_habits: OptionalList


class ManagerProfile(OfficeModel):
    """An immutable, server-versioned Tender Manager snapshot."""

    id: IdentifierText
    version: int = Field(ge=1)
    display_name: NameText
    title: RoleText
    persona: LongText
    personality: Personality
    working_preferences: OptionalList
    created_at: TimestampText
    updated_at: TimestampText


class ManagerProfileEdit(OfficeModel):
    """Engineer-editable Manager fields and the version being edited."""

    expected_version: int = Field(ge=1)
    display_name: NameText
    title: RoleText
    persona: LongText
    personality: Personality
    working_preferences: OptionalList


class StaffProfileDraft(OfficeModel):
    """Complete Manager-generated professional profile for a live colleague."""

    display_name: NameText
    role: RoleText
    title: RoleText
    specialisms: ProfessionalList
    persona: LongText
    personality: Personality
    responsibilities: ProfessionalList
    objectives: ProfessionalList
    methods: ProfessionalList
    deliverables: ProfessionalList
    success_criteria: ProfessionalList
    context_needs: ProfessionalList
    requested_tool_ids: CapabilityList
    creation_reason: LongText

    @model_validator(mode="before")
    @classmethod
    def drop_removed_method_requests(cls, value):
        # Saved profiles and definitions from before method packages were removed
        # still hold this field; it named nothing that exists now.
        if isinstance(value, dict) and "requested_skill_ids" in value:
            value = {key: item for key, item in value.items() if key != "requested_skill_ids"}
        return value


class StaffWorkOrder(OfficeModel):
    """The bounded brief and checks for one actual staff assignment."""

    brief: Annotated[str, Field(min_length=1, max_length=4000)]
    goal: LongText
    source_ids: SourceList
    expected_outputs: ProfessionalList
    completion_checks: ProfessionalList


@dataclass(frozen=True)
class ManagerCreationContext:
    """Server-only lineage supplied while the Manager creates staff."""

    tender_id: str
    run_id: str
    manager_profile_version: int
    scope_id: str


__all__ = [
    "ManagerCreationContext",
    "ManagerProfile",
    "ManagerProfileEdit",
    "OfficeConflict",
    "OfficeModel",
    "Personality",
    "StaffProfileDraft",
    "StaffWorkOrder",
]
