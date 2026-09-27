"""Reading the words on scanned pages, on this computer. Once a document is read, every page without text of its own
is read by OCR in the background, one page at a time: in English, or in Arabic where the document is Arabic. The
words go into the page's text and both search indexes, marked as OCR with the confidence, so the office checks
the figures that matter on the page image."""

import logging
import os
import threading
from collections.abc import Callable
from pathlib import Path

from sqlalchemy import select, text, update
from sqlalchemy.orm import Session, sessionmaker

from quantix.documents import library, readers
from quantix.documents.arabic import has_arabic, searchable
from quantix.documents.models import Document, Page

log = logging.getLogger("quantix.ocr")

DPI = 200  # scans are rendered at this resolution for OCR: small notes on drawings need it
MAX_SIDE = 3300  # pixels: an A3 sheet at 200 dpi; larger sheets are read at this size
IMAGE_WIDTH = 2000  # pixels an image document is read at
MIN_SCORE = 0.5  # words read with less confidence are left out
ARABIC_SHARE = 0.3  # a document whose letters are at least this Arabic is read with the Arabic model
SURE = 0.85  # a first page read this well in English needs no Arabic try


def models_dir(home: Path) -> Path:
    return home / "models" / "ocr"


class Unavailable(Exception):
    """The Arabic model couldn't be fetched, e.g. with no connection: its pages wait for the next start."""


class Engines:
    """The OCR models, loaded on first use. English comes with the OCR package. Arabic, which also reads the English
    on an Arabic page, is fetched once into the data home and checked against its pinned digest by the package."""

    _loaded: dict[tuple[str, str], object] = {}  # one copy of each model for the whole process
    _loading = threading.Lock()

    def __init__(self, home: Path):
        self.home = home

    def get(self, language: str):
        key = (language, str(models_dir(self.home)) if language == "ar" else "")
        with self._loading:
            if key not in self._loaded:
                self._loaded[key] = self._load(language)
            return self._loaded[key]

    def _load(self, language: str):
        from rapidocr import LangRec, ModelType, OCRVersion, RapidOCR

        params: dict[str, object] = {
            "Global.log_level": "warning",
            "Global.max_side_len": MAX_SIDE,
            "EngineConfig.onnxruntime.intra_op_num_threads": max(1, (os.cpu_count() or 2) // 2),
        }
        if language == "ar":
            models_dir(self.home).mkdir(parents=True, exist_ok=True)
            params |= {
                "Global.model_root_dir": str(models_dir(self.home)),
                "Rec.lang_type": LangRec.ARABIC,
                "Rec.ocr_version": OCRVersion.PPOCRV5,
                "Rec.model_type": ModelType.MOBILE,
            }
        try:
            return RapidOCR(params=params)
        except Exception as error:  # noqa: BLE001  (a failed download surfaces as the package's own errors)
            raise Unavailable(str(error)) from error


def lines(boxes, words, scores) -> tuple[str, float]:
    """The words the OCR kept, one text line per row of the page, cells in reading order joined by " | ", and
    their mean confidence."""
    kept = []
    for box, word, score in zip(boxes, words, scores, strict=True):
        if score >= MIN_SCORE and word.strip():
            ys, xs = [float(p[1]) for p in box], [float(p[0]) for p in box]
            kept.append((min(ys), max(ys), min(xs), word.strip(), float(score)))
    rows: list[tuple[float, float, list[tuple[float, str]]]] = []
    for top, bottom, left, word, _ in sorted(kept, key=lambda k: (k[0] + k[1]) / 2):
        centre = (top + bottom) / 2
        if rows and abs(centre - rows[-1][0]) < rows[-1][1] / 2:
            rows[-1][2].append((left, word))
        else:
            rows.append((centre, bottom - top, [(left, word)]))
    text_lines = []
    for _, _, cells in rows:
        right_to_left = has_arabic(" ".join(w for _, w in cells))
        ordered = sorted(cells, key=lambda c: -c[0] if right_to_left else c[0])
        text_lines.append(" | ".join(w for _, w in ordered))
    score = sum(k[4] for k in kept) / len(kept) if kept else 0.0
    return "\n".join(text_lines), score


def read_image(engine, png: bytes) -> tuple[str, float]:
    result = engine(png)
    if not result.txts:
        return "", 0.0
    return lines(result.boxes, result.txts, result.scores)


def arabic_share(session: Session, document_id: str) -> float | None:
    """How Arabic the document's own text is, or None when it has too little text to tell."""
    own = session.scalars(select(Page.text).where(Page.document_id == document_id, Page.ocr.is_(None)))
    letters = [c for c in "".join(own)[:200_000] if c.isalpha()]
    if len(letters) < 200:
        return None
    return sum(has_arabic(c) for c in letters) / len(letters)


class Ocr:
    """Reads scanned pages on a background thread. `after_page` runs after each page read, e.g. to wake the
    meaning indexer."""

    def __init__(self, home: Path, sessions: sessionmaker[Session], after_page: Callable[[], None] | None = None):
        self.home = home
        self.sessions = sessions
        self.after_page = after_page
        self.engines = Engines(home)
        self._languages: dict[str, str] = {}  # per document, once chosen
        self._waiting: set[str] = set()  # documents whose model couldn't be loaded this run
        self._wake = threading.Event()
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._run, name="quantix-ocr", daemon=True)

    def start(self) -> None:
        self._thread.start()
        self.wake()

    def wake(self) -> None:
        self._wake.set()

    def stop(self) -> None:
        self._stop.set()
        self._wake.set()
        self._thread.join(timeout=10)

    def _run(self) -> None:
        while not self._stop.is_set():
            self._wake.wait()
            self._wake.clear()
            try:
                while not self._stop.is_set() and self.read_next():
                    pass
            except Exception:  # noqa: BLE001  (the next wake tries again; reading documents goes on regardless)
                log.exception("OCR stopped")

    def read_next(self) -> bool:
        """Read the next scanned page. False when there is none left."""
        with self.sessions() as session:
            query = (
                select(Page.id, Page.number, Page.width, Document)
                .join(Document, Document.id == Page.document_id)
                .where(
                    Page.has_text.is_(False),
                    Page.ocr.is_(None),
                    Document.status == "read",
                    Document.kind.in_(("pdf", "image")),
                    Document.id.not_in(self._waiting),
                )
                .order_by(Document.created_at, Page.number)
                .limit(1)
            )
            found = session.execute(query).first()
            if found is None:
                return False
            page_id, number, points, document = found
            document_id, kind, path = document.id, document.kind, library.stored_file(self.home, document)
            same = (
                select(Page.text, Page.ocr, Page.ocr_score)
                .join(Document, Document.id == Page.document_id)
                .where(Document.sha256 == document.sha256, Page.number == number, Page.ocr.is_not(None))
                .where(Page.ocr != "failed")
            )
            earlier = session.execute(same.limit(1)).first()  # the same file in another tender, already read
            language = self._languages.get(document_id)
            if language is None:
                share = arabic_share(session, document_id)
                language = None if share is None else ("ar" if share >= ARABIC_SHARE else "en")
        if earlier is not None:
            words, language, score = earlier
            self._save(page_id, words, score or 0.0, language)
            return True
        try:
            if kind == "image":
                png = readers.render_image(path, IMAGE_WIDTH)
            else:
                png = readers.render_page(path, number, min(MAX_SIDE, round((points or 595) / 72 * DPI)))
            words, score, language = self._read(png, language)
        except Unavailable:
            log.warning("The Arabic OCR model couldn't be loaded; %s waits for the next start", document_id)
            self._waiting.add(document_id)
            return True
        except Exception:  # noqa: BLE001  (one page that can't be read must not stop the others)
            log.exception("OCR of %s page %s failed", document_id, number)
            self._save(page_id, "", 0.0, "failed")
            return True
        self._languages.setdefault(document_id, language)
        self._save(page_id, words, score, language)
        return True

    def _read(self, png: bytes, language: str | None) -> tuple[str, float, str]:
        """The page's words, their confidence and the model that read them. A document with no text of its own
        is tried in English first; if that reads its first page poorly, Arabic is tried and the better kept."""
        if language == "ar":
            return (*read_image(self.engines.get("ar"), png), "ar")
        words, score = read_image(self.engines.get("en"), png)
        if language is None and score < SURE:
            try:
                arabic = read_image(self.engines.get("ar"), png)
            except Unavailable:
                arabic = ("", 0.0)
            if arabic[1] > score:
                return (*arabic, "ar")
        return words, score, "en"

    def _save(self, page_id: int, words: str, score: float, language: str) -> None:
        with self.sessions() as session:
            page = session.get(Page, page_id)
            if page is None or page.has_text or page.ocr is not None:  # deleted or read meanwhile
                return
            visible = sum(not c.isspace() for c in words)
            if language == "failed" or visible < readers.SCAN_CHARACTERS:
                page.ocr = "failed" if language == "failed" else "empty"
                session.commit()
                return
            old = page.search_text
            session.execute(
                text("INSERT INTO pages_fts(pages_fts, rowid, search_text) VALUES ('delete', :id, :old)"),
                {"id": page.id, "old": old},
            )
            session.execute(
                update(Page)
                .where(Page.id == page.id)
                .values(text=words, search_text=searchable(words), has_text=True, ocr=language, ocr_score=score)
            )
            session.execute(
                text("INSERT INTO pages_fts(rowid, search_text) VALUES (:id, :new)"),
                {"id": page.id, "new": searchable(words)},
            )
            session.commit()
        if self.after_page is not None:
            self.after_page()
