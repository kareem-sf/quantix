"""Bounded page rendering and Tesseract OCR. Missing OCR is an exception, not success."""

from __future__ import annotations

import io
import os
import subprocess
import tempfile
import time
from collections.abc import Callable
from contextlib import closing, contextmanager
from dataclasses import dataclass
from pathlib import Path

import pypdfium2 as pdfium

from .documents import PDFIUM_LOCK
from .storage import runtime_tmp_dir
from .tesseract_runtime import tesseract_executable

OCR_TIMEOUT_SECONDS = 90
# Text recognition is most accurate near 300 DPI; bound very large sheets.
OCR_DPI = 300
OCR_MAX_PIXELS = 4200


class OCRTimeoutError(TimeoutError):
    """Tesseract exceeded the bounded page OCR time."""


class OCRProcessError(RuntimeError):
    """Tesseract failed after the permitted language fallback."""


@contextmanager
def _pdfium_lock(cancelled: Callable[[], bool] | None = None):
    while not PDFIUM_LOCK.acquire(timeout=0.1):
        if cancelled and cancelled():
            raise InterruptedError("PDF rendering cancelled")
    try:
        if cancelled and cancelled():
            raise InterruptedError("PDF rendering cancelled")
        yield
    finally:
        PDFIUM_LOCK.release()


def render_page_png(
    path: Path,
    page: int,
    *,
    home: Path | None = None,
    cancelled: Callable[[], bool] | None = None,
    for_ocr: bool = False,
) -> bytes:
    if page < 1:
        raise ValueError("Page must be a positive integer.")
    with _pdfium_lock(cancelled), closing(pdfium.PdfDocument(path)) as document:
        if page > len(document):
            raise ValueError("This page is outside the document.")
        with closing(document[page - 1]) as original:
            width, height = original.get_size()
            if for_ocr:
                scale = min(OCR_DPI / 72, OCR_MAX_PIXELS / max(width, height, 1))
            else:
                scale = min(2.0, 1800 / max(width, 1))
            with closing(original.render(scale=scale)) as bitmap:
                image = bitmap.to_pil().copy()
    output = io.BytesIO()
    image.save(output, format="PNG")
    return output.getvalue()


def ocr_available(*, home: Path | None = None) -> dict:
    executable = tesseract_executable(home)
    return {
        "tesseract": executable is not None,
        "engine": "tesseract" if executable else None,
        "available": executable is not None,
        "executable": str(executable) if executable else None,
    }


def ocr_png(
    png: bytes,
    *,
    home: Path | None = None,
    languages: str = "eng+ara",
    cancelled: Callable[[], bool] | None = None,
) -> str | None:
    if cancelled and cancelled():
        raise InterruptedError("OCR cancelled")
    executable = tesseract_executable(home)
    if executable is None:
        return None
    folder = runtime_tmp_dir(home)
    folder.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        prefix="ocr-page-", suffix=".png", dir=folder, delete=False
    ) as handle:
        image = Path(handle.name)
        handle.write(png)
    tessdata = executable.parent / "tessdata"
    env = None
    if tessdata.is_dir():
        env = {**os.environ, "TESSDATA_PREFIX": str(tessdata)}

    def run(command: list[str]):
        process = subprocess.Popen(
            command,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
        )
        started = time.monotonic()

        def stop_and_drain() -> None:
            if process.poll() is None:
                process.kill()
            try:
                process.communicate()
            except (OSError, ValueError):
                pass

        try:
            while True:
                if cancelled and cancelled():
                    stop_and_drain()
                    raise InterruptedError("OCR cancelled")
                remaining = OCR_TIMEOUT_SECONDS - (time.monotonic() - started)
                if remaining <= 0:
                    stop_and_drain()
                    raise OCRTimeoutError("OCR timed out for this page")
                try:
                    stdout, _ = process.communicate(timeout=min(0.1, remaining))
                    return process.returncode, stdout or b""
                except subprocess.TimeoutExpired:
                    pass
        finally:
            if process.poll() is None:
                stop_and_drain()

    try:
        return_code, output = run([str(executable), str(image), "stdout", "-l", languages])
        if return_code != 0 and "+" in languages:
            return_code, output = run([str(executable), str(image), "stdout", "-l", "eng"])
        if return_code != 0:
            raise OCRProcessError("OCR failed for this page after the language fallback.")
    finally:
        image.unlink(missing_ok=True)
    text = output.decode("utf-8", errors="replace").strip()
    return text or None


@dataclass(frozen=True)
class OCRPage:
    text: str
    # Mean Tesseract word confidence (0-100), weighted by word length.
    confidence: float


def _tsv_confidence(tsv: str) -> float:
    total = weight = 0.0
    for line in tsv.splitlines()[1:]:
        columns = line.split("\t")
        if len(columns) < 12 or not columns[11].strip():
            continue
        try:
            confidence = float(columns[10])
        except ValueError:
            continue
        if confidence < 0:
            continue
        size = len(columns[11].strip())
        total += confidence * size
        weight += size
    return round(total / weight, 1) if weight else 0.0


def ocr_page(
    png: bytes,
    *,
    home: Path | None = None,
    languages: str = "eng+ara",
    cancelled: Callable[[], bool] | None = None,
) -> OCRPage | None:
    """Recognise one page image, returning its text and word confidence in one pass."""

    if cancelled and cancelled():
        raise InterruptedError("OCR cancelled")
    executable = tesseract_executable(home)
    if executable is None:
        return None
    folder = runtime_tmp_dir(home)
    folder.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="ocr-page-", dir=folder))
    image = work / "page.png"
    image.write_bytes(png)
    tessdata = executable.parent / "tessdata"
    env = {**os.environ, "TESSDATA_PREFIX": str(tessdata)} if tessdata.is_dir() else None

    def run(langs: str) -> int:
        process = subprocess.Popen(
            [str(executable), str(image), str(work / "out"), "-l", langs, "txt", "tsv"],
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
        )
        started = time.monotonic()
        try:
            while process.poll() is None:
                if cancelled and cancelled():
                    process.kill()
                    raise InterruptedError("OCR cancelled")
                if time.monotonic() - started > OCR_TIMEOUT_SECONDS:
                    process.kill()
                    raise OCRTimeoutError("OCR timed out for this page")
                time.sleep(0.05)
            return process.returncode
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()

    try:
        code = run(languages)
        if code != 0 and "+" in languages:
            code = run("eng")
        if code != 0:
            raise OCRProcessError("OCR failed for this page after the language fallback.")
        text = (work / "out.txt").read_text(encoding="utf-8", errors="replace").strip()
        confidence = _tsv_confidence((work / "out.tsv").read_text(encoding="utf-8", errors="replace"))
    finally:
        for child in work.iterdir():
            child.unlink(missing_ok=True)
        work.rmdir()
    return OCRPage(text, confidence) if text else None
