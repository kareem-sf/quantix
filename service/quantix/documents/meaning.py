"""Search by meaning. A small multilingual model on this computer turns text into vectors, so a search finds what
says the same thing in other words, or in Arabic. Pages are indexed on a background thread once they are read;
short texts such as rate names are computed the first time they are searched. Vectors are kept by the text's
digest, so the same text is computed once."""

import hashlib
import logging
import os
import shutil
import threading
import urllib.request
from collections.abc import Callable
from pathlib import Path

import numpy as np
import onnxruntime
from sqlalchemy import exists, select
from sqlalchemy.dialects.sqlite import insert
from sqlalchemy.orm import Session, sessionmaker
from tokenizers import Tokenizer

from quantix.documents.models import Document, Page, PageChunk, Vector

log = logging.getLogger("quantix.meaning")

NAME = "multilingual-e5-small"
SOURCE = "https://huggingface.co/Xenova/multilingual-e5-small/resolve/761b726dd34fb83930e26aab4e9ac3899aa1fa78/"
FILES = {  # saved as: (path at the source, SHA-256)
    "model.onnx": ("onnx/model_quantized.onnx", "f80102d3f2a1229f387d3c81909990d8945513e347b0eab049f7de3c6f98c193"),
    "tokenizer.json": ("tokenizer.json", "0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39"),
}
# Set on a real tender's pages and on rate and firm names. The model rates everything fairly alike, so a passage is
# close when it stands out from the tender's typical passage; a short name has no such crowd to stand out from.
MARGIN = 0.05  # how far above the tender's median passage a close passage scores
CLOSE = 0.84  # the score from which a short text such as a rate's name is close
PASSAGE = 800  # characters in one indexed passage
PAGES = 32  # pages indexed in one round
RETRY = 900.0  # seconds before trying again to fetch the model after a failure

_model: "Model | None" = None
_loading = threading.Lock()


class Model:
    def __init__(self, folder: Path):
        self.tokenizer = Tokenizer.from_file(str(folder / "tokenizer.json"))
        self.tokenizer.enable_truncation(512)
        options = onnxruntime.SessionOptions()
        options.intra_op_num_threads = max(1, (os.cpu_count() or 2) // 2)  # faster than all, and leaves room
        self.session = onnxruntime.InferenceSession(str(folder / "model.onnx"), options)

    def embed(self, texts: list[str], kind: str = "passage") -> np.ndarray:
        """Unit vectors for the texts: kind is "query" for what is searched for, "passage" for what is searched."""
        order = sorted(range(len(texts)), key=lambda i: len(texts[i]))  # similar lengths together, less padding
        vectors = np.zeros((len(texts), 384), np.float32)
        for start in range(0, len(order), 16):
            batch = order[start : start + 16]
            encoded = self.tokenizer.encode_batch([f"{kind}: {texts[i]}" for i in batch])
            ids = np.zeros((len(batch), max(len(e.ids) for e in encoded)), np.int64)
            mask = np.zeros_like(ids)
            for row, e in enumerate(encoded):
                ids[row, : len(e.ids)] = e.ids
                mask[row, : len(e.ids)] = 1
            hidden = self.session.run(
                None, {"input_ids": ids, "attention_mask": mask, "token_type_ids": np.zeros_like(ids)}
            )[0]
            mean = (hidden * mask[..., None]).sum(axis=1) / mask.sum(axis=1, keepdims=True)
            vectors[batch] = mean / np.linalg.norm(mean, axis=1, keepdims=True)
        return vectors


def models_dir(home: Path) -> Path:
    return home / "models" / NAME


def load(home: Path) -> Model:
    """The model, fetched the first time and checked against its pinned digests."""
    global _model
    with _loading:
        if _model is None:
            folder = models_dir(home)
            fetch(folder)
            _model = Model(folder)
        return _model


def loaded() -> Model | None:
    """The model once it is ready; until then searches go by words alone."""
    return _model


def fetch(folder: Path) -> None:
    folder.mkdir(parents=True, exist_ok=True)
    for name, (path, sha256) in FILES.items():
        target = folder / name
        if target.exists() and _sha256(target) == sha256:
            continue
        partial = folder / f"{name}.part"
        with urllib.request.urlopen(SOURCE + path, timeout=60) as response, partial.open("wb") as out:
            shutil.copyfileobj(response, out, 1 << 20)
        if _sha256(partial) != sha256:
            partial.unlink()
            raise ValueError(f"The downloaded {name} doesn't match its pinned digest.")
        partial.replace(target)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def digest(text: str) -> str:
    return hashlib.sha256(f"{NAME}\n{text}".encode()).hexdigest()


def passages(text: str) -> list[tuple[int, int]]:
    """Where a page's passages start and stop: whole lines up to about PASSAGE characters, leaving out passages
    that aren't readable words, such as text from a PDF font Quantix can't map."""
    spans, start = [], 0
    while start < len(text):
        stop = min(len(text), start + PASSAGE)
        if stop < len(text):  # end at a line's end, or else between words
            floor = start + PASSAGE // 4
            cut = text.rfind("\n", floor, stop)
            cut = cut if cut >= 0 else text.rfind(" ", floor, stop)
            stop = cut + 1 if cut >= 0 else stop
        if _readable(text[start:stop]):
            spans.append((start, stop))
        start = stop
    return spans


def _readable(passage: str) -> bool:
    characters = [c for c in passage if not c.isspace()]
    return len(passage.split()) >= 10 and sum(c.isalnum() for c in characters) >= 0.7 * len(characters)


def vectors_of(session: Session, model: Model, texts: list[str]) -> np.ndarray:
    """The texts' vectors, computing those not seen before. New ones are kept when the session's work is saved."""
    digests = [digest(t) for t in texts]
    unique = list(dict.fromkeys(digests))
    known: dict[str, bytes] = {}
    with session.no_autoflush:  # the caller's changes stay unwritten, so no write lock is held while computing
        for i in range(0, len(unique), 500):
            found = session.execute(select(Vector.digest, Vector.vector).where(Vector.digest.in_(unique[i : i + 500])))
            known.update((d, v) for d, v in found)
    missing = {d: t for d, t in zip(digests, texts, strict=True) if d not in known}
    if missing:
        known.update(zip(missing, (v.tobytes() for v in model.embed(list(missing.values()))), strict=True))
        rows = [{"digest": d, "vector": known[d]} for d in missing]
        for i in range(0, len(rows), 500):
            session.execute(insert(Vector).values(rows[i : i + 500]).on_conflict_do_nothing())
    return np.frombuffer(b"".join(known[d] for d in digests), np.float32).reshape(len(texts), -1)


def closest[T](session: Session, query: str, items: list[T], text_of: Callable[[T], str]) -> list[T]:
    """The items whose text is close in meaning to the query, closest first; none while the model isn't ready."""
    model = loaded()
    if model is None or not items or not query.strip():
        return []
    scores = vectors_of(session, model, [text_of(i) for i in items]) @ model.embed([query], "query")[0]
    return [items[i] for i in np.argsort(-scores) if scores[i] >= CLOSE]


def close_pages(session: Session, tender_id: str, query: str, limit: int) -> list[tuple[int, float, int, int]]:
    """The tender's pages close in meaning to the query, closest first: (page id, similarity, and where its closest
    passage starts and stops)."""
    model = loaded()
    if model is None:
        return []
    rows = session.execute(
        select(PageChunk.page_id, PageChunk.start, PageChunk.stop, Vector.vector)
        .join(Vector, Vector.digest == PageChunk.digest)
        .join(Page, Page.id == PageChunk.page_id)
        .join(Document, Document.id == Page.document_id)
        .where(Document.tender_id == tender_id, Document.status == "read")
    ).all()
    if not rows:
        return []
    matrix = np.frombuffer(b"".join(r.vector for r in rows), np.float32).reshape(len(rows), -1)
    scores = matrix @ model.embed([query], "query")[0]
    floor = float(np.median(scores)) + MARGIN
    best: dict[int, tuple[float, int, int]] = {}
    for row, score in zip(rows, scores.tolist(), strict=True):
        if score >= floor and score > best.get(row.page_id, (0.0,))[0]:
            best[row.page_id] = (score, row.start, row.stop)
    ranked = sorted(best.items(), key=lambda item: -item[1][0])[:limit]
    return [(page_id, score, start, stop) for page_id, (score, start, stop) in ranked]


class Indexer:
    """Indexes read pages by meaning on a background thread, a few at a time, so the office keeps working. It
    fetches the model the first time, and without it the office searches by words alone."""

    def __init__(self, home: Path, sessions: sessionmaker[Session]):
        self.home = home
        self.sessions = sessions
        self._wake = threading.Event()
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._run, name="quantix-meaning", daemon=True)

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
        model = None
        while not self._stop.is_set():
            if model is None:
                try:
                    model = load(self.home)
                except Exception:  # noqa: BLE001  (offline, or the download failed: words still work)
                    log.warning("The meaning model isn't available; trying again later.", exc_info=True)
                    self._wake.wait(RETRY)
                    self._wake.clear()
                    continue
            self._wake.wait()
            self._wake.clear()
            try:
                while not self._stop.is_set() and self._index_next(model):
                    pass
            except Exception:  # noqa: BLE001  (a failed round is tried again on the next wake)
                log.exception("Indexing pages by meaning failed")

    def _index_next(self, model: Model) -> bool:
        with self.sessions() as session:
            waiting = session.execute(
                select(Page.id, Page.text)
                .join(Document, Document.id == Page.document_id)
                .where(
                    Document.status == "read",
                    Page.has_text,
                    ~exists().where(PageChunk.page_id == Page.id),
                )
                .limit(PAGES)
            ).all()
        if not waiting:
            return False
        chunks, texts = [], []
        for page_id, text in waiting:
            spans = passages(text)
            chunks += [PageChunk(page_id=page_id, start=a, stop=b, digest=digest(text[a:b])) for a, b in spans]
            chunks += [] if spans else [PageChunk(page_id=page_id, start=0, stop=0, digest=None)]
            texts += [text[a:b] for a, b in spans]
        with self.sessions() as session:
            if texts:
                vectors_of(session, model, texts)
            present = set(session.scalars(select(Page.id).where(Page.id.in_([p for p, _ in waiting]))))
            session.add_all(c for c in chunks if c.page_id in present)  # a page deleted meanwhile is left out
            session.commit()
        return True
