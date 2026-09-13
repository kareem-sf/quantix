"""Authenticated routes for later office capabilities."""

from __future__ import annotations

import re

from fastapi import APIRouter, HTTPException, Query

from .calculation_models import CalculationRecord, CalculationRequest
from .calculations import CalculationService
from .company_library import CompanyLibraryService
from .company_library_models import CompanyAsset, CompanyAssetDraft
from .documents import MAX_PDF_PAGES
from .execution_context import engineer_identity
from .extraction_adapters import ExtractionService
from .extraction_models import ReprocessRequest
from .office_reviews import (
    OfficeReviewService,
    ReviewContribution,
    ReviewDraft,
    ReviewResolution,
    ReviewSession,
)
from .office_watchers import OfficeWatcherService
from .staff_models import OfficeConflict
from .tender_calendar import TenderCalendarService
from .tender_profile import TenderProfileService
from .tender_profile_models import CalendarEvent, CalendarEventDraft, ProfilePatch, TenderProfile
from .watcher_models import WatchActivation, WatchDraft, WatchSpec
from .work_brief import WorkBriefService
from .work_brief_models import WorkBriefState
from .work_product_models import (
    WorkProductDraft,
    WorkProductPage,
    WorkProductRowPage,
    WorkProductVersion,
)
from .work_products import WorkProductService


def _call(work):
    try:
        return work()
    except KeyError as error:
        raise HTTPException(status_code=404, detail="This item could not be found.") from error
    except OfficeConflict as error:
        raise HTTPException(status_code=409, detail=str(error)) from error
    except ValueError as error:
        raise HTTPException(status_code=409, detail=str(error)) from error


def create_router(repo) -> APIRouter:
    router = APIRouter(prefix="/api", tags=["Later office"])
    products = WorkProductService(repo)
    briefs = WorkBriefService(repo)
    watches = OfficeWatcherService(repo)
    profiles = TenderProfileService(repo)
    calendar = TenderCalendarService(repo)
    calculations = CalculationService(repo)
    reviews = OfficeReviewService(repo)
    library = CompanyLibraryService(repo)
    extractions = ExtractionService(repo)

    @router.get("/tenders/{tender_id}/profile", response_model=TenderProfile)
    def get_profile(tender_id: str):
        return _call(lambda: profiles.get(tender_id))

    @router.patch("/tenders/{tender_id}/profile", response_model=TenderProfile)
    def patch_profile(tender_id: str, command: ProfilePatch):
        return _call(lambda: profiles.update(engineer_identity(tender_id), command))

    @router.post("/tenders/{tender_id}/calendar", response_model=CalendarEvent)
    def save_event(tender_id: str, command: CalendarEventDraft):
        return _call(lambda: calendar.save(engineer_identity(tender_id), command))

    @router.get("/tenders/{tender_id}/work-brief", response_model=WorkBriefState)
    def get_work_brief(tender_id: str):
        return _call(lambda: WorkBriefState(brief=briefs.current(tender_id)))

    @router.post("/tenders/{tender_id}/work-products", response_model=WorkProductVersion)
    def save_work_product(tender_id: str, command: WorkProductDraft):
        return _call(lambda: products.save_draft(engineer_identity(tender_id), command))

    @router.get("/tenders/{tender_id}/work-products", response_model=WorkProductPage)
    def list_work_products(
        tender_id: str,
        offset: int = Query(0, ge=0),
        limit: int = Query(50, ge=1, le=100),
    ):
        return _call(lambda: products.list(tender_id, offset=offset, limit=limit))

    @router.get(
        "/tenders/{tender_id}/work-products/{product_id}/versions",
        response_model=WorkProductPage,
    )
    def list_work_product_versions(
        tender_id: str,
        product_id: str,
        offset: int = Query(0, ge=0),
        limit: int = Query(50, ge=1, le=100),
    ):
        return _call(
            lambda: products.versions(
                tender_id, product_id, offset=offset, limit=limit
            )
        )

    @router.get(
        "/tenders/{tender_id}/work-products/{product_id}/versions/{version}",
        response_model=WorkProductVersion,
    )
    def get_work_product(
        tender_id: str, product_id: str, version: int, include_rows: bool = True
    ):
        def read():
            item = products.get(tender_id, product_id, version)
            return item if include_rows else item.model_copy(update={"rows": []})

        return _call(read)

    @router.get(
        "/tenders/{tender_id}/work-products/{product_id}/versions/{version}/rows",
        response_model=WorkProductRowPage,
    )
    def get_work_product_rows(
        tender_id: str,
        product_id: str,
        version: int,
        offset: int = Query(0, ge=0),
        limit: int = Query(50, ge=1, le=100),
    ):
        return _call(
            lambda: products.page(
                tender_id, product_id, version, offset=offset, limit=limit
            )
        )

    @router.post("/tenders/{tender_id}/watches", response_model=WatchSpec)
    def create_watch(tender_id: str, command: WatchDraft):
        return _call(lambda: watches.create(engineer_identity(tender_id), command))

    @router.post("/tenders/{tender_id}/watches/{watch_id}/activate", response_model=WatchSpec)
    def activate_watch(tender_id: str, watch_id: str, command: WatchActivation):
        return _call(lambda: watches.activate(engineer_identity(tender_id), watch_id, command))

    @router.post("/tenders/{tender_id}/calculations", response_model=CalculationRecord)
    def calculate(tender_id: str, command: CalculationRequest):
        return _call(lambda: calculations.calculate(engineer_identity(tender_id), command))

    @router.get(
        "/tenders/{tender_id}/calculations/{calculation_id}",
        response_model=CalculationRecord,
    )
    def get_calculation(tender_id: str, calculation_id: str):
        return _call(lambda: calculations.get(tender_id, calculation_id))

    @router.post("/tenders/{tender_id}/reviews", response_model=ReviewSession)
    def start_review(tender_id: str, command: ReviewDraft, author_total: str | None = None):
        return _call(lambda: reviews.start(engineer_identity(tender_id), command, author_total=author_total))

    @router.get("/tenders/{tender_id}/reviews", response_model=list[ReviewSession])
    def list_reviews(tender_id: str, status: str | None = None, limit: int = 50):
        return _call(lambda: reviews.list(tender_id, status=status, limit=limit))

    @router.get("/tenders/{tender_id}/reviews/{session_id}", response_model=ReviewSession)
    def get_review(tender_id: str, session_id: str):
        return _call(lambda: reviews.get(tender_id, session_id))

    @router.post("/tenders/{tender_id}/reviews/{session_id}/contributions", response_model=ReviewSession)
    def contribute_review(tender_id: str, session_id: str, command: ReviewContribution):
        return _call(lambda: reviews.contribute(engineer_identity(tender_id), session_id, command))

    @router.post("/tenders/{tender_id}/reviews/{session_id}/resolve", response_model=ReviewSession)
    def resolve_review(tender_id: str, session_id: str, command: ReviewResolution):
        return _call(lambda: reviews.resolve(engineer_identity(tender_id), session_id, command))

    @router.post("/company-assets", response_model=CompanyAsset)
    def propose_asset(command: CompanyAssetDraft):
        return _call(lambda: library.propose(engineer_identity(None), command))

    @router.post("/tenders/{tender_id}/extractions/reprocess")
    def reprocess_extraction(tender_id: str, command: ReprocessRequest):
        if not re.fullmatch(r"[0-9a-f]{64}", command.original_hash):
            raise HTTPException(status_code=409, detail="The original hash must be a SHA-256 hex identity.")
        if command.page_limit is not None and (
            type(command.page_limit) is not int
            or not 1 <= command.page_limit <= MAX_PDF_PAGES
        ):
            raise HTTPException(
                status_code=409,
                detail=f"The page limit must be between 1 and {MAX_PDF_PAGES}.",
            )

        try:
            artifacts = repo.list_artifacts(tender_id, current_only=False)
        except KeyError as error:
            raise HTTPException(status_code=404, detail="This Tender could not be found.") from error
        if not any(artifact["content_hash"] == command.original_hash for artifact in artifacts):
            raise HTTPException(
                status_code=404,
                detail="The source hash is not part of the selected Tender.",
            )
        return _call(
            lambda: extractions.reprocess(
                command.original_hash,
                successful_pages=command.page_limit,
                request=command,
            )
        )

    return router
