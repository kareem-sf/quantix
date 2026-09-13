"""Engineer-only observation of existing Tender runs."""

from fastapi import APIRouter, HTTPException, Query

from .run_activity_models import RunActivityDetail, RunActivityPage
from .run_activity_reader import RunActivityService


def create_router(repo, *, should_stop=None):
    router = APIRouter(
        prefix="/api/tenders/{tender_id}/runs/{run_id}/activity", tags=["Run activity"]
    )
    service = RunActivityService(repo, should_stop=should_stop)

    def read(operation):
        try:
            return operation()
        except KeyError as error:
            raise HTTPException(404, detail=str(error.args[0])) from error
        except ValueError as error:
            raise HTTPException(400, detail=str(error)) from error
        except InterruptedError as error:
            raise HTTPException(503, detail=str(error)) from error

    @router.get("", response_model=RunActivityPage)
    def activity(
        tender_id: str,
        run_id: str,
        after: str | None = Query(None, max_length=1024),
        before: str | None = Query(None, max_length=1024),
        limit: int = Query(100, ge=1, le=200),
        q: str | None = Query(None, max_length=500),
        actor_id: str | None = Query(None, max_length=160),
        category: str | None = Query(None, max_length=80),
        errors_only: bool = False,
    ):
        return read(
            lambda: service.page(
                tender_id,
                run_id,
                after=after,
                before=before,
                limit=limit,
                q=q,
                actor_id=actor_id,
                category=category,
                errors_only=errors_only,
            )
        )

    @router.get("/{event_id}", response_model=RunActivityDetail)
    def detail(
        tender_id: str,
        run_id: str,
        event_id: int,
        offset: int = Query(0, ge=0),
        limit: int = Query(16000, ge=1, le=64000),
    ):
        return read(lambda: service.detail(tender_id, run_id, event_id, offset=offset, limit=limit))

    return router
