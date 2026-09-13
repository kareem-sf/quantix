"""Import coverage exceptions and extraction-cache publication (audit D04, D05)."""

import json
import os
import threading
from dataclasses import dataclass
from pathlib import Path

import pytest

from quantix import intake
from quantix.repository import Repository


def _walk_with_denied(denied: str):
    real_walk = os.walk

    def walk(top, onerror=None, followlinks=False):
        for directory, dirs, names in real_walk(top, followlinks=followlinks):
            if Path(directory).name == denied:
                onerror(PermissionError(13, "Access is denied", str(directory)))
                dirs[:] = []
                continue
            yield directory, dirs, names

    return walk


def test_unreadable_subfolder_is_reported_instead_of_silently_skipped(tmp_path, monkeypatch):
    root = tmp_path / "package"
    (root / "Drawings").mkdir(parents=True)
    (root / "Drawings" / "plan.pdf").write_bytes(b"%PDF-1.4 hidden")
    (root / "Specs").mkdir()
    (root / "Specs" / "spec.pdf").write_bytes(b"%PDF-1.4 readable")
    monkeypatch.setattr(intake.os, "walk", _walk_with_denied("Drawings"))

    candidates = {item.name: item for item in intake._folder_files(root, threading.Event())}

    assert candidates["Specs/spec.pdf"].problem is None
    assert "could not be opened" in candidates["Drawings"].problem
    assert "Drawings/plan.pdf" not in candidates


def test_unreadable_selected_folder_fails_with_a_clear_reason(tmp_path, monkeypatch):
    root = tmp_path / "package"
    root.mkdir()
    monkeypatch.setattr(intake.os, "walk", _walk_with_denied("package"))

    with pytest.raises(ValueError, match="selected folder could not be opened"):
        intake._folder_files(root, threading.Event())


@dataclass
class _Extraction:
    status: str
    segments: list


def test_concurrent_extractions_publish_one_valid_cache(tmp_path, monkeypatch):
    repo = Repository(tmp_path / "home")
    source = tmp_path / "spec.pdf"
    source.write_bytes(b"%PDF-1.4 shared")
    barrier = threading.Barrier(2)

    def extract_document(path, name, cancelled):
        # Generous waits: a loaded machine must not turn scheduling into a failure.
        barrier.wait(timeout=60)
        return _Extraction(status="extracted", segments=[{"locator": "page:1", "text": "Concrete"}])

    monkeypatch.setattr(intake, "extract_document", extract_document)
    results, errors = [], []

    def run():
        try:
            results.append(intake._extract(repo, source, "spec.pdf", "a" * 64, threading.Event()))
        except Exception as error:  # noqa: BLE001 - surfaced by the assertion below
            errors.append(error)

    threads = [threading.Thread(target=run) for _ in range(2)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join(timeout=90)

    assert errors == []
    assert [item["status"] for item in results] == ["extracted", "extracted"]
    extractions = repo.home / "extractions"
    caches = list(extractions.glob("*.json"))
    assert len(caches) == 1
    assert json.loads(caches[0].read_text(encoding="utf-8"))["status"] == "extracted"
    assert list(extractions.glob("*.partial")) == []
