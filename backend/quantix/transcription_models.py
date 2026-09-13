"""Push-to-talk contracts. Transcripts are never approvals."""

from __future__ import annotations

from .staff_models import OfficeModel


class TranscriptionRequest(OfficeModel):
    transcript: str = ""
    auto_send: bool = False
    retain: bool = False
    language: str = "en"
    capability_id: str | None = None
