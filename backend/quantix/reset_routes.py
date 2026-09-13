"""Authenticated reset preview, status and explicit confirmation."""

from fastapi import APIRouter

from .reset_models import ResetPreview, ResetReceipt, ResetRequest, ResetStatus


def create_router(reset):
    router = APIRouter(prefix="/api/reset", tags=["Factory reset"])

    @router.get("/preview", response_model=ResetPreview)
    def preview():
        return reset.preview()

    @router.get("/status", response_model=ResetStatus | None)
    def status():
        return reset.status()

    @router.post("", response_model=ResetReceipt)
    async def confirm(command: ResetRequest):
        return await reset.confirm(command)

    return router
