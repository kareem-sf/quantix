"""Locate prepared software only; workers never install dependencies."""

import os
from pathlib import Path

from .common import RuntimeUnavailable


def component_root():
    value = os.environ.get("QUANTIX_AI_COMPONENT_ROOT")
    if not value or not Path(value).is_absolute() or not Path(value).is_dir():
        raise RuntimeUnavailable(
            "The prepared AI component directory is unavailable. Prepare this connection again."
        )
    return Path(value)
