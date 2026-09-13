"""Identify completed restores without opening a second database or taking backup locks."""

import hashlib
import re


def restore_basis(home):
    # Completed restore journals are published atomically under this directory
    # while the workspace service is stopped, and are not backup payloads.
    history = home / "restore-history"
    try:
        names = sorted(path.name for path in history.iterdir()
                       if re.fullmatch(r"[a-f0-9]{32}\.json", path.name))
    except FileNotFoundError:
        return None
    except OSError as error:
        raise ValueError("The workspace restore record could not be read. Repair its access before approving paid AI work.") from error
    return hashlib.sha256("\n".join(names).encode()).hexdigest() if names else None
