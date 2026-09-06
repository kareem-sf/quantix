"""Export the installed backend's API schema without starting jobs or a server."""

import json
import tempfile
from pathlib import Path

from quantix.api import create_app

root = Path(__file__).resolve().parent.parent
destination = root / ".quantix-dev" / "openapi.json"
destination.parent.mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix="quantix-schema-") as home:
    schema = create_app(Path(home), "schema-export-session").openapi()
destination.write_text(json.dumps(schema, indent=2), encoding="utf-8")
print("OpenAPI schema exported.")
