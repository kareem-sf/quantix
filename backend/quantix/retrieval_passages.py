"""Bounded retrieval passages: literal excerpts plus search-only context.

Embedding text may include a heading, table header or previous-page tail.
Citation offsets always point at the original evidence text. Prefixes are
applied once by the caller. Structure that was not observed is not invented.
"""

from __future__ import annotations

import hashlib
import re
import unicodedata
from dataclasses import dataclass, field

from .semantic_models import SemanticUnavailable

CHUNK_CHARS = 1200
CHUNK_OVERLAP = 120
CONTEXT_CHARS = 240
NEIGHBOR_CHARS = 180
MAX_EMBED_TOKENS = 480
PASSAGE_PREFIX = "passage: "
QUERY_PREFIX = "query: "

_HARAKAT = re.compile(r"[\u064b-\u065f\u0670\u0640]")
_UNIT = re.compile(
    r"(?<![\w.])(\d+(?:[.,]\d+)?)\s*(mm|cm|m2|m3|m²|m³|kg|t|mpa|kn)\b",
    re.IGNORECASE,
)
_IDENTIFIER = re.compile(r"[A-Za-z]*\d+(?:[./][A-Za-z]*\d+)+[A-Za-z]*")


@dataclass
class Passage:
    start: int
    end: int
    excerpt: str
    embed_text: str
    structure: dict
    context_prefix: str = ""
    context_uncertain: bool = False
    context_locator: str | None = None
    extras: dict = field(default_factory=dict)

    @property
    def excerpt_sha256(self) -> str:
        return hashlib.sha256(self.excerpt.encode("utf-8")).hexdigest()


def search_form(text: str) -> str:
    """Conservative Arabic/Latin search form. Does not rewrite stored evidence."""

    value = unicodedata.normalize("NFC", str(text or ""))
    value = _HARAKAT.sub("", value)
    value = value.replace("أ", "ا").replace("إ", "ا").replace("آ", "ا").replace("ى", "ي")
    return value.casefold()


def structure_from_row(row: dict) -> dict:
    metadata = row.get("metadata") if isinstance(row.get("metadata"), dict) else {}
    observed = {
        "heading": metadata.get("heading") or metadata.get("section_heading"),
        "heading_level": metadata.get("heading_level"),
        "block_kind": metadata.get("block_kind") or row.get("kind"),
        "page": row.get("page"),
        "sheet": row.get("sheet"),
        "cell_range": row.get("cell_range"),
        "locator": row.get("locator"),
        "style_name": metadata.get("style_name"),
        "header_uncertain": metadata.get("header_uncertain"),
        "structure_uncertain": metadata.get("structure_uncertain"),
        "row_role": metadata.get("row_role"),
        "column_interpretation": metadata.get("column_interpretation"),
    }
    return {key: value for key, value in observed.items() if value not in (None, "", [])}


def _header_context(row: dict) -> tuple[str, bool]:
    metadata = row.get("metadata") if isinstance(row.get("metadata"), dict) else {}
    headers = metadata.get("header_cells") or []
    if not headers:
        return "", bool(metadata.get("header_uncertain"))
    parts = []
    for item in headers:
        if not isinstance(item, dict):
            continue
        label = str(item.get("label") or "").strip()
        coordinate = str(item.get("column") or "").strip()
        if label:
            parts.append(f"{coordinate}: {label}" if coordinate else label)
    prefix = " | ".join(parts)[:CONTEXT_CHARS]
    return prefix, bool(metadata.get("header_uncertain", True))


def _section_context(row: dict) -> str:
    metadata = row.get("metadata") if isinstance(row.get("metadata"), dict) else {}
    if metadata.get("block_kind") == "heading":
        return ""
    heading = str(metadata.get("section_heading") or metadata.get("heading") or "").strip()
    return heading[:CONTEXT_CHARS]


def _neighbor_context(previous: dict | None, row: dict) -> tuple[str, str | None]:
    if not previous or previous.get("artifact_id") != row.get("artifact_id"):
        return "", None
    prev_page, page = previous.get("page"), row.get("page")
    if prev_page is None or page is None or int(page) != int(prev_page) + 1:
        return "", None
    tail = str(previous.get("text") or "")[-NEIGHBOR_CHARS:].strip()
    if not tail:
        return "", None
    return tail, previous.get("locator")


def context_for(row: dict, previous: dict | None = None) -> tuple[str, bool, str | None]:
    parts = []
    uncertain = bool((row.get("metadata") or {}).get("structure_uncertain"))
    section = _section_context(row)
    if section:
        parts.append(section)
    headers, header_uncertain = _header_context(row)
    if headers:
        parts.append(headers)
        uncertain = uncertain or header_uncertain
    neighbor, locator = _neighbor_context(previous, row)
    if neighbor:
        parts.append(neighbor)
        uncertain = True
    prefix = "\n".join(part for part in parts if part).strip()
    return prefix[: CONTEXT_CHARS + NEIGHBOR_CHARS], uncertain, locator


def _split_bounds(text: str, model, extra_prefix: str):
    start = 0
    while start < len(text):
        end = min(start + CHUNK_CHARS, len(text))
        if end < len(text):
            boundaries = (
                text.rfind("\n\n", start, end),
                text.rfind("\n", start, end),
                text.rfind(". ", start, end),
            )
            preferred = max(boundaries)
            if preferred > start + max(80, (end - start) // 2):
                end = preferred + (2 if text[preferred : preferred + 2] == ". " else 0)
        while True:
            excerpt = text[start:end]
            embed = extra_prefix + excerpt if extra_prefix else excerpt
            count = model.token_count(PASSAGE_PREFIX + embed)
            if count < MAX_EMBED_TOKENS:
                break
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


def passages_for_evidence(row: dict, *, model, previous: dict | None = None):
    text = str(row.get("text") or "")
    if not text.strip():
        return
    prefix, uncertain, context_locator = context_for(row, previous)
    extra = prefix + "\n" if prefix else ""
    structure = structure_from_row(row)
    for start, end, excerpt in _split_bounds(text, model, extra):
        yield Passage(
            start=start,
            end=end,
            excerpt=excerpt,
            embed_text=(extra + excerpt) if extra else excerpt,
            structure=structure,
            context_prefix=prefix,
            context_uncertain=uncertain,
            context_locator=context_locator,
        )


def prefix_once(kind: str, text: str) -> str:
    prefix = PASSAGE_PREFIX if kind == "passage" else QUERY_PREFIX
    if text.startswith(prefix):
        return text
    return prefix + text


def unit_pairs(text: str) -> set[tuple[str, str]]:
    pairs = set()
    for amount, unit in _UNIT.findall(search_form(text)):
        pairs.add((amount.replace(",", "."), unit.lower().replace("²", "2").replace("³", "3")))
    return pairs


def lexical_features(query: str, text: str) -> dict[str, float]:
    """Identifier/phrase/unit overlap. Not numerical or commercial interpretation."""

    query_text, passage = str(query or ""), str(text or "")
    q_form, p_form = search_form(query_text), search_form(passage)
    identifiers = {item.casefold() for item in _IDENTIFIER.findall(query_text)}
    identifier_hit = 0.0
    if identifiers:
        haystack = passage.casefold()
        identifier_hit = 1.0 if any(item in haystack for item in identifiers) else 0.0
    words = [item for item in re.findall(r"[^\W_]+", q_form) if len(item) > 2][:8]
    phrase_hit = 0.0
    if len(words) >= 2:
        phrase = " ".join(words[:6])
        phrase_hit = 1.0 if phrase in p_form else 0.0
    q_units, p_units = unit_pairs(query_text), unit_pairs(passage)
    unit_hit = 1.0 if q_units and q_units & p_units else 0.0
    return {
        "identifier": identifier_hit,
        "phrase": phrase_hit,
        "unit": unit_hit,
        "score": 0.03 * identifier_hit + 0.015 * phrase_hit + 0.02 * unit_hit,
    }
