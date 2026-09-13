"""Engineer-facing grounded research, citation and memory routes."""

from __future__ import annotations

from fastapi import APIRouter, HTTPException, Query
from pydantic import ValidationError

from .browser_research import BrowserResearchRuntime
from .execution_context import engineer_identity
from .knowledge_models import KnowledgeRecord
from .memory_models import (
    DependencyImpact,
    MemoryOverview,
    MemoryPromotionRequest,
    WorkingMemoryCommand,
    WorkingMemoryRecord,
)
from .memory_service import MemoryService
from .research_models import (
    BrowserResearchStatusModel,
    MarketObservation,
    MarketObservationCommand,
    PublicFetchCommand,
    PublicResearchReceipt,
    PublicSearchReceipt,
    ResearchCitation,
    ResearchCitationCommand,
)
from .research_search import PublicSearchService
from .research_service import ResearchService
from .sandbox_protocol import SandboxFailure
from .staff_models import OfficeConflict


def _call(work):
    try:
        return work()
    except ValidationError as error:
        raise HTTPException(422, "Check the research fields and try again.") from error
    except KeyError as error:
        raise HTTPException(404, str(error).strip("'")[:1000]) from error
    except (OfficeConflict, SandboxFailure, ValueError) as error:
        raise HTTPException(409, str(error)[:2000]) from error
    except OSError as error:
        raise HTTPException(
            409, "The public source could not be reached. Check the URL and try again."
        ) from error


async def _call_async(work):
    try:
        return await work()
    except ValidationError as error:
        raise HTTPException(422, "Check the research fields and try again.") from error
    except KeyError as error:
        raise HTTPException(404, str(error).strip("'")[:1000]) from error
    except (OfficeConflict, SandboxFailure, ValueError) as error:
        raise HTTPException(409, str(error)[:2000]) from error
    except OSError as error:
        raise HTTPException(
            409, "The public source could not be reached. Check the URL and try again."
        ) from error


def create_router(repo):
    research = ResearchService(repo)
    memory = MemoryService(repo)
    router = APIRouter(prefix="/api/tenders/{tender_id}", tags=["Research and memory"])
    browser = BrowserResearchRuntime(repo)
    public_search = PublicSearchService(repo)

    @router.get("/research/browser/status", response_model=BrowserResearchStatusModel)
    async def browser_status(tender_id: str):
        repo.get_tender(tender_id)
        return (await _call_async(browser.status)).__dict__

    @router.post("/research/browser/setup", response_model=BrowserResearchStatusModel)
    async def setup_browser(tender_id: str):
        repo.get_tender(tender_id)
        return (await _call_async(browser.setup)).__dict__

    @router.get("/research", response_model=list[PublicResearchReceipt])
    def list_research(
        tender_id: str,
        offset: int = Query(0, ge=0),
        limit: int = Query(50, ge=1, le=100),
    ):
        return _call(lambda: research.list(tender_id, offset=offset, limit=limit))

    @router.get(
        "/research/receipts/{receipt_id}", response_model=PublicResearchReceipt
    )
    def get_research_receipt(tender_id: str, receipt_id: str):
        return _call(lambda: research.get(tender_id, receipt_id))

    @router.get("/research/search", response_model=list[PublicSearchReceipt])
    def pending_search(tender_id: str, root_run_id: str = Query(..., min_length=1)):
        return _call(lambda: public_search.pending(tender_id, root_run_id))

    @router.get("/research/search/{request_id}", response_model=PublicSearchReceipt)
    def get_search(tender_id: str, request_id: str):
        return _call(lambda: public_search.get(tender_id, request_id))

    @router.post("/research/fetch", response_model=PublicResearchReceipt)
    def fetch_public(tender_id: str, command: PublicFetchCommand):
        return _call(lambda: research.fetch(engineer_identity(tender_id), command))

    @router.post("/research/browser/fetch", response_model=PublicResearchReceipt)
    async def fetch_rendered_public(tender_id: str, command: PublicFetchCommand):
        return await _call_async(
            lambda: research.fetch_with_browser(
                engineer_identity(tender_id), command, browser
            )
        )

    @router.post("/research/citations", response_model=ResearchCitation)
    def cite_public(tender_id: str, command: ResearchCitationCommand):
        return _call(lambda: research.cite(engineer_identity(tender_id), command))

    @router.get("/research/citations/{citation_id}", response_model=ResearchCitation)
    def get_citation(tender_id: str, citation_id: str):
        return _call(lambda: research.get_citation(tender_id, citation_id))

    @router.get("/research/market", response_model=list[MarketObservation])
    def list_market(
        tender_id: str,
        offset: int = Query(0, ge=0),
        limit: int = Query(100, ge=1, le=100),
    ):
        return _call(
            lambda: research.list_market_observations(tender_id, offset=offset, limit=limit)
        )

    @router.post("/research/market", response_model=MarketObservation)
    def record_market(tender_id: str, command: MarketObservationCommand):
        return _call(
            lambda: research.record_market_observation(engineer_identity(tender_id), command)
        )

    @router.get("/memory", response_model=MemoryOverview)
    def memory_overview(tender_id: str):
        return _call(lambda: memory.overview(tender_id))

    @router.get("/memory/working", response_model=list[WorkingMemoryRecord])
    def working_memory(
        tender_id: str,
        offset: int = Query(0, ge=0),
        limit: int = Query(50, ge=1, le=100),
    ):
        return _call(lambda: memory.list(tender_id, offset=offset, limit=limit))

    @router.post("/memory", response_model=WorkingMemoryRecord)
    def save_memory(tender_id: str, command: WorkingMemoryCommand):
        return _call(lambda: memory.save(engineer_identity(tender_id), command))

    @router.get("/memory/affected", response_model=list[DependencyImpact])
    def affected_memory(tender_id: str):
        return _call(lambda: memory.affected(tender_id))

    @router.get("/memory/{memory_id}", response_model=WorkingMemoryRecord)
    def get_memory(tender_id: str, memory_id: str):
        return _call(lambda: memory.get(tender_id, memory_id))

    @router.post("/memory/{memory_id}/promote", response_model=KnowledgeRecord)
    def promote_memory(tender_id: str, memory_id: str, command: MemoryPromotionRequest):
        return _call(lambda: memory.promote(tender_id, memory_id, command))

    return router


__all__ = ["create_router"]
