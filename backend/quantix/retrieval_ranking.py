"""Keyword query shaping, reciprocal-rank fusion and span diversity.

Raw FTS and cosine scores stay on their own channels. Fusion uses ranks only.
"""

from __future__ import annotations

import base64
import json
import re
from collections.abc import Iterable, Mapping, Sequence

RRF_K = 60
WEAK_SPREAD = 0.015
RANKING_VERSION = "rrf-2-features"
KEYWORD_SCAN_CEILING = 2000
MAX_SPANS = 3
_TERM = re.compile(r"[^\W_]+(?:[-.][^\W_]+)*", re.UNICODE)
_IDENTIFIER = re.compile(r"[A-Za-z]*\d+(?:[./][A-Za-z]*\d+)+[A-Za-z]*")


def keyword_expression(query: str) -> str:
    """Quoted OR of tokens, plus identifier/phrase variants. Never rewrites evidence."""

    terms = _TERM.findall(str(query))[:16]
    extras = _IDENTIFIER.findall(str(query))
    tokens: list[str] = []
    for token in [*terms, *extras]:
        if token not in tokens:
            tokens.append(token)
        translated = token.translate(_WESTERN_DIGITS)
        if translated != token and translated not in tokens:
            tokens.append(translated)
        if len(tokens) == 16:
            break
    if not tokens:
        return ""
    quoted = ['"' + token.replace('"', '""') + '"' for token in tokens]
    expression = " OR ".join(quoted)
    if len(quoted) > 1:
        expression = f"({expression}) OR ({' '.join(quoted[:8])})"
    return expression


_WESTERN_DIGITS = str.maketrans("0123456789", "٠١٢٣٤٥٦٧٨٩")


def filename_needle(query: str) -> str:
    value = str(query or "").strip().casefold()
    return value if len(value) >= 2 else ""


def apply_lexical_features(hits: Sequence[Mapping], query: str) -> list[dict]:
    """Small identifier/phrase/unit features on fused ranks. Not cosine confidence."""

    from .retrieval_passages import lexical_features

    scored = []
    for hit in hits:
        features = lexical_features(query, str(hit.get("text") or ""))
        item = dict(hit)
        item["_lexical"] = features
        item["score"] = round(float(hit.get("score") or 0) + features["score"], 6)
        scored.append(item)
    return sorted(
        scored,
        key=lambda item: (
            -item["score"],
            str(item.get("relative_path") or ""),
            str(item.get("id") or ""),
        ),
    )


def unsupported_reason(
    hits: Sequence[Mapping],
    *,
    weak: bool,
    meaning_status: str | None,
    actual: str,
) -> str | None:
    if actual == "meaning" and meaning_status not in {None, "ready", "not_requested"}:
        return "index_unavailable"
    if not hits:
        return "no_match"

    def _weak(hit):
        if isinstance(hit, Mapping):
            return bool(hit.get("weak_match"))
        return bool(getattr(hit, "weak_match", False))

    if weak and all(_weak(hit) for hit in hits):
        return "weak_meaning_only"
    return None


def identity_key(hit: Mapping) -> tuple[str, str]:
    digest = hit.get("content_hash") or hit.get("artifact_id") or hit["id"]
    return (str(digest), str(hit.get("locator") or hit["id"]))


def rrf_fuse(
    channels: Sequence[tuple[str, Sequence[Mapping]]],
    *,
    limit: int,
) -> list[dict]:
    fused: dict[tuple[str, str], dict] = {}
    for channel, hits in channels:
        for rank, hit in enumerate(hits):
            key = identity_key(hit)
            entry = fused.setdefault(
                key, {"hit": dict(hit), "score": 0.0, "found_by": set(), "duplicates": []}
            )
            entry["score"] += 1 / (RRF_K + rank + 1)
            entry["found_by"].add(channel)
            if channel == "meaning":
                entry["meaning"] = (hit.get("metadata") or {}).get("semantic_match") or {}
            existing = identity_key(entry["hit"])
            if existing == key and hit.get("id") != entry["hit"].get("id"):
                entry["duplicates"].append(hit)
    ordered = sorted(
        fused.values(),
        key=lambda item: (
            -item["score"],
            str(item["hit"].get("relative_path") or ""),
            str(item["hit"].get("id") or ""),
        ),
    )
    results = []
    for entry in ordered[:limit]:
        hit = dict(entry["hit"])
        found_by = sorted(entry["found_by"])
        hit["score"] = round(entry["score"], 6)
        hit["found_by"] = "+".join(found_by)
        hit["_duplicates"] = entry["duplicates"]
        span = entry.get("meaning") or {}
        if span:
            hit["meaning_span"] = {
                "start": span.get("start", 0),
                "end": span.get("end", 0),
                "heading": (span.get("structure") or {}).get("heading"),
            }
        results.append(hit)
    return results


def weak_meaning_only(words: Sequence[Mapping], meaning: Sequence[Mapping]) -> bool:
    scores = sorted(
        (
            float(
                (hit.get("metadata") or {}).get("semantic_match", {}).get("semantic_score") or 0.0
            )
            for hit in meaning
        ),
        reverse=True,
    )
    if not scores or words:
        return False
    sample = scores[:20]
    spread = sample[0] - (sum(sample) / len(sample))
    return spread < WEAK_SPREAD


def non_overlapping_spans(chunks: Iterable[Mapping], *, limit: int = MAX_SPANS) -> list[dict]:
    chosen: list[dict] = []
    for chunk in sorted(
        chunks, key=lambda item: (-float(item["score"]), int(item["start"]), item["id"])
    ):
        start, end = int(chunk["start"]), int(chunk["end"])
        if any(not (end <= item["start"] or start >= item["end"]) for item in chosen):
            continue
        chosen.append(dict(chunk))
        if len(chosen) >= limit:
            break
    return sorted(chosen, key=lambda item: int(item["start"]))


def encode_cursor(payload: dict) -> str:
    raw = json.dumps(payload, separators=(",", ":"), sort_keys=True).encode("utf-8")
    return base64.urlsafe_b64encode(raw).decode("ascii").rstrip("=")


def decode_cursor(value: str) -> dict:
    padded = value + "=" * ((4 - len(value) % 4) % 4)
    try:
        payload = json.loads(base64.urlsafe_b64decode(padded.encode("ascii")))
    except (ValueError, json.JSONDecodeError) as error:
        raise ValueError("The search continuation is not valid.") from error
    if not isinstance(payload, dict):
        raise ValueError("The search continuation is not valid.")
    return payload


def group_distinct(
    rows: Sequence[Mapping],
    *,
    limit: int,
    skip: int = 0,
) -> tuple[list[dict], bool]:
    groups: dict[tuple[str, str], dict] = {}
    order: list[tuple[str, str]] = []
    for row in rows:
        key = identity_key(row)
        if key not in groups:
            groups[key] = {"hit": dict(row), "duplicates": []}
            order.append(key)
        elif row.get("id") != groups[key]["hit"].get("id"):
            groups[key]["duplicates"].append(dict(row))
    selected = order[skip : skip + limit]
    hits = []
    for key in selected:
        item = dict(groups[key]["hit"])
        item["_duplicates"] = groups[key]["duplicates"]
        hits.append(item)
    more = skip + limit < len(order)
    return hits, more
