"""Confirmed reset journal and atomic service admission; native code owns the purge.

This module can be imported by the launcher before diagnostics, database or
worker initialization. It never removes workspace files or provider auth files.
"""

from __future__ import annotations

import asyncio
import ctypes
import hashlib
import json
import os
import re
import sqlite3
import threading
from contextlib import closing
from datetime import UTC, datetime
from pathlib import Path
from uuid import uuid4

from .reset_credentials import WindowsCredentialAdapter, checked_owned_path
from .reset_models import ResetPreview, ResetReceipt, ResetRequest, ResetStatus
from .storage import normal_home

JOURNAL_NAME = "pending-reset.json"
PHASES = {"cleaning_credentials", "credential_error", "ready", "deleting", "failed"}
PENDING_DETAIL = "Quantix reset is pending. Finish the reset before opening Tender work."
CLEANING_DETAIL = "Removing Quantix's saved AI and mail credentials."
CREDENTIAL_ERROR_DETAIL = "Quantix could not finish removing its saved credentials. Close any Quantix AI sign-in windows and choose Retry."
READY_DETAIL = "Saved credentials are removed. Close Quantix to finish deleting its data."


def load_journal(home: Path) -> dict | None:
    """Validate local control data without following a redirected reset marker."""
    root = Path(home).absolute()
    path = checked_owned_path(root, root / JOURNAL_NAME)
    if not path.exists():
        return None
    try:
        info = path.stat()
        if not path.is_file() or info.st_nlink != 1 or info.st_size > 8 * 1024 * 1024:
            raise ValueError
        value = json.loads(path.read_text(encoding="utf-8"))
        if (
            not isinstance(value, dict)
            or type(value.get("format")) is not int
            or value.get("format") != 1
            or not re.fullmatch(r"[a-f0-9]{32}", str(value.get("reset_id", "")))
            or value.get("home") != str(root)
            or value.get("phase") not in PHASES
            or type(value.get("credentials_cleared")) is not bool
            or not re.fullmatch(r"[a-f0-9]{64}", str(value.get("fingerprint", "")))
            or not isinstance(value.get("confirmed_at"), str)
            or not isinstance(value.get("detail"), str)
            or (
                value["phase"] in {"ready", "deleting", "failed"}
                and not value["credentials_cleared"]
            )
            or (
                value["phase"] in {"cleaning_credentials", "credential_error"}
                and value["credentials_cleared"]
            )
        ):
            raise ValueError
        targets = value.get("credential_targets")
        if targets is not None and (
            not isinstance(targets, list)
            or any(
                not isinstance(item, dict)
                or set(item) != {"type", "target"}
                or type(item["type"]) is not int
                or item["type"] != 1
                or not isinstance(item["target"], str)
                or not item["target"]
                or len(item["target"]) > 32767
                or "\0" in item["target"]
                for item in targets
            )
        ):
            raise ValueError
        return value
    except (OSError, ValueError, TypeError) as error:
        raise ValueError(
            "The pending reset record could not be verified. Keep Quantix closed and repair this reset record before continuing."
        ) from error


def write_journal(home: Path, journal: dict) -> None:
    path = checked_owned_path(home, home / JOURNAL_NAME)
    temporary = checked_owned_path(home, home / f".pending-reset-{uuid4().hex}.tmp")
    try:
        with temporary.open("x", encoding="utf-8") as stream:
            json.dump(journal, stream, ensure_ascii=False, separators=(",", ":"))
            stream.flush()
            os.fsync(stream.fileno())
        if os.name == "nt":
            from ctypes import wintypes

            kernel = ctypes.WinDLL("Kernel32.dll", use_last_error=True)
            kernel.MoveFileExW.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR, wintypes.DWORD]
            kernel.MoveFileExW.restype = wintypes.BOOL
            # REPLACE_EXISTING | WRITE_THROUGH: inventory publication is durable
            # before an OS credential can be removed.
            if not kernel.MoveFileExW(str(temporary), str(path), 0x1 | 0x8):
                raise ctypes.WinError(ctypes.get_last_error())
        else:
            os.replace(temporary, path)
            descriptor = os.open(home, os.O_RDONLY)
            try:
                os.fsync(descriptor)
            finally:
                os.close(descriptor)
    finally:
        temporary.unlink(missing_ok=True)


class FactoryResetService:
    def __init__(self, home: Path, *, repo=None, credentials=None, busy=None, close_clients=None):
        self.home = checked_owned_path(Path(home).absolute(), Path(home).absolute())
        self.repo = repo
        self.credentials = credentials or WindowsCredentialAdapter()
        self.supported = bool(
            self.credentials.supported and (credentials is not None or self.home == normal_home())
        )
        self.busy = busy or (lambda: [])
        self.close_clients = close_clients
        self._lock = threading.RLock()
        self._requests = 0
        self._mutation_revision = 0
        self._cleanup = asyncio.Lock()
        self._journal = load_journal(self.home)
        self._nonce = uuid4().hex

    @property
    def pending(self):
        return self._journal is not None

    def status(self) -> ResetStatus | None:
        with self._lock:
            if not self._journal:
                return None
            # Native cleanup can advance the journal after the API receipt.
            self._journal = load_journal(self.home) or self._journal
            value = self._journal
            return ResetStatus(
                reset_id=value["reset_id"],
                state=value["phase"],
                detail=value["detail"],
                credentials_cleared=value["credentials_cleared"],
                fingerprint=value["fingerprint"],
            )

    def enter_request(self, method: str, path: str) -> tuple[str, str] | None:
        allowed = (method, path) in {
            ("GET", "/api/health"),
            ("GET", "/healthz"),
            ("GET", "/api/reset/preview"),
            ("GET", "/api/reset/status"),
            ("POST", "/api/reset"),
            ("GET", "/api/diagnostics"),
            ("POST", "/api/shutdown"),
        }
        with self._lock:
            if allowed or method == "OPTIONS":
                # Normal health reads account readiness. Count an already
                # admitted read; pending health uses only stable reset metadata.
                if (method, path) == ("GET", "/api/health") and not self.pending:
                    self._requests += 1
                    return method, path
                return None
            if self.pending:
                raise ValueError(PENDING_DETAIL)
            self._requests += 1
            return method, path

    def leave_request(self, tracked: tuple[str, str] | None):
        if tracked:
            with self._lock:
                self._requests -= 1
                method, path = tracked
                if method not in {"GET", "HEAD", "OPTIONS"} and path != "/api/diagnostics/events":
                    self._mutation_revision += 1

    def _metadata(self):
        counts = {"tender_count": 0, "artifact_count": 0, "account_count": 0}
        account_ids = []
        identities = []
        active = False
        database = checked_owned_path(self.home, self.home / "quantix.sqlite")
        if database.exists():
            with closing(sqlite3.connect(database.as_uri() + "?mode=ro", uri=True)) as conn:
                tables = {
                    row[0]
                    for row in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")
                }
                for table, name in (
                    ("tenders", "tender_count"),
                    ("artifacts", "artifact_count"),
                    ("ai_connections", "account_count"),
                ):
                    if table in tables:
                        counts[name] = conn.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
                        identities.append(
                            (
                                table,
                                [
                                    row[0]
                                    for row in conn.execute(f"SELECT id FROM {table} ORDER BY id")
                                ],
                            )
                        )
                if "ai_connections" in tables:
                    account_ids = [row[0] for row in conn.execute("SELECT id FROM ai_connections")]
                if "runs" in tables:
                    active = (
                        conn.execute(
                            "SELECT 1 FROM runs WHERE status IN ('queued','running') LIMIT 1"
                        ).fetchone()
                        is not None
                    )
        backups = checked_owned_path(self.home, self.home / "backups")
        files = []
        if backups.is_dir():
            for path in backups.iterdir():
                if path.suffix.lower() == ".zip":
                    checked_owned_path(self.home, path)
                    if path.is_file():
                        info = path.stat()
                        files.append((path.name, info.st_size, info.st_mtime_ns))
        counts["backup_count"] = len(files)
        return counts, account_ids, active, [sorted(files), identities]

    def _preview(self):
        counts, _, active, versions = self._metadata()
        blockers = list(self.busy())
        if active:
            blockers.append("Finish or stop the current Tender work before resetting Quantix.")
        if self._requests:
            blockers.append(
                "Another workspace request is still finishing. Wait for it and try again."
            )
        if not self.supported:
            blockers.append(
                "Reset is available in the Windows desktop app using the normal Quantix home."
            )
        fingerprint = hashlib.sha256(
            json.dumps(
                [str(self.home), counts, versions, self._nonce, self._mutation_revision],
                sort_keys=True,
            ).encode()
        ).hexdigest()
        if self._journal:
            fingerprint = self._journal["fingerprint"]
        return ResetPreview(
            supported=self.supported,
            home=str(self.home),
            **counts,
            blockers=list(dict.fromkeys(blockers)),
            fingerprint=fingerprint,
        )

    def preview(self) -> ResetPreview:
        with self._lock:
            return self._preview()

    async def confirm(self, command: ResetRequest) -> ResetReceipt:
        # The lock protects the no-await admission decision against sync HTTP
        # handlers as well as concurrent confirmations on the API event loop.
        with self._lock:
            if not self.supported:
                raise ValueError(
                    "Reset is unavailable here. Open the Windows desktop app using the normal Quantix home."
                )
            if self._journal:
                if command.fingerprint != self._journal["fingerprint"]:
                    raise ValueError(
                        "Use the original reset preview to retry this confirmed reset."
                    )
                if self._journal["credentials_cleared"]:
                    return self._receipt()
            else:
                preview = self._preview()
                if preview.blockers:
                    raise ValueError(" ".join(preview.blockers))
                if command.fingerprint != preview.fingerprint:
                    raise ValueError(
                        "The workspace changed after this preview. Review the reset details again."
                    )
                journal = {
                    "format": 1,
                    "reset_id": uuid4().hex,
                    "home": str(self.home),
                    "phase": "cleaning_credentials",
                    "credentials_cleared": False,
                    "fingerprint": command.fingerprint,
                    "confirmed_at": datetime.now(UTC).isoformat(),
                    "detail": CLEANING_DETAIL,
                    "credential_targets": None,
                }
                write_journal(self.home, journal)
                self._journal = journal
        async with self._cleanup:
            if self._journal["credentials_cleared"]:
                return self._receipt()
            try:
                self._save(phase="cleaning_credentials", detail=CLEANING_DETAIL)
                if self.close_clients:
                    await self.close_clients()
                _, account_ids, _, _ = self._metadata()
                if self._journal.get("credential_targets") is None:
                    targets = await asyncio.to_thread(
                        self.credentials.inventory, self.home, account_ids
                    )
                    # This durable inventory must precede the FIRST deletion.
                    self._save(credential_targets=targets)
                await asyncio.to_thread(
                    self.credentials.validate_targets,
                    self.home,
                    account_ids,
                    self._journal["credential_targets"],
                )
                for target in self._journal["credential_targets"]:
                    await asyncio.to_thread(self.credentials.delete, target)
                self._clear_session_credentials()
                self._save(phase="ready", credentials_cleared=True, detail=READY_DETAIL)
            except Exception:
                self._save(
                    phase="credential_error",
                    credentials_cleared=False,
                    detail=CREDENTIAL_ERROR_DETAIL,
                )
            return self._receipt()

    def _save(self, **changes):
        with self._lock:
            value = self._journal | changes
            write_journal(self.home, value)
            self._journal = value

    def _receipt(self):
        return ResetReceipt(
            reset_id=self._journal["reset_id"],
            state=self._journal["phase"],
            detail=self._journal["detail"],
        )

    def _clear_session_credentials(self):
        from .ai_connections import AIConnectionService

        with AIConnectionService._states_lock:
            state = AIConnectionService._states.get(str(self.home))
            if state:
                with state.lock:
                    state.sessions.clear()
                    if state.leases or state.exclusive:
                        raise ValueError("An AI account is still in use.")
                AIConnectionService._states.pop(str(self.home), None)
