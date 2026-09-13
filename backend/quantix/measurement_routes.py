"""Measurement routes use the application's existing bearer/origin enforcement."""

from typing import Annotated

from fastapi import APIRouter, Query

from .estimate_models import QuantityProposal
from .measurement_models import (
    MeasurementCalculation,
    MeasurementCreate,
    MeasurementInput,
    MeasurementLink,
    MeasurementPage,
    MeasurementRecord,
)
from .measurements import MeasurementService


def create_router(repo):
    router = APIRouter(prefix="/api", tags=["Calibrated PDF measurements"])
    service = MeasurementService(repo)

    @router.get(
        "/tenders/{tender_id}/artifacts/{artifact_id}/measurement-page",
        response_model=MeasurementPage,
    )
    def page(tender_id: str, artifact_id: str, page: Annotated[int, Query(ge=1)] = 1):
        return service.page(tender_id, artifact_id, page)

    @router.post(
        "/tenders/{tender_id}/measurements/calculate", response_model=MeasurementCalculation
    )
    def calculate(tender_id: str, request: MeasurementInput):
        return service.calculate(tender_id, request.model_dump())

    @router.post("/tenders/{tender_id}/measurements", response_model=MeasurementRecord)
    def create(tender_id: str, request: MeasurementCreate):
        return service.create(tender_id, request.model_dump())

    @router.get("/tenders/{tender_id}/measurements", response_model=list[MeasurementRecord])
    def records(
        tender_id: str,
        artifact_id: str | None = None,
        offset: Annotated[int, Query(ge=0)] = 0,
        limit: Annotated[int, Query(ge=1, le=100)] = 50,
    ):
        return service.list(tender_id, artifact_id=artifact_id, offset=offset, limit=limit)

    @router.get(
        "/tenders/{tender_id}/measurements/{measurement_id}", response_model=MeasurementRecord
    )
    def record(tender_id: str, measurement_id: str):
        return service.get(tender_id, measurement_id)

    @router.post(
        "/tenders/{tender_id}/measurements/{measurement_id}/link", response_model=QuantityProposal
    )
    def link(tender_id: str, measurement_id: str, request: MeasurementLink):
        return service.link(tender_id, measurement_id, request.model_dump())

    return router
