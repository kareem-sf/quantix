"""Local, revision-aware semantic retrieval. Never falls back to keyword search."""

from __future__ import annotations

import hashlib
import importlib.metadata
import json
import os
import sqlite3
import subprocess
import sys
import threading
import time
from contextlib import contextmanager
from datetime import UTC, datetime
from pathlib import Path

import numpy as np

from .db import record
from .processes import stop_owned_process_tree
from .semantic_models import SemanticStatus, SemanticUnavailable

MODEL_NAME = "intfloat/multilingual-e5-small"
MODEL_REVISION = "614241f622f53c4eeff9890bdc4f31cfecc418b3"
MODEL_DIM = 384
MODEL_FILES = (
    "config.json",
    "tokenizer.json",
    "tokenizer_config.json",
    "special_tokens_map.json",
    "onnx/model.onnx",
)
CHUNK_CHARS = 1200
CHUNK_OVERLAP = 120
MAX_EVIDENCE = 100_000
MAX_SOURCE_BYTES = 64 * 1024 * 1024
MAX_CHUNKS = 50_000
MAX_OCCURRENCES = 300_000
BATCH_SIZE = 16
DOWNLOAD_TIMEOUT = 900
_INDEX_LOCK = threading.Lock()
_MODEL_LOCK = threading.Lock()


def _hash(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


try:
    _FASTEMBED_VERSION = importlib.metadata.version("fastembed")
except importlib.metadata.PackageNotFoundError:
    _FASTEMBED_VERSION = "0.8.0"
MODEL_FINGERPRINT = _hash(
    f"{MODEL_NAME}:{MODEL_REVISION}:{MODEL_DIM}:MEAN:normalized:query:passage:fastembed={_FASTEMBED_VERSION}:chunks-v1:{CHUNK_CHARS}:{CHUNK_OVERLAP}"
)
EXTRACTOR_FINGERPRINT = hashlib.sha256(
    b"".join(
        Path(__file__).with_name(name).read_bytes() for name in ("documents.py", "document_word.py")
    )
).hexdigest()


def _cancel(cancelled):
    if cancelled and cancelled():
        raise InterruptedError("Semantic indexing cancelled")


def _progress(callback, percent, detail):
    if callback:
        callback(percent, detail)


@contextmanager
def _lock(lock, cancelled=None):
    while not lock.acquire(timeout=0.1):
        _cancel(cancelled)
    try:
        _cancel(cancelled)
        yield
    finally:
        lock.release()


def model_available(path: Path) -> bool:
    return all((path / name).is_file() and (path / name).stat().st_size > 0 for name in MODEL_FILES)


def load_model(path: Path):
    """Load only known local ONNX/tokenizer files; never fetch during search."""
    try:
        from fastembed import TextEmbedding
        from fastembed.common.model_description import ModelSource, PoolingType
    except ImportError as exc:
        raise SemanticUnavailable(
            "model_missing", "Local embedding dependencies are not installed."
        ) from exc
    with _MODEL_LOCK:
        if not any(m["model"] == MODEL_NAME for m in TextEmbedding.list_supported_models()):
            TextEmbedding.add_custom_model(
                model=MODEL_NAME,
                pooling=PoolingType.MEAN,
                normalization=True,
                sources=ModelSource(hf=MODEL_NAME),
                dim=MODEL_DIM,
                model_file="onnx/model.onnx",
            )
        return TextEmbedding(
            model_name=MODEL_NAME,
            specific_model_path=str(path),
            cache_dir=str(path.parent),
            local_files_only=True,
            providers=["CPUExecutionProvider"],
            threads=2,
        )


def _download_model(path: Path):
    from huggingface_hub import snapshot_download

    snapshot_download(
        repo_id=MODEL_NAME,
        revision=MODEL_REVISION,
        local_dir=str(path),
        allow_patterns=list(MODEL_FILES) + ["README.md"],
        token=False,
        max_workers=2,
    )
    load_model(path)


def _ensure_model(path: Path, cancelled=None, progress=None):
    if model_available(path):
        return
    _cancel(cancelled)
    _progress(
        progress, 5, "Downloading the local multilingual search model. No Tender text is sent."
    )
    path.parent.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.update(
        {
            "HF_HOME": str(path.parent / "huggingface"),
            "HF_XET_CACHE": str(path.parent / "xet"),
            "HF_HUB_DISABLE_IMPLICIT_TOKEN": "1",
            "HF_HUB_DISABLE_TELEMETRY": "1",
            "HF_HUB_DISABLE_XET": "1",
            "HF_HUB_DOWNLOAD_TIMEOUT": "30",
        }
    )
    process = subprocess.Popen(
        [sys.executable, "-m", "quantix.semantic", "download", str(path)],
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
    )
    started = time.monotonic()
    try:
        while process.poll() is None:
            _cancel(cancelled)
            if time.monotonic() - started > DOWNLOAD_TIMEOUT:
                raise SemanticUnavailable(
                    "model_missing",
                    "The local model download timed out. Retry indexing to resume the download.",
                )
            try:
                process.wait(timeout=0.2)
            except subprocess.TimeoutExpired:
                pass
        _cancel(cancelled)
        if process.returncode or not model_available(path):
            raise SemanticUnavailable(
                "model_missing",
                "The multilingual model could not be downloaded or loaded. Check the connection and retry indexing.",
            )
    finally:
        stop_owned_process_tree(process)


def _vector(value) -> np.ndarray:
    vector = np.asarray(value, dtype=np.float32)
    if vector.shape != (MODEL_DIM,) or not np.isfinite(vector).all():
        raise SemanticUnavailable(
            "invalid_vector",
            "The embedding model or stored index returned an invalid vector. Rebuild the index.",
        )
    norm = float(np.linalg.norm(vector))
    if not norm > 0:
        raise SemanticUnavailable("invalid_vector", "The embedding model returned an empty vector.")
    return vector / norm


def _chunks(text, model):
    start = 0
    while start < len(text):
        end = min(start + CHUNK_CHARS, len(text))
        # E5 is trained for 512 tokens. token_count may itself be capped by the
        # tokenizer; splitting below 480 prevents silently indexing a truncation.
        while model.token_count("passage: " + text[start:end]) >= 480:
            end = start + (end - start) // 2
            if end <= start:
                raise SemanticUnavailable(
                    "limit_exceeded",
                    "A source fragment cannot fit the embedding model's input limit.",
                )
        if text[start:end].strip():
            yield start, end, text[start:end]
        if end == len(text):
            break
        start = max(start + 1, end - min(CHUNK_OVERLAP, (end - start) // 5))


class SemanticService:
    def __init__(self, repo):
        self.repo = repo
        self.path = repo.home / "semantic.sqlite"
        self.model_path = repo.home / "models" / f"multilingual-e5-small-{MODEL_REVISION}"
        self._model = None
        self._model_fingerprint = None
        self._inference_lock = threading.Lock()
        with self._connect() as conn:
            conn.executescript("""
                CREATE TABLE IF NOT EXISTS vectors(model TEXT NOT NULL,chunk_hash TEXT NOT NULL,vector BLOB NOT NULL,PRIMARY KEY(model,chunk_hash));
                CREATE TABLE IF NOT EXISTS occurrences(tender_id TEXT NOT NULL,evidence_id TEXT NOT NULL,chunk_hash TEXT NOT NULL,start INTEGER NOT NULL,end INTEGER NOT NULL,PRIMARY KEY(tender_id,evidence_id,start));
                CREATE INDEX IF NOT EXISTS occurrences_tender ON occurrences(tender_id,chunk_hash);
                CREATE TABLE IF NOT EXISTS states(tender_id TEXT PRIMARY KEY,data_json TEXT NOT NULL);
            """)

    @contextmanager
    def _connect(self):
        conn = sqlite3.connect(self.path, timeout=30)
        conn.row_factory = sqlite3.Row
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute("PRAGMA synchronous=FULL")
        try:
            yield conn
            conn.commit()
        except BaseException:
            conn.rollback()
            raise
        finally:
            conn.close()

    def _snapshot(self, tid, cancelled=None):
        self.repo.get_tender(tid)
        rows, size, digest = [], 0, hashlib.sha256()
        with self.repo.db.connect() as conn:
            cursor = conn.execute(
                """SELECT e.*,a.name AS artifact_name,a.relative_path,a.content_hash,a.metadata_json AS artifact_metadata
                FROM evidence e JOIN artifacts a ON a.id=e.artifact_id WHERE a.tender_id=? AND a.is_current=1 ORDER BY e.id""",
                (tid,),
            )
            for row in cursor:
                _cancel(cancelled)
                item = record(row)
                size += len(item["text"].encode("utf-8"))
                if len(rows) >= MAX_EVIDENCE or size > MAX_SOURCE_BYTES:
                    raise SemanticUnavailable(
                        "limit_exceeded",
                        "This Tender exceeds the local semantic index limit. Exact search remains available.",
                    )
                digest.update(
                    json.dumps(dict(row), ensure_ascii=False, sort_keys=True).encode("utf-8")
                )
                rows.append(item)
        return rows, digest.hexdigest()

    def status(self, tid):
        try:
            rows, fingerprint = self._snapshot(tid)
        except SemanticUnavailable as exc:
            return SemanticStatus(
                status="limit_exceeded",
                ready=False,
                model=MODEL_NAME,
                model_fingerprint=MODEL_FINGERPRINT,
                extractor_fingerprint=EXTRACTOR_FINGERPRINT,
                source_fingerprint="",
                evidence_count=0,
                detail=str(exc),
            ).model_dump()
        with self._connect() as conn:
            row = conn.execute("SELECT data_json FROM states WHERE tender_id=?", (tid,)).fetchone()
        stored = json.loads(row[0]) if row else {}
        with self._connect() as conn:
            present = conn.execute(
                "SELECT COUNT(*) FROM occurrences o JOIN vectors v ON o.chunk_hash=v.chunk_hash WHERE o.tender_id=? AND v.model=?",
                (tid, stored.get("model_fingerprint", "")),
            ).fetchone()[0]
        if not rows:
            state, detail = (
                "empty",
                "Import readable source evidence before building search by meaning.",
            )
        elif not model_available(self.model_path):
            state, detail = (
                "model_missing",
                "Download the local multilingual model by building the semantic index.",
            )
        elif not stored:
            state, detail = (
                "not_indexed",
                "Build the semantic index to search these sources by meaning.",
            )
        elif (
            stored.get("source_fingerprint") != fingerprint
            or stored.get("model_fingerprint") != MODEL_FINGERPRINT
            or stored.get("extractor_fingerprint") != EXTRACTOR_FINGERPRINT
        ):
            state, detail = (
                "stale",
                "Sources or processing versions changed. Rebuild search by meaning.",
            )
        elif present != stored.get("chunk_count"):
            state, detail = (
                "not_indexed",
                "The stored semantic index is incomplete. Rebuild search by meaning.",
            )
        else:
            state, detail = "ready", "Search by meaning is ready for the current source evidence."
        return SemanticStatus(
            status=state,
            ready=state == "ready",
            model=MODEL_NAME,
            model_fingerprint=MODEL_FINGERPRINT,
            extractor_fingerprint=EXTRACTOR_FINGERPRINT,
            source_fingerprint=fingerprint,
            evidence_count=len(rows),
            chunk_count=stored.get("chunk_count", 0),
            unique_chunks=stored.get("unique_chunks", 0),
            indexed_at=stored.get("indexed_at"),
            detail=detail,
        ).model_dump()

    def _get_model(self):
        if self._model is None or self._model_fingerprint != MODEL_FINGERPRINT:
            try:
                self._model = load_model(self.model_path)
            except SemanticUnavailable:
                raise
            except Exception as exc:
                raise SemanticUnavailable(
                    "model_missing",
                    "The local embedding model could not be loaded. Rebuild the model cache.",
                ) from exc
            self._model_fingerprint = MODEL_FINGERPRINT
        return self._model

    def index(self, tid, cancelled=None, progress=None):
        self.repo.get_tender(tid)
        with _lock(_INDEX_LOCK, cancelled), _lock(self._inference_lock, cancelled):
            _progress(progress, 0, "Reading current source evidence for local semantic indexing.")
            rows, fingerprint = self._snapshot(tid, cancelled)
            _cancel(cancelled)
            if not rows:
                return self.status(tid)
            _ensure_model(self.model_path, cancelled, progress)
            model = self._get_model()
            texts, occurrences = {}, []
            _progress(
                progress, 10, "Preparing bounded source passages and preserving their locations."
            )
            for row in rows:
                _cancel(cancelled)
                for start, end, text in _chunks(row["text"], model):
                    digest = _hash(text)
                    texts[digest] = text
                    occurrences.append((tid, row["id"], digest, start, end))
                    if len(texts) > MAX_CHUNKS or len(occurrences) > MAX_OCCURRENCES:
                        raise SemanticUnavailable(
                            "limit_exceeded",
                            "This Tender exceeds the local semantic passage limit. Exact search remains available.",
                        )
            existing = set()
            with self._connect() as conn:
                keys = list(texts)
                for start in range(0, len(keys), 500):
                    batch = keys[start : start + 500]
                    placeholders = ",".join("?" for _ in batch)
                    existing.update(
                        r[0]
                        for r in conn.execute(
                            f"SELECT chunk_hash FROM vectors WHERE model=? AND chunk_hash IN ({placeholders})",
                            [MODEL_FINGERPRINT, *batch],
                        )
                    )
            missing = [key for key in texts if key not in existing]
            for start in range(0, len(missing), BATCH_SIZE):
                _cancel(cancelled)
                keys = missing[start : start + BATCH_SIZE]
                try:
                    vectors = list(
                        model.passage_embed(
                            ["passage: " + texts[key] for key in keys],
                            batch_size=BATCH_SIZE,
                            parallel=None,
                        )
                    )
                except Exception as exc:
                    raise SemanticUnavailable(
                        "embedding_failed",
                        "The local model could not embed this batch of source passages.",
                    ) from exc
                if len(vectors) != len(keys):
                    raise SemanticUnavailable(
                        "invalid_vector",
                        "The embedding batch did not return one vector per passage.",
                    )
                encoded = [
                    (MODEL_FINGERPRINT, key, _vector(vector).astype("<f4").tobytes())
                    for key, vector in zip(keys, vectors)
                ]
                _cancel(cancelled)
                with self._connect() as conn:
                    conn.executemany("INSERT OR REPLACE INTO vectors VALUES(?,?,?)", encoded)
                _progress(
                    progress,
                    15 + int(80 * min(start + len(keys), len(missing)) / max(1, len(missing))),
                    "Embedding source passages locally.",
                )
            _cancel(cancelled)
            if self._snapshot(tid, cancelled)[1] != fingerprint:
                raise SemanticUnavailable(
                    "source_changed",
                    "Sources changed during indexing. Rebuild to include the current revisions.",
                )
            state = {
                "source_fingerprint": fingerprint,
                "model_fingerprint": MODEL_FINGERPRINT,
                "extractor_fingerprint": EXTRACTOR_FINGERPRINT,
                "evidence_count": len(rows),
                "chunk_count": len(occurrences),
                "unique_chunks": len(texts),
                "indexed_at": datetime.now(UTC).isoformat(),
            }
            with self._connect() as conn:
                conn.execute("DELETE FROM occurrences WHERE tender_id=?", (tid,))
                conn.executemany("INSERT INTO occurrences VALUES(?,?,?,?,?)", occurrences)
                conn.execute("INSERT OR REPLACE INTO states VALUES(?,?)", (tid, json.dumps(state)))
            _progress(progress, 100, "Local semantic indexing completed.")
            return self.status(tid)

    def search(self, tid, query, limit=20, *, area=None, status=None, collapse_duplicates=False):
        if not isinstance(query, str) or not query.strip() or len(query) > 2000:
            raise ValueError("Enter a search phrase of up to 2,000 characters.")
        limit = max(1, min(int(limit), 100))
        state = self.status(tid)
        if not state["ready"]:
            raise SemanticUnavailable(state["status"], state["detail"])
        with _lock(self._inference_lock):
            model = self._get_model()
            if model.token_count("query: " + query) >= 480:
                raise ValueError(
                    "Use a shorter search phrase so the model can read the whole query."
                )
            try:
                query_vectors = list(model.query_embed("query: " + query))
                if len(query_vectors) != 1:
                    raise ValueError("Expected one query vector")
                query_vector = _vector(query_vectors[0])
            except SemanticUnavailable:
                raise
            except Exception as exc:
                raise SemanticUnavailable(
                    "embedding_failed", "The local model could not embed the search phrase."
                ) from exc
        # The live source join excludes superseded evidence even when a revision
        # lands after the status check. No foreign Tender can join this query.
        with self._connect() as conn:
            conn.execute("ATTACH DATABASE ? AS source", (str(self.repo.db.path),))
            vectors = conn.execute(
                """SELECT DISTINCT v.chunk_hash,v.vector FROM vectors v JOIN occurrences o ON o.chunk_hash=v.chunk_hash
                JOIN source.evidence e ON e.id=o.evidence_id JOIN source.artifacts a ON a.id=e.artifact_id
                WHERE o.tender_id=? AND a.tender_id=? AND a.is_current=1 AND v.model=?
                AND (? IS NULL OR a.area=?) AND (? IS NULL OR a.status=?)""",
                (tid, tid, MODEL_FINGERPRINT, area, area, status, status),
            ).fetchall()
            if len(vectors) > MAX_CHUNKS:
                raise SemanticUnavailable(
                    "limit_exceeded", "The stored index exceeds the local search limit."
                )
            scores = {
                row["chunk_hash"]: float(
                    np.dot(_vector(np.frombuffer(row["vector"], dtype="<f4")), query_vector)
                )
                for row in vectors
            }
            references = conn.execute(
                """SELECT o.evidence_id,o.chunk_hash,o.start,o.end,a.content_hash,e.locator
                FROM occurrences o JOIN source.evidence e ON e.id=o.evidence_id JOIN source.artifacts a ON a.id=e.artifact_id
                WHERE o.tender_id=? AND a.tender_id=? AND a.is_current=1
                AND (? IS NULL OR a.area=?) AND (? IS NULL OR a.status=?)""",
                (tid, tid, area, area, status, status),
            )
            best = {}
            identities = {}
            for row in references:
                score = scores.get(row["chunk_hash"])
                eid = row["evidence_id"]
                identities[eid] = (row["content_hash"], row["locator"])
                if score is not None and (eid not in best or score > best[eid]["score"]):
                    best[eid] = {"score": score, "start": row["start"], "end": row["end"]}
            selected = []
            seen = set()
            for eid in sorted(best, key=lambda eid: (-best[eid]["score"], eid)):
                identity = identities[eid] if collapse_duplicates else eid
                if identity in seen:
                    continue
                seen.add(identity)
                selected.append(eid)
                if len(selected) == limit:
                    break
            if not selected:
                return []
            # Fetch full source text only for final hits, not once per chunk.
            placeholders = ",".join("?" for _ in selected)
            rows = conn.execute(
                f"""SELECT e.*,a.name AS artifact_name,a.relative_path FROM source.evidence e
                JOIN source.artifacts a ON a.id=e.artifact_id WHERE a.tender_id=? AND a.is_current=1 AND e.id IN ({placeholders})""",
                [tid, *selected],
            ).fetchall()
        results = []
        for row in rows:
            item = record(row)
            match = best[item["id"]]
            start, end = match["start"], match["end"]
            item["score"] = match["score"]
            item["metadata"] = dict(item.get("metadata", {})) | {
                "semantic_match": {
                    "start": start,
                    "end": end,
                    "text": item["text"][start:end],
                    "model": MODEL_NAME,
                }
            }
            results.append(item)
        return sorted(
            results, key=lambda item: (-item["score"], item["relative_path"], item["id"])
        )[:limit]


if __name__ == "__main__" and len(sys.argv) == 3 and sys.argv[1] == "download":
    _download_model(Path(sys.argv[2]))
