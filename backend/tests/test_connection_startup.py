"""Real isolated service restart, using synthetic local-session credentials."""

import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request


def test_connection_record_follows_real_service_stop_and_restart(tmp_path):
    home = tmp_path / "workspace"
    connection = home / "runtime" / "connection.json"
    for index, token in enumerate(("a" * 64, "b" * 64)):
        with (tmp_path / f"startup-{index}.log").open("wb") as log:
            process = subprocess.Popen(
                [sys.executable, "-m", "quantix", "--home", str(home), "--port", "0"],
                env={**os.environ, "QUANTIX_SESSION_TOKEN": token},
                stdout=log,
                stderr=subprocess.STDOUT,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
            try:
                deadline = time.monotonic() + 45
                record = None
                while time.monotonic() < deadline:
                    assert process.poll() is None, "Synthetic service exited during startup"
                    try:
                        record = json.loads(connection.read_text(encoding="utf-8"))
                        assert record["token"] == token
                        request = urllib.request.Request(
                            record["base_url"] + "/health",
                            headers={"Authorization": "Bearer " + token},
                        )
                        with urllib.request.urlopen(request, timeout=1) as response:
                            assert response.status == 200
                        break
                    except (
                        FileNotFoundError,
                        PermissionError,
                        urllib.error.URLError,
                        TimeoutError,
                    ):
                        time.sleep(0.1)
                else:
                    raise AssertionError("Synthetic service did not become healthy")
                assert record is not None
                request = urllib.request.Request(
                    record["base_url"] + "/shutdown",
                    method="POST",
                    data=b"",
                    headers={"Authorization": "Bearer " + token},
                )
                with urllib.request.urlopen(request, timeout=5) as response:
                    assert response.status == 200
                assert process.wait(timeout=15) == 0
                assert not connection.exists()
            finally:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=5)
