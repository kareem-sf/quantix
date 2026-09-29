"""Office settings, kept in ~/.quantix/settings.json."""

from pathlib import Path
from typing import Any

from quantix.core import jsonfile

DEFAULTS: dict[str, Any] = {
    # "engineer": the office stops at every gate. "autonomous": it approves its own gates, marked for review.
    "office_mode": "engineer",
    # The connection and model the office works with: {"connection_id": ..., "model": ...}.
    "office_ai": None,
    # The most AI tokens one tender's office may use; it pauses there. None for no limit.
    "tender_allowance": None,
    # Windows notifications while Quantix isn't in front. "all": new decisions and a team that finished;
    # "decisions": new decisions only; "off": none.
    "notifications": "all",
}


def _path(home: Path) -> Path:
    return home / "settings.json"


def load(home: Path) -> dict[str, Any]:
    return {**DEFAULTS, **jsonfile.read(_path(home), {})}


def save(home: Path, **values: Any) -> dict[str, Any]:
    jsonfile.update(_path(home), {}, lambda data: data.update(values))
    return load(home)
