"""Append-only staff notebook records."""

from __future__ import annotations

from typing import Literal

from pydantic import Field

from .staff_models import IdentifierText, OfficeModel, TimestampText

NotebookKind = Literal[
    "finding",
    "assumption",
    "open_question",
    "failed_approach",
    "handover",
    "preference",
]


class NotebookEntryDraft(OfficeModel):
    kind: NotebookKind
    text: str = Field(min_length=1, max_length=8000)
    refs: list[str] = Field(default_factory=list, max_length=50)
    applicability: str = Field(default="current_assignment", max_length=200)
    supersedes_id: IdentifierText | None = None
    assignment_id: IdentifierText | None = None


class NotebookEntry(NotebookEntryDraft):
    id: IdentifierText
    tender_id: IdentifierText
    staff_id: IdentifierText
    created_at: TimestampText
    actor_id: IdentifierText | None = None
    profile_version: int | None = Field(default=None, ge=1)
    route_binding_id: IdentifierText | None = None
    root_run_id: IdentifierText | None = None
    source_scope: str = Field(default="unbound", min_length=1, max_length=40)
    current: bool = True
    stale: bool = False


class NotebookQuery(OfficeModel):
    staff_id: IdentifierText
    query: str = Field(default="", max_length=1000)
    kind: NotebookKind | None = None
    current: bool | None = None
    limit: int = Field(default=50, ge=1, le=200)
    cursor: str | None = Field(default=None, max_length=1000)


class NotebookPage(OfficeModel):
    items: list[NotebookEntry] = Field(default_factory=list, max_length=200)
    next_cursor: str | None = Field(default=None, max_length=1000)
    total: int = Field(default=0, ge=0)
