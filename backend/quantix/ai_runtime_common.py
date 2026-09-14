"""SDK-free connection paths and sanitized process-boundary errors."""

import os
import re
from pathlib import Path


class RuntimeUnavailable(ValueError):
    """The chosen AI component cannot perform this operation as configured."""


class RuntimeConnectionFailure(ConnectionError):
    """Sanitized provider transport failure; never switches a route itself."""


def runtime_home(repo, connection: dict) -> Path:
    connection_id = connection["id"]
    if not re.fullmatch(r"[A-Za-z0-9_-]{1,100}", connection_id):
        raise ValueError("Invalid AI connection identifier.")
    return repo.home / "ai-runtimes" / connection_id


def child_environment(home: Path) -> dict[str, str]:
    """Also neutralise inherited keys in SDKs that merge env with os.environ."""
    allowed = {
        "SYSTEMROOT",
        "WINDIR",
        "COMSPEC",
        "PATH",
        "PATHEXT",
        "SYSTEMDRIVE",
        "PROGRAMDATA",
        "NUMBER_OF_PROCESSORS",
        "PROCESSOR_ARCHITECTURE",
        "LANG",
        "LC_ALL",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "REQUESTS_CA_BUNDLE",
    }
    env = {key: value if key.upper() in allowed else "" for key, value in os.environ.items()}
    for directory in (home, home / "tmp", home / "config", home / "data", home / "cache"):
        directory.mkdir(parents=True, exist_ok=True)
    env.update(
        {
            "HOME": str(home),
            "USERPROFILE": str(home),
            "APPDATA": str(home / "config"),
            "LOCALAPPDATA": str(home / "data"),
            "XDG_CONFIG_HOME": str(home / "config"),
            "XDG_DATA_HOME": str(home / "data"),
            "XDG_CACHE_HOME": str(home / "cache"),
            "TEMP": str(home / "tmp"),
            "TMP": str(home / "tmp"),
            "CODEX_HOME": str(home / "codex"),
            "DISABLE_TELEMETRY": "1",
            "DO_NOT_TRACK": "1",
            "OTEL_SDK_DISABLED": "true",
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": os.devnull,
        }
    )
    return env
