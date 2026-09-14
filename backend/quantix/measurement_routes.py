"""The drawing page reader used by the document viewer."""

from typing import Annotated

from fastapi import APIRouter, Query

from .measurement_models import MeasurementPage
from .measurements import MeasurementService


def create_router(repo):
    router = APIRouter(prefix="/api", tags=["Drawing pages"])
    service = MeasurementService(repo)

    @router.get(
        "/tenders/{tender_id}/artifacts/{artifact_id}/measurement-page",
        response_model=MeasurementPage,
    )
    def page(tender_id: str, artifact_id: str, page: Annotated[int, Query(ge=1)] = 1):
        return service.page(tender_id, artifact_id, page)

    return router
