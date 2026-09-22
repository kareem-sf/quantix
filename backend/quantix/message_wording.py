"""Plain wording for package-analysis chat lines, old and new.

Older package summaries are stored with internal terms ("indexed", "structured
output", "worker operation"). The engineer reads these messages, so they are
shown in the same plain words new summaries use. Stored text is never changed.
"""

from __future__ import annotations

import re

# What the engineer reads when an analysis stage could not finish. The error
# itself stays in the run's stage event.
STAGE_FAILED = {
    "map": "**Package overview:** I couldn't finish it. Ask me to try again.",
    "index": "**Search:** it couldn't be prepared. I can still read the documents one by one.",
    "structure": "**BOQ:** I couldn't read it automatically. Ask me to check it.",
}

_OLD_STAGE_LABELS = {
    "Mapping the tender package": "map",
    "Indexing tender evidence": "index",
    "Extracting BOQ, schedules and tables": "structure",
}

_OLD_STAGE_LINE = re.compile(
    r"\*\*(?P<stage>" + "|".join(map(re.escape, _OLD_STAGE_LABELS)) + r"):\*\* (?P<detail>[^\n]*)"
)
_OLD_REGISTERED = re.compile(r"I registered and indexed the tender package \((\d+) documents?\)\.")

# Record IDs mean nothing to an engineer. IDs cited in brackets become source
# links in the app, so only code-quoted IDs are dropped here.
_QUOTED_ID = re.compile(r"\s?`[0-9a-f]{32}`")


def stage_waiting(detail: str) -> str:
    return f"**Package overview:** {detail.rstrip('.')}."


def registered(count: int) -> str:
    return f"I've read the tender package ({count} documents)."


def _old_stage(match: re.Match[str]) -> str:
    detail = match.group("detail")
    if detail.startswith("Choose the AI"):
        return stage_waiting(detail)
    return STAGE_FAILED[_OLD_STAGE_LABELS[match.group("stage")]]


def plain_message(role: str, content: str) -> str:
    """Return the text the engineer sees for a stored Manager or system message."""

    if role not in {"manager", "system"} or not content:
        return content
    text = _OLD_STAGE_LINE.sub(_old_stage, content)
    text = _OLD_REGISTERED.sub(lambda match: registered(int(match.group(1))), text)
    return _QUOTED_ID.sub("", text)
