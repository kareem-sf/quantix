"""Connection publication stays complete while desktop readers are active."""

import json
import os
import threading

import pytest

from quantix.connection_record import write_connection_record


def test_reader_sees_previous_record_until_complete_replacement(tmp_path, monkeypatch):
    path = tmp_path / "runtime" / "connection.json"
    first = {"base_url": "http://127.0.0.1:10001/api", "token": "a" * 64}
    second = {"base_url": "http://127.0.0.1:10002/api", "token": "b" * 64}
    write_connection_record(path, first)
    replace = os.replace

    def inspect_then_replace(source, destination):
        assert json.loads(path.read_text()) == first
        assert json.loads(source.read_text()) == second
        replace(source, destination)

    monkeypatch.setattr("quantix.connection_record.os.replace", inspect_then_replace)
    write_connection_record(path, second)
    assert json.loads(path.read_text()) == second
    assert list(path.parent.iterdir()) == [path]


def test_failed_publication_preserves_previous_record_and_cleans_temporary_file(
    tmp_path, monkeypatch
):
    path = tmp_path / "connection.json"
    initial = {"base_url": "http://127.0.0.1:10001/api", "token": "a" * 64}
    write_connection_record(path, initial)

    def fail(*_args):
        raise PermissionError("Synthetic replacement failure")

    monkeypatch.setattr("quantix.connection_record.os.replace", fail)
    with pytest.raises(PermissionError):
        write_connection_record(path, {**initial, "token": "b" * 64})
    assert json.loads(path.read_text()) == initial
    assert list(tmp_path.iterdir()) == [path]


def test_concurrent_readers_never_observe_truncated_json(tmp_path):
    path = tmp_path / "connection.json"
    value = {"base_url": "http://127.0.0.1:10001/api", "token": "a" * 64}
    write_connection_record(path, value)
    stop = threading.Event()
    failures = []
    reads = []

    def read():
        while not stop.is_set():
            try:
                assert len(json.loads(path.read_text())["token"]) == 64
                reads.append(True)
            except PermissionError:
                # A Windows reader can see a transient sharing violation during
                # replacement; the desktop connection handshake retries it.
                continue
            except Exception as error:
                failures.append(error)
                return
            finally:
                # Match a polling reader, rather than starving replacement with
                # Python handles that do not share delete access on Windows.
                stop.wait(0.002)

    reader = threading.Thread(target=read)
    reader.start()
    try:
        for index in range(60):
            write_connection_record(path, {**value, "token": f"{index:064d}"})
    finally:
        stop.set()
        reader.join(timeout=5)
    assert not reader.is_alive()
    assert not failures
    assert reads


def test_publication_retries_a_transient_windows_sharing_violation(tmp_path, monkeypatch):
    path = tmp_path / "connection.json"
    replace = os.replace
    attempts = []

    def temporarily_locked(source, destination):
        attempts.append(True)
        if len(attempts) == 1:
            raise PermissionError("Synthetic sharing violation")
        replace(source, destination)

    monkeypatch.setattr("quantix.connection_record.os.replace", temporarily_locked)
    write_connection_record(path, {"base_url": "http://127.0.0.1:10001/api", "token": "a" * 64})
    assert len(attempts) == 2
    assert json.loads(path.read_text())["token"] == "a" * 64
