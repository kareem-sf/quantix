"""Small JSON files under the data home (auth.json, settings.json)."""

import json
import os
import threading
from collections.abc import Callable
from pathlib import Path
from typing import Any

_lock = threading.Lock()


def read(path: Path, default: dict[str, Any]) -> dict[str, Any]:
    if not path.exists():
        return default
    return json.loads(path.read_text(encoding="utf-8"))


def update(path: Path, default: dict[str, Any], change: Callable[[dict[str, Any]], None]) -> dict[str, Any]:
    """Read, change and write back in one step, so two requests can't overwrite each other."""
    with _lock:
        data = read(path, default)
        change(data)
        path.parent.mkdir(parents=True, exist_ok=True)
        temporary = path.with_suffix(".tmp")
        temporary.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        os.replace(temporary, path)
        return data
