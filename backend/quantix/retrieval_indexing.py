"""Desired/published retrieval generations and coalesced background preparation."""

from __future__ import annotations

import asyncio
import threading
from typing import TYPE_CHECKING

from .semantic_models import SemanticUnavailable

if TYPE_CHECKING:
    from .repository import Repository

_IMPORTING = "SELECT 1 FROM runs WHERE tender_id=? AND kind='import' AND status IN ('queued','running') LIMIT 1"


class RetrievalIndexer:
    """One coalesced meaning-index request per Tender, outside the AI execution lane."""

    def __init__(self, repo: "Repository"):
        self.repo = repo
        self.closing = False
        self._loop: asyncio.AbstractEventLoop | None = None
        self._pending: set[str] = set()
        self._cancel: dict[str, threading.Event] = {}
        self._active: str | None = None
        self._pump: asyncio.Task | None = None
        self._lock = threading.Lock()

    def bind_loop(self, loop: asyncio.AbstractEventLoop) -> None:
        self._loop = loop

    def busy(self) -> bool:
        with self._lock:
            return self._active is not None or bool(self._pending)

    def request_refresh(self, tender_id: str, *, force: bool = False):
        """Record that this Tender's desired generation should be published.

        Import still in progress is deferred so a moving snapshot is not indexed.
        Manual rebuild passes force=True.
        """
        self.repo.get_tender(tender_id)
        if self.closing:
            return self.status(tender_id)
        if not force and self._import_active(tender_id):
            with self._lock:
                self._pending.add(tender_id)
            return self.status(tender_id)
        with self._lock:
            self._pending.add(tender_id)
            if self._active == tender_id:
                event = self._cancel.get(tender_id)
                if event:
                    event.set()
        self._kick()
        return self.status(tender_id)

    def cancel(self, tender_id: str):
        self.repo.get_tender(tender_id)
        with self._lock:
            self._pending.discard(tender_id)
            event = self._cancel.get(tender_id)
        if event:
            event.set()
        self._mark(tender_id, status="stopped", detail="Meaning search preparation was stopped.")
        return self.status(tender_id)

    def recover(self) -> None:
        """Resume preparation for Tenders whose published generation is behind."""
        from .semantic import SemanticService

        service = SemanticService(self.repo)
        with self.repo.db.connect() as conn:
            ids = [row[0] for row in conn.execute("SELECT id FROM tenders ORDER BY id")]
        for tender_id in ids:
            state = service.status(tender_id)
            if state["evidence_count"] and state["status"] in {
                "not_indexed",
                "stale",
                "stopped",
                "failed",
                "model_missing",
            }:
                self.request_refresh(tender_id)

    def status(self, tender_id: str):
        from .semantic import SemanticService

        return SemanticService(self.repo).status(tender_id)

    def refresh_now(self, tender_id: str, cancelled=None, progress=None):
        from .semantic import SemanticService

        return SemanticService(self.repo).index(tender_id, cancelled, progress)

    async def close(self) -> None:
        self.closing = True
        with self._lock:
            events = list(self._cancel.values())
            self._pending.clear()
        for event in events:
            event.set()
        pump = self._pump
        if pump is not None:
            pump.cancel()
            try:
                await pump
            except asyncio.CancelledError:
                pass

    def _import_active(self, tender_id: str) -> bool:
        with self.repo.db.connect() as conn:
            return conn.execute(_IMPORTING, (tender_id,)).fetchone() is not None

    def _kick(self) -> None:
        loop = self._loop
        if loop is None:
            try:
                loop = asyncio.get_running_loop()
                self._loop = loop
            except RuntimeError:
                return
        if not loop.is_running():
            return

        def start():
            if self.closing:
                return
            if self._pump is None or self._pump.done():
                self._pump = loop.create_task(self._run_loop(), name="quantix-retrieval-index")

        loop.call_soon_threadsafe(start)

    async def _run_loop(self) -> None:
        while not self.closing:
            with self._lock:
                if not self._pending:
                    self._active = None
                    return
                tender_id = sorted(self._pending)[0]
                self._pending.discard(tender_id)
                cancelled = threading.Event()
                self._cancel[tender_id] = cancelled
                self._active = tender_id
            try:
                if self._import_active(tender_id):
                    # Import started after we dequeued. Wait for its completion notify.
                    continue
                self._mark(
                    tender_id,
                    status="running",
                    progress=0,
                    detail="Preparing search by meaning for the current documents.",
                )

                def progress(percent, detail):
                    self._mark(tender_id, status="running", progress=percent, detail=detail)

                try:
                    await asyncio.to_thread(self.refresh_now, tender_id, cancelled.is_set, progress)
                    self._mark(
                        tender_id,
                        status="idle",
                        progress=100,
                        detail="Search by meaning is ready for the current source evidence.",
                    )
                except InterruptedError:
                    self._mark(
                        tender_id,
                        status="stopped",
                        detail="Meaning search preparation was stopped.",
                    )
                except SemanticUnavailable as error:
                    status = "failed" if error.code not in {"empty"} else "idle"
                    if error.code == "model_missing":
                        status = "failed"
                    self._mark(
                        tender_id,
                        status=status,
                        error=str(error),
                        detail=str(error),
                    )
                except OSError as error:
                    self._mark(
                        tender_id,
                        status="failed",
                        error=str(error),
                        detail="This computer does not have enough free disk or memory to finish meaning search.",
                    )
                except Exception as error:  # noqa: BLE001 - surface one recovery action
                    self._mark(
                        tender_id,
                        status="failed",
                        error=str(error),
                        detail="Meaning search could not be prepared. Retry from Documents.",
                    )
            finally:
                with self._lock:
                    self._cancel.pop(tender_id, None)
                    if self._active == tender_id:
                        self._active = None

    def _mark(self, tender_id, *, status, progress=None, detail="", error=None):
        from .semantic import SemanticService

        SemanticService(self.repo).mark_refresh(
            tender_id, status=status, progress=progress, detail=detail, error=error
        )
