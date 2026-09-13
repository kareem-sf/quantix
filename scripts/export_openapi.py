"""Export the installed backend's API schema without starting jobs or a server."""

from __future__ import annotations

import json
import os
import tempfile
from pathlib import Path


def export_schema(*, home: Path | None = None) -> Path:
    """Write the OpenAPI schema for an isolated temporary service.

    A pending factory-reset journal is refused before any ordinary runtime
    directories or schema-export services are created. The destination remains
    the normal home's runtime schema file; the application itself is built
    against a disposable TemporaryDirectory under that runtime tmp path.
    """

    from quantix.factory_reset import load_journal
    from quantix.storage import (
        normal_home,
        prepare_process_environment,
        runtime_tmp_dir,
        schema_file,
    )

    root = home if home is not None else normal_home()
    if load_journal(root) is not None:
        raise SystemExit(
            "Quantix reset is pending. Finish the reset before exporting the API schema."
        )
    previous_tempdir = tempfile.tempdir
    previous_env = {key: os.environ.get(key) for key in ("TMPDIR", "TEMP", "TMP")}
    prepared = prepare_process_environment(root)
    from quantix.api import create_app

    destination = schema_file(prepared)
    destination.parent.mkdir(parents=True, exist_ok=True)
    try:
        with tempfile.TemporaryDirectory(
            prefix="quantix-schema-", dir=runtime_tmp_dir(prepared)
        ) as isolated:
            app = create_app(Path(isolated), "schema-export-session")
            schema = app.openapi()
            app.state.diagnostics.close()
        prepare_process_environment(prepared)
        destination.write_text(json.dumps(schema, indent=2), encoding="utf-8")
        return destination
    finally:
        tempfile.tempdir = previous_tempdir
        for key, value in previous_env.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value


def main() -> None:
    export_schema()
    print("OpenAPI schema exported.")


if __name__ == "__main__":
    main()
