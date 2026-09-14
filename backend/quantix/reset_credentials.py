"""Metadata-only Windows credential cleanup for the exact Quantix workspace.

Ownership and naming follow the pinned source audit in
.superpowers/sdd/factory-reset/credentials-audit.md. Never use the Python
keyring enumeration wrapper: it copies credential blobs into Python values.
"""

from __future__ import annotations

import ctypes
import hashlib
import re
import sys
from ctypes import wintypes
from pathlib import Path


def checked_owned_path(home: Path, path: Path) -> Path:
    root, candidate = Path(home).absolute(), Path(path).absolute()
    if not candidate.is_relative_to(root):
        raise ValueError("A reset path leaves the Quantix home.")
    # Inspect lexical ancestors before resolving: resolve() would conceal an
    # outside junction, including one at the home itself.
    for part in [candidate, *candidate.parents]:
        if part.is_symlink() or part.is_junction():
            raise ValueError("Reset paths cannot use links or junctions.")
    if not candidate.resolve().is_relative_to(root.resolve()):
        raise ValueError("A reset path leaves the Quantix home.")
    return candidate


class _Credential(ctypes.Structure):
    _fields_ = [
        ("Flags", wintypes.DWORD),
        ("Type", wintypes.DWORD),
        ("TargetName", wintypes.LPWSTR),
        ("Comment", wintypes.LPWSTR),
        ("LastWritten", wintypes.FILETIME),
        ("CredentialBlobSize", wintypes.DWORD),
        ("CredentialBlob", ctypes.POINTER(ctypes.c_ubyte)),
        ("Persist", wintypes.DWORD),
        ("AttributeCount", wintypes.DWORD),
        ("Attributes", ctypes.c_void_p),
        ("TargetAlias", wintypes.LPWSTR),
        ("UserName", wintypes.LPWSTR),
    ]


class NativeWindowsCredentials:
    def __init__(self, *, advapi=None, kernel32=None):
        if advapi is None:
            advapi = ctypes.WinDLL("Advapi32.dll", use_last_error=True)
            advapi.CredEnumerateW.argtypes = [
                wintypes.LPCWSTR,
                wintypes.DWORD,
                ctypes.POINTER(wintypes.DWORD),
                ctypes.POINTER(ctypes.POINTER(ctypes.POINTER(_Credential))),
            ]
            advapi.CredEnumerateW.restype = wintypes.BOOL
            advapi.CredDeleteW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD]
            advapi.CredDeleteW.restype = wintypes.BOOL
            advapi.CredFree.argtypes = [ctypes.c_void_p]
            advapi.CredFree.restype = None
        self.advapi = advapi
        self.kernel32 = kernel32

    def enumerate(self) -> list[dict]:
        count = wintypes.DWORD()
        buffer = ctypes.POINTER(ctypes.POINTER(_Credential))()
        if not self.advapi.CredEnumerateW(None, 0, ctypes.byref(count), ctypes.byref(buffer)):
            error = ctypes.get_last_error()
            if error == 1168:
                return []
            raise ctypes.WinError(error)
        try:
            # CredentialBlob and every other value-bearing field remain untouched.
            return [
                {
                    "type": int(buffer[index].contents.Type),
                    "target": buffer[index].contents.TargetName,
                }
                for index in range(count.value)
            ]
        finally:
            self.advapi.CredFree(buffer)

    def delete(self, target: dict) -> None:
        if target["type"] != 1:
            raise ValueError("Only owned generic credentials can be removed.")
        if not self.advapi.CredDeleteW(target["target"], 1, 0):
            error = ctypes.get_last_error()
            if error != 1168:
                raise ctypes.WinError(error)

    def canonicalize(self, path: Path) -> str:
        if self.kernel32 is None:
            kernel = ctypes.WinDLL("Kernel32.dll", use_last_error=True)
            kernel.CreateFileW.argtypes = [
                wintypes.LPCWSTR,
                wintypes.DWORD,
                wintypes.DWORD,
                ctypes.c_void_p,
                wintypes.DWORD,
                wintypes.DWORD,
                wintypes.HANDLE,
            ]
            kernel.CreateFileW.restype = wintypes.HANDLE
            kernel.GetFinalPathNameByHandleW.argtypes = [
                wintypes.HANDLE,
                wintypes.LPWSTR,
                wintypes.DWORD,
                wintypes.DWORD,
            ]
            kernel.GetFinalPathNameByHandleW.restype = wintypes.DWORD
            kernel.CloseHandle.argtypes = [wintypes.HANDLE]
            kernel.CloseHandle.restype = wintypes.BOOL
            self.kernel32 = kernel
        kernel = self.kernel32
        # Share read/write/delete; inspect the directory itself without opening
        # auth files. BACKUP_SEMANTICS permits a directory handle.
        handle = kernel.CreateFileW(str(path), 0x80, 7, None, 3, 0x02200000, None)
        if handle == ctypes.c_void_p(-1).value:
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            length = kernel.GetFinalPathNameByHandleW(handle, None, 0, 0)
            if not length:
                raise ctypes.WinError(ctypes.get_last_error())
            buffer = ctypes.create_unicode_buffer(length + 1)
            written = kernel.GetFinalPathNameByHandleW(handle, buffer, len(buffer), 0)
            if not written or written >= len(buffer):
                raise ctypes.WinError(ctypes.get_last_error())
            # Rust std::fs::canonicalize uses this extended-length representation.
            # Do not strip the prefix or change its case before hashing.
            return buffer.value
        finally:
            kernel.CloseHandle(handle)


class WindowsCredentialAdapter:
    def __init__(self, *, native=None):
        self.supported = sys.platform == "win32"
        self._native = native

    @property
    def native(self):
        if not self.supported:
            raise ValueError("Credential cleanup is available only on Windows.")
        if self._native is None:
            self._native = NativeWindowsCredentials()
        return self._native

    def _ownership(self, home: Path, account_ids: list[str]):
        if not self.supported:
            raise ValueError("Credential cleanup is available only on Windows.")
        root = checked_owned_path(home, home)
        identity = hashlib.sha256(str(root).encode()).hexdigest()[:16]
        services = (f"Quantix-{identity}", f"Quantix-mail-{identity}")
        ids = set(account_ids)
        if any(
            not isinstance(value, str) or not re.fullmatch(r"[A-Za-z0-9_-]{1,100}", value)
            for value in ids
        ):
            raise ValueError("A saved AI account has an unsafe account identifier.")
        runtimes = checked_owned_path(root, root / "ai-runtimes")
        if runtimes.exists():
            for child in runtimes.iterdir():
                checked_owned_path(root, child)
                if child.is_dir() and re.fullmatch(r"[A-Za-z0-9_-]{1,100}", child.name):
                    ids.add(child.name)
        targets = set()
        for identifier in sorted(ids):
            codex = checked_owned_path(root, runtimes / identifier / "codex")
            if not codex.exists():
                continue
            if not codex.is_dir():
                raise ValueError("A Quantix Codex profile is not a directory.")
            canonical = self.native.canonicalize(codex)
            # Resolve only for containment validation; hash the native string.
            comparable = canonical
            if comparable.startswith("\\\\?\\UNC\\"):
                comparable = "\\\\" + comparable[8:]
            elif comparable.startswith("\\\\?\\"):
                comparable = comparable[4:]
            if not Path(comparable).resolve().is_relative_to(root.resolve()):
                raise ValueError("A Codex credential path leaves the Quantix home.")
            digest = hashlib.sha256(canonical.encode("utf-8")).hexdigest()[:16]
            targets.update((f"cli|{digest}.Codex Auth", f"secrets|{digest}.codex"))
        return services, targets

    @staticmethod
    def _owned(target, services, codex_targets):
        return target in codex_targets or any(
            target == service or target.endswith("@" + service) for service in services
        )

    def inventory(self, home: Path, account_ids: list[str]) -> list[dict]:
        services, codex_targets = self._ownership(home, account_ids)
        return sorted(
            (
                entry
                for entry in self.native.enumerate()
                if entry["type"] == 1
                and isinstance(entry["target"], str)
                and self._owned(entry["target"], services, codex_targets)
            ),
            key=lambda entry: entry["target"],
        )

    def validate_targets(self, home: Path, account_ids: list[str], targets: list[dict]):
        services, codex_targets = self._ownership(home, account_ids)
        if any(
            target["type"] != 1 or not self._owned(target["target"], services, codex_targets)
            for target in targets
        ):
            raise ValueError("A saved reset credential does not belong to this Quantix home.")

    def delete(self, target: dict):
        self.native.delete(target)
