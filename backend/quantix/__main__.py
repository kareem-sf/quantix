"""Launch the private local service owned by the Quantix desktop session."""

import argparse
import os
import secrets
import socket
import sys
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description="Quantix local Tender Office")
    parser.add_argument("--home", type=Path, default=None)
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--connection-file", type=Path)
    # Service commands used by frozen sidecars are parsed below so their
    # positional arguments remain compatible with the existing entry points.
    args, command_args = parser.parse_known_args()
    known_commands = {
        ("semantic-download",),
        ("word-convert",),
    }
    if command_args and (
        any(value.startswith("-") for value in command_args)
        or tuple(command_args[:1]) not in known_commands
        or (command_args[0] == "semantic-download" and len(command_args) != 2)
        or (command_args[0] == "word-convert" and len(command_args) != 4)
    ):
        parser.error(f"unrecognised arguments: {' '.join(command_args)}")
    from .reset_credentials import checked_owned_path
    from .storage import connection_file, logs_dir, prepare_process_environment, resolve_home
    lexical_home = (args.home.expanduser() if args.home else Path.home() / ".quantix").absolute()
    checked_owned_path(lexical_home, lexical_home)
    home = resolve_home(args.home)
    from .factory_reset import load_journal
    reset = load_journal(home)
    if reset and (reset["phase"] in {"ready", "deleting"} or command_args):
        raise SystemExit("Quantix reset is pending. Open the desktop launcher to finish the reset before starting its service.")
    home = prepare_process_environment(home)
    # Keep this import and initialization before service/database imports so
    # startup failures have the same local diagnostic destination as requests.
    from .diagnostics import initialize, record, record_exception
    initialize(logs_dir(home))
    record("service_start", phase="startup", outcome="starting")
    if len(command_args) == 2 and command_args[0] == "semantic-download":
        try:
            from .semantic import _download_model
            _download_model(Path(command_args[1]))
            record("service_command", phase="semantic_download", outcome="completed")
            return
        except BaseException as error:
            record_exception("service_command_failed", error, phase="semantic_download")
            raise
    if len(command_args) == 4 and command_args[0] == "word-convert":
        try:
            from .document_word import _run
            result = _run(*(Path(value) for value in command_args[1:4]))
            record("service_command", phase="word_convert", outcome="completed", exit_code=result)
            raise SystemExit(result)
        except SystemExit:
            raise
        except BaseException as error:
            record_exception("service_command_failed", error, phase="word_convert")
            raise
    try:
        import uvicorn
        from filelock import FileLock, Timeout

        from .api import create_app
        from .backup import apply_pending_restore
    except BaseException as error:
        record_exception("service_start_failed", error, phase="startup")
        raise

    token = os.environ.get("QUANTIX_SESSION_TOKEN") or secrets.token_hex(32)
    if len(token) < 32:
        raise SystemExit(
            "Start Quantix through its desktop launcher so a private local session is created."
        )
    try:
        connection = args.connection_file or connection_file(home)
        with FileLock(home / "workspace.lock", timeout=0):
            record("service_start", phase="startup", outcome="workspace_lock_acquired")
            if load_journal(home) is None:
                apply_pending_restore(home)
            app = create_app(home, token)
            listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            listener.bind(("127.0.0.1", args.port))
            listener.listen(2048)
            listener.setblocking(False)
            actual_port = listener.getsockname()[1]
            server = uvicorn.Server(
                uvicorn.Config(
                    app,
                    host="127.0.0.1",
                    port=actual_port,
                    access_log=False,
                    log_level="warning",
                    # Browser keep-alive sockets can stay half-closed on Windows and
                    # would otherwise hold shutdown open until the launcher kills it.
                    timeout_graceful_shutdown=5,
                )
            )
            app.state.request_shutdown = lambda: setattr(server, "should_exit", True)
            if connection:
                from .connection_record import write_connection_record
                write_connection_record(connection, {"base_url": f"http://127.0.0.1:{actual_port}/api", "token": token})
            try:
                record("service_start", phase="startup", outcome="ready")
                server.run(sockets=[listener])
            finally:
                record("service_stop", phase="shutdown", outcome="stopping")
                listener.close()
                if connection:
                    connection.unlink(missing_ok=True)
            record("service_stop", phase="shutdown", outcome="stopped")
    except Timeout:
        record("service_start", phase="startup", outcome="workspace_already_open")
        print(
            "This Quantix workspace is already open. Return to the existing window.",
            file=sys.stderr,
        )
        raise SystemExit(1)
    except BaseException as error:
        record_exception("service_start_failed", error, phase="startup")
        raise


if __name__ == "__main__":
    main()
