"""Release budget reservations for provider requests that were never processed."""

from __future__ import annotations

import json

from .ai_api_errors import REJECTION_DETAILS
from .db import dump, new_id, now

RELEASED_DETAIL = (
    "The provider rejected this request before processing it, so nothing was billed. "
    "Its reservation was released."
)


def released_usage(data: dict) -> dict:
    """Return a usage row whose conservative hold no longer counts against budgets."""

    return data | {
        "status": "rejected",
        "reserved_usd": 0,
        "estimated_cost_usd": 0,
        "reserved_search_calls": 0,
        "search_usage_complete": True,
        "search_usage_uncertain": False,
        "detail": RELEASED_DETAIL,
    }


def release_rejected_reservations(repo) -> int:
    """Repair holds that earlier versions left uncertain after a rejected request.

    Only rows with no recorded provider usage, belonging to a failed run whose
    saved error is one of the exact pre-processing rejection messages, are
    released. Every release is recorded as a decision.
    """

    released = 0
    with repo.db.connect(write=True) as conn:
        exists = conn.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='ai_usage'"
        ).fetchone()
        if not exists:
            return 0
        placeholders = ",".join("?" for _ in REJECTION_DETAILS)
        rows = conn.execute(
            "SELECT u.id, u.tender_id, u.data_json FROM ai_usage u JOIN runs r ON r.id = u.run_id "
            f"WHERE r.status = 'failed' AND r.error IN ({placeholders})",
            REJECTION_DETAILS,
        ).fetchall()
        for identifier, tender_id, raw in rows:
            data = json.loads(raw)
            if (
                data.get("status") != "uncertain"
                or data.get("input_tokens")
                or data.get("output_tokens")
                or data.get("web_search_calls")
            ):
                continue
            conn.execute("UPDATE ai_usage SET data_json=? WHERE id=?", (dump(released_usage(data)), identifier))
            conn.execute(
                "INSERT INTO decisions VALUES(?,?,?,?,?,?,?)",
                (new_id(), tender_id, "ai_usage", identifier, "release_rejected", RELEASED_DETAIL, now()),
            )
            released += 1
    return released
