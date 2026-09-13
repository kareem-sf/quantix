"""Locate prepared software only; workers never install dependencies."""

import os
from pathlib import Path

from .common import RuntimeUnavailable, explicit_executable


def component_root():
    value = os.environ.get("QUANTIX_AI_COMPONENT_ROOT")
    if not value or not Path(value).is_absolute() or not Path(value).is_dir():
        raise RuntimeUnavailable("The prepared AI component directory is unavailable. Prepare this connection again.")
    return Path(value)


def node_executable(home, connection):
    explicit = explicit_executable(connection, "node_path")
    if explicit:
        return explicit
    path = component_root() / "node" / ("node.exe" if os.name == "nt" else "bin/node")
    return path if path.is_file() else None
