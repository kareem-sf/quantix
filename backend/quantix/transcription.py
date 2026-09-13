"""Push-to-talk transcripts become editable unsent drafts. They never approve work."""

from __future__ import annotations

from .db import new_id, now
from .storage import transcription_dir

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS transcriptions(
        id TEXT PRIMARY KEY,
        tender_id TEXT,
        draft TEXT NOT NULL,
        retained INTEGER NOT NULL,
        created_at TEXT NOT NULL
    )
    """,
)


class TranscriptionService:
    def __init__(self, repo):
        self.repo = repo
        transcription_dir(repo.home).mkdir(parents=True, exist_ok=True)
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)

    def transcribe(
        self,
        transcript: str,
        auto_send: bool,
        *,
        audio: bytes | None = None,
        retain: bool = False,
        tender_id: str | None = None,
    ) -> dict:
        if auto_send:
            auto_send = False
        audio_path = None
        if audio:
            audio_path = transcription_dir(self.repo.home) / f"{new_id()}.wav"
            audio_path.write_bytes(audio)
            if not retain:
                audio_path.unlink()
                audio_path = None
        with self.repo.atomic() as conn:
            conn.execute(
                """INSERT INTO transcriptions(id,tender_id,draft,retained,created_at)
                    VALUES(?,?,?,?,?)""",
                (new_id(), tender_id, transcript, 1 if retain else 0, now()),
            )
        return {
            "text_is_unsent_draft": True,
            "approvals_created": 0,
            "always_listening": False,
            "draft": transcript,
            "audio_retained": audio_path is not None,
        }
