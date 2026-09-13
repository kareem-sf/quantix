"""Authenticated by the root API; no route transmits submission files."""

from fastapi import APIRouter
from fastapi.responses import FileResponse

from .submission_models import (
    SubmissionApproval,
    SubmissionPreview,
    SubmissionRecord,
    SubmissionSelection,
)
from .submissions import SubmissionService


def create_router(repo):
    router = APIRouter(prefix="/api", tags=["Submission exports"])
    service = SubmissionService(repo)

    @router.get("/tenders/{tender_id}/submissions", response_model=list[SubmissionRecord])
    def list_submissions(tender_id: str):
        return service.list(tender_id)

    @router.post("/tenders/{tender_id}/submissions/preview", response_model=SubmissionPreview)
    def preview(tender_id: str, request: SubmissionSelection):
        return service.preview(tender_id, request.model_dump())

    @router.post("/tenders/{tender_id}/submissions", response_model=SubmissionRecord)
    def approve(tender_id: str, request: SubmissionApproval):
        return service.approve(tender_id, request.model_dump())

    @router.get(
        "/tenders/{tender_id}/submissions/{submission_id}/download", response_class=FileResponse
    )
    def download(tender_id: str, submission_id: str):
        path = service.path(tender_id, submission_id)
        return FileResponse(path, filename=path.name, media_type="application/zip")

    return router
