"""Hybrid Tender retrieval: meaning search first, fused with exact keyword matches.

Meaning (local multilingual E5 vectors) finds the passage whatever the wording or
language; keywords keep identifiers, clause numbers, grades and quantities exact.
Results are fused with reciprocal-rank fusion, deduplicated by file content and
locator, and the permitted scope is applied before ranking so a restricted
colleague never loses a permitted passage to one it may not see.

Scores are not confidence. A result found only by meaning, in a search where no
passage shares the query's words and the meaning scores are flat, is marked
`weak_match`: it is a place to look, not evidence that the answer exists.
"""

from __future__ import annotations

from collections.abc import Callable
from typing import TYPE_CHECKING, Literal

if TYPE_CHECKING:
    from .repository import Repository

RRF_K = 60
WEAK_SPREAD = 0.015

Mode = Literal["auto", "meaning", "exact"]


def hybrid_search(
    repo: "Repository",
    tender_id: str,
    query: str,
    limit: int = 8,
    *,
    mode: Mode = "auto",
    semantic=None,
    allowed: Callable[[str], bool] | None = None,
    area: str | None = None,
    status: str | None = None,
) -> tuple[list[dict], dict]:
    """Return fused hits and how the search was done.

    Each hit is a Tender evidence record with `found_by` ("meaning", "words" or
    both), `score` (fusion rank score), an optional `meaning_span` and `weak_match`.
    """

    limit = max(1, min(int(limit), 50))
    filters = {key: value for key, value in (("area", area), ("status", status)) if value is not None}
    # A scope filter removes candidates, so gather a wider pool before filtering.
    pool = 100 if allowed is not None else limit * 4
    words = [] if mode == "meaning" else repo.search(tender_id, query, limit=pool, **filters)
    meaning, meaning_state = [], "not_requested" if mode == "exact" else "unavailable"
    if mode != "exact":
        try:
            if semantic is None:
                from .semantic import SemanticService

                semantic = SemanticService(repo)
            meaning = semantic.search(tender_id, query, limit=100 if allowed is not None else limit * 3,
                                      collapse_duplicates=True, **filters)
            meaning_state = "ready"
        except Exception as error:  # noqa: BLE001 - keyword results still answer; the state says why
            meaning_state = getattr(error, "code", "unavailable")
    if allowed is not None:
        words = [hit for hit in words if allowed(hit["id"])]
        meaning = [hit for hit in meaning if allowed(hit["id"])]

    hashes = {artifact["id"]: artifact.get("content_hash") or artifact["id"] for artifact in repo.list_artifacts(tender_id)}
    fused: dict[tuple, dict] = {}
    for source, hits in (("meaning", meaning), ("words", words)):
        for rank, hit in enumerate(hits):
            artifact_id = hit.get("artifact_id") or hit["id"]
            identity = (hashes.get(artifact_id, artifact_id), hit.get("locator") or hit["id"])
            entry = fused.setdefault(identity, {"hit": hit, "score": 0.0, "found_by": set()})
            entry["score"] += 1 / (RRF_K + rank + 1)
            entry["found_by"].add(source)
            if source == "meaning":
                entry["meaning"] = (hit.get("metadata") or {}).get("semantic_match") or {}

    meaning_scores = sorted((m.get("semantic_match", {}).get("semantic_score", 0.0)
                             for m in (hit.get("metadata") or {} for hit in meaning)), reverse=True)
    spread = (meaning_scores[0] - sum(meaning_scores[:20]) / len(meaning_scores[:20])) if meaning_scores else 0.0
    flat_meaning = not words and spread < WEAK_SPREAD

    results = []
    for entry in sorted(fused.values(), key=lambda item: -item["score"])[:limit]:
        hit = dict(entry["hit"])
        found_by = sorted(entry["found_by"])
        hit["score"] = round(entry["score"], 6)
        hit["found_by"] = "+".join(found_by)
        span = entry.get("meaning") or {}
        if span:
            hit["meaning_span"] = {"start": span.get("start", 0), "end": span.get("end", 0),
                                   "heading": (span.get("structure") or {}).get("heading")}
        hit["weak_match"] = found_by == ["meaning"] and flat_meaning
        results.append(hit)
    info = {
        "mode": mode,
        "meaning": meaning_state,
        "keyword_hits": len(words),
        "meaning_hits": len(meaning),
        "weak_results": bool(results) and all(hit["weak_match"] for hit in results),
    }
    return results, info
