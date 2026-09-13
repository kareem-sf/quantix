"""Focused reader reprocessing regressions for PDFium/OCR boundaries."""

from __future__ import annotations

import hashlib
import json
import multiprocessing
import subprocess
import sys
from pathlib import Path

import pypdfium2 as pdfium
import pytest

from quantix.extraction_models import ReprocessRequest
from quantix.repository import Repository


def _blank_pdf(path: Path, pages: int = 1) -> bytes:
    document = pdfium.PdfDocument.new()
    for _ in range(pages):
        page = document.new_page(200, 100)
        page.close()
    document.save(path)
    document.close()
    return path.read_bytes()


def _nested_render_probe(path_text: str, home_text: str, output) -> None:
    """Run actual PDFium rendering below the reprocess call in a child process."""

    from quantix import extraction_worker
    from quantix.extraction_adapters import ExtractionService

    path = Path(path_text)
    home = Path(home_text)
    repo = Repository(home)
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    extraction_worker.ocr_available = lambda **kwargs: {
        "available": True,
        "tesseract": True,
        "engine": "synthetic",
        "executable": "synthetic",
    }
    extraction_worker.ocr_png = lambda png, **kwargs: "OCR recovered from the blank page"
    try:
        result = ExtractionService(repo).reprocess(
            digest,
            path=path,
            request=ReprocessRequest(
                original_hash=digest,
                reader_id="ocr",
                reader_version="synthetic-1",
            ),
        )
        output.put({"ok": True, "result": result})
    except BaseException as error:  # pragma: no cover - reported by parent
        output.put({"ok": False, "error": f"{type(error).__name__}: {error}"})


def test_real_blank_page_ocr_render_completes_without_nested_pdfium_deadlock(tmp_path):
    source = tmp_path / "blank.pdf"
    _blank_pdf(source)
    ctx = multiprocessing.get_context("spawn")
    queue = ctx.Queue()
    process = ctx.Process(
        target=_nested_render_probe,
        args=(str(source), str(tmp_path / "child-home"), queue),
    )
    process.start()
    process.join(timeout=5)
    if process.is_alive():
        process.terminate()
        process.join(timeout=2)
        raise AssertionError("OCR reprocessing deadlocked while rendering under PDFium")
    assert process.exitcode == 0
    result = queue.get(timeout=2)
    assert result["ok"] is True, result
    assert result["result"]["extracted_pages"] == 1
    assert result["result"]["exception_pages"] == 0


def test_blank_page_without_ocr_is_an_explicit_exception(tmp_path, monkeypatch):
    from quantix import extraction_worker
    from quantix.extraction_adapters import ExtractionService

    source = tmp_path / "blank.pdf"
    body = _blank_pdf(source)
    digest = hashlib.sha256(body).hexdigest()
    home = tmp_path / "home"
    repo = Repository(home)
    stored = repo.objects / digest
    stored.write_bytes(body)
    monkeypatch.setattr(
        extraction_worker,
        "ocr_available",
        lambda **kwargs: {"available": False, "tesseract": False, "engine": None, "executable": None},
    )

    result = ExtractionService(repo).reprocess(digest, path=stored)

    assert result["extracted_pages"] == 0
    assert result["exception_pages"] == 1
    with repo.db.connect() as conn:
        segments = json.loads(
            conn.execute(
                "SELECT segments_json FROM extraction_versions WHERE id=?", (result["id"],)
            ).fetchone()[0]
        )
    assert segments[0]["state"] == "no_text"


def test_ocr_callback_runs_after_pdfium_release_and_keeps_complete_text(tmp_path, monkeypatch):
    from quantix import extraction_worker
    from quantix.documents import PDFIUM_LOCK
    from quantix.extraction_adapters import ExtractionService

    source = tmp_path / "blank.pdf"
    body = _blank_pdf(source)
    digest = hashlib.sha256(body).hexdigest()
    repo = Repository(tmp_path / "home")
    stored = repo.objects / digest
    stored.write_bytes(body)
    observed: list[bool] = []
    complete_text = "OCR page text " + ("kept " * 1000)
    monkeypatch.setattr(
        extraction_worker,
        "ocr_available",
        lambda **kwargs: {"available": True, "tesseract": True, "engine": "synthetic", "executable": "synthetic"},
    )
    monkeypatch.setattr(extraction_worker, "render_page_png", lambda *args, **kwargs: b"png")

    def ocr(png, **kwargs):
        observed.append(PDFIUM_LOCK.locked())
        return complete_text

    monkeypatch.setattr(extraction_worker, "ocr_png", ocr)
    result = ExtractionService(repo).reprocess(
        digest,
        path=stored,
        request=ReprocessRequest(original_hash=digest, reader_id="ocr", reader_version="synthetic-1"),
    )

    assert observed == [False]
    assert result["extracted_pages"] == 1
    with repo.db.connect() as conn:
        segments = json.loads(
            conn.execute(
                "SELECT segments_json FROM extraction_versions WHERE id=?", (result["id"],)
            ).fetchone()[0]
        )
    assert segments[0]["text"] == complete_text
    assert stored.read_bytes() == body


def test_reprocess_checks_cancellation_before_next_page(tmp_path, monkeypatch):
    from quantix import extraction_worker
    from quantix.extraction_adapters import ExtractionService

    source = tmp_path / "blank.pdf"
    body = _blank_pdf(source, pages=2)
    digest = hashlib.sha256(body).hexdigest()
    repo = Repository(tmp_path / "home")
    stored = repo.objects / digest
    stored.write_bytes(body)
    monkeypatch.setattr(
        extraction_worker,
        "ocr_available",
        lambda **kwargs: {"available": False, "tesseract": False, "engine": None, "executable": None},
    )
    calls = 0

    def cancelled():
        nonlocal calls
        calls += 1
        return calls > 2

    try:
        ExtractionService(repo).reprocess(digest, path=stored, cancelled=cancelled)
    except InterruptedError:
        pass
    else:
        raise AssertionError("Reprocessing must stop when cancellation is observed")


def test_ocr_temp_files_are_unique_and_removed_after_each_call(tmp_path, monkeypatch):
    from quantix import extraction_worker

    executable = tmp_path / "tesseract.exe"
    executable.write_bytes(b"synthetic")
    monkeypatch.setattr(extraction_worker, "tesseract_executable", lambda home: executable)
    paths: list[Path] = []

    class Complete:
        returncode = 0

        def __init__(self, command, **kwargs):
            paths.append(Path(command[1]))

        def poll(self):
            return self.returncode

        def communicate(self, timeout=None):
            return b"text", b""

        def wait(self, timeout=None):
            return self.returncode

    monkeypatch.setattr(extraction_worker.subprocess, "Popen", Complete)
    assert extraction_worker.ocr_png(b"png", home=tmp_path) == "text"
    assert extraction_worker.ocr_png(b"png", home=tmp_path) == "text"
    assert paths[0] != paths[1]
    assert all(not path.exists() for path in paths)


def test_ocr_drains_substantial_stdout_and_stderr_before_exit(tmp_path, monkeypatch):
    from quantix import extraction_worker

    monkeypatch.setattr(
        extraction_worker, "tesseract_executable", lambda home: Path(sys.executable)
    )
    real_popen = extraction_worker.subprocess.Popen
    script = "import sys; sys.stdout.write('O'*262144); sys.stderr.write('E'*262144)"

    def launch(command, **kwargs):
        return real_popen([sys.executable, "-c", script], **kwargs)

    monkeypatch.setattr(extraction_worker.subprocess, "Popen", launch)
    monkeypatch.setattr(extraction_worker, "OCR_TIMEOUT_SECONDS", 3)

    result = extraction_worker.ocr_png(b"png", home=tmp_path)

    assert result is not None
    assert len(result) == 262144
    assert result.startswith("O")


def test_ocr_timeout_and_cancellation_are_bounded(tmp_path, monkeypatch):
    from quantix import extraction_worker

    executable = tmp_path / "tesseract.exe"
    executable.write_bytes(b"synthetic")
    monkeypatch.setattr(extraction_worker, "tesseract_executable", lambda home: executable)
    with pytest.raises(InterruptedError):
        extraction_worker.ocr_png(b"png", home=tmp_path, cancelled=lambda: True)

    class Hanging:
        returncode = None

        def __init__(self, command, **kwargs):
            pass

        def poll(self):
            return None

        def wait(self, timeout=None):
            if self.returncode is None:
                raise subprocess.TimeoutExpired("tesseract", timeout)
            return self.returncode

        def kill(self):
            self.returncode = 1

        def communicate(self, timeout=None):
            return b"", b""

    monkeypatch.setattr(extraction_worker.subprocess, "Popen", Hanging)
    monkeypatch.setattr(extraction_worker, "OCR_TIMEOUT_SECONDS", 0)
    with pytest.raises(extraction_worker.OCRTimeoutError):
        extraction_worker.ocr_png(b"png", home=tmp_path)


def test_empty_or_error_ocr_pages_remain_exceptions(tmp_path, monkeypatch):
    from quantix import extraction_worker
    from quantix.extraction_adapters import ExtractionService

    source = tmp_path / "blank.pdf"
    body = _blank_pdf(source, pages=2)
    digest = hashlib.sha256(body).hexdigest()
    repo = Repository(tmp_path / "home")
    stored = repo.objects / digest
    stored.write_bytes(body)
    monkeypatch.setattr(
        extraction_worker,
        "ocr_available",
        lambda **kwargs: {"available": True, "tesseract": True, "engine": "synthetic", "executable": "synthetic"},
    )
    monkeypatch.setattr(extraction_worker, "render_page_png", lambda *args, **kwargs: b"png")
    calls = 0

    def ocr(png, **kwargs):
        nonlocal calls
        calls += 1
        if calls == 1:
            return None
        raise RuntimeError("synthetic OCR failure")

    monkeypatch.setattr(extraction_worker, "ocr_png", ocr)
    result = ExtractionService(repo).reprocess(
        digest,
        path=stored,
        request=ReprocessRequest(original_hash=digest, reader_id="ocr", reader_version="synthetic-1"),
    )

    assert result["extracted_pages"] == 0
    assert result["exception_pages"] == 2
    with repo.db.connect() as conn:
        segments = json.loads(
            conn.execute(
                "SELECT segments_json FROM extraction_versions WHERE id=?", (result["id"],)
            ).fetchone()[0]
        )
    assert [segment["state"] for segment in segments] == ["ocr_empty", "ocr_error"]


def test_failed_language_fallback_does_not_publish_partial_ocr(tmp_path, monkeypatch):
    from quantix import extraction_worker
    from quantix.extraction_adapters import ExtractionService

    source = tmp_path / "blank.pdf"
    body = _blank_pdf(source)
    digest = hashlib.sha256(body).hexdigest()
    repo = Repository(tmp_path / "home")
    stored = repo.objects / digest
    stored.write_bytes(body)
    monkeypatch.setattr(
        extraction_worker,
        "ocr_available",
        lambda **kwargs: {"available": True, "tesseract": True, "engine": "synthetic", "executable": "synthetic"},
    )
    monkeypatch.setattr(extraction_worker, "render_page_png", lambda *args, **kwargs: b"png")
    monkeypatch.setattr(extraction_worker, "tesseract_executable", lambda home: Path("tesseract"))
    return_codes = iter([1, 1])

    class Failed:
        def __init__(self, command, **kwargs):
            self.returncode = next(return_codes)

        def poll(self):
            return self.returncode

        def communicate(self, timeout=None):
            return b"partial stdout", b"diagnostic stderr"

        def wait(self, timeout=None):
            return self.returncode

    monkeypatch.setattr(extraction_worker.subprocess, "Popen", Failed)
    result = ExtractionService(repo).reprocess(
        digest,
        path=stored,
        request=ReprocessRequest(original_hash=digest, reader_id="ocr", reader_version="synthetic-1"),
    )

    assert result["extracted_pages"] == 0
    assert result["exception_pages"] == 1
    with repo.db.connect() as conn:
        segments = json.loads(
            conn.execute(
                "SELECT segments_json FROM extraction_versions WHERE id=?", (result["id"],)
            ).fetchone()[0]
        )
    assert segments[0]["state"] == "ocr_error"


def test_cancellation_after_final_page_blocks_persistence(tmp_path, monkeypatch):
    from quantix import extraction_worker
    from quantix.extraction_adapters import ExtractionService

    source = tmp_path / "blank.pdf"
    body = _blank_pdf(source)
    digest = hashlib.sha256(body).hexdigest()
    repo = Repository(tmp_path / "home")
    stored = repo.objects / digest
    stored.write_bytes(body)
    final_page_done = False
    monkeypatch.setattr(
        extraction_worker,
        "ocr_available",
        lambda **kwargs: {"available": True, "tesseract": True, "engine": "synthetic", "executable": "synthetic"},
    )
    monkeypatch.setattr(extraction_worker, "render_page_png", lambda *args, **kwargs: b"png")

    def ocr(png, **kwargs):
        nonlocal final_page_done
        final_page_done = True
        return "final page"

    monkeypatch.setattr(extraction_worker, "ocr_png", ocr)

    with pytest.raises(InterruptedError):
        ExtractionService(repo).reprocess(
            digest,
            path=stored,
            request=ReprocessRequest(original_hash=digest, reader_id="ocr", reader_version="synthetic-1"),
            cancelled=lambda: final_page_done,
        )
    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM extraction_versions").fetchone()[0] == 0
