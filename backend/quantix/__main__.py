"""Launch the private local service owned by the Quantix desktop session."""

import argparse
import os
import sys
from pathlib import Path

import uvicorn
from filelock import FileLock, Timeout

from .api import create_app
from .backup import apply_pending_restore


def main():
    parser = argparse.ArgumentParser(description="Quantix local Tender Office")
    parser.add_argument(
        "--home",
        type=Path,
        default=Path(os.environ.get("LOCALAPPDATA", str(Path.home()))) / "Quantix",
    )
    parser.add_argument("--port", type=int, default=8765)
    args = parser.parse_args()
    token = os.environ.get("QUANTIX_SESSION_TOKEN", "")
    if len(token) < 32:
        raise SystemExit(
            "Start Quantix through its desktop launcher so a private local session is created."
        )
    args.home.mkdir(parents=True, exist_ok=True)
    try:
        with FileLock(args.home / "workspace.lock", timeout=0):
            apply_pending_restore(args.home)
            app = create_app(args.home, token)
            server = uvicorn.Server(
                uvicorn.Config(
                    app, host="127.0.0.1", port=args.port, access_log=False, log_level="warning"
                )
            )
            app.state.request_shutdown = lambda: setattr(server, "should_exit", True)
            server.run()
    except Timeout:
        print(
            "This Quantix workspace is already open. Return to the existing window.",
            file=sys.stderr,
        )
        raise SystemExit(1)


if __name__ == "__main__":
    main()
