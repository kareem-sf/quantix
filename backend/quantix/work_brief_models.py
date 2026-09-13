"""The Tender Manager's saved working brief. It records progress, never authority."""

from __future__ import annotations

from typing import Annotated, Literal

from pydantic import Field

from .staff_models import IdentifierText, OfficeModel

BriefStatus = Literal["in_progress", "waiting_for_engineer", "complete"]
BriefStepState = Literal["to_do", "in_progress", "done", "blocked"]
QuestionOwner = Literal["engineer", "manager", "colleague"]

MAX_BRIEF_CHARACTERS = 12000


class BriefStep(OfficeModel):
    title: str = Field(min_length=1, max_length=200)
    state: BriefStepState
    note: str = Field(default="", max_length=500)


class BriefPoint(OfficeModel):
    """A point already settled for this work, with the Tender sources behind it."""

    text: str = Field(min_length=1, max_length=500)
    source_ids: list[IdentifierText] = Field(default_factory=list, max_length=10)


class BriefQuestion(OfficeModel):
    text: str = Field(min_length=1, max_length=400)
    owner: QuestionOwner
    affects: str = Field(default="", max_length=300)


class WorkBriefDraft(OfficeModel):
    outcome: str = Field(min_length=1, max_length=600)
    status: BriefStatus
    next_step: str = Field(default="", max_length=400)
    done_when: list[Annotated[str, Field(min_length=1, max_length=300)]] = Field(
        default_factory=list, max_length=8
    )
    steps: list[BriefStep] = Field(default_factory=list, max_length=20)
    settled: list[BriefPoint] = Field(default_factory=list, max_length=20)
    open_questions: list[BriefQuestion] = Field(default_factory=list, max_length=12)
    work_product_ids: list[IdentifierText] = Field(default_factory=list, max_length=20)


class BriefWorkProduct(OfficeModel):
    product_id: IdentifierText
    title: str
    kind: str
    version: int = Field(ge=1)
    dependency_state: Literal["current", "needs_review"]


class WorkBrief(WorkBriefDraft):
    id: IdentifierText
    tender_id: IdentifierText
    version: int = Field(ge=1)
    run_id: IdentifierText
    author: IdentifierText
    created_at: str
    work_products: list[BriefWorkProduct] = Field(default_factory=list)
    dependency_state: Literal["current", "needs_review"] = "current"
    review_reasons: list[str] = Field(default_factory=list)
    progress_current: bool = True
    latest_work_run_id: IdentifierText | None = None


class WorkBriefState(OfficeModel):
    brief: WorkBrief | None = None
