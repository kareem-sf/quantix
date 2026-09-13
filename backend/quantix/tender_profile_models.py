"""Tender profile and calendar contracts. Unknown facts stay unknown."""

from __future__ import annotations

from .staff_models import IdentifierText, OfficeModel


class TenderProfile(OfficeModel):
    tender_id: IdentifierText
    revision: int
    contract_type: str | None = None
    country: str | None = None
    geography: str | None = None
    currencies: list[str] = []
    working_languages: list[str] = []
    timezone: str | None = None
    measurement_method: str | None = None
    edition: str | None = None
    source_refs: list[str] = []
    permitted_destinations: list[str] = []


class ProfilePatch(OfficeModel):
    expected_revision: int
    contract_type: str | None = None
    country: str | None = None
    geography: str | None = None
    currencies: list[str] | None = None
    working_languages: list[str] | None = None
    timezone: str | None = None
    measurement_method: str | None = None
    edition: str | None = None
    source_refs: list[str] | None = None
    permitted_destinations: list[str] | None = None
    idempotency_key: IdentifierText


class CalendarEventDraft(OfficeModel):
    kind: str
    local_time: str
    timezone: str
    mandatory: bool = True
    source_refs: list[str] = []
    owner: str = "engineer"
    reminder_rule: str | None = None
    idempotency_key: IdentifierText


class CalendarEvent(OfficeModel):
    id: IdentifierText
    kind: str
    local_time: str
    timezone: str
    utc_time: str
    mandatory: bool
    source_refs: list[str]
    owner: str
    reminder_rule: str | None = None
