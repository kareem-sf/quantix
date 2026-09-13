"""Estimate and draft-output routes, protected by the application's /api authentication."""

from fastapi import APIRouter
from fastapi.responses import FileResponse

from .estimate_models import (
    EngineerDecision,
    EstimateItem,
    EstimateView,
    ItemUpdate,
    OutputRecord,
    OutputRequest,
    QuantityProposal,
    QuantityRequest,
    RateApproval,
    RateProposalRecord,
    SourceBoqProposal,
    SourceRowExclusion,
)
from .estimates import EstimateService
from .outputs import OutputService


def create_router(repo):
    router = APIRouter(prefix="/api", tags=["Estimates and outputs"])
    estimates, outputs = EstimateService(repo), OutputService(repo)

    @router.get("/tenders/{tender_id}/estimate", response_model=EstimateView)
    def view(tender_id: str):
        return estimates.view(tender_id)

    @router.post("/tenders/{tender_id}/estimate/refresh", response_model=EstimateView)
    def refresh(tender_id: str):
        return estimates.refresh(tender_id)

    @router.post("/tenders/{tender_id}/estimate/source-rows", response_model=EstimateItem)
    def propose_source_row(tender_id: str, request: SourceBoqProposal):
        return estimates.propose_source_row(tender_id, request.model_dump())

    @router.post("/tenders/{tender_id}/estimate/source-rows/{item_id}/exclude", response_model=EstimateView)
    def exclude_source_row(tender_id: str, item_id: str, request: SourceRowExclusion):
        return estimates.exclude_source_row(tender_id, item_id, request.model_dump())

    @router.patch("/tenders/{tender_id}/estimate/items/{item_id}", response_model=EstimateItem)
    def update(tender_id: str, item_id: str, request: ItemUpdate):
        return estimates.update_item(tender_id, item_id, request.model_dump(exclude_unset=True))

    @router.post(
        "/tenders/{tender_id}/estimate/items/{item_id}/quantity-proposals",
        response_model=QuantityProposal,
    )
    def propose(tender_id: str, item_id: str, request: QuantityRequest):
        return estimates.propose_quantity(tender_id, item_id, request.model_dump())

    @router.post(
        "/tenders/{tender_id}/estimate/quantity-proposals/{proposal_id}/approve",
        response_model=EstimateView,
    )
    def approve(tender_id: str, proposal_id: str, decision: EngineerDecision):
        return estimates.approve_quantity(tender_id, proposal_id, decision.model_dump())

    @router.get(
        "/tenders/{tender_id}/estimate/rate-proposals", response_model=list[RateProposalRecord]
    )
    def rate_proposals(tender_id: str):
        return estimates.list_rate_proposals(tender_id)

    @router.post(
        "/tenders/{tender_id}/estimate/rate-proposals/{proposal_id}/approve",
        response_model=RateProposalRecord,
    )
    def approve_rate(tender_id: str, proposal_id: str, decision: RateApproval):
        return estimates.approve_rate(tender_id, proposal_id, decision.model_dump())

    @router.get("/tenders/{tender_id}/outputs", response_model=list[OutputRecord])
    def list_outputs(tender_id: str):
        return outputs.list(tender_id)

    @router.post("/tenders/{tender_id}/outputs", response_model=OutputRecord)
    def generate(tender_id: str, request: OutputRequest):
        return outputs.generate(tender_id, request.model_dump())

    @router.get("/tenders/{tender_id}/outputs/{output_id}/download", response_class=FileResponse)
    def download(tender_id: str, output_id: str):
        path = outputs.path(tender_id, output_id)
        media = (
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            if path.suffix == ".xlsx"
            else "application/vnd.ms-excel.sheet.macroEnabled.12"
            if path.suffix == ".xlsm"
            else "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        )
        return FileResponse(path, filename=path.name, media_type=media)

    return router
