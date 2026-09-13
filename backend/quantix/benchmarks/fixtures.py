"""Read-only validation of checked-in synthetic originals before normal import."""

import hashlib
import json
from pathlib import Path

from docx import Document


def fixture_root():
    return Path(__file__).with_name("fixtures")


def validate_case_fixture(case, manifest=None):
    root = fixture_root()
    manifest = manifest or json.loads((root / "manifest.json").read_text(encoding="utf-8"))
    entries = manifest["cases"].get(case.id)
    if entries is None or set(entries) != {source.id for source in case.sources}:
        raise ValueError("The synthetic fixture inventory does not match its case definition.")
    for source in case.sources:
        path = root / case.id / f"{source.id}.docx"
        expected = entries[source.id]
        if (
            path.is_symlink()
            or not path.is_file()
            or hashlib.sha256(path.read_bytes()).hexdigest() != expected["sha256"]
        ):
            raise ValueError("A checked-in benchmark original changed or is missing.")
        text = "\n".join(paragraph.text for paragraph in Document(path).paragraphs)
        if (
            text != source.text
            or hashlib.sha256(text.encode()).hexdigest() != expected["text_sha256"]
        ):
            raise ValueError("Synthetic document contents differ from the explicit case facts.")
    if {path.name for path in (root / case.id).iterdir()} != {
        f"{source.id}.docx" for source in case.sources
    }:
        raise ValueError("An unexpected file is present in the synthetic import fixture.")
    return root / case.id


def validate_fixtures(cases):
    manifest = json.loads((fixture_root() / "manifest.json").read_text(encoding="utf-8"))
    if set(manifest["cases"]) != {case.id for case in cases}:
        raise ValueError("The fixture manifest must contain exactly the benchmark cases.")
    for case in cases:
        validate_case_fixture(case, manifest)
