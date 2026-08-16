from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path
from typing import Any

from docling.models.stages.ocr.rapid_ocr_model import RapidOcrModel
from docling.models.utils.hf_model_download import download_hf_model


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
    path = Path(__file__).with_name("approved-model-sources.json")
    approval = json.loads(path.read_text(encoding="utf-8"))
    require_keys(approval, {"schema_version", "huggingface", "rapidocr"}, "approval")
    if approval["schema_version"] != 1:
        raise ValueError("unsupported model approval schema")
    return approval


def download_huggingface_models(output: Path, sources: list[dict[str, Any]]) -> None:
    for source in sources:
        require_keys(source, {"repo_id", "revision", "directory"}, "Hugging Face source")
        revision = source["revision"]
        if not isinstance(revision, str) or re.fullmatch(r"[0-9a-f]{40}", revision) is None:
            raise ValueError("Hugging Face models require an immutable commit revision")
        expected_directory = source["repo_id"].replace("/", "--")
        if source["directory"] != expected_directory:
            raise ValueError("Hugging Face model directory does not match its repository")
        download_hf_model(
            repo_id=source["repo_id"],
            revision=revision,
            local_dir=output / source["directory"],
            progress=False,
        )


def download_and_verify_rapidocr(output: Path, approval: dict[str, Any]) -> None:
    require_keys(approval, {"backend", "language", "artifacts"}, "RapidOCR approval")
    if approval["backend"] != "onnxruntime" or approval["language"] != "ch":
        raise ValueError("Quantix v0 requires the approved RapidOCR runtime profile")
    model_root = output / RapidOcrModel._model_repo_folder
    RapidOcrModel.download_models(
        backend=approval["backend"],
        lang=approval["language"],
        local_dir=model_root,
        progress=False,
    )
    expected: dict[str, dict[str, Any]] = {}
    for artifact in approval["artifacts"]:
        require_keys(artifact, {"path", "size_bytes", "sha256"}, "RapidOCR artifact")
        expected[artifact["path"]] = artifact
    actual = {
        path.relative_to(model_root).as_posix(): path
        for path in model_root.rglob("*")
        if path.is_file()
    }
    if set(actual) != set(expected):
        raise ValueError("RapidOCR downloaded an unapproved artifact set")
    for relative, path in actual.items():
        artifact = expected[relative]
        if path.stat().st_size != artifact["size_bytes"] or sha256(path) != artifact["sha256"]:
            raise ValueError(f"RapidOCR artifact failed approval: {relative}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path, required=True)
    arguments = parser.parse_args()
    arguments.output_dir.mkdir(parents=True, exist_ok=True)
    approval = load_approval()
    download_huggingface_models(arguments.output_dir, approval["huggingface"])
    download_and_verify_rapidocr(arguments.output_dir, approval["rapidocr"])


if __name__ == "__main__":
    main()
