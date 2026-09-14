"""Scoped Tender retrieval used by HTTP and agent tools.

Permitted artifacts are applied in candidate SQL before ranking. An empty
permitted list is no access, never unrestricted. Duplicate copies share one
result slot and keep their other occurrences for inspection.
"""

from __future__ import annotations

from collections.abc import Callable, Sequence
from typing import TYPE_CHECKING

from .retrieval_models import (
    RetrievalCoverage,
    RetrievalHit,
    RetrievalOccurrence,
    RetrievalOpenTarget,
    RetrievalResponse,
    RetrievalSpan,
)
from .retrieval_ranking import (
    KEYWORD_SCAN_CEILING,
    RANKING_VERSION,
    apply_lexical_features,
    decode_cursor,
    encode_cursor,
    rrf_fuse,
    unsupported_reason,
    weak_meaning_only,
)
from .retrieval_ranking import (
    RRF_K as RRF_K,
)
from .retrieval_ranking import (
    WEAK_SPREAD as WEAK_SPREAD,
)
from .semantic_models import SemanticUnavailable

if TYPE_CHECKING:
    from .repository import Repository

Mode = str


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
    document_kind: str | None = None,
    artifact_ids: Sequence[str] | None = None,
    evidence_ids: Sequence[str] | None = None,
) -> tuple[list[dict], dict]:
    """Compatibility wrapper around retrieve(). Prefer retrieve() for new callers."""

    requested = {"exact": "words", "meaning": "meaning", "auto": "auto", "words": "words"}.get(
        mode, "combined" if mode == "combined" else "auto"
    )
    if mode == "exact":
        requested = "words"
    response = retrieve(
        repo,
        tender_id,
        query,
        mode=requested,
        limit=limit,
        semantic=semantic,
        allowed=allowed,
        area=area,
        status=status,
        document_kind=document_kind,
        artifact_ids=artifact_ids,
        evidence_ids=evidence_ids,
    )
    hits = []
    for item in response.hits:
        hit = item.model_dump()
        hit["found_by"] = item.found_by
        hit["weak_match"] = item.weak_match
        if item.spans:
            primary = item.spans[0]
            hit["meaning_span"] = {
                "start": primary.start,
                "end": primary.end,
                "heading": primary.heading,
            }
        hits.append(hit)
    info = {
        "mode": mode,
        "meaning": response.coverage.meaning_status or "not_requested",
        "keyword_hits": 0,
        "meaning_hits": 0,
        "weak_results": bool(hits) and all(hit.get("weak_match") for hit in hits),
        "truncated": response.coverage.truncated,
        "actual_mode": response.actual_mode,
    }
    return hits, info


def retrieve(
    repo: "Repository",
    tender_id: str,
    query: str,
    *,
    mode: str = "auto",
    limit: int = 20,
    cursor: str | None = None,
    semantic=None,
    allowed: Callable[[str], bool] | None = None,
    area: str | None = None,
    status: str | None = None,
    document_kind: str | None = None,
    artifact_ids: Sequence[str] | None = None,
    evidence_ids: Sequence[str] | None = None,
    collection: str = "tender_evidence",
) -> RetrievalResponse:
    repo.get_tender(tender_id)
    requested = mode if mode in {"auto", "words", "meaning", "combined"} else "auto"
    skip = 0
    if cursor:
        payload = decode_cursor(cursor)
        if payload.get("q") != query or payload.get("mode") != requested:
            raise ValueError("The search continuation does not match this query.")
        skip = max(int(payload.get("skip") or 0), 0)
    limitations: list[str] = []
    if collection != "tender_evidence":
        return RetrievalResponse(
            hits=[],
            requested_mode=requested,  # type: ignore[arg-type]
            actual_mode="words",
            ranking_version=RANKING_VERSION,
            generation=None,
            coverage=RetrievalCoverage(),
            limitations=["Saved-work collections are not part of source search yet."],
        )
    if not str(query).strip():
        return RetrievalResponse(
            hits=[],
            requested_mode=requested,  # type: ignore[arg-type]
            actual_mode="words",
            ranking_version=RANKING_VERSION,
            generation=None,
            coverage=RetrievalCoverage(),
        )

    if artifact_ids is not None:
        artifact_ids = list(artifact_ids)
    if evidence_ids is not None:
        evidence_ids = list(evidence_ids)

    meaning_status = "not_requested"
    meaning_ready = False
    generation = None
    if requested in {"auto", "meaning", "combined"}:
        try:
            if semantic is None:
                from .semantic import SemanticService

                semantic = SemanticService(repo)
            if hasattr(semantic, "status"):
                state = semantic.status(tender_id)
                generation = state.get("source_fingerprint")
                meaning_ready = bool(state.get("ready"))
                meaning_status = "ready" if meaning_ready else state.get("status") or "unavailable"
            else:
                meaning_ready = True
                meaning_status = "ready"
        except SemanticUnavailable as error:
            meaning_ready = False
            meaning_status = getattr(error, "code", "unavailable")
        except Exception as error:  # noqa: BLE001 - words can still answer in auto
            meaning_ready = False
            meaning_status = getattr(error, "code", "unavailable")

    if requested in {"meaning", "combined"} and not meaning_ready:
        raise SemanticUnavailable(
            meaning_status if meaning_status not in {None, "not_requested"} else "unavailable",
            "Prepare meaning search before using this search method.",
        )

    actual = requested
    if requested == "auto":
        if meaning_ready:
            actual = "combined"
        else:
            actual = "words"
            if meaning_status not in {"not_requested", "ready"}:
                limitations.append("Meaning search is not ready; showing exact-word matches.")

    page = max(1, min(int(limit), 50))
    fetch = page + skip
    words: list[dict] = []
    meaning: list[dict] = []
    scanned = 0
    ceiling = KEYWORD_SCAN_CEILING
    truncated = False
    more_keywords = False
    if actual != "meaning":
        keyword_limit = ceiling if allowed is not None else fetch
        words, meta = repo.search_keyword(
            tender_id,
            query,
            limit=keyword_limit,
            area=area,
            status=status,
            document_kind=document_kind,
            artifact_ids=artifact_ids,
            evidence_ids=evidence_ids,
            ceiling=ceiling,
        )
        scanned = meta["scanned"]
        truncated = bool(meta["truncated"])
        more_keywords = bool(meta["more"])
        ceiling = meta["ceiling"]
        words = _permitted(words, allowed)[:fetch]
    if actual != "words":
        meaning_limit = 100 if allowed is not None else fetch
        try:
            meaning = semantic.search(
                tender_id,
                query,
                meaning_limit,
                area=area,
                status=status,
                collapse_duplicates=True,
                document_kind=document_kind,
                artifact_ids=artifact_ids,
                evidence_ids=evidence_ids,
            )
            meaning_status = "ready"
            meaning = _permitted(meaning, allowed)[:fetch]
        except SemanticUnavailable:
            raise
        except Exception as error:  # noqa: BLE001
            if actual == "meaning":
                raise
            meaning_status = getattr(error, "code", "unavailable")
            meaning = []

    def _valid(hit):
        return _revalidate(repo, tender_id, hit, artifact_ids, evidence_ids, allowed)

    words = [hit for hit in (_valid(item) for item in words) if hit]
    meaning = [hit for hit in (_valid(item) for item in meaning) if hit]
    if actual == "words":
        ranked = list(words)
        for hit in ranked:
            hit["found_by"] = hit.get("found_by") or "words"
    elif actual == "meaning":
        ranked = list(meaning)
        for hit in ranked:
            hit["found_by"] = "meaning"
    else:
        ranked = rrf_fuse((("meaning", meaning), ("words", words)), limit=fetch)

    ranked = apply_lexical_features(ranked, query)
    weak = actual != "words" and weak_meaning_only(words, meaning)
    page_hits = ranked[skip : skip + page]
    more = skip + page < len(ranked) or more_keywords or truncated
    hits = [_to_hit(hit, weak) for hit in page_hits]
    continuation = None
    if more and hits:
        continuation = encode_cursor({"q": query, "mode": requested, "skip": skip + len(hits)})
    if truncated:
        limitations.append("Further matches may exist; the search stopped at its work limit.")
    reason = unsupported_reason(hits, weak=weak, meaning_status=meaning_status, actual=actual)
    if reason == "no_match":
        limitations.append("No source passage matched this question.")
    elif reason == "weak_meaning_only":
        limitations.append(
            "No supported source answer was found. These meaning-only results are places to look, not evidence that the answer exists."
        )
    return RetrievalResponse(
        hits=hits,
        requested_mode=requested,  # type: ignore[arg-type]
        actual_mode=actual,  # type: ignore[arg-type]
        ranking_version=RANKING_VERSION,
        generation=generation,
        coverage=RetrievalCoverage(
            truncated=truncated,
            scanned=scanned,
            ceiling=ceiling,
            meaning_status=meaning_status,
            unsupported_answer=reason is not None,
            unsupported_reason=reason,
        ),
        limitations=limitations,
        continuation=continuation,
    )


def _permitted(hits: Sequence[dict], allowed: Callable[[str], bool] | None) -> list[dict]:
    if allowed is None:
        return list(hits)
    kept = []
    for hit in hits:
        if allowed(hit["id"]):
            kept.append(hit)
            continue
        for other in hit.get("_duplicates") or []:
            if allowed(other["id"]):
                promoted = dict(other)
                promoted["_duplicates"] = [
                    item
                    for item in [hit, *(hit.get("_duplicates") or [])]
                    if item.get("id") != other["id"]
                ]
                kept.append(promoted)
                break
    return kept


def _revalidate(
    repo: "Repository",
    tender_id: str,
    hit: dict,
    artifact_ids: Sequence[str] | None,
    evidence_ids: Sequence[str] | None,
    allowed: Callable[[str], bool] | None,
) -> dict | None:
    try:
        evidence = repo.get_evidence(tender_id, hit["id"])
        artifact = repo.get_artifact(tender_id, evidence["artifact_id"])
    except (KeyError, ValueError):
        return None
    if not artifact.get("is_current"):
        return None
    if artifact_ids is not None and artifact["id"] not in set(artifact_ids):
        return None
    if evidence_ids is not None and evidence["id"] not in set(evidence_ids):
        return None
    if allowed is not None and not allowed(evidence["id"]):
        return None
    hit = dict(hit)
    hit["artifact_id"] = artifact["id"]
    hit["artifact_name"] = artifact["name"]
    hit["relative_path"] = artifact["relative_path"]
    hit["content_hash"] = artifact["content_hash"]
    hit["source_version"] = artifact["version"]
    hit["document_kind"] = artifact["kind"]
    hit["locator"] = evidence["locator"]
    hit["text"] = evidence["text"]
    return hit


def _to_hit(hit: dict, weak: bool) -> RetrievalHit:
    found_by = str(hit.get("found_by") or "words")
    match = (hit.get("metadata") or {}).get("semantic_match") or {}
    spans = []
    raw_spans = match.get("spans") or []
    if raw_spans:
        for span in raw_spans:
            spans.append(
                RetrievalSpan(
                    start=int(span.get("start") or 0),
                    end=int(span.get("end") or 0),
                    found_by="meaning",
                    heading=(span.get("structure") or {}).get("heading"),
                )
            )
    elif hit.get("meaning_span"):
        span = hit["meaning_span"]
        spans.append(
            RetrievalSpan(
                start=int(span.get("start") or 0),
                end=int(span.get("end") or 0),
                found_by="meaning",
                heading=span.get("heading"),
            )
        )
    elif "meaning" in found_by and match:
        spans.append(
            RetrievalSpan(
                start=int(match.get("start") or 0),
                end=int(match.get("end") or 0),
                found_by="meaning",
                heading=(match.get("structure") or {}).get("heading"),
            )
        )
    duplicates = []
    for other in hit.get("_duplicates") or []:
        duplicates.append(
            RetrievalOccurrence(
                evidence_id=other["id"],
                artifact_id=other.get("artifact_id") or "",
                relative_path=other.get("relative_path") or "",
                locator=other.get("locator") or "",
            )
        )
    metadata = dict(hit.get("metadata") or {})
    return RetrievalHit(
        id=hit["id"],
        artifact_id=hit["artifact_id"],
        artifact_name=hit.get("artifact_name") or "",
        relative_path=hit.get("relative_path") or "",
        locator=hit.get("locator") or "",
        text=hit.get("text") or "",
        page=hit.get("page"),
        sheet=hit.get("sheet"),
        cell_range=hit.get("cell_range"),
        kind=hit.get("kind") or "text",
        metadata=metadata,
        score=float(hit.get("score") or 0),
        found_by=found_by,
        weak_match=found_by == "meaning" and weak,
        content_hash=hit.get("content_hash"),
        source_version=hit.get("source_version"),
        document_kind=hit.get("document_kind"),
        extraction_id=hit.get("extraction_id"),
        extraction_current=bool(hit.get("extraction_current", True)),
        spans=spans,
        duplicate_occurrences=duplicates,
        open_target=RetrievalOpenTarget(
            evidence_id=hit["id"],
            artifact_id=hit["artifact_id"],
            locator=hit.get("locator") or "",
            start=spans[0].start if spans else None,
            end=spans[0].end if spans else None,
        ),
    )
