"""Read-only AI SDK UI projections of durable Quantix work.

The stream observes a job; closing it never cancels, starts or retries work.
Only committed Manager messages become final text. Model deltas remain drafts.
"""

from __future__ import annotations

import asyncio
import json
from collections.abc import Callable

from .run_activity_reader import RunActivityService, event_cursor, project_activity

_TERMINAL = {"completed", "failed", "cancelled", "interrupted"}
_TEXT_LIMIT = 65536


class RunChatStream:
    def __init__(self, repo, *, should_stop: Callable[[], bool] | None = None):
        self.repo = repo
        self.should_stop = should_stop or (lambda: False)
        self.activity = RunActivityService(repo, should_stop=self.should_stop)

    def require_run(self, tender_id: str, run_id: str):
        if self.should_stop():
            raise InterruptedError(
                "Quantix is closing this workspace. Reopen it before viewing work."
            )
        run = self.repo.get_run(run_id)
        if run["tender_id"] != tender_id:
            raise KeyError("This work does not belong to the selected Tender.")
        return run

    def _snapshot(self, tender_id: str, run_id: str, after: int):
        # One short synchronous snapshot. No database reader survives a yield.
        with self.repo.db.connect() as conn:
            if not conn.in_transaction:
                conn.execute("BEGIN")
            run = conn.execute(
                "SELECT status,detail,progress FROM runs WHERE id=? AND tender_id=?",
                (run_id, tender_id),
            ).fetchone()
            if run is None:
                raise KeyError("This work is no longer available.")
            events = conn.execute(
                "SELECT * FROM run_events WHERE run_id=? AND id>? ORDER BY id LIMIT 200",
                (run_id, after),
            ).fetchall()
            messages = []
            if run["status"] in _TERMINAL:
                messages = conn.execute(
                    "SELECT id,content FROM messages WHERE tender_id=? AND run_id=? AND role='manager' ORDER BY created_at,id LIMIT 20",
                    (tender_id, run_id),
                ).fetchall()
            return dict(run), [dict(item) for item in events], [dict(item) for item in messages]

    async def chunks(self, tender_id: str, run_id: str, *, after: int = 0, cursor: str | None = None):
        if self.should_stop():
            return
        self.require_run(tender_id, run_id)
        if after < 0:
            raise ValueError("The stream cursor must be non-negative.")
        yield {"type": "start", "messageId": f"run:{run_id}"}
        if cursor:
            with self.repo.db.connect() as conn:
                after, reset = self.activity._decode(conn, run_id, cursor)
            if reset:
                after = 0
                yield {"type": "data-reset", "data": {"reset_required": True}, "transient": True}
        drafts: dict[str, str] = {}
        activity_cursor = cursor
        while True:
            if self.should_stop():
                return
            run, events, messages = self._snapshot(tender_id, run_id, after)
            for event in events:
                after = event["id"]
                activity_cursor = event_cursor(run_id, event)
                data = json.loads(event["data_json"] or "{}")
                assignment_id = data.get("assignment_id")
                is_staff = isinstance(assignment_id, str) and 0 < len(assignment_id) <= 160
                actor = (
                    {
                        "assignment_id": assignment_id,
                        "actor_id": str(data.get("actor_id") or "")[:160],
                    }
                    if is_staff
                    else {}
                )
                draft_id = f"draft:{assignment_id}" if is_staff else f"draft:{run_id}"
                if event["kind"] == "assistant_text_delta" and isinstance(data.get("text"), str):
                    if data.get("reset") is True:
                        drafts[draft_id] = ""
                    drafts[draft_id] = (drafts.get(draft_id, "") + data["text"])[-_TEXT_LIMIT:]
                    yield {
                        "type": "data-staff-draft" if is_staff else "data-draft",
                        "id": draft_id,
                        "data": {"text": drafts[draft_id], "tentative": True, "preview_only": True, **actor},
                    }
                elif event["kind"] == "assistant_reasoning_summary" and isinstance(
                    data.get("text"), str
                ):
                    yield {
                        "type": "data-staff-reasoning" if is_staff else "data-reasoning-summary",
                        "id": f"reasoning:{assignment_id if is_staff else run_id}",
                        "data": {"text": data["text"][:_TEXT_LIMIT], "preview_only": True, **actor},
                    }
                else:
                    # Arbitrary event payloads can contain private SDK/account
                    # details. Only the existing public activity message crosses.
                    yield {
                        "type": "data-activity",
                        "id": f"activity:{event['id']}",
                        "data": project_activity(event).model_dump(),
                    }
            yield {
                "type": "data-run",
                "id": run_id,
                "data": {
                    "run_id": run_id,
                    "status": run["status"],
                    "detail": run["detail"],
                    "progress": run["progress"],
                    "cursor": after,
                    "activity_cursor": activity_cursor,
                    "history_key": self.repo._activity_instance,
                },
            }
            # Drain retained events before closing a terminal run.
            if len(events) == 200:
                continue
            if run["status"] in _TERMINAL:
                for message in messages:
                    part_id = f"message:{message['id']}"
                    yield {"type": "text-start", "id": part_id}
                    yield {"type": "text-delta", "id": part_id, "delta": message["content"]}
                    yield {"type": "text-end", "id": part_id}
                yield {
                    "type": "finish",
                    "finishReason": "stop" if run["status"] == "completed" else "error",
                }
                return
            await asyncio.sleep(0.35)
