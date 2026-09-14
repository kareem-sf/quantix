"""The engineer's review of the team's quantity takeoff."""

from fastapi import APIRouter

from .takeoff import TakeoffService
from .takeoff_models import TakeoffLine, TakeoffReview


def create_router(repo):
    router = APIRouter(prefix="/api", tags=["Quantity takeoff"])
    service = TakeoffService(repo)

    @router.get("/tenders/{tender_id}/takeoff", response_model=list[TakeoffLine])
    def lines(tender_id: str):
        return service.list(tender_id)

    @router.post("/tenders/{tender_id}/takeoff/{line_id}/review", response_model=TakeoffLine)
    def review(tender_id: str, line_id: str, request: TakeoffReview):
        return service.review(tender_id, line_id, request)

    return router
