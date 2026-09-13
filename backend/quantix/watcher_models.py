"""Local watchers. Activation authorizes only the displayed actions."""

from __future__ import annotations

from typing import Literal

from pydantic import Field

from .staff_models import IdentifierText, OfficeModel


class WatchDraft(OfficeModel):
    scope: str = Field(min_length=1, max_length=200)
    trigger: str = Field(min_length=1, max_length=80)
    schedule: str = "0 * * * *"
    timezone: str = "UTC"
    stop_condition: str = "manual"
    budget: int = Field(ge=0, le=100000)
    notification_policy: str = "meaningful_change"
    idempotency_key: IdentifierText


class WatchSpec(OfficeModel):
    id: IdentifierText
    scope: str
    trigger: str
    schedule: str
    timezone: str
    last_successful_check: str | None = None
    next_due: str | None = None
    stop_condition: str
    budget: int
    notification_policy: str
    state: Literal["draft", "active", "paused", "stopped", "exhausted"]
    missed_checks: int = 0
    fingerprint: IdentifierText


class WatchActivation(OfficeModel):
    fingerprint: IdentifierText
    idempotency_key: IdentifierText


class WatchRunReceipt(OfficeModel):
    watch_id: IdentifierText
    notifications: int
    missed_checks: int
    commercial_sends: int
    catch_up: int
    provider_calls: int
