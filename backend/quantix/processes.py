"""Bounded cleanup for subprocess trees created and retained by this service."""

import os
import subprocess
import sys
from pathlib import Path


def stop_owned_process_tree(process: subprocess.Popen, timeout: float = 3) -> None:
    """Stop an owned Popen tree, retaining its process handle during cleanup.

    Pass only a Popen object created by the caller, never a process found by
    name. Windows venv launchers can have descendants that terminate() misses.
    """
    if process.poll() is not None:
        return
    if sys.platform == "win32":
        taskkill = Path(os.environ["SystemRoot"]) / "System32" / "taskkill.exe"
        subprocess.run(
            [str(taskkill), "/PID", str(process.pid), "/T", "/F"],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            creationflags=subprocess.CREATE_NO_WINDOW,
            timeout=timeout,
            check=False,
        )
    else:
        # Do not signal a group that this service did not explicitly create.
        process.terminate()
    try:
        process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=timeout)
