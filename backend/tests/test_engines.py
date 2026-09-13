"""Bundled OCR and meaning-search engines are found, verified and never mismatched."""

import hashlib
import json

from quantix import engines


def _bundle(root, *, revision="rev-1", damage=False):
    files = {
        "tesseract/tesseract.exe": b"binary",
        "tesseract/tessdata/ara.traineddata": b"arabic",
        "models/e5/onnx/model.onnx": b"vectors",
    }
    for relative, content in files.items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
    manifest = {
        "format": 1,
        "tesseract": {"executable": "tesseract/tesseract.exe"},
        "model": {"name": "intfloat/multilingual-e5-small", "revision": revision, "path": "models/e5"},
        "files": {
            relative: {"size": len(content), "sha256": hashlib.sha256(content).hexdigest()}
            for relative, content in files.items()
        },
    }
    (root / "engines.json").write_text(json.dumps(manifest), encoding="utf-8")
    if damage:
        (root / "models/e5/onnx/model.onnx").write_bytes(b"VECTORS")  # same size, different bytes
    return root


def _fresh(monkeypatch, root):
    monkeypatch.setenv("QUANTIX_ENGINES_DIR", str(root))
    engines._VERIFIED.clear()


def test_bundled_engines_are_located_and_verified(tmp_path, monkeypatch):
    root = _bundle(tmp_path / "engines")
    _fresh(monkeypatch, root)
    assert engines.engines_dir() == root
    assert engines.verify(full=True) == {"ocr": True, "meaning": True, "detail": None}
    assert engines.tesseract_path() == root / "tesseract" / "tesseract.exe"
    assert engines.model_path("intfloat/multilingual-e5-small", "rev-1") == root / "models" / "e5"


def test_a_different_model_revision_is_never_used(tmp_path, monkeypatch):
    _fresh(monkeypatch, _bundle(tmp_path / "engines", revision="rev-2"))
    assert engines.model_path("intfloat/multilingual-e5-small", "rev-1") is None


def test_damaged_files_are_detected_by_the_full_check(tmp_path, monkeypatch):
    _fresh(monkeypatch, _bundle(tmp_path / "engines", damage=True))
    assert engines.verify()["meaning"] is True  # the quick size check cannot see it
    engines._VERIFIED.clear()
    result = engines.verify(full=True)
    assert (result["ocr"], result["meaning"]) == (True, False)
    assert "models/e5/onnx/model.onnx" in result["detail"]


def test_missing_bundle_reports_not_ready(tmp_path, monkeypatch):
    _fresh(monkeypatch, tmp_path / "nothing")
    monkeypatch.setattr(engines, "__file__", str(tmp_path / "quantix" / "engines.py"))
    assert engines.verify() == {"ocr": False, "meaning": False, "detail": "The bundled engines folder is missing."}
    assert engines.tesseract_path() is None
