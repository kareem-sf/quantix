"""Cancellation must stop owned Windows subprocess descendants only."""

import importlib
import importlib.util
import subprocess
import sys
import time

import pytest

pytestmark = pytest.mark.skipif(sys.platform != "win32", reason="Windows process ownership")


def running(pid):
    import pywintypes
    import win32api
    import win32process

    try:
        handle = win32api.OpenProcess(0x1000, False, pid)
    except (OSError, pywintypes.error):
        return False
    try:
        return win32process.GetExitCodeProcess(handle) == 259
    finally:
        handle.Close()


def test_stopping_owned_tree_leaves_unrelated_process_alive(tmp_path):
    assert importlib.util.find_spec("quantix.processes"), "Owned process-tree cleanup is missing"
    stop = importlib.import_module("quantix.processes").stop_owned_process_tree
    child_file = tmp_path / "child.pid"
    script = "import subprocess,sys,time,pathlib; child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); pathlib.Path(sys.argv[1]).write_text(str(child.pid)); time.sleep(30)"
    owned = subprocess.Popen(
        [sys.executable, "-c", script, str(child_file)], creationflags=subprocess.CREATE_NO_WINDOW
    )
    unrelated = subprocess.Popen(
        [sys.executable, "-c", "import time; time.sleep(30)"],
        creationflags=subprocess.CREATE_NO_WINDOW,
    )
    try:
        deadline = time.monotonic() + 5
        while not child_file.exists() and time.monotonic() < deadline:
            time.sleep(0.02)
        assert child_file.exists()
        child_pid = int(child_file.read_text())
        assert running(child_pid)
        stop(owned)
        assert owned.poll() is not None and not running(child_pid)
        assert unrelated.poll() is None
        stop(owned)  # repeated cleanup must be harmless
    finally:
        stop(owned)
        stop(unrelated)
