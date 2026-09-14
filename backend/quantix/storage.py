"""Paths owned by a Quantix application workspace.

The normal workspace is deliberately independent of platform specific data
directory conventions.  An explicit root is useful for isolated developer
workspaces and tests; it is never inferred from the legacy ``QUANTIX_HOME``
environment variable.
"""

from __future__ import annotations

import os
import tempfile
from pathlib import Path

_process_home: Path | None = None
_desktop_profile_environment: dict[str, str | None] | None = None


def normal_home() -> Path:
    """Return the current user's normal Quantix workspace root."""
    return (Path.home() / ".quantix").resolve()


def resolve_home(home: Path | str | None = None) -> Path:
    """Resolve an explicit workspace root, or the normal root when omitted."""
    if home is None:
        return normal_home()
    return Path(home).expanduser().resolve()


def runtime_dir(home: Path | str | None = None) -> Path:
    return resolve_home(home) / "runtime"


def runtime_tmp_dir(home: Path | str | None = None) -> Path:
    return runtime_dir(home) / "tmp"


def cache_dir(home: Path | str | None = None) -> Path:
    return resolve_home(home) / "cache"


def models_dir(home: Path | str | None = None) -> Path:
    return resolve_home(home) / "models"


def logs_dir(home: Path | str | None = None) -> Path:
    return resolve_home(home) / "logs"


def ai_components_dir(home: Path | str | None = None) -> Path:
    return resolve_home(home) / "ai-components"


def scratch_dir(home: Path | str | None = None) -> Path:
    return resolve_home(home) / "scratch"


def transcription_dir(home: Path | str | None = None) -> Path:
    return runtime_tmp_dir(home) / "transcription"


def connection_file(home: Path | str | None = None) -> Path:
    return runtime_dir(home) / "connection.json"


def schema_file(home: Path | str | None = None) -> Path:
    return runtime_dir(home) / "openapi.json"


def ensure_runtime_dirs(home: Path | str | None = None) -> Path:
    """Create only the application runtime/cache directories needed at start."""
    root = resolve_home(home)
    runtime_dir(root).mkdir(parents=True, exist_ok=True)
    runtime_tmp_dir(root).mkdir(parents=True, exist_ok=True)
    cache_dir(root).mkdir(parents=True, exist_ok=True)
    logs_dir(root).mkdir(parents=True, exist_ok=True)
    scratch_dir(root).mkdir(parents=True, exist_ok=True)
    transcription_dir(root).mkdir(parents=True, exist_ok=True)
    return root


def current_home() -> Path:
    """Return the root most recently selected for this process."""
    return _process_home or normal_home()


def desktop_browser_context() -> dict[str, str | None]:
    """Return the user's browser profile environment captured before routing caches."""
    if _desktop_profile_environment is not None:
        return dict(_desktop_profile_environment)
    return {
        key: os.environ.get(key)
        for key in ("HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA",
                    "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME")
    }


def prepare_process_environment(home: Path | str | None = None) -> Path:
    """Route process temporary and library cache locations into ``home``.

    This is called before service dependencies are imported.  It intentionally
    leaves ``HOME`` and browser profile variables alone: normal browser sign-in
    continues to use the user's own browser profile.
    """
    global _process_home, _desktop_profile_environment
    if _desktop_profile_environment is None:
        _desktop_profile_environment = {
            key: os.environ.get(key)
            for key in ("HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA",
                        "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME")
        }
    root = ensure_runtime_dirs(home)
    _process_home = root
    temporary = runtime_tmp_dir(root)
    cache = cache_dir(root)
    huggingface = cache / "huggingface"
    xet = huggingface / "xet"
    transformers = huggingface / "transformers"
    fastembed = cache / "fastembed"
    for directory in (huggingface, xet, transformers, fastembed, cache / "uv", cache / "pip"):
        directory.mkdir(parents=True, exist_ok=True)
    # CPython caches tempfile.gettempdir() after its first lookup. Keep that
    # cache aligned with the selected root for libraries that use
    # NamedTemporaryFile without an explicit directory.
    tempfile.tempdir = str(temporary)
    values = {
        "TMPDIR": str(temporary),
        "TEMP": str(temporary),
        "TMP": str(temporary),
        "UV_CACHE_DIR": str(cache / "uv"),
        "PIP_CACHE_DIR": str(cache / "pip"),
        "HF_HOME": str(huggingface),
        "HF_HUB_CACHE": str(huggingface / "hub"),
        "HF_XET_CACHE": str(xet),
        "TRANSFORMERS_CACHE": str(transformers),
        "FASTEMBED_CACHE_PATH": str(fastembed),
        "TORCH_HOME": str(cache / "torch"),
    }
    os.environ.update(values)
    return root
