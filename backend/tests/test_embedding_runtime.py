"""Verified model manifest and one bounded embedding runtime per home."""

import subprocess
import sys
import threading
import time
from pathlib import Path

import numpy as np
import pytest

from quantix.embedding_runtime import (
    MODEL_DIM,
    MODEL_FILES,
    activate_directory,
    packed_matrix,
    release_runtimes,
    runtime_for,
    score_exact,
    verify_model,
    write_manifest,
)


def _model_tree(root: Path, *, fingerprint="fp-test", runtime_version="0.8.0"):
    for name in MODEL_FILES:
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(f"{name}-bytes".encode())
    write_manifest(root, fingerprint=fingerprint, runtime_version=runtime_version)
    return root


def test_missing_and_corrupt_files_are_not_verified(tmp_path):
    missing = tmp_path / "missing"
    missing.mkdir()
    check = verify_model(missing, fingerprint="fp-test", full=True)
    assert check.ok is False
    assert "missing" in check.detail.lower() or "manifest" in check.detail.lower()

    intact = _model_tree(tmp_path / "intact")
    assert verify_model(intact, fingerprint="fp-test", full=True).ok is True

    damaged = _model_tree(tmp_path / "damaged")
    original = (damaged / "onnx/model.onnx").read_bytes()
    (damaged / "onnx/model.onnx").write_bytes(b"X" * len(original))
    from quantix.embedding_runtime import forget_verified

    forget_verified(damaged)
    check = verify_model(damaged, fingerprint="fp-test", full=True)
    assert check.ok is False
    assert "damaged" in check.detail.lower()


def test_offline_startup_does_not_download(tmp_path, monkeypatch):
    from quantix import embedding_runtime

    def forbidden(*_args, **_kwargs):
        raise AssertionError("Offline startup must not download the model.")

    monkeypatch.setattr(embedding_runtime, "download_model", forbidden)
    check = verify_model(tmp_path / "absent", fingerprint="fp-test", full=False)
    assert check.ok is False


def test_altered_runtime_or_fingerprint_rejects_activation(tmp_path):
    root = _model_tree(tmp_path / "model", fingerprint="fp-old", runtime_version="0.0.1")
    from quantix.embedding_runtime import forget_verified

    forget_verified(root)
    assert verify_model(root, fingerprint="fp-new", full=True).ok is False
    forget_verified(root)
    assert verify_model(root, fingerprint="fp-old", full=True).ok is False


def test_atomic_activation_replaces_the_directory_completely(tmp_path):
    dest = tmp_path / "model"
    dest.mkdir()
    (dest / "stale.txt").write_text("old", encoding="utf-8")
    staging = tmp_path / "staging"
    _model_tree(staging, fingerprint="fp-test")
    activate_directory(staging, dest)
    assert (dest / "onnx/model.onnx").is_file()
    assert not (dest / "stale.txt").exists()
    assert not staging.exists()


def test_cancelled_download_stops_the_child(tmp_path, monkeypatch):
    from quantix import embedding_runtime

    actual = subprocess.Popen
    processes = []

    def process(command, **kwargs):
        if command[-2:] == ["/T", "/F"]:
            return actual(command, **kwargs)
        assert command[-2] == "semantic-download"
        child = actual([sys.executable, "-c", "import time; time.sleep(30)"], **kwargs)
        processes.append(child)
        return child

    monkeypatch.setattr(embedding_runtime.subprocess, "Popen", process)
    started = time.monotonic()
    with pytest.raises(InterruptedError):
        embedding_runtime.ensure_model(
            tmp_path / "model",
            cancelled=lambda: time.monotonic() - started > 0.2,
            fingerprint="fp-test",
            runtime_version="0.8.0",
        )
    assert time.monotonic() - started < 3
    assert processes and all(item.poll() is not None for item in processes)


def test_one_runtime_per_home_and_separate_homes(tmp_path):
    release_runtimes()
    first = runtime_for(tmp_path / "a", "fp")
    again = runtime_for(tmp_path / "a", "fp")
    other = runtime_for(tmp_path / "b", "fp")
    assert first is again
    assert first is not other
    release_runtimes()


def test_parallel_loads_share_one_model(tmp_path):
    release_runtimes()
    runtime = runtime_for(tmp_path, "fp")
    loaded = []

    def loader(_path):
        time.sleep(0.05)
        loaded.append(1)
        return object()

    def worker():
        with runtime._lock:
            if runtime._model is None:
                runtime._model = loader(tmp_path)
                runtime.loads += 1

    threads = [threading.Thread(target=worker) for _ in range(8)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    assert runtime.loads == 1
    assert loaded == [1]
    release_runtimes()


def test_inference_is_serialised_and_idle_release_unloads(tmp_path):
    release_runtimes()
    runtime = runtime_for(tmp_path, "fp", idle_seconds=0.2)

    class Slow:
        def passage_embed(self, texts, **_kwargs):
            time.sleep(0.05)
            yield np.ones(MODEL_DIM, dtype=np.float32)

    model = Slow()

    def worker():
        runtime.embed_passages(model, ["passage: one"], batch_size=1)

    threads = [threading.Thread(target=worker) for _ in range(4)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    assert runtime.max_active_inference == 1
    runtime._model = object()
    runtime._touch()
    time.sleep(0.35)
    assert runtime._model is None
    release_runtimes()


def test_vectorised_exact_scores_match_a_python_loop():
    query = np.zeros(MODEL_DIM, dtype=np.float32)
    query[0] = 1
    blobs = []
    expected = []
    for index in range(12):
        vector = np.zeros(MODEL_DIM, dtype=np.float32)
        vector[0] = 1 if index % 2 == 0 else 0
        vector[1] = 0 if index % 2 == 0 else 1
        vector = vector / np.linalg.norm(vector)
        blobs.append(vector.astype("<f4").tobytes())
        expected.append(float(np.dot(vector, query)))
    scores = score_exact(query, packed_matrix(blobs))
    assert list(scores) == pytest.approx(expected)


def test_exact_scoring_stays_within_a_second_at_supported_chunk_counts():
    query = np.zeros(MODEL_DIM, dtype=np.float32)
    query[0] = 1
    matrix = np.zeros((50_000, MODEL_DIM), dtype=np.float32)
    matrix[:, 0] = 1
    started = time.perf_counter()
    scores = score_exact(query, matrix)
    elapsed = time.perf_counter() - started
    assert scores.shape == (50_000,)
    assert elapsed < 1.0
