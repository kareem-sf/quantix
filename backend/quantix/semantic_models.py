"""Public semantic-search status; results retain the existing evidence shape."""

from typing import Literal

from pydantic import BaseModel


class SemanticStatus(BaseModel):
    status: Literal["empty", "model_missing", "not_indexed", "ready", "stale", "limit_exceeded"]
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


class SemanticUnavailable(ValueError):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code
