"""A tender's documents: stored copies, background reading and search."""

import hashlib
import logging
import threading
from collections.abc import Callable
from datetime import UTC, datetime
from pathlib import Path, PurePosixPath
from typing import BinaryIO

from sqlalchemy import select, text, update
from sqlalchemy.orm import Session, sessionmaker

from quantix.documents import readers
from quantix.documents.arabic import searchable
from quantix.documents.models import Document, Page

log = logging.getLogger("quantix.documents")


def files_dir(home: Path, tender_id: str) -> Path:
    return home / "tenders" / tender_id / "files"


def stored_file(home: Path, document: Document) -> Path:
    """Copies are named by their content, so a file is never moved or overwritten, even while it is being read."""
    return files_dir(home, document.tender_id) / f"{document.sha256}{PurePosixPath(document.path).suffix.lower()}"


def clean_path(name: str) -> str:
    """A safe path inside the package, from the name the browser sent (it may include folders)."""
    parts = [p for p in PurePosixPath(name.replace("\\", "/")).parts if p not in ("", ".", "/")]
    if not parts or ".." in parts:
        raise ValueError(f"Not a usable file name: {name}")
    return "/".join(parts)


def store(session: Session, home: Path, tender_id: str, name: str, content: BinaryIO) -> Document | None:
    """Keep a copy of one supplied file. The same file again is ignored; a changed file replaces the older copy."""
    path = clean_path(name)
    data = content.read()
    digest = hashlib.sha256(data).hexdigest()
    current = session.scalars(
        select(Document).where(Document.tender_id == tender_id, Document.path == path, Document.status != "replaced")
    ).all()
    if any(d.sha256 == digest for d in current):
        return None
    for older in current:
        older.status = "replaced"
        older.note = "A newer copy of this file was added."
    document = Document(tender_id=tender_id, path=path, kind=readers.kind_of(path), size=len(data), sha256=digest)
    target = stored_file(home, document)
    if not target.exists():
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    session.add(document)
    session.commit()
    return document


def documents(session: Session, tender_id: str) -> list[Document]:
    query = select(Document).where(Document.tender_id == tender_id).order_by(Document.path)
    return list(session.scalars(query))


def superseded(session: Session, document_id: str | None) -> bool:
    """Whether a newer copy of the file has replaced this one."""
    document = session.get(Document, document_id) if document_id else None
    return document is not None and document.status == "replaced"


def copies(session: Session, document: Document) -> list[Document]:
    """Every copy of the same file in the tender, the current one included."""
    query = select(Document).where(Document.tender_id == document.tender_id, Document.path == document.path)
    return list(session.scalars(query))


def newer_copy(session: Session, document: Document) -> Document | None:
    """The copy that replaced this one, or None if it is current."""
    return next((d for d in copies(session, document) if d.status != "replaced" and d.id != document.id), None)


def page(session: Session, document_id: str, number: int) -> Page | None:
    return session.scalars(select(Page).where(Page.document_id == document_id, Page.number == number)).first()


def search(session: Session, tender_id: str, query: str, limit: int = 40) -> list[dict]:
    """Pages containing every word of the query, best matches first."""
    words = [w for w in searchable(query).split() if w]
    if not words:
        return []
    match = " ".join('"' + w.replace('"', '""') + '"' for w in words)
    rows = session.execute(
        text(
            "SELECT d.id, d.path, p.number, p.text "
            "FROM pages_fts JOIN pages p ON p.id = pages_fts.rowid JOIN documents d ON d.id = p.document_id "
            "WHERE pages_fts MATCH :match AND d.tender_id = :tender AND d.status = 'read' "
            "ORDER BY rank LIMIT :limit"
        ),
        {"match": match, "tender": tender_id, "limit": limit},
    )
    return [
        {"document_id": r[0], "name": r[1].rsplit("/", 1)[-1], "page": r[2], "snippet": snippet(r[3], words)}
        for r in rows
    ]


def snippet(page_text: str, words: list[str], width: int = 200) -> str:
    """The page's own words around the best line, with matching words in [brackets]."""
    lines = [line for line in page_text.splitlines() if line.strip()] or [""]
    best = max(lines, key=lambda line: sum(w in searchable(line) for w in words))
    marked = [f"[{word}]" if any(w in searchable(word) for w in words) else word for word in best.split()]
    result = " ".join(marked)
    return result if len(result) <= width else result[: width - 1].rsplit(" ", 1)[0] + " …"


class Reader:
    """Reads waiting documents one at a time on a background thread.

    `on_read` runs in the same transaction that saves a read document, before anything else can see it as read, and
    returns what to add to its note. `after_read` runs once it is saved."""

    def __init__(
        self,
        home: Path,
        sessions: sessionmaker[Session],
        on_read: Callable[[Session, Document], str | None] | None = None,
        after_read: Callable[[], None] | None = None,
    ):
        self.home = home
        self.sessions = sessions
        self.on_read = on_read
        self.after_read = after_read
        self._wake = threading.Event()
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._run, name="quantix-reader", daemon=True)

    def start(self) -> None:
        with self.sessions() as session:  # documents interrupted by a restart are read again
            session.execute(update(Document).where(Document.status == "reading").values(status="waiting"))
            session.commit()
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
            while not self._stop.is_set() and self._read_next():
                pass

    def _read_next(self) -> bool:
        with self.sessions() as session:
            document = session.scalars(select(Document).where(Document.status == "waiting").limit(1)).first()
            if document is None:
                return False
            # Conditional updates, so a newer copy that replaces this one meanwhile is never overwritten.
            claimed = session.execute(
                update(Document)
                .where(Document.id == document.id, Document.status == "waiting")
                .values(status="reading")
            )
            session.commit()
            if claimed.rowcount == 0:
                return True
            path, kind, document_id = stored_file(self.home, document), document.kind, document.id
            try:
                pages = readers.read_file(path, kind)
            except readers.Unreadable as reason:
                outcome, note, pages = "unreadable", str(reason), []
            except Exception:
                log.exception("Reading %s failed", document.id)
                outcome, note, pages = "failed", "Quantix couldn't read this file.", []
            else:
                outcome, note = "read", None
            try:
                if outcome == "read":
                    for p in pages:
                        session.add(
                            Page(
                                document_id=document.id,
                                number=p.number,
                                text=p.text,
                                search_text=searchable(p.text),
                                has_text=p.has_text,
                                width=p.width,
                                height=p.height,
                            )
                        )
                    scans = sum(not p.has_text for p in pages)
                    if kind == "pdf" and scans:
                        note = (
                            f"{scans} of {len(pages)} pages are scans without text. "
                            "The office reads them from the page image."
                        )
                    session.execute(update(Document).where(Document.id == document.id).values(page_count=len(pages)))
                if self.on_read is not None:
                    session.refresh(document)
                    try:
                        added = self.on_read(session, document)
                    except Exception:  # noqa: BLE001  (the document is read even if what follows from it fails)
                        log.exception("Following up the reading of %s failed", document.id)
                        added = None
                    note = " ".join(n for n in (note, added) if n) or None
                saved = session.execute(
                    update(Document)
                    .where(Document.id == document.id, Document.status == "reading")
                    .values(status=outcome, note=note, read_at=datetime.now(UTC))
                )
                session.commit()
                if saved.rowcount and self.after_read is not None:
                    self.after_read()
            except Exception:  # noqa: BLE001  (one bad file must never stop the reader for every other file)
                session.rollback()
                log.exception("Saving %s failed", document_id)
                session.execute(
                    update(Document)
                    .where(Document.id == document_id, Document.status == "reading")
                    .values(status="failed", note="Quantix couldn't read this file.")
                )
                session.commit()
            return True
