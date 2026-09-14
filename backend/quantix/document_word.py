"""Optional, isolated Microsoft Word conversion for legacy binary documents.

Office is never started by unit tests. The subprocess opens a temporary copy,
and the original path is never handed to COM. No Excel automation is used.
"""

from __future__ import annotations

import importlib.util
import json
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from collections.abc import Callable
from pathlib import Path

from .processes import stop_owned_process_tree
from .storage import current_home, runtime_tmp_dir

WORD_TIMEOUT_SECONDS = 60
MAX_WORD_BYTES = 32 * 1024 * 1024
WORD_LOCK = threading.Lock()


class WordConversionUnavailable(Exception):
    """The document remains registered with an explicit conversion exception."""


def _terminate_owned_word(state: Path) -> None:
    """Only terminate the newly created Word process with its recorded identity."""
    if sys.platform != "win32" or not state.is_file():
        return
    try:
        import win32api
        import win32process

        details = json.loads(state.read_text(encoding="utf-8"))
        handle = win32api.OpenProcess(0x0410 | 0x0001, False, int(details["pid"]))
        try:
            created = win32process.GetProcessTimes(handle)["CreationTime"].isoformat()
            executable = win32process.GetModuleFileNameEx(handle, 0)
            if created == details["created"] and Path(executable).name.lower() == "winword.exe":
                win32api.TerminateProcess(handle, 1)
        finally:
            handle.Close()
    except (OSError, ValueError, KeyError, ImportError):
        pass
    except Exception:
        # A process may have exited between the identity check and cleanup.
        pass


def convert_legacy_word(path: Path, cancelled: Callable[[], bool] | None = None) -> bytes:
    """Return DOCX bytes without exposing the source file to Word itself."""
    while not WORD_LOCK.acquire(timeout=0.1):
        if cancelled and cancelled():
            raise InterruptedError("Word conversion cancelled")
    try:
        return _convert_legacy_word(path, cancelled)
    finally:
        WORD_LOCK.release()


def _convert_legacy_word(path: Path, cancelled: Callable[[], bool] | None) -> bytes:
    if cancelled and cancelled():
        raise InterruptedError("Word conversion cancelled")
    if sys.platform != "win32" or importlib.util.find_spec("win32com") is None:
        raise WordConversionUnavailable(
            "Microsoft Word automation is unavailable on this computer."
        )
    path = Path(path)
    if path.stat().st_size > MAX_WORD_BYTES:
        raise WordConversionUnavailable("This legacy Word file exceeds the conversion size limit.")
    temp_root = runtime_tmp_dir(current_home())
    temp_root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="quantix-word-", dir=temp_root) as folder:
        temp = Path(folder)
        source, target, state = temp / "source.doc", temp / "converted.docx", temp / "worker.json"
        shutil.copyfile(path, source)
        command = [
            sys.executable,
            *(
                ["word-convert", "--home", str(current_home())]
                if getattr(sys, "frozen", False)
                else ["-m", "quantix.document_word"]
            ),
            str(source),
            str(target),
            str(state),
        ]
        process = subprocess.Popen(
            command,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
        )
        started = time.monotonic()
        try:
            while process.poll() is None:
                if cancelled and cancelled():
                    raise InterruptedError("Word conversion cancelled")
                if time.monotonic() - started >= WORD_TIMEOUT_SECONDS:
                    raise WordConversionUnavailable(
                        "Microsoft Word conversion timed out; review this document manually."
                    )
                try:
                    process.wait(timeout=0.1)
                except subprocess.TimeoutExpired:
                    pass
            if process.returncode != 0 or not target.is_file():
                raise WordConversionUnavailable(
                    "Microsoft Word could not convert this document without interaction."
                )
            if target.stat().st_size > MAX_WORD_BYTES:
                raise WordConversionUnavailable(
                    "The converted Word document exceeds the extraction size limit."
                )
            return target.read_bytes()
        finally:
            try:
                stop_owned_process_tree(process)
            finally:
                _terminate_owned_word(state)


def _save_document(app, source: Path, target: Path) -> None:
    security, alerts, links = (
        app.AutomationSecurity,
        app.DisplayAlerts,
        app.Options.UpdateLinksAtOpen,
    )
    document = None
    try:
        app.Visible = False
        app.DisplayAlerts = 0
        app.AutomationSecurity = 3  # msoAutomationSecurityForceDisable
        app.Options.UpdateLinksAtOpen = False
        document = app.Documents.Open(
            FileName=str(source),
            ConfirmConversions=False,
            ReadOnly=True,
            AddToRecentFiles=False,
            Visible=False,
            OpenAndRepair=False,
            NoEncodingDialog=True,
        )
        document.SaveAs2(FileName=str(target), FileFormat=16, AddToRecentFiles=False)
    finally:
        try:
            if document is not None:
                document.Close(SaveChanges=0)
        finally:
            try:
                app.Options.UpdateLinksAtOpen = links
            finally:
                try:
                    app.AutomationSecurity = security
                finally:
                    app.DisplayAlerts = alerts


def _run(source: Path, target: Path, state: Path) -> int:
    import pythoncom
    import pywintypes
    import win32api
    import win32com.client
    import win32process

    pythoncom.CoInitialize()
    app = None
    owns_app = False
    try:
        existing = set(win32process.EnumProcesses())
        app = win32com.client.DispatchEx("Word.Application")
        candidates = []
        for pid in set(win32process.EnumProcesses()) - existing:
            handle = None
            try:
                handle = win32api.OpenProcess(0x0410, False, pid)
                if Path(win32process.GetModuleFileNameEx(handle, 0)).name.lower() == "winword.exe":
                    candidates.append(
                        (pid, win32process.GetProcessTimes(handle)["CreationTime"].isoformat())
                    )
            except (OSError, pywintypes.error):
                continue
            finally:
                if handle is not None:
                    handle.Close()
        if len(candidates) != 1:
            # Never control or terminate an instance with ambiguous ownership.
            return 2
        owns_app = True
        pid, created = candidates[0]
        state.write_text(json.dumps({"pid": pid, "created": created}), encoding="utf-8")
        _save_document(app, source, target)
        return 0
    except Exception:
        return 1
    finally:
        if app is not None and owns_app:
            try:
                app.Quit(SaveChanges=0)
            except Exception:
                pass
        pythoncom.CoUninitialize()


if __name__ == "__main__":
    sys.exit(_run(*(Path(value) for value in sys.argv[1:4])))
