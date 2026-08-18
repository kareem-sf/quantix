from __future__ import annotations

import argparse
import hashlib
import json
import urllib.request
from pathlib import Path
from typing import Any

from rapidocr.inference_engine.base import FileInfo, InferSession
from rapidocr.utils.typings import EngineType, ModelType, OCRVersion, TaskType

_MODEL_ROOT = Path(__file__).with_name("approved-model-sources.json")


def require_keys(value: dict[str, Any], expected: set[str], label: str) -> None:
    if set(value) != expected:
        raise ValueError(f"{label} contains unsupported fields")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def load_approval() -> dict[str, Any]:
    approval = json.loads(_MODEL_ROOT.read_text(encoding="utf-8"))
    require_keys(approval, {"schema_version", "rapidocr", "embeddings"}, "approval")
    if approval["schema_version"] != 3:
        raise ValueError("unsupported model approval schema")
    return approval


def resolved_artifacts() -> dict[str, str]:
    sizes: dict[str, ModelType] = {
        "det": ModelType("small"),
        "cls": ModelType("mobile"),
        "rec": ModelType("small"),
    }
    versions: dict[str, OCRVersion] = {
        "det": OCRVersion.PPOCRV6,
        "cls": OCRVersion.PPOCRV4,
        "rec": OCRVersion.PPOCRV6,
    }
    tasks: dict[str, TaskType] = {
        "det": TaskType.DET,
        "cls": TaskType.CLS,
        "rec": TaskType.REC,
    }
    urls: dict[str, str] = {}
    for task in ("det", "cls", "rec"):
        file_info = FileInfo(
            EngineType.ONNXRUNTIME, versions[task], tasks[task], "ch", sizes[task]
        )
        info = InferSession.get_model_url(file_info)
        model_url = info["model_dir"]
        urls[Path(model_url).name] = model_url
    return urls


def download(url: str, destination: Path) -> None:
    request = urllib.request.Request(url, headers={"User-Agent": "quantix-ocr-prepare"})
    with urllib.request.urlopen(request, timeout=600) as response:
        destination.parent.mkdir(parents=True, exist_ok=True)
        with destination.open("wb") as stream:
            while True:
                block = response.read(1024 * 1024)
                if not block:
                    break
                stream.write(block)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path, required=True)
    arguments = parser.parse_args()
    arguments.output_dir.mkdir(parents=True, exist_ok=True)
    approval = load_approval()["rapidocr"]
    require_keys(approval, {"backend", "language", "artifacts"}, "RapidOCR approval")
    if approval["backend"] != "onnxruntime" or approval["language"] != "ch":
        raise ValueError("Quantix v0 requires the approved RapidOCR runtime profile")
    expected: dict[str, dict[str, Any]] = {}
    for artifact in approval["artifacts"]:
        require_keys(artifact, {"path", "size_bytes", "sha256"}, "RapidOCR artifact")
        expected[artifact["path"]] = artifact
    urls = resolved_artifacts()
    if set(urls) != set(expected):
        raise ValueError("RapidOCR resolved an unapproved artifact set")
    for name, artifact in expected.items():
        destination = arguments.output_dir / name
        if destination.is_file() and sha256(destination) == artifact["sha256"]:
            continue
        download(urls[name], destination)
        if (
            destination.stat().st_size != artifact["size_bytes"]
            or sha256(destination) != artifact["sha256"]
        ):
            raise ValueError(f"RapidOCR artifact failed approval: {name}")

    embeddings = load_approval()["embeddings"]
    require_keys(embeddings, {"model", "dimensions", "artifacts"}, "embedding approval")
    if (
        embeddings["model"] != "intfloat/multilingual-e5-small"
        or embeddings["dimensions"] != 384
    ):
        raise ValueError("Quantix v0 requires the approved multilingual embedding profile")
    approved_paths = {
        "embeddings/model.onnx",
        "embeddings/tokenizer.json",
        "embeddings/config.json",
        "embeddings/special_tokens_map.json",
        "embeddings/tokenizer_config.json",
    }
    artifacts = embeddings["artifacts"]
    if {artifact.get("path") for artifact in artifacts} != approved_paths:
        raise ValueError("embedding approval contains an unexpected artifact set")
    for artifact in artifacts:
        require_keys(
            artifact,
            {"path", "url", "size_bytes", "sha256"},
            "embedding artifact",
        )
        destination = arguments.output_dir / artifact["path"]
        if destination.is_file() and sha256(destination) == artifact["sha256"]:
            continue
        download(artifact["url"], destination)
        if (
            destination.stat().st_size != artifact["size_bytes"]
            or sha256(destination) != artifact["sha256"]
        ):
            raise ValueError(
                f"embedding artifact failed approval: {artifact['path']}"
            )


if __name__ == "__main__":
    main()
