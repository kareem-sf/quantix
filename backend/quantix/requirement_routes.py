"""Submission requirement routes protected by the root API session controls."""

from typing import Annotated

from fastapi import APIRouter, Query

from .estimate_models import EngineerDecision
from .requirement_models import (
    RequirementDecision,
    RequirementOutputLink,
    RequirementProposal,
    RequirementRecord,
)
from .tender_requirements import RequirementService


def create_router(repo):
    router = APIRouter(prefix="/api", tags=["Submission requirements"])
    service = RequirementService(repo)

    @router.get("/tenders/{tender_id}/requirements", response_model=list[RequirementRecord])
    def records(tender_id: str, include_withdrawn: bool = False, offset: Annotated[int, Query(ge=0)] = 0, limit: Annotated[int, Query(ge=1, le=100)] = 50):
        return service.list(tender_id, include_withdrawn=include_withdrawn, offset=offset, limit=limit)

    @router.post("/tenders/{tender_id}/requirements", response_model=RequirementRecord)
    def propose(tender_id: str, request: RequirementProposal):
        return service.create(tender_id, request.model_dump(mode="json"))

    @router.get("/tenders/{tender_id}/requirements/{requirement_id}", response_model=RequirementRecord)
    def get(tender_id: str, requirement_id: str):
        return service.get(tender_id, requirement_id)

    @router.post("/tenders/{tender_id}/requirements/{requirement_id}/decision", response_model=RequirementRecord)
    def decide(tender_id: str, requirement_id: str, request: RequirementDecision):
        return service.decide(tender_id, requirement_id, request.model_dump())

    @router.post("/tenders/{tender_id}/requirements/{requirement_id}/outputs", response_model=RequirementRecord)
    def link(tender_id: str, requirement_id: str, request: RequirementOutputLink):
        return service.link_output(tender_id, requirement_id, request.model_dump())

    @router.post("/tenders/{tender_id}/requirements/{requirement_id}/outputs/{output_id}/unlink", response_model=RequirementRecord)
    def unlink(tender_id: str, requirement_id: str, output_id: str, request: EngineerDecision):
        return service.unlink_output(tender_id, requirement_id, output_id, request.model_dump())

    return router
