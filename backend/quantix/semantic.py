"""Local, revision-aware semantic retrieval. Never falls back to keyword search."""

from __future__ import annotations

import errno
import hashlib
import importlib.metadata
import json
import re
import sqlite3
import sys
import threading
from contextlib import contextmanager
from datetime import UTC, datetime
from pathlib import Path

import numpy as np

from . import embedding_runtime as _embed
from .db import now, record
from .embedding_runtime import (
    MODEL_DIM,
    MODEL_NAME,
    MODEL_REVISION,
    default_model_path,
    model_verified,
    packed_matrix,
    runtime_for,
    score_exact,
)
from .semantic_models import SemanticStatus, SemanticUnavailable

MODEL_FILES = _embed.MODEL_FILES

CHUNK_CHARS = 1200
CHUNK_OVERLAP = 120
MAX_EVIDENCE = 100_000
MAX_SOURCE_BYTES = 64 * 1024 * 1024
MAX_CHUNKS = 50_000
MAX_OCCURRENCES = 300_000
BATCH_SIZE = 16
_INDEX_LOCK = threading.Lock()


def _hash(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


try:
    _FASTEMBED_VERSION = importlib.metadata.version("fastembed")
except importlib.metadata.PackageNotFoundError:
    _FASTEMBED_VERSION = "0.8.0"
MODEL_FINGERPRINT = _hash(
    f"{MODEL_NAME}:{MODEL_REVISION}:{MODEL_DIM}:MEAN:normalized:query:passage:fastembed={_FASTEMBED_VERSION}:chunks-v3-passages:{CHUNK_CHARS}:{CHUNK_OVERLAP}"
)
EXTRACTOR_FINGERPRINT = hashlib.sha256(
    b"".join(
        Path(__file__).with_name(name).read_bytes()
        for name in (
            "documents.py",
            "document_word.py",
            "extraction_adapters.py",
            "extraction_publication.py",
            "retrieval_passages.py",
        )
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
    return model_verified(path, fingerprint=MODEL_FINGERPRINT, full=True)


def load_model(path: Path):
    """Load only known local ONNX/tokenizer files; never fetch during search."""
    from .embedding_runtime import load_model as _load

    return _load(path)


def _download_model(path: Path):
    _embed.download_model(path, fingerprint=MODEL_FINGERPRINT, runtime_version=_FASTEMBED_VERSION)


def _ensure_model(path: Path, cancelled=None, progress=None):
    if model_available(path):
        return
    _embed.ensure_model(
        path,
        cancelled=cancelled,
        progress=progress,
        fingerprint=MODEL_FINGERPRINT,
        runtime_version=_FASTEMBED_VERSION,
    )


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
    from .retrieval_passages import passages_for_evidence

    for passage in passages_for_evidence({"text": text}, model=model):
        yield passage.start, passage.end, passage.excerpt


def _bilingual_boost(query: str, passage: str) -> float:
    """Held-out Task 1/4 ablation did not justify keeping this heuristic.

    The three-group glossary overlapped the old pump cases and lowered overall
    recall@10 versus no boost. Record 0 so scores stay comparable.
    """

    del query, passage
    return 0.0


def _language(text: str) -> str:
    arabic = bool(re.search(r"[\u0600-\u06ff]", text))
    latin = bool(re.search(r"[A-Za-z]", text))
    if arabic and latin:
        return "mixed"
    if arabic:
        return "ar"
    if latin:
        return "en"
    return "other"


def _structure(row: dict) -> dict:
    from .retrieval_passages import structure_from_row

    return structure_from_row(row)


class SemanticService:
    def __init__(self, repo):
        self.repo = repo
        self.path = repo.home / "semantic.sqlite"
        self.model_path = default_model_path(repo.home)
        self._matrices: dict[tuple, tuple[list[str], np.ndarray]] = {}
        with self._connect() as conn:
            conn.executescript("""
                CREATE TABLE IF NOT EXISTS vectors(model TEXT NOT NULL,chunk_hash TEXT NOT NULL,vector BLOB NOT NULL,PRIMARY KEY(model,chunk_hash));
                CREATE TABLE IF NOT EXISTS occurrences(tender_id TEXT NOT NULL,evidence_id TEXT NOT NULL,chunk_hash TEXT NOT NULL,start INTEGER NOT NULL,end INTEGER NOT NULL,structure_json TEXT NOT NULL DEFAULT '{}',language TEXT NOT NULL DEFAULT 'other',chunk_sha256 TEXT NOT NULL DEFAULT '',PRIMARY KEY(tender_id,evidence_id,start));
                CREATE INDEX IF NOT EXISTS occurrences_tender ON occurrences(tender_id,chunk_hash);
                CREATE TABLE IF NOT EXISTS states(tender_id TEXT PRIMARY KEY,data_json TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS refresh(tender_id TEXT PRIMARY KEY,status TEXT NOT NULL,progress INTEGER NOT NULL DEFAULT 0,detail TEXT NOT NULL DEFAULT '',error TEXT,updated_at TEXT NOT NULL);
            """)
            columns = {row["name"] for row in conn.execute("PRAGMA table_info(occurrences)")}
            for name, definition in (
                ("structure_json", "TEXT NOT NULL DEFAULT '{}'"),
                ("language", "TEXT NOT NULL DEFAULT 'other'"),
                ("chunk_sha256", "TEXT NOT NULL DEFAULT ''"),
            ):
                if name not in columns:
                    conn.execute(f"ALTER TABLE occurrences ADD COLUMN {name} {definition}")

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

    def _evidence_rows(self, tid, cancelled=None):
        self.repo.get_tender(tid)
        rows, size = [], 0
        with self.repo.db.connect() as conn:
            cursor = conn.execute(
                """SELECT e.*,a.name AS artifact_name,a.relative_path,a.content_hash,a.metadata_json AS artifact_metadata
                FROM evidence e JOIN artifacts a ON a.id=e.artifact_id
                WHERE a.tender_id=? AND a.is_current=1 AND COALESCE(e.is_current,1)=1 ORDER BY e.id""",
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
                rows.append(item)
        return rows

    def _stored(self, tid) -> dict:
        with self._connect() as conn:
            row = conn.execute("SELECT data_json FROM states WHERE tender_id=?", (tid,)).fetchone()
        return json.loads(row[0]) if row else {}

    def _refresh_row(self, tid) -> dict:
        with self._connect() as conn:
            row = conn.execute(
                "SELECT status,progress,detail,error FROM refresh WHERE tender_id=?", (tid,)
            ).fetchone()
        if not row:
            return {}
        return {
            "status": row["status"],
            "progress": row["progress"],
            "detail": row["detail"],
            "error": row["error"],
        }

    def mark_refresh(self, tid, *, status, progress=None, detail="", error=None):
        current = self._refresh_row(tid)
        payload = {
            "status": status,
            "progress": current.get("progress", 0) if progress is None else int(progress),
            "detail": detail or current.get("detail") or "",
            "error": error,
            "updated_at": now(),
        }
        with self._connect() as conn:
            conn.execute(
                "INSERT OR REPLACE INTO refresh(tender_id,status,progress,detail,error,updated_at) VALUES(?,?,?,?,?,?)",
                (
                    tid,
                    payload["status"],
                    payload["progress"],
                    payload["detail"],
                    payload["error"],
                    payload["updated_at"],
                ),
            )

    def _snapshot(self, tid, cancelled=None):
        """Return current evidence and the transactional desired generation."""
        rows = self._evidence_rows(tid, cancelled)
        return rows, self.repo.retrieval_generation(tid)

    def status(self, tid):
        self.repo.get_tender(tid)
        try:
            count, size = self.repo.current_evidence_stats(tid)
        except SemanticUnavailable as exc:
            return self._status_payload("limit_exceeded", str(exc), desired=0, evidence_count=0)
        if count > MAX_EVIDENCE or size > MAX_SOURCE_BYTES:
            return self._status_payload(
                "limit_exceeded",
                "This Tender exceeds the local semantic index limit. Exact search remains available.",
                desired=self.repo.retrieval_generation(tid),
                evidence_count=count,
            )
        desired = self.repo.retrieval_generation(tid)
        stored = self._stored(tid)
        refresh = self._refresh_row(tid)
        published = stored.get("published_generation")
        if published is None and stored.get("source_fingerprint", "").isdigit():
            published = int(stored["source_fingerprint"])
        with self._connect() as conn:
            present = conn.execute(
                "SELECT COUNT(*) FROM occurrences o JOIN vectors v ON o.chunk_hash=v.chunk_hash WHERE o.tender_id=? AND v.model=?",
                (tid, stored.get("model_fingerprint", "")),
            ).fetchone()[0]
        recovery = None
        error = refresh.get("error")
        complete = bool(
            stored
            and published == desired
            and stored.get("model_fingerprint") == MODEL_FINGERPRINT
            and stored.get("extractor_fingerprint") == EXTRACTOR_FINGERPRINT
            and present == stored.get("chunk_count")
        )
        if not count:
            state, detail = (
                "empty",
                "Import readable source evidence before building search by meaning.",
            )
        elif complete:
            state, detail = "ready", "Search by meaning is ready for the current source evidence."
        elif refresh.get("status") == "running":
            state = "updating" if published not in (None, 0) else "preparing"
            detail = refresh.get("detail") or "Preparing search by meaning."
        elif not model_available(self.model_path):
            state, detail = (
                "model_missing",
                "Download the local multilingual model by building the semantic index.",
            )
            recovery = "Prepare search to download the local multilingual model."
        elif refresh.get("status") == "stopped":
            state, detail = (
                "stopped",
                refresh.get("detail") or "Meaning search preparation was stopped.",
            )
            recovery = "Prepare search to continue."
        elif refresh.get("status") == "failed":
            state, detail = (
                "failed",
                refresh.get("detail") or "Meaning search could not be prepared.",
            )
            recovery = "Prepare search to retry."
        elif not stored:
            state, detail = (
                "not_indexed",
                "Build the semantic index to search these sources by meaning.",
            )
            recovery = "Prepare search."
        else:
            state, detail = (
                "stale",
                "Sources or processing versions changed. Rebuild search by meaning.",
            )
            recovery = "Prepare search to update meaning matches."
        return self._status_payload(
            state,
            detail,
            desired=desired,
            published=published if isinstance(published, int) else None,
            evidence_count=count,
            stored=stored,
            recovery=recovery,
            error=error,
            progress=refresh.get("progress") or (100 if state == "ready" else 0),
        )

    def _status_payload(
        self,
        state,
        detail,
        *,
        desired,
        evidence_count,
        published=None,
        stored=None,
        recovery=None,
        error=None,
        progress=0,
    ):
        stored = stored or {}
        return SemanticStatus(
            status=state,
            ready=state == "ready",
            model=MODEL_NAME,
            model_fingerprint=MODEL_FINGERPRINT,
            extractor_fingerprint=EXTRACTOR_FINGERPRINT,
            source_fingerprint=str(desired),
            evidence_count=evidence_count,
            chunk_count=stored.get("chunk_count", 0),
            unique_chunks=stored.get("unique_chunks", 0),
            indexed_at=stored.get("indexed_at"),
            detail=detail,
            desired_generation=desired,
            published_generation=published,
            progress=int(progress or 0),
            recovery_action=recovery,
            last_error=error,
        ).model_dump()

    def _runtime(self):
        return runtime_for(self.repo.home, MODEL_FINGERPRINT)

    def _get_model(self):
        runtime = self._runtime()
        with runtime._lock:
            if runtime._model is None:
                runtime._model = load_model(self.model_path)
                runtime.loads += 1
            runtime._touch()
            return runtime._model

    def index(self, tid, cancelled=None, progress=None):
        self.repo.get_tender(tid)
        runtime = self._runtime()
        with _lock(_INDEX_LOCK, cancelled):
            _progress(progress, 0, "Reading current source evidence for local semantic indexing.")
            rows, generation = self._snapshot(tid, cancelled)
            _cancel(cancelled)
            if not rows:
                return self.status(tid)
            _ensure_model(self.model_path, cancelled, progress)
            model = self._get_model()
            from .retrieval_passages import passages_for_evidence, prefix_once

            texts, occurrences = {}, []
            _progress(
                progress, 10, "Preparing bounded source passages and preserving their locations."
            )
            previous = None
            ordered_rows = sorted(
                rows,
                key=lambda item: (
                    item.get("relative_path") or "",
                    item.get("page") if item.get("page") is not None else 10**9,
                    item["id"],
                ),
            )
            for row in ordered_rows:
                _cancel(cancelled)
                for passage in passages_for_evidence(row, model=model, previous=previous):
                    digest = _hash(passage.embed_text)
                    texts[digest] = passage.embed_text
                    structure = dict(passage.structure)
                    if passage.context_prefix and passage.context_uncertain:
                        structure["context_uncertain"] = True
                        if passage.context_locator:
                            structure["context_locator"] = passage.context_locator
                    occurrences.append(
                        (
                            tid,
                            row["id"],
                            digest,
                            passage.start,
                            passage.end,
                            json.dumps(structure, ensure_ascii=False, separators=(",", ":")),
                            _language(passage.excerpt),
                            passage.excerpt_sha256,
                        )
                    )
                    if len(texts) > MAX_CHUNKS or len(occurrences) > MAX_OCCURRENCES:
                        raise SemanticUnavailable(
                            "limit_exceeded",
                            "This Tender exceeds the local semantic passage limit. Exact search remains available.",
                        )
                previous = row
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
                    vectors = runtime.embed_passages(
                        model,
                        [prefix_once("passage", texts[key]) for key in keys],
                        batch_size=BATCH_SIZE,
                    )
                except OSError as exc:
                    if getattr(exc, "errno", None) in {errno.ENOSPC, errno.ENOMEM}:
                        raise SemanticUnavailable(
                            "limit_exceeded",
                            "This computer does not have enough free disk or memory to finish meaning search.",
                        ) from exc
                    raise SemanticUnavailable(
                        "embedding_failed",
                        "The local model could not embed this batch of source passages.",
                    ) from exc
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
                try:
                    with self._connect() as conn:
                        conn.executemany("INSERT OR REPLACE INTO vectors VALUES(?,?,?)", encoded)
                except OSError as exc:
                    if getattr(exc, "errno", None) == errno.ENOSPC:
                        raise SemanticUnavailable(
                            "limit_exceeded",
                            "This computer does not have enough free disk to store meaning search.",
                        ) from exc
                    raise
                _progress(
                    progress,
                    15 + int(80 * min(start + len(keys), len(missing)) / max(1, len(missing))),
                    "Embedding source passages locally.",
                )
            _cancel(cancelled)
            if self.repo.retrieval_generation(tid) != generation:
                raise SemanticUnavailable(
                    "source_changed",
                    "Sources changed during indexing. Rebuild to include the current revisions.",
                )
            state = {
                "source_fingerprint": str(generation),
                "published_generation": generation,
                "desired_generation": generation,
                "model_fingerprint": MODEL_FINGERPRINT,
                "extractor_fingerprint": EXTRACTOR_FINGERPRINT,
                "evidence_count": len(rows),
                "chunk_count": len(occurrences),
                "unique_chunks": len(texts),
                "indexed_at": datetime.now(UTC).isoformat(),
            }
            with self._connect() as conn:
                conn.execute("DELETE FROM occurrences WHERE tender_id=?", (tid,))
                conn.executemany(
                    """
                    INSERT INTO occurrences(
                        tender_id,evidence_id,chunk_hash,start,end,structure_json,
                        language,chunk_sha256
                    ) VALUES(?,?,?,?,?,?,?,?)
                    """,
                    occurrences,
                )
                conn.execute("INSERT OR REPLACE INTO states VALUES(?,?)", (tid, json.dumps(state)))
                unused = conn.execute(
                    """SELECT v.chunk_hash FROM vectors v WHERE v.model=? AND NOT EXISTS (
                        SELECT 1 FROM occurrences o WHERE o.chunk_hash=v.chunk_hash
                    )""",
                    (MODEL_FINGERPRINT,),
                ).fetchall()
                if unused:
                    conn.executemany(
                        "DELETE FROM vectors WHERE model=? AND chunk_hash=?",
                        [(MODEL_FINGERPRINT, row[0]) for row in unused],
                    )
            self._matrices = {key: value for key, value in self._matrices.items() if key[0] != tid}
            _progress(progress, 100, "Local semantic indexing completed.")
            return self.status(tid)

    def search(
        self,
        tid,
        query,
        limit=20,
        *,
        area=None,
        status=None,
        collapse_duplicates=False,
        document_kind=None,
        artifact_ids=None,
        evidence_ids=None,
        max_spans=3,
    ):
        if not isinstance(query, str) or not query.strip() or len(query) > 2000:
            raise ValueError("Enter a search phrase of up to 2,000 characters.")
        if artifact_ids is not None and len(artifact_ids) == 0:
            return []
        if evidence_ids is not None and len(evidence_ids) == 0:
            return []
        limit = max(1, min(int(limit), 100))
        state = self.status(tid)
        if not state["ready"]:
            raise SemanticUnavailable(state["status"], state["detail"])
        model = self._get_model()
        from .retrieval_passages import prefix_once

        if model.token_count(prefix_once("query", query)) >= 480:
            raise ValueError("Use a shorter search phrase so the model can read the whole query.")
        try:
            query_vectors = self._runtime().embed_query(model, prefix_once("query", query))
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
        from .retrieval_ranking import MAX_SPANS, non_overlapping_spans

        span_limit = max(1, min(int(max_spans), MAX_SPANS))
        with self._connect() as conn:
            conn.execute("ATTACH DATABASE ? AS source", (str(self.repo.db.path),))
            scope_sql, _scope_params = self.repo._retrieval_scope(conn, artifact_ids, evidence_ids)
            filters = """o.tender_id=? AND a.tender_id=? AND a.is_current=1 AND COALESCE(e.is_current,1)=1 AND v.model=?
                AND (? IS NULL OR a.area=?) AND (? IS NULL OR a.status=?)
                AND (? IS NULL OR a.kind=?)"""
            filter_params = (
                tid,
                tid,
                MODEL_FINGERPRINT,
                area,
                area,
                status,
                status,
                document_kind,
                document_kind,
            )
            vectors = conn.execute(
                f"""SELECT DISTINCT v.chunk_hash,v.vector FROM vectors v JOIN occurrences o ON o.chunk_hash=v.chunk_hash
                JOIN source.evidence e ON e.id=o.evidence_id JOIN source.artifacts a ON a.id=e.artifact_id
                WHERE {filters} {scope_sql}""",
                filter_params,
            ).fetchall()
            if len(vectors) > MAX_CHUNKS:
                raise SemanticUnavailable(
                    "limit_exceeded", "The stored index exceeds the local search limit."
                )
            hashes = [row["chunk_hash"] for row in vectors]
            cache_key = (
                tid,
                state.get("published_generation"),
                MODEL_FINGERPRINT,
                area,
                status,
                document_kind,
            )
            cached = self._matrices.get(cache_key)
            if cached is not None and cached[0] == hashes:
                matrix = cached[1]
            else:
                matrix = packed_matrix([row["vector"] for row in vectors])
                self._matrices = {cache_key: (hashes, matrix)}
            scores = score_exact(query_vector, matrix)
            semantic_scores = {
                digest: float(score) for digest, score in zip(hashes, scores, strict=True)
            }
            ref_filters = """o.tender_id=? AND a.tender_id=? AND a.is_current=1 AND COALESCE(e.is_current,1)=1
                AND (? IS NULL OR a.area=?) AND (? IS NULL OR a.status=?)
                AND (? IS NULL OR a.kind=?)"""
            ref_params = (tid, tid, area, area, status, status, document_kind, document_kind)
            references = conn.execute(
                f"""SELECT o.evidence_id,o.chunk_hash,o.start,o.end,o.structure_json,
                o.language,o.chunk_sha256,a.content_hash,a.version AS source_version,a.kind AS document_kind,
                a.id AS artifact_id,e.locator,
                substr(e.text,o.start+1,o.end-o.start) AS chunk_text
                FROM occurrences o JOIN source.evidence e ON e.id=o.evidence_id JOIN source.artifacts a ON a.id=e.artifact_id
                WHERE {ref_filters} {scope_sql}""",
                ref_params,
            )
            chunks_by_eid: dict[str, list] = {}
            identities = {}
            extras = {}
            for row in references:
                semantic_score = semantic_scores.get(row["chunk_hash"])
                eid = row["evidence_id"]
                identities[eid] = (row["content_hash"], row["locator"])
                extras[eid] = {
                    "content_hash": row["content_hash"],
                    "source_version": row["source_version"],
                    "document_kind": row["document_kind"],
                    "artifact_id": row["artifact_id"],
                }
                if semantic_score is None:
                    continue
                passage = row["chunk_text"]
                boost = _bilingual_boost(query, passage)
                chunks_by_eid.setdefault(eid, []).append(
                    {
                        "id": eid,
                        "score": semantic_score + boost,
                        "semantic_score": semantic_score,
                        "bilingual_lexical_boost": boost,
                        "start": row["start"],
                        "end": row["end"],
                        "structure": json.loads(row["structure_json"]),
                        "language": row["language"],
                        "chunk_sha256": row["chunk_sha256"],
                        "source_content_hash": row["content_hash"],
                        "source_version": row["source_version"],
                    }
                )
            best = {}
            for eid, chunks in chunks_by_eid.items():
                spans = non_overlapping_spans(chunks, limit=span_limit)
                if not spans:
                    continue
                primary = max(spans, key=lambda item: (item["score"], -item["start"]))
                best[eid] = {**primary, "spans": spans}
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
            placeholders = ",".join("?" for _ in selected)
            rows = conn.execute(
                f"""SELECT e.*,a.name AS artifact_name,a.relative_path,a.content_hash,
                a.version AS source_version,a.kind AS document_kind
                FROM source.evidence e
                JOIN source.artifacts a ON a.id=e.artifact_id
                WHERE a.tender_id=? AND a.is_current=1 AND COALESCE(e.is_current,1)=1 AND e.id IN ({placeholders})""",
                [tid, *selected],
            ).fetchall()
        results = []
        for row in rows:
            item = record(row)
            match = best[item["id"]]
            start, end = match["start"], match["end"]
            item["score"] = match["score"]
            item["content_hash"] = extras[item["id"]]["content_hash"]
            item["source_version"] = extras[item["id"]]["source_version"]
            item["document_kind"] = extras[item["id"]]["document_kind"]
            item["metadata"] = dict(item.get("metadata", {})) | {
                "semantic_match": {
                    "start": start,
                    "end": end,
                    "text": item["text"][start:end],
                    "model": MODEL_NAME,
                    "semantic_score": match["semantic_score"],
                    "bilingual_lexical_boost": match["bilingual_lexical_boost"],
                    "chunk_sha256": match["chunk_sha256"],
                    "language": match["language"],
                    "structure": match["structure"],
                    "source_content_hash": match["source_content_hash"],
                    "source_version": match["source_version"],
                    "spans": [
                        {
                            "start": span["start"],
                            "end": span["end"],
                            "score": span["score"],
                            "semantic_score": span["semantic_score"],
                            "structure": span["structure"],
                        }
                        for span in match.get("spans", [])
                    ],
                }
            }
            results.append(item)
        return sorted(
            results, key=lambda item: (-item["score"], item["relative_path"], item["id"])
        )[:limit]


if __name__ == "__main__" and len(sys.argv) == 3 and sys.argv[1] == "download":
    _download_model(Path(sys.argv[2]))
