"""AI connections and their keys, kept in ~/.quantix/auth.json."""

import uuid
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from quantix.core import jsonfile


def _path(home: Path) -> Path:
    return home / "auth.json"


def _empty() -> dict[str, Any]:
    return {"connections": []}


def all_connections(home: Path) -> list[dict[str, Any]]:
    return jsonfile.read(_path(home), _empty())["connections"]


def get(home: Path, connection_id: str) -> dict[str, Any] | None:
    return next((c for c in all_connections(home) if c["id"] == connection_id), None)


def add(home: Path, provider: str, label: str, api_key: str, base_url: str | None) -> dict[str, Any]:
    connection = {
        "id": uuid.uuid4().hex,
        "provider": provider,
        "label": label,
        "base_url": base_url,
        "api_key": api_key,
        "checks": {},
    }
    jsonfile.update(_path(home), _empty(), lambda data: data["connections"].append(connection))
    return connection


def remove(home: Path, connection_id: str) -> None:
    def change(data: dict[str, Any]) -> None:
        data["connections"] = [c for c in data["connections"] if c["id"] != connection_id]

    jsonfile.update(_path(home), _empty(), change)


def record_check(home: Path, connection_id: str, model: str, ok: bool, message: str) -> None:
    result = {"ok": ok, "message": message, "checked_at": datetime.now(UTC).isoformat()}

    def change(data: dict[str, Any]) -> None:
        for connection in data["connections"]:
            if connection["id"] == connection_id:
                connection["checks"][model] = result

    jsonfile.update(_path(home), _empty(), change)
