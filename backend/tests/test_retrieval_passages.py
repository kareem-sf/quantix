"""Passage context, search-only normalization and lexical features."""

from quantix.repository import Repository
from quantix.retrieval_passages import (
    PASSAGE_PREFIX,
    lexical_features,
    passages_for_evidence,
    prefix_once,
    search_form,
    unit_pairs,
)
from quantix.retrieval_ranking import apply_lexical_features, unsupported_reason
from quantix.retrieval_service import retrieve


class TinyModel:
    def token_count(self, text):
        return len(text) // 4


def test_search_form_is_conservative_and_does_not_equate_units():
    assert "ـ" not in search_form("الخرسانـة")
    assert search_form("إنتاج") == search_form("انتاج")
    assert unit_pairs("cover 20 mm") != unit_pairs("length 20 m")
    assert ("20", "mm") in unit_pairs("20 mm cover")
    assert lexical_features("20 mm cover", "Minimum cover is 20 mm.")["unit"] == 1.0
    assert lexical_features("20 mm cover", "The wall is 20 m long.")["unit"] == 0.0


def test_identifier_and_phrase_features_do_not_rewrite_evidence():
    text = "Clause 14.7.1 requires an advance payment guarantee."
    features = lexical_features("clause 14.7.1 guarantee", text)
    assert features["identifier"] == 1.0
    assert lexical_features("advance payment guarantee", text)["phrase"] == 1.0
    assert text.endswith("guarantee.")


def test_prefix_is_applied_exactly_once():
    once = prefix_once("passage", "Roof membrane")
    assert once.startswith(PASSAGE_PREFIX)
    assert prefix_once("passage", once) == once
    assert prefix_once("query", "query: rain") == "query: rain"


def test_passages_keep_literal_offsets_when_context_is_prefixed():
    row = {
        "id": "e1",
        "text": "Provide a four millimetre SBS membrane to the roof slab.",
        "metadata": {"section_heading": "Roof waterproofing", "block_kind": "word_paragraph"},
        "locator": "paragraph:2",
    }
    passages = list(passages_for_evidence(row, model=TinyModel()))
    assert passages
    passage = passages[0]
    assert passage.excerpt.startswith("Provide a four")
    assert passage.embed_text.startswith("Roof waterproofing")
    assert row["text"][passage.start : passage.end] == passage.excerpt
    assert PASSAGE_PREFIX not in passage.embed_text


def test_cross_page_context_is_flagged_uncertain():
    previous = {
        "id": "p1",
        "artifact_id": "a",
        "page": 3,
        "locator": "page:3",
        "text": "Unless stated otherwise, membranes are torch-applied. Do not use cold adhesive.",
    }
    row = {
        "id": "p2",
        "artifact_id": "a",
        "page": 4,
        "locator": "page:4",
        "text": "Provide the roof waterproofing membrane specified on this page.",
        "metadata": {"structure_uncertain": True},
    }
    passage = next(passages_for_evidence(row, model=TinyModel(), previous=previous))
    assert "torch-applied" in passage.embed_text
    assert passage.excerpt.startswith("Provide the roof")
    assert passage.context_uncertain is True
    assert passage.context_locator == "page:3"


def test_chunk_boundary_does_not_drop_a_negation():
    text = ("A" * 1100) + " Delivery hose is excluded from the rate."
    passages = list(passages_for_evidence({"text": text}, model=TinyModel()))
    joined = " ".join(item.excerpt for item in passages)
    assert "excluded" in joined
    assert all(text[item.start : item.end] == item.excerpt for item in passages)


def test_lexical_rerank_prefers_the_identifier_hit():
    hits = [
        {"id": "a", "text": "General payment terms.", "score": 0.02, "relative_path": "z.pdf"},
        {
            "id": "b",
            "text": "Clause 8.3.2 retention is five percent.",
            "score": 0.02,
            "relative_path": "a.pdf",
        },
    ]
    ranked = apply_lexical_features(hits, "clause 8.3.2 retention")
    assert ranked[0]["id"] == "b"


def test_unsupported_answer_is_not_a_cosine_cutoff(tmp_path):
    repo = Repository(tmp_path / "home")
    tender = repo.create_tender("Absent")
    body = b"Waterproof membrane to the roof."
    import hashlib

    artifact, _ = repo.register_artifact(
        tender["id"],
        "Spec/roof.pdf",
        hashlib.sha256(body).hexdigest(),
        len(body),
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [{"locator": "page:1", "text": body.decode()}],
        },
    )
    response = retrieve(repo, tender["id"], "orbital period of Neptune", mode="words")
    assert response.coverage.unsupported_answer is True
    assert response.coverage.unsupported_reason == "no_match"
    assert unsupported_reason([], weak=False, meaning_status="ready", actual="words") == "no_match"
    assert artifact["id"]
