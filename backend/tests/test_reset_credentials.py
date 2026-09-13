"""Exact credential ownership and Win32 behavior, without accessing the keyring."""

import ctypes
import hashlib
import importlib
from pathlib import Path

import pytest


def module():
    try:
        return importlib.import_module("quantix.reset_credentials")
    except ModuleNotFoundError:
        pytest.fail("The metadata-only reset credential adapter is missing")


class Native:
    def __init__(self, entries):
        self.entries = entries
        self.paths = []
        self.remaining = {entry["target"] for entry in entries}

    def enumerate(self):
        return self.entries

    def canonicalize(self, path):
        self.paths.append(path)
        return "\\\\?\\" + str(path)

    def delete(self, target):
        self.remaining.discard(target["target"])


def test_exact_home_services_and_orphaned_codex_profiles_leave_independent_credentials(tmp_path):
    m = module()
    home = tmp_path / "home"
    (home / "ai-runtimes" / "orphan" / "codex").mkdir(parents=True)
    identity = hashlib.sha256(str(home).encode()).hexdigest()[:16]
    ai, mail = f"Quantix-{identity}", f"Quantix-mail-{identity}"
    codex_hash = hashlib.sha256(("\\\\?\\" + str(home / "ai-runtimes" / "orphan" / "codex")).encode()).hexdigest()[:16]
    owned = {ai, f"orphan-ai@{ai}", f"openai@{ai}", mail, f"smtp@{mail}", f"imap@{mail}",
             f"cli|{codex_hash}.Codex Auth", f"secrets|{codex_hash}.codex"}
    foreign = {"Quantix-other-home", "imap@Quantix-mail-other", "cli|global.Codex Auth",
               "secrets|global.codex", ai + "-not-owned", "prefix" + ai}
    native = Native([{"type": 1, "target": target} for target in owned | foreign] + [{"type": 2, "target": ai}])
    adapter = m.WindowsCredentialAdapter(native=native)
    targets = adapter.inventory(home, ["saved_without_runtime"])
    assert {entry["target"] for entry in targets} == owned
    assert all(entry["type"] == 1 for entry in targets)
    for target in targets:
        adapter.delete(target)
    assert native.remaining == foreign
    assert native.paths == [home / "ai-runtimes" / "orphan" / "codex"]


def test_inventory_rejects_external_runtime_junction_before_deriving_keyring_identity(tmp_path, monkeypatch):
    m = module()
    home = tmp_path / "home"
    candidate = home / "ai-runtimes" / "linked"
    (candidate / "codex").mkdir(parents=True)
    original = Path.is_junction
    monkeypatch.setattr(Path, "is_junction", lambda path: path == candidate or original(path))
    native = Native([])
    with pytest.raises(ValueError, match="link|junction"):
        m.WindowsCredentialAdapter(native=native).inventory(home, [])
    assert native.paths == []


def test_invalid_database_account_id_cannot_escape_owned_home(tmp_path):
    m = module()
    tmp_path.joinpath("home").mkdir()
    with pytest.raises(ValueError, match="account"):
        m.WindowsCredentialAdapter(native=Native([])).inventory(tmp_path / "home", ["../../global"])


def test_saved_credential_inventory_is_revalidated_against_exact_home_ownership(tmp_path):
    m = module()
    home = tmp_path / "home"
    home.mkdir()
    adapter = m.WindowsCredentialAdapter(native=Native([]))
    with pytest.raises(ValueError, match="belong"):
        adapter.validate_targets(home, [], [{"type": 1, "target": "cli|global.Codex Auth"}])


def test_non_windows_adapter_is_unavailable_without_loading_os_apis(monkeypatch):
    m = module()
    monkeypatch.setattr(m.sys, "platform", "linux")
    adapter = m.WindowsCredentialAdapter()
    assert not adapter.supported
    with pytest.raises(ValueError, match="Windows"):
        adapter.inventory(Path("/synthetic"), [])


@pytest.mark.parametrize("error,missing", [(1168, True), (5, False), (1312, False)])
def test_win32_deletion_only_treats_not_found_as_success(error, missing):
    m = module()

    class API:
        def CredDeleteW(self, target, kind, flags):
            assert (target, kind, flags) == ("exact-target", 1, 0)
            ctypes.set_last_error(error)
            return 0

    native = m.NativeWindowsCredentials(advapi=API())
    if missing:
        native.delete({"type": 1, "target": "exact-target"})
    else:
        with pytest.raises(OSError):
            native.delete({"type": 1, "target": "exact-target"})


def test_win32_enumeration_copies_only_metadata_and_always_frees_buffer(monkeypatch):
    m = module()
    credential = m._Credential()
    credential.Type = 1
    credential.TargetName = "synthetic-target"
    credential.CredentialBlobSize = 6
    secret = ctypes.create_string_buffer(b"secret")
    credential.CredentialBlob = ctypes.cast(secret, ctypes.POINTER(ctypes.c_ubyte))
    pointers = (ctypes.POINTER(m._Credential) * 1)(ctypes.pointer(credential))
    freed = []

    class API:
        def CredEnumerateW(self, prefix, flags, count, output):
            assert prefix is None and flags == 0
            ctypes.cast(count, ctypes.POINTER(ctypes.c_ulong))[0] = 1
            ctypes.cast(output, ctypes.POINTER(ctypes.POINTER(ctypes.POINTER(m._Credential))))[0] = pointers
            return 1

        def CredFree(self, value):
            freed.append(bool(value))

    monkeypatch.setattr(ctypes, "string_at", lambda *_: pytest.fail("Credential blobs must not be decoded"))
    result = m.NativeWindowsCredentials(advapi=API()).enumerate()
    assert result == [{"type": 1, "target": "synthetic-target"}]
    assert freed == [True]


def test_windows_canonicalization_keeps_native_prefix_and_case_and_closes_handle():
    m = module()
    canonical = "\\\\?\\C:\\Users\\Synthetic\\.quantix\\ai-runtimes\\a\\codex"
    closed = []

    class Kernel:
        def CreateFileW(self, path, access, share, security, disposition, flags, template):
            assert access == 0x80 and share == 7 and disposition == 3
            assert flags == 0x02200000
            return 42

        def GetFinalPathNameByHandleW(self, handle, buffer, capacity, flags):
            assert handle == 42 and flags == 0
            if buffer is None:
                return len(canonical) + 1
            assert capacity > len(canonical)
            buffer.value = canonical
            return len(canonical)

        def CloseHandle(self, handle):
            closed.append(handle)

    native = m.NativeWindowsCredentials(advapi=object(), kernel32=Kernel())
    assert native.canonicalize(Path("C:/synthetic/input")) == canonical
    assert closed == [42]
