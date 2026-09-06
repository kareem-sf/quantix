"""Semantic retrieval behavior uses deterministic vectors only in these tests."""

import importlib
import importlib.util
import subprocess
import sys
import time

import numpy as np
import pytest

from quantix.repository import Repository


class FakeEmbedding:
    def __init__(self):
        self.passages = []

    def token_count(self, text):
        return len(text) // 4

    def passage_embed(self, texts, **kwargs):
        for text in texts:
            assert text.startswith("passage: ")
            self.passages.append(text)
            vector = np.zeros(384, dtype=np.float32)
            vector[0 if "membrane" in text.lower() else 1] = 1
            yield vector

    def query_embed(self, query, **kwargs):
        assert query.startswith("query: ")
        vector = np.zeros(384, dtype=np.float32)
        vector[0 if "rain" in query.lower() else 1] = 1
        yield vector


@pytest.fixture
def setup(tmp_path, monkeypatch):
    assert importlib.util.find_spec("quantix.semantic"), "Semantic retrieval is missing"
    module = importlib.import_module("quantix.semantic")
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic tender")["id"]
    fake = FakeEmbedding()
    monkeypatch.setattr(module, "model_available", lambda path: True)
    monkeypatch.setattr(module, "load_model", lambda path: fake)
    return module, repo, tender, fake


def source(repo, tid, name, text, digest="a" * 64):
    artifact, _ = repo.register_artifact(
        tid,
        name,
        digest,
        100,
        {
            "kind": "pdf",
            "status": "extracted",
            "metadata": {"extractor_version": "test-v1"},
            "segments": [{"locator": "page:7", "text": text, "page": 7}],
        },
    )
    return repo.artifact_evidence(tid, artifact["id"])[0]


def test_meaning_search_uses_vectors_and_preserves_original_evidence(setup):
    module, repo, tid, fake = setup
    waterproof = source(repo, tid, "waterproof.pdf", "Install a continuous waterproof membrane.")
    source(repo, tid, "concrete.pdf", "Concrete shall achieve 30 MPa.", "b" * 64)
    service = module.SemanticService(repo)
    assert service.status(tid)["status"] == "not_indexed"
    assert service.index(tid)["status"] == "ready"
    # No lexical overlap with the winning passage.
    results = service.search(tid, "Keep rain outside", limit=1)
    assert len(results) == 1 and results[0]["id"] == waterproof["id"]
    assert results[0]["page"] == 7 and results[0]["locator"] == "page:7"
    assert results[0]["text"] == waterproof["text"]
    assert results[0]["score"] == pytest.approx(1)
    reopened = module.SemanticService(Repository(repo.home))
    assert reopened.status(tid)["status"] == "ready"
    assert reopened.search(tid, "Keep rain outside", 1)[0]["id"] == waterproof["id"]


def test_duplicates_share_vectors_but_retain_every_scoped_occurrence(setup):
    module, repo, tid, fake = setup
    first = source(repo, tid, "A/spec.pdf", "Waterproof membrane")
    second = source(repo, tid, "B/spec.pdf", "Waterproof membrane")
    other = repo.create_tender("Other tender")["id"]
    foreign = source(repo, other, "foreign.pdf", "Waterproof membrane")
    service = module.SemanticService(repo)
    state = service.index(tid)
    assert state["evidence_count"] == 2 and state["unique_chunks"] == 1
    assert fake.passages == ["passage: Waterproof membrane"]
    hits = service.search(tid, "rain", 10)
    assert {r["id"] for r in hits} == {first["id"], second["id"]}
    assert foreign["id"] not in {r["id"] for r in hits}
    with pytest.raises(module.SemanticUnavailable):
        service.search(other, "rain")
    with pytest.raises(KeyError):
        service.status("unknown")


def test_revised_sources_make_index_stale_and_never_return_superseded_evidence(setup):
    module, repo, tid, fake = setup
    old = source(repo, tid, "spec.pdf", "Waterproof membrane")
    service = module.SemanticService(repo)
    service.index(tid)
    new = source(repo, tid, "spec.pdf", "Concrete revision", "b" * 64)
    assert service.status(tid)["status"] == "stale"
    with pytest.raises(module.SemanticUnavailable) as error:
        service.search(tid, "rain")
    assert error.value.code == "stale"
    service.index(tid)
    assert {r["id"] for r in service.search(tid, "concrete")} == {new["id"]}
    assert old["id"] != new["id"]


def test_collapsed_search_limits_distinct_sources_after_area_filter(setup):
    module, repo, tid, _ = setup
    first = source(repo, tid, "A/spec.pdf", "Waterproof membrane")
    second = source(repo, tid, "B/spec.pdf", "Waterproof membrane")
    third = source(repo, tid, "B/concrete.pdf", "Concrete", "b" * 64)
    service = module.SemanticService(repo)
    service.index(tid)
    hits = service.search(tid, "rain", 2, collapse_duplicates=True)
    assert len(hits) == 2
    assert third["id"] in {hit["id"] for hit in hits}
    assert len({first["id"], second["id"]} & {hit["id"] for hit in hits}) == 1
    area = repo.get_artifact(tid, second["artifact_id"])["area"]
    scoped = service.search(tid, "rain", 1, area=area, collapse_duplicates=True)
    assert scoped[0]["id"] == second["id"]


def test_cancellation_keeps_previous_index_and_reuses_finished_vectors(setup, monkeypatch):
    module, repo, tid, fake = setup
    source(repo, tid, "first.pdf", "Waterproof membrane")
    service = module.SemanticService(repo)
    service.index(tid)
    source(repo, tid, "new.pdf", "Concrete", "b" * 64)
    phases = []
    with pytest.raises(InterruptedError):
        service.index(
            tid,
            cancelled=lambda: bool(phases),
            progress=lambda percent, detail: phases.append(detail),
        )
    assert service.status(tid)["status"] == "stale"
    before = len(fake.passages)
    service.index(tid)
    assert len(fake.passages) == before + 1


def test_model_and_extractor_fingerprints_invalidate_persisted_vectors(setup, monkeypatch):
    module, repo, tid, fake = setup
    source(repo, tid, "a.pdf", "Waterproof membrane")
    service = module.SemanticService(repo)
    service.index(tid)
    monkeypatch.setattr(module, "EXTRACTOR_FINGERPRINT", "changed-parser")
    assert service.status(tid)["status"] == "stale"
    service.index(tid)
    monkeypatch.setattr(module, "MODEL_FINGERPRINT", "changed-model")
    assert service.status(tid)["status"] == "stale"
    service.index(tid)
    assert len(fake.passages) == 2


def test_missing_model_or_index_never_falls_back_to_keyword_search(setup, monkeypatch):
    module, repo, tid, fake = setup
    source(repo, tid, "a.pdf", "Waterproof membrane")
    monkeypatch.setattr(module, "model_available", lambda path: False)
    monkeypatch.setattr(repo, "search", lambda *args: pytest.fail("Keyword fallback is forbidden"))
    service = module.SemanticService(repo)
    assert service.status(tid)["status"] == "model_missing"
    with pytest.raises(module.SemanticUnavailable) as error:
        service.search(tid, "rain")
    assert error.value.code == "model_missing"


def test_chunks_are_bounded_and_keep_source_offsets_without_replacing_original_text(
    setup, monkeypatch
):
    module, repo, tid, fake = setup
    monkeypatch.setattr(module, "CHUNK_CHARS", 100)
    text = "Waterproof membrane. " * 30
    evidence = source(repo, tid, "long.pdf", text)
    service = module.SemanticService(repo)
    state = service.index(tid)
    assert state["chunk_count"] > 1
    assert all(len(p.removeprefix("passage: ")) <= 100 for p in fake.passages)
    hit = service.search(tid, "rain", 1)[0]
    assert hit["text"] == text and hit["id"] == evidence["id"]
    match = hit["metadata"]["semantic_match"]
    assert text[match["start"] : match["end"]] == match["text"]


def test_invalid_vectors_and_index_bounds_are_visible_without_publishing(setup, monkeypatch):
    module, repo, tid, fake = setup
    source(repo, tid, "a.pdf", "Waterproof membrane")
    monkeypatch.setattr(fake, "passage_embed", lambda *args, **kwargs: iter([np.full(384, np.nan)]))
    service = module.SemanticService(repo)
    with pytest.raises(module.SemanticUnavailable):
        service.index(tid)
    assert service.status(tid)["status"] == "not_indexed"
    monkeypatch.setattr(module, "MAX_EVIDENCE", 0)
    with pytest.raises(module.SemanticUnavailable) as error:
        service.index(tid)
    assert error.value.code == "limit_exceeded"


def test_source_change_during_embedding_prevents_false_ready_state(setup, monkeypatch):
    module, repo, tid, fake = setup
    source(repo, tid, "a.pdf", "Waterproof membrane")
    embed = fake.passage_embed

    def change(texts, **kwargs):
        yield from embed(texts, **kwargs)
        source(repo, tid, "a.pdf", "Changed concrete", "b" * 64)

    monkeypatch.setattr(fake, "passage_embed", change)
    service = module.SemanticService(repo)
    with pytest.raises(module.SemanticUnavailable) as error:
        service.index(tid)
    assert error.value.code == "source_changed"
    assert service.status(tid)["status"] != "ready"


def test_missing_stored_vectors_make_index_unavailable_instead_of_empty_results(setup):
    module, repo, tid, fake = setup
    source(repo, tid, "a.pdf", "Waterproof membrane")
    service = module.SemanticService(repo)
    service.index(tid)
    with service._connect() as conn:
        conn.execute("DELETE FROM vectors")
    assert service.status(tid)["ready"] is False
    with pytest.raises(module.SemanticUnavailable):
        service.search(tid, "rain")


def test_model_download_cancellation_stops_child_without_customer_text(setup, monkeypatch):
    module, repo, tid, fake = setup
    monkeypatch.setattr(module, "model_available", lambda path: False)
    actual = subprocess.Popen
    processes = []

    def process(command, **kwargs):
        if command[-2:] == ["/T", "/F"]:
            return actual(command, **kwargs)
        assert command[-2] == "download"
        assert "HF_HUB_DISABLE_IMPLICIT_TOKEN" in kwargs["env"]
        child = actual([sys.executable, "-c", "import time; time.sleep(30)"], **kwargs)
        processes.append(child)
        return child

    monkeypatch.setattr(module.subprocess, "Popen", process)
    started = time.monotonic()
    with pytest.raises(InterruptedError):
        module._ensure_model(
            repo.home / "models" / "test", cancelled=lambda: time.monotonic() - started > 0.2
        )
    assert time.monotonic() - started < 3
    assert processes and all(p.poll() is not None for p in processes)
