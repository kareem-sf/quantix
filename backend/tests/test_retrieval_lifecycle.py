"""Generation tracking, coalesced maintenance and Words-only while rebuilding."""

import asyncio
import time

import numpy as np
import pytest

from quantix.jobs import JobManager
from quantix.repository import Repository
from quantix.retrieval_indexing import RetrievalIndexer
from quantix.retrieval_service import retrieve
from quantix.semantic import SemanticService
from quantix.semantic_models import SemanticUnavailable
from quantix.settings import SettingsService


class FakeEmbedding:
    def __init__(self):
        self.passages = []

    def token_count(self, text):
        return len(text) // 4

    def passage_embed(self, texts, **kwargs):
        for text in texts:
            self.passages.append(text)
            vector = np.zeros(384, dtype=np.float32)
            vector[0 if "membrane" in text.lower() else 1] = 1
            yield vector

    def query_embed(self, query, **kwargs):
        vector = np.zeros(384, dtype=np.float32)
        vector[0 if "rain" in query.lower() else 1] = 1
        yield vector


@pytest.fixture
def setup(tmp_path, monkeypatch):
    from quantix import semantic

    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic tender")["id"]
    fake = FakeEmbedding()
    monkeypatch.setattr(semantic, "model_available", lambda path: True)
    monkeypatch.setattr(semantic, "load_model", lambda path: fake)
    return semantic, repo, tender, fake


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


def test_evidence_writers_advance_generation_and_status_does_not_hash_rows(setup):
    module, repo, tid, fake = setup
    assert repo.retrieval_generation(tid) == 0
    source(repo, tid, "spec.pdf", "Waterproof membrane")
    assert repo.retrieval_generation(tid) == 1
    service = module.SemanticService(repo)
    before = repo.retrieval_generation(tid)
    state = service.status(tid)
    assert state["desired_generation"] == before
    assert state["source_fingerprint"] == str(before)
    assert state["status"] == "not_indexed"
    source(repo, tid, "spec.pdf", "Concrete revision", "b" * 64)
    assert repo.retrieval_generation(tid) == 2
    assert service.status(tid)["desired_generation"] == 2


def test_empty_visual_stubs_do_not_advance_generation(setup):
    module, repo, tid, fake = setup
    source(repo, tid, "drawing.pdf", "Waterproof membrane")
    generation = repo.retrieval_generation(tid)
    artifact = repo.list_artifacts(tid)[0]
    with repo.atomic() as conn:
        conn.execute(
            "INSERT INTO evidence(id,artifact_id,locator,text,page,kind,metadata_json) VALUES(?,?,?,?,?,?,?)",
            ("visual-1", artifact["id"], "Page 1", "", 1, "visual", "{}"),
        )
    assert repo.retrieval_generation(tid) == generation


def test_words_only_while_rebuilding_and_strict_meaning_refuses(setup):
    module, repo, tid, fake = setup
    source(repo, tid, "spec.pdf", "Waterproof membrane")
    service = module.SemanticService(repo)
    service.index(tid)
    source(repo, tid, "extra.pdf", "Concrete", "b" * 64)
    assert service.status(tid)["status"] == "stale"
    response = retrieve(repo, tid, "rain", mode="auto", semantic=service)
    assert response.actual_mode == "words"
    assert any("Meaning search is not ready" in item for item in response.limitations)
    with pytest.raises(SemanticUnavailable) as error:
        retrieve(repo, tid, "rain", mode="meaning", semantic=service)
    assert error.value.code == "stale"


def test_preparing_status_is_not_ready_and_does_not_serve_stale_matches(setup):
    module, repo, tid, fake = setup
    source(repo, tid, "spec.pdf", "Waterproof membrane")
    service = module.SemanticService(repo)
    service.index(tid)
    source(repo, tid, "extra.pdf", "Concrete", "b" * 64)
    service.mark_refresh(
        tid, status="running", progress=40, detail="Embedding source passages locally."
    )
    state = service.status(tid)
    assert state["status"] == "updating"
    assert state["ready"] is False
    with pytest.raises(SemanticUnavailable) as error:
        service.search(tid, "rain")
    assert error.value.code == "updating"


def test_start_index_does_not_create_a_blocking_run(setup):
    module, repo, tid, fake = setup
    source(repo, tid, "spec.pdf", "Waterproof membrane")
    jobs = JobManager(repo, SettingsService(repo))
    jobs.start_index(tid)
    assert jobs.active(tid) == []
    assert all(run["kind"] != "index" for run in repo.list_runs(tid))


def test_repeated_refresh_requests_coalesce(setup):
    module, repo, tid, fake = setup
    source(repo, tid, "spec.pdf", "Waterproof membrane")
    indexer = RetrievalIndexer(repo)
    indexer.request_refresh(tid)
    indexer.request_refresh(tid)
    indexer.request_refresh(tid)
    assert indexer._pending == {tid}


def test_obsolete_vectors_are_removed_without_deleting_originals(setup, tmp_path):
    module, repo, tid, fake = setup
    original = repo.objects / ("a" * 64)
    original.write_bytes(b"original tender bytes")
    source(repo, tid, "spec.pdf", "Waterproof membrane")
    service = module.SemanticService(repo)
    service.index(tid)
    with service._connect() as conn:
        first = {row[0] for row in conn.execute("SELECT chunk_hash FROM vectors")}
    source(repo, tid, "spec.pdf", "Concrete only", "b" * 64)
    service.index(tid)
    with service._connect() as conn:
        remaining = {row[0] for row in conn.execute("SELECT chunk_hash FROM vectors")}
    assert first
    assert first.isdisjoint(remaining)
    assert original.read_bytes() == b"original tender bytes"


def test_restore_rebuilds_from_desired_generation_after_derived_index_is_removed(setup):
    module, repo, tid, fake = setup
    source(repo, tid, "spec.pdf", "Waterproof membrane")
    service = module.SemanticService(repo)
    service.index(tid)
    assert service.status(tid)["ready"] is True
    generation = repo.retrieval_generation(tid)
    service.path.unlink()
    for suffix in ("-wal", "-shm"):
        service.path.with_name(service.path.name + suffix).unlink(missing_ok=True)
    reopened = module.SemanticService(Repository(repo.home))
    state = reopened.status(tid)
    assert state["ready"] is False
    assert state["desired_generation"] == generation
    assert state["status"] == "not_indexed"


async def test_maintenance_runs_outside_the_ai_lane(setup):
    module, repo, tid, fake = setup
    source(repo, tid, "spec.pdf", "Waterproof membrane")
    jobs = JobManager(repo, SettingsService(repo))
    jobs.indexer.bind_loop(asyncio.get_running_loop())
    started = asyncio.Event()
    finished = asyncio.Event()
    original = SemanticService.index

    def blocked(self, tender_id, cancelled=None, progress=None):
        started.set()
        time.sleep(0.2)
        result = original(self, tender_id, cancelled, progress)
        finished.set()
        return result

    module.SemanticService.index = blocked
    jobs.indexer.request_refresh(tid, force=True)
    await asyncio.wait_for(started.wait(), timeout=2)
    assert jobs.active(tid) == []
    await asyncio.wait_for(finished.wait(), timeout=2)
    assert jobs.indexer.status(tid)["ready"] is True


def test_disk_pressure_failure_keeps_originals(setup, monkeypatch):
    module, repo, tid, fake = setup
    original = repo.objects / ("a" * 64)
    original.write_bytes(b"keep me")
    source(repo, tid, "spec.pdf", "Waterproof membrane")
    service = module.SemanticService(repo)

    def full(*_args, **_kwargs):
        raise OSError(28, "No space left on device")

    monkeypatch.setattr(service, "_connect", full)
    with pytest.raises((OSError, module.SemanticUnavailable)):
        service.index(tid)
    assert original.read_bytes() == b"keep me"
