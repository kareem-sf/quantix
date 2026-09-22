"""Run the service on loopback: python -m quantix --port 8765, with the access token in QUANTIX_TOKEN.

The data home is ~/.quantix, or QUANTIX_HOME when set (for example, a scratch folder for testing)."""

import argparse
import os
from pathlib import Path

import uvicorn

from quantix.api.app import create_app


def main() -> None:
    parser = argparse.ArgumentParser(prog="quantix")
    parser.add_argument("--port", type=int, required=True)
    args = parser.parse_args()
    token = os.environ.get("QUANTIX_TOKEN")
    if not token:
        raise SystemExit("QUANTIX_TOKEN must be set.")
    home = Path(os.environ.get("QUANTIX_HOME") or Path.home() / ".quantix")
    app = create_app(home, token)
    uvicorn.run(app, host="127.0.0.1", port=args.port, log_level="warning")


if __name__ == "__main__":
    main()
