"""Write the service's OpenAPI schema to a file: python -m quantix.openapi <path>."""

import json
import sys
from pathlib import Path

from quantix.api.app import create_app


def main() -> None:
    # The schema is built without starting the app, so the data home is never opened.
    schema = create_app(Path.home() / ".quantix", "schema-only").openapi()
    Path(sys.argv[1]).write_text(json.dumps(schema, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
