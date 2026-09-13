"""Project-map routes share the application's local authentication boundary."""

from fastapi import APIRouter

from .map_models import (
    NodeDecision,
    NodeInput,
    NodeRecord,
    ProjectMapView,
    ReviewInput,
    ReviewRecord,
)
from .project_map import ProjectMapService


def create_router(repo):
    router = APIRouter(prefix="/api", tags=["Project map and source reviews"])
    service = ProjectMapService(repo)

    @router.get("/tenders/{tender_id}/project-map", response_model=ProjectMapView)
    def view(tender_id: str):
        return service.view(tender_id)

    @router.post("/tenders/{tender_id}/project-map/nodes", response_model=NodeRecord)
    def propose(tender_id: str, request: NodeInput):
        return service.propose(tender_id, request.model_dump())

    @router.post("/tenders/{tender_id}/project-map/nodes/{node_id}/decision", response_model=NodeRecord)
    def decide(tender_id: str, node_id: str, request: NodeDecision):
        return service.decide(tender_id, node_id, request.model_dump())

    @router.post("/tenders/{tender_id}/project-map/reviews", response_model=ReviewRecord)
    def review(tender_id: str, request: ReviewInput):
        return service.review(tender_id, request.model_dump())

    return router
