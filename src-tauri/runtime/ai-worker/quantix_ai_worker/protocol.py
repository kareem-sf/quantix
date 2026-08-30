"""Protocol helpers: newline-delimited JSON frames over stdin/stdout."""

import json
import sys

MAX_FRAME_BYTES = 256 * 1024
MAX_EVENT_FRAMES = 2_000

TERMINAL_KINDS = ("result", "failure")


class ProtocolError(Exception):
    """The host sent an input the worker refuses to interpret."""


class ProtocolOutput:
    def __init__(self) -> None:
        try:
            sys.stdout.reconfigure(encoding="utf-8", errors="replace")
        except (AttributeError, ValueError):  # pragma: no cover
            pass
        self._stdout = sys.stdout
        self.terminal_sent = False
        self.event_budget = MAX_EVENT_FRAMES

    def send(self, frame: dict) -> None:
        kind = frame.get("kind")
        if kind in TERMINAL_KINDS:
            if self.terminal_sent:
                raise ProtocolError("a second terminal frame was requested")
            self.terminal_sent = True
        if kind == "event":
            if self.event_budget <= 0:
                return
            self.event_budget -= 1
        line = json.dumps(frame, ensure_ascii=True, separators=(",", ":"))
        if len(line) > MAX_FRAME_BYTES:
            raise ProtocolError(f"worker frame exceeded its byte budget ({kind})")
        try:
            self._stdout.write(line + "\n")
            self._stdout.flush()
        except OSError:  # pragma: no cover - the host hung up
            pass


def parse_frame(line: bytes) -> dict:
    if len(line) > MAX_FRAME_BYTES:
        raise ProtocolError("host frame exceeded its byte budget")
    try:
        frame = json.loads(line.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ProtocolError(f"host frame was not valid JSON: {error}") from error
    if not isinstance(frame, dict) or not isinstance(frame.get("kind"), str):
        raise ProtocolError("host frame is missing a string kind")
    return frame
