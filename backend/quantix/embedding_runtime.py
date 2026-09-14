"""Verified local embedding model loading and a bounded runtime per home."""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from uuid import uuid4

import numpy as np

from .processes import stop_owned_process_tree
from .semantic_models import SemanticUnavailable
from .storage import cache_dir, current_home, models_dir

MODEL_NAME = "intfloat/multilingual-e5-small"
MODEL_REVISION = "614241f622f53c4eeff9890bdc4f31cfecc418b3"
MODEL_DIM = 384
MODEL_FILES = (
    "config.json",
    "tokenizer.json",
    "tokenizer_config.json",
    "special_tokens_map.json",
    "onnx/model.onnx",
)
MANIFEST_NAME = "quantix-model.json"
DOWNLOAD_TIMEOUT = 900
IDLE_RELEASE_SECONDS = 600
INFERENCE_THREADS = 2

_RUNTIMES: dict[tuple[str, str], "EmbeddingRuntime"] = {}
_RUNTIMES_LOCK = threading.Lock()
_VERIFIED: dict[str, bool] = {}
_VERIFIED_LOCK = threading.Lock()


@dataclass(frozen=True)
class ModelCheck:
    ok: bool
    detail: str = ""
    fingerprint: str = ""
    path: Path | None = None


def _digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            hasher.update(block)
    return hasher.hexdigest()


def _file_inventory(root: Path) -> dict[str, dict[str, int | str]]:
    inventory = {}
    for name in MODEL_FILES:
        path = root / name
        inventory[name] = {"size": path.stat().st_size, "sha256": _digest(path)}
    return inventory


def write_manifest(root: Path, *, fingerprint: str, runtime_version: str) -> None:
    payload = {
        "format": 1,
        "name": MODEL_NAME,
        "revision": MODEL_REVISION,
        "dim": MODEL_DIM,
        "pooling": "mean",
        "normalization": True,
        "prefixes": {"query": "query: ", "passage": "passage: "},
        "files": _file_inventory(root),
        "runtime": {"fastembed": runtime_version},
        "fingerprint": fingerprint,
    }
    target = root / MANIFEST_NAME
    temporary = root / f".{MANIFEST_NAME}.{uuid4().hex}.tmp"
    temporary.write_text(json.dumps(payload, indent=2, sort_keys=True), encoding="utf-8")
    os.replace(temporary, target)


def read_manifest(root: Path) -> dict:
    path = root / MANIFEST_NAME
    if not path.is_file():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    return data if isinstance(data, dict) else {}


def files_present(path: Path) -> bool:
    return all((path / name).is_file() and (path / name).stat().st_size > 0 for name in MODEL_FILES)


def model_verified(path: Path, *, fingerprint: str | None = None, full: bool = True) -> bool:
    return verify_model(path, fingerprint=fingerprint, full=full).ok


def verify_model(path: Path, *, fingerprint: str | None = None, full: bool = True) -> ModelCheck:
    """Check required files, recorded digests and optional representation fingerprint."""

    path = Path(path)
    from .engines import model_path, verify

    bundled = model_path(MODEL_NAME, MODEL_REVISION)
    if bundled is not None and path.resolve() == bundled.resolve():
        result = verify(full=full)
        if not result.get("meaning"):
            return ModelCheck(
                ok=False,
                detail=result.get("detail") or "The bundled model is missing or damaged.",
                path=path,
            )
        return ModelCheck(ok=True, detail="", path=path, fingerprint=fingerprint or "")

    cache_key = f"{path.resolve()}|{fingerprint or ''}|{int(full)}"
    with _VERIFIED_LOCK:
        if cache_key in _VERIFIED:
            return ModelCheck(ok=_VERIFIED[cache_key], path=path, fingerprint=fingerprint or "")

    if not files_present(path):
        check = ModelCheck(
            ok=False, detail="The local multilingual model files are missing.", path=path
        )
    else:
        manifest = read_manifest(path)
        expected_files = (manifest.get("files") or {}) if manifest else {}
        detail = ""
        ok = True
        if fingerprint and manifest.get("fingerprint") and manifest["fingerprint"] != fingerprint:
            ok = False
            detail = "The saved model representation does not match this Quantix version."
        elif fingerprint and manifest.get("runtime", {}).get("fastembed"):
            try:
                import importlib.metadata

                installed = importlib.metadata.version("fastembed")
            except importlib.metadata.PackageNotFoundError:
                installed = ""
            if installed and manifest["runtime"]["fastembed"] != installed:
                ok = False
                detail = "The embedding runtime version changed. Prepare search again."
        for name in MODEL_FILES:
            file_path = path / name
            try:
                size = file_path.stat().st_size
            except OSError:
                ok = False
                detail = detail or f"A model file cannot be read: {name}"
                break
            recorded = expected_files.get(name) or {}
            if recorded.get("size") not in (None, size):
                ok = False
                detail = detail or f"A model file is the wrong size: {name}"
                break
            if full and recorded.get("sha256") and _digest(file_path) != recorded["sha256"]:
                ok = False
                detail = detail or f"A model file is damaged: {name}"
                break
            if full and not recorded:
                # Workspace copies must have a verified manifest before use.
                ok = False
                detail = detail or "The local model has no verified manifest."
                break
        if ok and manifest.get("name") not in (None, MODEL_NAME):
            ok = False
            detail = "The saved model is not the multilingual E5 model Quantix uses."
        if ok and manifest.get("revision") not in (None, MODEL_REVISION):
            ok = False
            detail = "The saved model revision does not match the pinned multilingual E5 model."
        check = ModelCheck(
            ok=ok,
            detail=detail,
            path=path,
            fingerprint=str(manifest.get("fingerprint") or fingerprint or ""),
        )

    with _VERIFIED_LOCK:
        _VERIFIED[cache_key] = check.ok
    return check


def forget_verified(path: Path | None = None) -> None:
    with _VERIFIED_LOCK:
        if path is None:
            _VERIFIED.clear()
            return
        prefix = str(Path(path).resolve())
        for key in [item for item in _VERIFIED if item.startswith(prefix)]:
            _VERIFIED.pop(key, None)


def activate_directory(staging: Path, dest: Path) -> None:
    """Replace dest with staging in one directory rename. Never leaves dest half-written."""

    dest = Path(dest)
    staging = Path(staging)
    dest.parent.mkdir(parents=True, exist_ok=True)
    if not dest.exists():
        os.replace(staging, dest)
        return
    retired = dest.with_name(f".{dest.name}.retired-{uuid4().hex[:8]}")
    os.replace(dest, retired)
    try:
        os.replace(staging, dest)
    except BaseException:
        if dest.exists():
            shutil.rmtree(dest, ignore_errors=True)
        os.replace(retired, dest)
        raise
    shutil.rmtree(retired, ignore_errors=True)


def load_model(path: Path):
    """Load only known local ONNX/tokenizer files; never fetch during search."""
    try:
        from fastembed import TextEmbedding
        from fastembed.common.model_description import ModelSource, PoolingType
    except ImportError as exc:
        raise SemanticUnavailable(
            "model_missing", "Local embedding dependencies are not installed."
        ) from exc
    embedding_cache = cache_dir(current_home()) / "fastembed"
    embedding_cache.mkdir(parents=True, exist_ok=True)
    if not any(m["model"] == MODEL_NAME for m in TextEmbedding.list_supported_models()):
        TextEmbedding.add_custom_model(
            model=MODEL_NAME,
            pooling=PoolingType.MEAN,
            normalization=True,
            sources=ModelSource(hf=MODEL_NAME),
            dim=MODEL_DIM,
            model_file="onnx/model.onnx",
        )
    return TextEmbedding(
        model_name=MODEL_NAME,
        specific_model_path=str(path),
        cache_dir=str(embedding_cache),
        local_files_only=True,
        providers=["CPUExecutionProvider"],
        threads=INFERENCE_THREADS,
    )


def download_model(path: Path, *, fingerprint: str, runtime_version: str) -> None:
    from huggingface_hub import snapshot_download

    dest = Path(path)
    staging = dest.with_name(f".{dest.name}.staging-{uuid4().hex[:8]}")
    if staging.exists():
        shutil.rmtree(staging)
    staging.mkdir(parents=True, exist_ok=True)
    try:
        snapshot_download(
            repo_id=MODEL_NAME,
            revision=MODEL_REVISION,
            local_dir=str(staging),
            allow_patterns=list(MODEL_FILES) + ["README.md"],
            token=False,
            max_workers=2,
        )
        if not files_present(staging):
            raise SemanticUnavailable(
                "model_missing",
                "The multilingual model download did not include every required file.",
            )
        load_model(staging)
        write_manifest(staging, fingerprint=fingerprint, runtime_version=runtime_version)
        activate_directory(staging, dest)
        forget_verified(dest)
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise


def _cancel(cancelled):
    if cancelled and cancelled():
        raise InterruptedError("Semantic indexing cancelled")


def ensure_model(
    path: Path, cancelled=None, progress=None, *, fingerprint: str, runtime_version: str
):
    if model_verified(path, fingerprint=fingerprint, full=True):
        return
    _cancel(cancelled)
    if progress:
        progress(5, "Downloading the local multilingual search model. No Tender text is sent.")
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    cache = cache_dir(current_home()) / "huggingface"
    cache.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.update(
        {
            "HF_HOME": str(cache),
            "HF_HUB_CACHE": str(cache / "hub"),
            "HF_XET_CACHE": str(cache / "xet"),
            "HF_HUB_DISABLE_IMPLICIT_TOKEN": "1",
            "HF_HUB_DISABLE_TELEMETRY": "1",
            "HF_HUB_DISABLE_XET": "1",
            "HF_HUB_DOWNLOAD_TIMEOUT": "30",
        }
    )
    process = subprocess.Popen(
        [
            sys.executable,
            *(
                ["semantic-download", "--home", str(current_home())]
                if getattr(sys, "frozen", False)
                else ["-m", "quantix", "--home", str(current_home()), "semantic-download"]
            ),
            str(path),
        ],
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
    )
    started = time.monotonic()
    try:
        while process.poll() is None:
            _cancel(cancelled)
            if time.monotonic() - started > DOWNLOAD_TIMEOUT:
                raise SemanticUnavailable(
                    "model_missing",
                    "The local model download timed out. Retry indexing to resume the download.",
                )
            try:
                process.wait(timeout=0.2)
            except subprocess.TimeoutExpired:
                pass
        _cancel(cancelled)
        if process.returncode or not model_verified(path, fingerprint=fingerprint, full=True):
            raise SemanticUnavailable(
                "model_missing",
                "The multilingual model could not be downloaded or loaded. Check the connection and retry indexing.",
            )
    finally:
        stop_owned_process_tree(process)


def score_exact(query: np.ndarray, matrix: np.ndarray) -> np.ndarray:
    """Exact cosine scores for L2-normalized rows. No approximate index."""

    vector = np.asarray(query, dtype=np.float32).reshape(-1)
    table = np.asarray(matrix, dtype=np.float32)
    if table.size == 0:
        return np.empty(0, dtype=np.float32)
    if table.ndim != 2 or table.shape[1] != vector.shape[0]:
        raise SemanticUnavailable(
            "invalid_vector",
            "The embedding model or stored index returned an invalid vector. Rebuild the index.",
        )
    return table @ vector


def packed_matrix(blobs: list[bytes]) -> np.ndarray:
    if not blobs:
        return np.zeros((0, MODEL_DIM), dtype=np.float32)
    matrix = np.frombuffer(b"".join(blobs), dtype="<f4")
    if matrix.size != len(blobs) * MODEL_DIM:
        raise SemanticUnavailable(
            "invalid_vector",
            "The stored index contains a vector of the wrong size. Rebuild the index.",
        )
    return matrix.reshape(len(blobs), MODEL_DIM)


class EmbeddingRuntime:
    """One loaded model per application home and representation fingerprint."""

    def __init__(self, home: Path, fingerprint: str, *, idle_seconds: float = IDLE_RELEASE_SECONDS):
        self.home = Path(home)
        self.fingerprint = fingerprint
        self.idle_seconds = idle_seconds
        self._model = None
        self._lock = threading.RLock()
        self._inference = threading.Lock()
        self._timer: threading.Timer | None = None
        self.loads = 0
        self.inferences = 0
        self._active_inference = 0
        self.max_active_inference = 0

    def load(self, path: Path):
        with self._lock:
            if self._model is not None:
                self._touch()
                return self._model
            try:
                self._model = load_model(path)
            except SemanticUnavailable:
                raise
            except Exception as exc:
                raise SemanticUnavailable(
                    "model_missing",
                    "The local embedding model could not be loaded. Rebuild the model cache.",
                ) from exc
            self.loads += 1
            self._touch()
            return self._model

    def embed_passages(self, model, texts, *, batch_size: int):
        with self._inference_session():
            return list(model.passage_embed(texts, batch_size=batch_size, parallel=None))

    def embed_query(self, model, text: str):
        with self._inference_session():
            return list(model.query_embed(text))

    def release(self) -> None:
        with self._lock:
            if self._timer is not None:
                self._timer.cancel()
                self._timer = None
            self._model = None

    def _touch(self) -> None:
        if self._timer is not None:
            self._timer.cancel()
            self._timer = None
        if self.idle_seconds and self.idle_seconds > 0:
            timer = threading.Timer(self.idle_seconds, self.release)
            timer.daemon = True
            self._timer = timer
            timer.start()

    def _inference_session(self):
        runtime = self

        class _Guard:
            def __enter__(self):
                runtime._inference.acquire()
                runtime._active_inference += 1
                runtime.max_active_inference = max(
                    runtime.max_active_inference, runtime._active_inference
                )
                runtime.inferences += 1
                return runtime

            def __exit__(self, exc_type, exc, tb):
                runtime._active_inference -= 1
                runtime._inference.release()
                runtime._touch()
                return False

        return _Guard()


def runtime_for(
    home: Path, fingerprint: str, *, idle_seconds: float | None = None
) -> EmbeddingRuntime:
    key = (str(Path(home).resolve()), fingerprint)
    with _RUNTIMES_LOCK:
        existing = _RUNTIMES.get(key)
        if existing is None:
            existing = EmbeddingRuntime(
                home,
                fingerprint,
                idle_seconds=IDLE_RELEASE_SECONDS if idle_seconds is None else idle_seconds,
            )
            _RUNTIMES[key] = existing
        return existing


def release_runtimes(home: Path | None = None) -> None:
    with _RUNTIMES_LOCK:
        if home is None:
            items = list(_RUNTIMES.values())
            _RUNTIMES.clear()
        else:
            prefix = str(Path(home).resolve())
            keys = [key for key in _RUNTIMES if key[0] == prefix]
            items = [_RUNTIMES.pop(key) for key in keys]
    for runtime in items:
        runtime.release()


def default_model_path(home: Path) -> Path:
    from .engines import model_path

    return model_path(MODEL_NAME, MODEL_REVISION) or (
        models_dir(home) / f"multilingual-e5-small-{MODEL_REVISION}"
    )
