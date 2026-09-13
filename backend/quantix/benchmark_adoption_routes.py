from fastapi import APIRouter, HTTPException, Query

from .benchmark_adoption import BenchmarkAdoptionService
from .benchmark_adoption_models import BenchmarkAdoptionDecision, BenchmarkAdoptionReview
from .benchmarks.models import BenchmarkReport


def create_router(repo):
    service = BenchmarkAdoptionService(repo)
    router = APIRouter(prefix="/api/benchmark-adoption", tags=["AI benchmark reviews"])

    @router.get("", response_model=list[BenchmarkAdoptionDecision])
    def decisions():
        return service.list()

    @router.post("/review", response_model=BenchmarkAdoptionDecision)
    def review(command: BenchmarkAdoptionReview):
        try:
            return service.review(command)
        except KeyError as error:
            raise HTTPException(404, str(error)) from error
        except ValueError as error:
            raise HTTPException(409, str(error)) from error

    @router.get("/reports/{report_id}", response_model=BenchmarkReport)
    def verified_report(report_id: str, expected_hash: str = Query(pattern=r"^[a-f0-9]{64}$")):
        try:
            return service.read_report(report_id, expected_hash)
        except KeyError as error:
            raise HTTPException(404, str(error)) from error
        except ValueError as error:
            raise HTTPException(409, str(error)) from error

    return router
