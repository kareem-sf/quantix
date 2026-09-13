"""Data-only worker entry point, invoked by the managed private Python runtime."""

import sys
from pathlib import Path

# Isolated Python deliberately ignores the working directory and PYTHONPATH.
# Only this hash-checked, bundled source directory is added for the worker.
sys.path.insert(0, str(Path(__file__).resolve().parent))

from quantix_ai_worker.server import main

if __name__ == "__main__":
    main()
