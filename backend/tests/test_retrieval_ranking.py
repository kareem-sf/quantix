"""Distinct selection, kind filters, ties and multiple spans happen before the limit."""

import hashlib

import numpy as np

from quantix.repository import Repository
from quantix.retrieval_ranking import RANKING_VERSION, decode_cursor
from quantix.retrieval_service import retrieve


def _source(repo, tender_id, path, text, *, kind="pdf", locator="page:1"):
    body = text.encode()
    artifact, _ = repo.register_artifact(
        tender_id,
        path,
        hashlib.sha256(body).hexdigest(),
        len(body),
        {
            "kind": kind,
            "status": "extracted",
            "segments": [{"locator": locator, "text": text, "page": 1}],
        },
    )
    return artifact, repo.artifact_evidence(tender_id, artifact["id"])[0]


def test_duplicate_window_returns_distinct_matches_not_only_copies(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Window")
    for index in range(40):
        _source(
            repo,
            tender["id"],
            f"Copies/spec-{index:02d}.pdf",
            "Waterproof membrane to the roof slab.",
        )
    unique_artifact, unique = _source(
        repo, tender["id"], "Other/curing.pdf", "Waterproof curing of the roof slab membrane."
    )
    hits = repo.search(tender["id"], "waterproof membrane", limit=2)
    ids = {hit["id"] for hit in hits}
    assert len(hits) == 2
    assert unique["id"] in ids
    assert unique_artifact["id"] in {hit["artifact_id"] for hit in hits}


def test_document_kind_is_applied_before_the_result_limit(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Kinds")
    for index in range(8):
        _source(
            repo,
            tender["id"],
            f"Drawings/sheet-{index}.pdf",
            "Concrete grade C30/37 blinding.",
            kind="pdf",
        )
    sheet, row = _source(
        repo,
        tender["id"],
        "Estimate/boq.xlsx",
        "Concrete grade C30/37 blinding quantity 12.5",
        kind="xlsx",
    )
    response = retrieve(repo, tender["id"], "C30/37", mode="words", limit=3, document_kind="xlsx")
    assert [hit.artifact_id for hit in response.hits] == [sheet["id"]]
    assert response.hits[0].id == row["id"]


def test_equal_ranks_break_ties_by_path_then_id(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Ties")
    _source(
        repo,
        tender["id"],
        "Zulu/spec.pdf",
        "The waterproofing clause requires a torch-applied membrane in Zulu.",
    )
    _source(
        repo,
        tender["id"],
        "Alpha/spec.pdf",
        "The waterproofing clause requires a torch-applied membrane in Alpha.",
    )
    response = retrieve(repo, tender["id"], "waterproofing clause", mode="words", limit=2)
    paths = [hit.relative_path for hit in response.hits]
    assert paths == sorted(paths)


def test_identifier_and_filename_queries_match(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Ids")
    _source(repo, tender["id"], "Structure/grades.pdf", "Foundations use concrete class C30/37.")
    named, _ = _source(
        repo, tender["id"], "Roof/waterproof-en.pdf", "Provide a four millimetre SBS membrane."
    )
    grades = retrieve(repo, tender["id"], "C30/37", mode="words")
    assert grades.hits
    files = retrieve(repo, tender["id"], "waterproof-en.pdf", mode="words")
    assert named["id"] in {hit.artifact_id for hit in files.hits}


def test_continuation_reports_truncation_without_claiming_exhaustion(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Page")
    for index in range(6):
        _source(
            repo,
            tender["id"],
            f"Spec/clause-{index}.pdf",
            f"Retention clause {index} five percent.",
        )
    first = retrieve(repo, tender["id"], "retention clause", mode="words", limit=2)
    assert first.continuation
    assert first.ranking_version == RANKING_VERSION
    payload = decode_cursor(first.continuation)
    assert payload["skip"] == 2
    second = retrieve(
        repo, tender["id"], "retention clause", mode="words", limit=2, cursor=first.continuation
    )
    first_ids = {hit.id for hit in first.hits}
    second_ids = {hit.id for hit in second.hits}
    assert first_ids.isdisjoint(second_ids)


def test_one_source_keeps_multiple_non_overlapping_spans(tmp_path, monkeypatch):
    from quantix import semantic

    class FakeEmbedding:
        def token_count(self, text):
            return len(text) // 4

        def passage_embed(self, texts, **_kwargs):
            for text in texts:
                vector = np.zeros(384, dtype=np.float32)
                vector[0 if "membrane" in text.lower() else 1] = 1
                yield vector

        def query_embed(self, query, **_kwargs):
            vector = np.zeros(384, dtype=np.float32)
            vector[0] = 1
            yield vector

    repo = Repository(tmp_path)
    tender = repo.create_tender("Spans")
    body = (
        "Install a continuous waterproof membrane."
        + (" padding." * 400)
        + " Seal the membrane laps."
    )
    _source(repo, tender["id"], "Roof/spec.pdf", body)
    monkeypatch.setattr(semantic, "model_available", lambda _path: True)
    monkeypatch.setattr(semantic, "load_model", lambda _path: FakeEmbedding())
    service = semantic.SemanticService(repo)
    assert service.index(tender["id"])["status"] == "ready"
    hits = service.search(tender["id"], "Keep rain outside", limit=1)
    spans = hits[0]["metadata"]["semantic_match"]["spans"]
    assert len(spans) >= 2
    assert spans[0]["end"] <= spans[1]["start"]
