"""Public semantic-search status; results retain the existing evidence shape."""

from typing import Literal

from pydantic import BaseModel

SemanticState = Literal[
    "empty",
    "model_missing",
    "not_indexed",
    "ready",
    "stale",
    "limit_exceeded",
    "preparing",
    "updating",
    "stopped",
    "failed",
]


class SemanticStatus(BaseModel):
    status: SemanticState
    ready: bool
    model: str
    model_fingerprint: str
    extractor_fingerprint: str
    source_fingerprint: str
    evidence_count: int
    chunk_count: int = 0
    unique_chunks: int = 0
    indexed_at: str | None = None
    detail: str
    desired_generation: int = 0
    published_generation: int | None = None
    progress: int = 0
    recovery_action: str | None = None
    last_error: str | None = None


class SemanticUnavailable(ValueError):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code
