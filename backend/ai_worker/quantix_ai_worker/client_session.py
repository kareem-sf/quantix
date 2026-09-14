"""Server-bound native session paths and supplied public response events."""

import re
import time
from uuid import UUID, uuid4

from .common import RuntimeUnavailable


def session_binding(context, connection, route):
    binding = getattr(context, "session_binding", None)
    if binding is None:
        return None
    if (
        not isinstance(binding, dict)
        or not re.fullmatch(r"[a-f0-9]{32}", str(binding.get("id", "")))
        or binding.get("connection_id") != connection.get("id")
        or binding.get("connection_revision") != connection.get("revision")
        or binding.get("protocol") != connection.get("protocol")
        or binding.get("model_id") != route.get("model_id")
        or binding.get("run_id") != context.operation_id
        or not re.fullmatch(r"[a-f0-9]{64}", str(binding.get("scope_fingerprint", "")))
        or not re.fullmatch(r"[a-f0-9]{64}", str(binding.get("settings_fingerprint", "")))
    ):
        raise RuntimeUnavailable(
            "The original-client session binding does not match this operation."
        )
    identifier = binding.get("provider_session_id")
    if identifier is not None:
        if not isinstance(identifier, str) or not re.fullmatch(r"[A-Za-z0-9_-]{1,200}", identifier):
            raise RuntimeUnavailable("The saved native session identifier is invalid.")
        if connection["protocol"] == "grok_build":
            try:
                UUID(identifier)
            except ValueError:
                raise RuntimeUnavailable(
                    "Grok can safely resume only an exact native UUID, never a session title. Start a new session."
                ) from None
    return binding


def work_directory(context, binding, fallback_id):
    root = context.account_home.resolve()
    work = (
        root
        / "work"
        / ("sessions" if binding else context.operation_id)
        / (binding["id"] if binding else fallback_id)
    )
    work = work.resolve()
    if not work.is_relative_to(root):
        raise RuntimeUnavailable(
            "The original-client working directory is outside its private account home."
        )
    work.mkdir(parents=True, exist_ok=True)
    return work


class ClientEventBuffer:
    def __init__(self, context):
        self.context = context
        self.pending = ""
        self.total = 0
        self.events = 0
        self.last_flush = time.monotonic()
        self.item_id = None
        self.has_text = False
        self.reset = False
        self.json_output = False
        self.summaries = {}
        self.summary_pending = {}
        self.request_id = uuid4().hex
        self.drafts = {}
        self.finished = False

    async def text(self, delta, *, item_id=None):
        if not isinstance(delta, str) or not delta:
            return
        if item_id is not None and item_id != self.item_id:
            await self.flush()
            self.reset = self.has_text
            self.item_id = item_id
            self.json_output = delta.lstrip().startswith(("{", "["))
        if self.json_output:
            return
        self.pending += delta
        self.drafts[self.item_id] = self.drafts.get(self.item_id, "") + delta
        if len(self.pending) >= 256 or time.monotonic() - self.last_flush >= 0.2:
            await self.flush()

    async def flush(self):
        while self.pending:
            text, self.pending = self.pending[:2048], self.pending[2048:]
            await self.context.control.event(
                "assistant_text_delta",
                "Original client response draft.",
                {
                    "text": text,
                    "request_id": self.request_id,
                    "item_id": self.item_id,
                    "delta": True,
                    **({"reset": True} if self.reset else {}),
                },
            )
            self.reset = False
            self.has_text = True
            self.events += 1
            self.total += len(text)
        self.last_flush = time.monotonic()

    async def summary(self, delta, *, item_id=None, summary_index=0):
        if not isinstance(delta, str) or not delta:
            return
        key = (item_id, summary_index)
        self.summaries[key] = self.summaries.get(key, "") + delta
        self.summary_pending[key] = self.summary_pending.get(key, "") + delta
        if len(self.summary_pending[key]) >= 256 or time.monotonic() - self.last_flush >= 0.2:
            await self.context.control.event(
                "assistant_reasoning_summary",
                "Original client supplied a reasoning summary.",
                {
                    "text": self.summary_pending.pop(key),
                    "request_id": self.request_id,
                    "item_id": item_id,
                    "summary_index": summary_index,
                    "delta": True,
                },
            )
            self.events += 1
            self.last_flush = time.monotonic()

    def summary_snapshot(self, text, *, item_id, summary_index):
        """Reconcile the client's completed public section without duplication."""
        if isinstance(text, str) and text:
            self.summaries[(item_id, summary_index)] = text

    async def finish(self, *, complete=True):
        if self.finished:
            return
        if complete:
            await self.flush()
        for (item_id, summary_index), text in self.summaries.items():
            await self.context.control.event(
                "assistant_reasoning_summary",
                "Original client supplied a reasoning summary.",
                {
                    "text": text,
                    "request_id": self.request_id,
                    "item_id": item_id,
                    "summary_index": summary_index,
                    "complete": complete,
                    "delta": False,
                },
            )
            self.events += 1
        for item_id, text in self.drafts.items():
            await self.context.control.event(
                "runtime_public_section",
                "Original client response draft.",
                {
                    "text": text,
                    "request_id": self.request_id,
                    "item_id": item_id,
                    "complete": complete,
                    "delta": False,
                    "category": "draft",
                },
            )
        self.finished = True

    async def request_activity(self, phase, *, payload=None):
        await self.context.control.event(
            "runtime_request_activity",
            "Content supplied by Quantix",
            {"request_id": self.request_id, "phase": phase, "payload": payload or {}},
        )
