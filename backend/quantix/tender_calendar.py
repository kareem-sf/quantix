"""Engineering calendar with original local wording and explicit UTC."""

from __future__ import annotations

from datetime import datetime, timedelta, timezone

from .db import dump, new_id, now
from .execution_context import OfficeExecutionIdentity
from .tender_profile_models import CalendarEvent, CalendarEventDraft


def utc_from_local(local_time: str) -> str:
    stamp = datetime.fromisoformat(local_time)
    if stamp.tzinfo is None:
        raise ValueError("Calendar events need an explicit timezone offset or IANA zone.")
    utc = stamp.astimezone(timezone.utc).replace(microsecond=0)
    return utc.strftime("%Y-%m-%dT%H:%M:%SZ")


def add_calendar_days(start: str, days: int, holidays: set[str] | None = None) -> str:
    cursor = datetime.fromisoformat(start).date()
    remaining = days
    holidays = holidays or set()
    while remaining > 0:
        cursor += timedelta(days=1)
        if cursor.isoformat() not in holidays:
            remaining -= 1
    return cursor.isoformat()


class TenderCalendarService:
    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            conn.execute(
                """
                CREATE TABLE IF NOT EXISTS tender_calendar_events (
                    id TEXT PRIMARY KEY,
                    tender_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    local_time TEXT NOT NULL,
                    timezone TEXT NOT NULL,
                    utc_time TEXT NOT NULL,
                    mandatory INTEGER NOT NULL,
                    source_refs_json TEXT NOT NULL,
                    owner TEXT NOT NULL,
                    reminder_rule TEXT,
                    created_at TEXT NOT NULL
                )
                """
            )

    def save(self, ctx: OfficeExecutionIdentity, draft: CalendarEventDraft) -> CalendarEvent:
        utc_time = utc_from_local(draft.local_time)
        identifier, stamp = new_id(), now()
        with self.repo.atomic() as conn:
            conn.execute(
                """
                INSERT INTO tender_calendar_events(
                    id,tender_id,kind,local_time,timezone,utc_time,mandatory,source_refs_json,
                    owner,reminder_rule,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    identifier,
                    ctx.tender_id,
                    draft.kind,
                    draft.local_time,
                    draft.timezone,
                    utc_time,
                    1 if draft.mandatory else 0,
                    dump(draft.source_refs),
                    draft.owner,
                    draft.reminder_rule,
                    stamp,
                ),
            )
        return CalendarEvent(
            id=identifier,
            kind=draft.kind,
            local_time=draft.local_time,
            timezone=draft.timezone,
            utc_time=utc_time,
            mandatory=draft.mandatory,
            source_refs=draft.source_refs,
            owner=draft.owner,
            reminder_rule=draft.reminder_rule,
        )
