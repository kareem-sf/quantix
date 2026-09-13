"""Authenticated transcription routes. Audio is never auto-sent."""

from __future__ import annotations

from fastapi import APIRouter, File, Form, UploadFile

from .transcription import TranscriptionService
from .transcription_models import TranscriptionRequest


def create_transcription_router(repo) -> APIRouter:
    service = TranscriptionService(repo)
    router = APIRouter(prefix="/api", tags=["Voice input"])

    @router.post("/tenders/{tender_id}/transcriptions")
    async def transcribe(
        tender_id: str,
        transcript: str = Form(default=""),
        auto_send: bool = Form(default=False),
        retain: bool = Form(default=False),
        audio: UploadFile | None = File(default=None),
    ):
        body = await audio.read() if audio is not None else None
        if body is not None and len(body) > 6 * 1024 * 1024:
            body = None
        return service.transcribe(
            transcript,
            auto_send,
            audio=body,
            retain=retain,
            tender_id=tender_id,
        )

    @router.post("/tenders/{tender_id}/transcriptions/json")
    def transcribe_json(tender_id: str, command: TranscriptionRequest):
        return service.transcribe(
            command.transcript,
            command.auto_send,
            retain=command.retain,
            tender_id=tender_id,
        )

    return router
