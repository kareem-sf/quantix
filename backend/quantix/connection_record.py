"""Publish the desktop connection as one complete, private record."""

import json
import os
import tempfile
import time
from pathlib import Path


def write_connection_record(path: Path, value: dict[str, str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w",
            encoding="utf-8",
            dir=path.parent,
            prefix=f".{path.name}.",
            suffix=".tmp",
            delete=False,
        ) as handle:
            temporary = Path(handle.name)
            os.chmod(temporary, 0o600)
            json.dump(value, handle)
            handle.flush()
            os.fsync(handle.fileno())
        # Readers see either the previous complete session or the next one.
        for attempt in range(5):
            try:
                os.replace(temporary, path)
                break
            except PermissionError:
                # Windows readers/scanners can briefly hold a non-delete-sharing
                # handle. Keep the old complete record while that handle closes.
                if attempt == 4:
                    raise
                time.sleep(0.02 * (attempt + 1))
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
