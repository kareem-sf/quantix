"""Semantic retrieval preserves structure without the unvalidated bilingual boost."""

from __future__ import annotations

import hashlib

import numpy as np

from quantix.repository import Repository


class TopicEmbedding:
    def token_count(self, text):
        return len(text) // 4

    def passage_embed(self, texts, **_kwargs):
        for text in texts:
            vector = np.zeros(384, dtype=np.float32)
            lowered = text.lower()
            vector[0 if "pump" in lowered or "مضخة" in text else 1] = 1
            yield vector

    def query_embed(self, query, **_kwargs):
        vector = np.zeros(384, dtype=np.float32)
        vector[0] = 1
        yield vector


def _source(repo, tender_id, name, text, *, heading):
    body = text.encode()
    artifact, _ = repo.register_artifact(
        tender_id,
        name,
        hashlib.sha256(body).hexdigest(),
        len(body),
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [
                {
                    "locator": "page:4/table:plant",
                    "page": 4,
                    "kind": "table",
                    "text": text,
                    "metadata": {
                        "heading": heading,
                        "block_kind": "table",
                    },
                }
            ],
        },
    )
    return repo.artifact_evidence(tender_id, artifact["id"])[0]


def test_arabic_query_reranks_english_construction_terms_and_keeps_structure(tmp_path, monkeypatch):
    from quantix import semantic

    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic bilingual Tender")
    pump = _source(
        repo,
        tender["id"],
        "Plant/pump.pdf",
        "Concrete pump output is 70 m3 per hour.\n\nDelivery hose is excluded.",
        heading="Concrete placing plant",
    )
    _source(
        repo,
        tender["id"],
        "Plant/crane.pdf",
        "Tower crane capacity is 12 tonnes.",
        heading="Lifting plant",
    )
    fake = TopicEmbedding()
    monkeypatch.setattr(semantic, "model_available", lambda _path: True)
    monkeypatch.setattr(semantic, "load_model", lambda _path: fake)
    service = semantic.SemanticService(repo)
    assert service.index(tender["id"])["status"] == "ready"

    hit = service.search(tender["id"], "قدرة مضخة الخرسانة", limit=1)[0]
    assert hit["id"] == pump["id"]
    assert hit["text"] == pump["text"]
    assert hit["locator"] == "page:4/table:plant"
    match = hit["metadata"]["semantic_match"]
    assert match["language"] == "en"
    assert match["structure"]["heading"] == "Concrete placing plant"
    assert match["structure"]["block_kind"] == "table"
    assert match["structure"]["page"] == 4
    assert match["structure"]["locator"] == "page:4/table:plant"
    assert match["chunk_sha256"] == hashlib.sha256(match["text"].encode("utf-8")).hexdigest()
    assert match["source_content_hash"] == hashlib.sha256(pump["text"].encode("utf-8")).hexdigest()
    assert match["source_version"] == 1
    assert match["bilingual_lexical_boost"] == 0
