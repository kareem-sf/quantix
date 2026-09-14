"""Measure the current production rankers on the labeled corpus.

Does not change ranking. Meaning search can disable the bilingual boost only by
temporarily replacing the private helper in this process.
"""

from __future__ import annotations

import hashlib
import json
import os
import platform
import sys
import time
from datetime import UTC, datetime
from pathlib import Path
from statistics import fmean
from uuid import uuid4

from quantix.repository import Repository
from quantix.retrieval_eval import evaluate_run, percentile, ranking_config_hash
from quantix.retrieval_service import hybrid_search
from quantix.semantic import SemanticService, model_available
from quantix.semantic_models import SemanticUnavailable

from .dataset import QUESTIONS, validate_dataset
from .install import annotate_hits, apply_pump_revision, install_corpus

METHODS = ("words", "meaning", "combined", "meaning_noboost")


def _machine() -> dict:
    return {
        "python": sys.version.split()[0],
        "platform": platform.platform(),
        "machine": platform.machine(),
        "processor": platform.processor(),
        "cpu_count": os.cpu_count(),
    }


def _memory_peak_bytes() -> int | None:
    try:
        import resource
    except ImportError:
        return None
    usage = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    if sys.platform == "darwin":
        return int(usage)
    if sys.platform.startswith("linux"):
        return int(usage) * 1024
    return None


def _allowed(mapping: dict, permitted: list[str] | None):
    if permitted is None:
        return None
    allowed = {
        evidence_id
        for document_id in permitted
        for evidence_id in mapping[document_id]["evidence"].values()
    }
    return lambda evidence_id: evidence_id in allowed


def _search(method: str, repo, service, tender_id: str, query: str, limit: int, allowed):
    from quantix import semantic as semantic_mod

    if method == "words":
        hits = repo.search(tender_id, query, limit=limit)
        if allowed is not None:
            hits = [hit for hit in hits if allowed(hit["id"])]
        return hits
    if method == "combined":
        hits, _info = hybrid_search(
            repo, tender_id, query, limit, semantic=service, allowed=allowed
        )
        return hits
    if service is None:
        raise RuntimeError("meaning_unavailable")
    if method == "meaning_noboost":
        original = semantic_mod._bilingual_boost
        semantic_mod._bilingual_boost = lambda *_args, **_kwargs: 0.0
        try:
            hits = service.search(tender_id, query, limit=limit)
        finally:
            semantic_mod._bilingual_boost = original
    else:
        hits = service.search(tender_id, query, limit=limit)
    if allowed is not None:
        hits = [hit for hit in hits if allowed(hit["id"])]
    return hits


def _local_model(service) -> dict:
    from quantix.semantic import MODEL_FILES, MODEL_FINGERPRINT, MODEL_NAME, MODEL_REVISION

    files = {}
    for relative in MODEL_FILES:
        path = service.model_path / relative
        if not path.is_file():
            return {"available": False, "detail": f"missing:{relative}"}
        with path.open("rb") as handle:
            digest = hashlib.file_digest(handle, "sha256").hexdigest()
        files[relative] = {"bytes": path.stat().st_size, "sha256": digest}
    return {
        "available": True,
        "name": MODEL_NAME,
        "revision": MODEL_REVISION,
        "model_fingerprint": MODEL_FINGERPRINT,
        "path": str(service.model_path),
        "files": files,
        "registry_verified": None,
    }


def _measure_method(method, repo, service, tender_id, mapping, questions, limit):
    bilingual = method != "meaning_noboost"
    config_hash = ranking_config_hash(bilingual_boost=bilingual and method != "words")
    rankings: dict[str, list] = {}
    latencies: list[float] = []
    for question in questions:
        if question["collection"] != "tender_evidence" or question["exhaustive"]:
            rankings[question["id"]] = []
            continue
        started = time.perf_counter()
        hits = _search(
            method,
            repo,
            service,
            tender_id,
            question["query"],
            limit,
            _allowed(mapping, question["permitted_documents"]),
        )
        latencies.append(round((time.perf_counter() - started) * 1000, 3))
        rankings[question["id"]] = annotate_hits(hits, mapping, tender_id)
    return config_hash, rankings, latencies


def _pack(method, config_hash, rankings, latencies, dataset, tender_id):
    bilingual = method != "meaning_noboost" and method != "words"
    report = evaluate_run(
        QUESTIONS,
        rankings,
        corpus_hash=dataset["corpus_hash"],
        config_hash=config_hash,
        expected_corpus_hash=dataset["corpus_hash"],
        expected_config_hash=config_hash,
        tender_id=tender_id,
    )
    return {
        "available": True,
        "config_hash": config_hash,
        "bilingual_boost": bilingual,
        "summary": report["summary"],
        "splits": report["splits"],
        "categories": report["categories"],
        "languages": report["languages"],
        "cases": report["cases"],
        "latencies_ms": {
            "count": len(latencies),
            "mean": round(fmean(latencies), 3) if latencies else None,
            "p50": percentile(latencies, 0.50),
            "p95": percentile(latencies, 0.95),
            "cold_first": latencies[0] if latencies else None,
            "warm": latencies[1:] or None,
        },
    }


def run_baseline(home: Path, *, limit: int = 10) -> dict:
    home = home.expanduser().resolve()
    home.mkdir(parents=True, exist_ok=True)
    dataset = validate_dataset()
    repo = Repository(home)
    tender = repo.create_tender(f"Retrieval baseline {datetime.now(UTC).isoformat()}")
    mapping = install_corpus(repo, tender["id"])
    foreign = repo.create_tender("Foreign retrieval distractor")
    repo.register_artifact(
        foreign["id"],
        "Plant/pump-en.pdf",
        "f" * 64,
        40,
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [
                {
                    "locator": "page:1",
                    "text": "Foreign tender pump output 70 cubic metres per hour.",
                    "page": 1,
                }
            ],
        },
    )

    service = SemanticService(repo)
    meaning_state: dict = {"available": model_available(service.model_path)}
    index_seconds = None
    reindex_seconds = None
    if meaning_state["available"]:
        started = time.perf_counter()
        try:
            service.index(tender["id"])
            index_seconds = round(time.perf_counter() - started, 3)
        except SemanticUnavailable as exc:
            meaning_state = {"available": False, "detail": str(exc), "code": exc.code}
            service = None
    else:
        meaning_state["detail"] = "The local embedding model files are not present."
        service = None

    base_questions = [question for question in QUESTIONS if question["phase"] == "base"]
    revision_questions = [
        question for question in QUESTIONS if question["phase"] == "after_revision"
    ]
    stored: dict[str, dict] = {}
    for method in METHODS:
        if method != "words" and service is None:
            stored[method] = {
                "available": False,
                "reason": "meaning_unavailable",
                "config_hash": None,
                "rankings": {question["id"]: [] for question in QUESTIONS},
                "latencies": [],
            }
            continue
        config_hash, rankings, latencies = _measure_method(
            method, repo, service, tender["id"], mapping, base_questions, limit
        )
        stored[method] = {
            "config_hash": config_hash,
            "rankings": rankings,
            "latencies": latencies,
        }

    mapping = apply_pump_revision(repo, tender["id"], mapping)
    if service is not None:
        started = time.perf_counter()
        service.index(tender["id"])
        reindex_seconds = round(time.perf_counter() - started, 3)
    for method in METHODS:
        if method != "words" and service is None:
            continue
        _config_hash, revision_rankings, revision_latencies = _measure_method(
            method, repo, service, tender["id"], mapping, revision_questions, limit
        )
        stored[method]["rankings"].update(revision_rankings)
        stored[method]["latencies"].extend(revision_latencies)

    methods = {}
    for method in METHODS:
        payload = stored[method]
        if payload.get("available") is False:
            methods[method] = {
                "available": False,
                "reason": payload.get("reason"),
                "config_hash": None,
                "summary": None,
                "latencies_ms": None,
                "cases": [],
            }
            continue
        methods[method] = _pack(
            method,
            payload["config_hash"],
            payload["rankings"],
            payload["latencies"],
            dataset,
            tender["id"],
        )

    model_record = meaning_state
    if meaning_state.get("available"):
        model_record = _local_model(SemanticService(repo))

    output = home / "benchmarks" / f"retrieval-baseline-{uuid4().hex}.json"
    result = {
        "schema_version": 2,
        "suite": "baseline",
        "run_at": datetime.now(UTC).isoformat(),
        "dataset": dataset,
        "isolated_home": str(home),
        "tender_id": tender["id"],
        "foreign_tender_id": foreign["id"],
        "machine": _machine(),
        "model": model_record,
        "index_seconds": index_seconds,
        "reindex_seconds": reindex_seconds,
        "memory_peak_bytes": _memory_peak_bytes(),
        "methods": {
            name: {key: value for key, value in payload.items() if key != "cases"}
            | {"case_count": len(payload.get("cases") or [])}
            for name, payload in methods.items()
        },
        "method_cases": {name: payload.get("cases") for name, payload in methods.items()},
        "review_thresholds": _thresholds(methods),
        "result_path": str(output),
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    return result


def _thresholds(methods: dict) -> dict:
    """Record measured baselines. These are review references, not tuning targets."""

    def pick(method, field):
        summary = (methods.get(method) or {}).get("summary") or {}
        return summary.get(field)

    return {
        "note": (
            "Set from this measured run before any ranker change. Critical "
            "permission and provenance failures remain blocking. Do not tune "
            "on held-out answers."
        ),
        "words": {
            "recall_at_10": pick("words", "recall_at_10"),
            "mrr": pick("words", "mrr"),
            "passed": pick("words", "passed"),
        },
        "meaning": {
            "recall_at_10": pick("meaning", "recall_at_10"),
            "mrr": pick("meaning", "mrr"),
            "passed": pick("meaning", "passed"),
        },
        "combined": {
            "recall_at_10": pick("combined", "recall_at_10"),
            "mrr": pick("combined", "mrr"),
            "passed": pick("combined", "passed"),
        },
        "meaning_noboost": {
            "recall_at_10": pick("meaning_noboost", "recall_at_10"),
            "mrr": pick("meaning_noboost", "mrr"),
        },
        "critical_failures_block_adoption": True,
    }
