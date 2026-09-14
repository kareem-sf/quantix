"""Score labeled retrieval rankings. This module does not rank sources.

A result may pass only when the corpus and ranker-config fingerprints match the
expected baseline and no critical permission or provenance failure is present.
Average recall cannot waive those failures. Missing measurements stay unset;
they are never recorded as zero.
"""

from __future__ import annotations

import hashlib
import json
import math
import statistics
from typing import Any, Iterable, Mapping, Sequence

SCHEMA_VERSION = 1
K_VALUES = (5, 10)

CRITICAL = frozenset(
    {
        "corpus_hash_mismatch",
        "config_hash_mismatch",
        "foreign_tender",
        "forbidden_passage",
        "stale_as_current",
        "supported_answer_on_absent",
    }
)


def canonical_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def fingerprint(value: Any) -> str:
    return hashlib.sha256(canonical_json(value).encode("utf-8")).hexdigest()


def ranking_config(bilingual_boost: bool) -> dict[str, Any]:
    from . import retrieval_service, semantic

    return {
        "schema_version": SCHEMA_VERSION,
        "words": "fts5_or_16_tokens_dedupe_hash_locator",
        "meaning": {
            "model_fingerprint": semantic.MODEL_FINGERPRINT,
            "model_name": semantic.MODEL_NAME,
            "model_revision": semantic.MODEL_REVISION,
            "chunk_chars": semantic.CHUNK_CHARS,
            "chunk_overlap": semantic.CHUNK_OVERLAP,
            "bilingual_boost": bilingual_boost,
            "collapse_duplicates": True,
        },
        "combined": {
            "rrf_k": retrieval_service.RRF_K,
            "weak_spread": retrieval_service.WEAK_SPREAD,
        },
    }


def ranking_config_hash(*, bilingual_boost: bool) -> str:
    return fingerprint(ranking_config(bilingual_boost))


def _span_key(item: Mapping[str, Any]) -> tuple[str, str]:
    return (str(item["document"]), str(item["locator"]))


def _grades(question: Mapping[str, Any]) -> dict[tuple[str, str], int]:
    grades: dict[tuple[str, str], int] = {}
    for span in question.get("relevant") or []:
        key = _span_key(span)
        grade = int(span.get("grade", 2))
        grades[key] = max(grades.get(key, 0), grade)
    return grades


def _hit_key(hit: Mapping[str, Any]) -> tuple[str, str] | None:
    document = hit.get("document")
    locator = hit.get("locator")
    if document and locator:
        return (str(document), str(locator))
    return None


def _dcg(grades: Sequence[float]) -> float:
    return sum((2**grade - 1) / math.log2(index + 2) for index, grade in enumerate(grades))


def _ndcg(ranked_grades: Sequence[float], ideal_grades: Sequence[float], k: int) -> float | None:
    ideal = _dcg(sorted(ideal_grades, reverse=True)[:k])
    if ideal == 0:
        return None
    return _dcg(list(ranked_grades)[:k]) / ideal


def evaluate_case(
    question: Mapping[str, Any],
    hits: Sequence[Mapping[str, Any]],
    *,
    corpus_hash: str,
    config_hash: str,
    expected_corpus_hash: str,
    expected_config_hash: str,
    tender_id: str | None = None,
) -> dict[str, Any]:
    """Score one labeled question against an ordered hit list."""

    critical: list[str] = []
    if corpus_hash != expected_corpus_hash:
        critical.append("corpus_hash_mismatch")
    if config_hash != expected_config_hash:
        critical.append("config_hash_mismatch")

    collection = str(question.get("collection") or "tender_evidence")
    available = collection == "tender_evidence" and not question.get("exhaustive")
    permitted = question.get("permitted_documents")
    permitted_set = set(permitted) if permitted is not None else None
    grades = _grades(question)
    ranked_keys: list[tuple[str, str]] = []
    ranked_grades: list[int] = []
    seen: set[tuple[str, str]] = set()

    for hit in hits:
        hit_tender = hit.get("tender_id")
        if tender_id and hit_tender and hit_tender != tender_id:
            critical.append("foreign_tender")
        if hit.get("foreign_tender"):
            critical.append("foreign_tender")
        key = _hit_key(hit)
        if key is None:
            continue
        document, _locator = key
        if permitted_set is not None and document not in permitted_set:
            critical.append("forbidden_passage")
        if hit.get("stale_as_current") or (
            hit.get("is_current") is True and hit.get("superseded") is True
        ):
            critical.append("stale_as_current")
        if key in seen:
            continue
        seen.add(key)
        ranked_keys.append(key)
        ranked_grades.append(grades.get(key, 0))

    unique_critical = list(dict.fromkeys(critical))
    absent = bool(question.get("absent"))
    weak_only = bool(hits) and all(hit.get("weak_match") for hit in hits)
    false_supported = absent and bool(hits) and not weak_only
    if false_supported:
        unique_critical.append("supported_answer_on_absent")
        unique_critical = list(dict.fromkeys(unique_critical))

    ranks = {key: index + 1 for index, key in enumerate(ranked_keys) if key in grades}
    primary = [key for key, grade in grades.items() if grade >= 2]
    targets = primary or list(grades)
    first_rank = min((ranks[key] for key in targets if key in ranks), default=None)
    metrics: dict[str, Any] = {
        "available": available,
        "rank": first_rank,
        "mrr": None
        if not available or absent
        else (0.0 if first_rank is None else 1.0 / first_rank),
        "false_supported": false_supported,
        "all_relevant_found": bool(targets) and all(key in ranks for key in targets),
        "duplicate_diversity": (
            None
            if not ranked_keys
            else round(len(set(ranked_keys[:10])) / min(10, len(ranked_keys)), 4)
        ),
        "identifier_accurate": None,
    }
    if available and not absent:
        for k in K_VALUES:
            found = sum(1 for key in targets if ranks.get(key, 10**9) <= k)
            metrics[f"recall_at_{k}"] = found / len(targets) if targets else None
            metrics[f"ndcg_at_{k}"] = _ndcg(ranked_grades, list(grades.values()), k)
        if question.get("category") in {"clause_numbers", "material_grades", "numeric_units"}:
            metrics["identifier_accurate"] = first_rank == 1
    else:
        for k in K_VALUES:
            metrics[f"recall_at_{k}"] = None
            metrics[f"ndcg_at_{k}"] = None

    passed = (
        available
        and not unique_critical
        and ((not absent and first_rank is not None) or (absent and not false_supported))
    )
    if not available:
        passed = not unique_critical
    return {
        "id": question["id"],
        "split": question.get("split"),
        "category": question.get("category"),
        "query_language": question.get("query_language"),
        "passage_language": question.get("passage_language"),
        "available": available,
        "unavailable_reason": (
            None
            if available
            else (
                "exhaustive_not_topk" if question.get("exhaustive") else f"collection:{collection}"
            )
        ),
        "passed": passed and not unique_critical,
        "critical": unique_critical,
        "relevant_ranks": {f"{doc}#{loc}": ranks.get((doc, loc)) for doc, loc in targets},
        "metrics": metrics,
    }


def _mean(values: Iterable[float | None]) -> float | None:
    present = [value for value in values if value is not None]
    if not present:
        return None
    return statistics.fmean(present)


def aggregate(cases: Sequence[Mapping[str, Any]]) -> dict[str, Any]:
    measured = [case for case in cases if case.get("available") and not case.get("critical")]
    quality = [case for case in measured if case.get("metrics", {}).get("mrr") is not None]
    absent_cases = [
        case
        for case in cases
        if case.get("available")
        and case.get("metrics", {}).get("false_supported") is not None
        and case.get("category") == "unrelated"
    ]
    return {
        "cases": len(cases),
        "measured": len(quality),
        "unavailable": sum(1 for case in cases if not case.get("available")),
        "critical_failures": sum(1 for case in cases if case.get("critical")),
        "recall_at_5": _mean(case["metrics"].get("recall_at_5") for case in quality),
        "recall_at_10": _mean(case["metrics"].get("recall_at_10") for case in quality),
        "ndcg_at_10": _mean(case["metrics"].get("ndcg_at_10") for case in quality),
        "mrr": _mean(case["metrics"].get("mrr") for case in quality),
        "identifier_accuracy": _mean(
            case["metrics"].get("identifier_accurate")
            for case in quality
            if case["metrics"].get("identifier_accurate") is not None
        ),
        "false_supported_rate": _mean(
            float(case["metrics"]["false_supported"]) for case in absent_cases
        ),
        "scoped_recall_at_10": _mean(
            case["metrics"].get("recall_at_10")
            for case in quality
            if case.get("category") == "scoped_forbidden"
        ),
        "passed": not any(case.get("critical") for case in cases),
    }


def evaluate_run(
    questions: Sequence[Mapping[str, Any]],
    rankings: Mapping[str, Sequence[Mapping[str, Any]]],
    *,
    corpus_hash: str,
    config_hash: str,
    expected_corpus_hash: str,
    expected_config_hash: str,
    tender_id: str | None = None,
) -> dict[str, Any]:
    cases = [
        evaluate_case(
            question,
            rankings.get(question["id"], []),
            corpus_hash=corpus_hash,
            config_hash=config_hash,
            expected_corpus_hash=expected_corpus_hash,
            expected_config_hash=expected_config_hash,
            tender_id=tender_id,
        )
        for question in questions
    ]
    by_split: dict[str, list[dict[str, Any]]] = {}
    by_category: dict[str, list[dict[str, Any]]] = {}
    by_language: dict[str, list[dict[str, Any]]] = {}
    for case, question in zip(cases, questions, strict=True):
        by_split.setdefault(str(question.get("split")), []).append(case)
        by_category.setdefault(str(question.get("category")), []).append(case)
        pair = f"{question.get('query_language')}->{question.get('passage_language')}"
        by_language.setdefault(pair, []).append(case)
    summary = aggregate(cases)
    return {
        "schema_version": SCHEMA_VERSION,
        "corpus_hash": corpus_hash,
        "config_hash": config_hash,
        "summary": summary,
        "splits": {name: aggregate(group) for name, group in sorted(by_split.items())},
        "categories": {name: aggregate(group) for name, group in sorted(by_category.items())},
        "languages": {name: aggregate(group) for name, group in sorted(by_language.items())},
        "cases": cases,
    }


def percentile(values: Sequence[float], p: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    index = (len(ordered) - 1) * p
    lower = math.floor(index)
    upper = math.ceil(index)
    if lower == upper:
        return ordered[lower]
    weight = index - lower
    return ordered[lower] * (1 - weight) + ordered[upper] * weight
