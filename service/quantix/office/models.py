"""The office's records. Everything the engineer sees in the office comes from these rows."""

import uuid
from datetime import UTC, datetime
from typing import Any

from sqlalchemy import JSON, Boolean, ForeignKey, Integer, String, Text
from sqlalchemy.orm import Mapped, mapped_column

from quantix.core.db import Base, UTCDateTime

ENGINEER = "engineer"  # the sender id the engineer's own messages use
TEAM = "team"  # the team room channel; a direct chat with the engineer uses the staff member's id as its channel


def _id() -> str:
    return uuid.uuid4().hex


def _now() -> datetime:
    return datetime.now(UTC)


class Staff(Base):
    __tablename__ = "staff"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    tender_id: Mapped[str] = mapped_column(ForeignKey("tenders.id", ondelete="CASCADE"))
    is_manager: Mapped[bool] = mapped_column(Boolean, default=False)
    name: Mapped[str] = mapped_column(String(100))
    role: Mapped[str] = mapped_column(String(100))
    # Generated for this tender: discipline, experience, background, working_style, opinions, voice.
    profile: Mapped[dict[str, Any]] = mapped_column(JSON)
    status: Mapped[str] = mapped_column(String(20), default="active")  # active | released
    now: Mapped[str | None] = mapped_column(String(300))  # what they are doing, from their real tool calls
    last_read: Mapped[int] = mapped_column(Integer, default=0)  # the newest message id they have been shown
    # the Tender Manager: when he last saw his review queue, so new proposals wake him
    reviewed_up_to: Mapped[datetime | None] = mapped_column(UTCDateTime)
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=_now)

    @property
    def first_name(self) -> str:
        return self.name.split()[0]


class Message(Base):
    __tablename__ = "messages"

    id: Mapped[int] = mapped_column(Integer, primary_key=True, autoincrement=True)
    tender_id: Mapped[str] = mapped_column(ForeignKey("tenders.id", ondelete="CASCADE"))
    sender: Mapped[str] = mapped_column(String(32))  # a staff id, or ENGINEER
    channel: Mapped[str] = mapped_column(String(32))  # TEAM, or a staff id for that person's chat with the engineer
    kind: Mapped[str] = mapped_column(String(20), default="message")  # message | concern | task | note
    text: Mapped[str] = mapped_column(Text)
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=_now)


class Task(Base):
    __tablename__ = "tasks"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    tender_id: Mapped[str] = mapped_column(ForeignKey("tenders.id", ondelete="CASCADE"))
    staff_id: Mapped[str] = mapped_column(ForeignKey("staff.id", ondelete="CASCADE"))
    title: Mapped[str] = mapped_column(String(300))
    brief: Mapped[str] = mapped_column(Text)
    status: Mapped[str] = mapped_column(String(20), default="open")  # open | done
    result: Mapped[str | None] = mapped_column(Text)
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=_now)
    done_at: Mapped[datetime | None] = mapped_column(UTCDateTime)


class TurnRecord(Base):
    """One person's turn, written as it starts and filled in as it ends: what they called, what Quantix sent back,
    what it used and how it ended. The office resumes from these after a restart; they also explain the work."""

    __tablename__ = "turns"

    id: Mapped[int] = mapped_column(Integer, primary_key=True, autoincrement=True)
    tender_id: Mapped[str] = mapped_column(ForeignKey("tenders.id", ondelete="CASCADE"))
    staff_id: Mapped[str] = mapped_column(ForeignKey("staff.id", ondelete="CASCADE"))
    model: Mapped[str] = mapped_column(String(200))
    started_at: Mapped[datetime] = mapped_column(UTCDateTime, default=_now)
    ended_at: Mapped[datetime | None] = mapped_column(UTCDateTime)
    # done | out_of_steps | tool_failed | ai_failed | stopped | failed; none while running, or if Quantix stopped
    ended: Mapped[str | None] = mapped_column(String(20))
    note: Mapped[str | None] = mapped_column(Text)  # why it failed, in plain words
    calls: Mapped[list[dict[str, Any]]] = mapped_column(JSON, default=list)  # {tool, sent_back}: the reason or null
    requests: Mapped[int] = mapped_column(Integer, default=0)
    input_tokens: Mapped[int] = mapped_column(Integer, default=0)
    cached_tokens: Mapped[int] = mapped_column(Integer, default=0)
    output_tokens: Mapped[int] = mapped_column(Integer, default=0)


class Decision(Base):
    """Something waiting for the engineer: a question with options, or a gate to approve."""

    __tablename__ = "decisions"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=_id)
    tender_id: Mapped[str] = mapped_column(ForeignKey("tenders.id", ondelete="CASCADE"))
    raised_by: Mapped[str] = mapped_column(String(32))
    gate: Mapped[str | None] = mapped_column(String(40))
    title: Mapped[str] = mapped_column(String(300))
    text: Mapped[str] = mapped_column(Text)
    options: Mapped[list[str]] = mapped_column(JSON, default=list)
    # An escalation: the record the office couldn't settle, and where the problem shows ({label, document_id,
    # page, boq_item_id}); its options are the Manager's suggested corrections
    subject_kind: Mapped[str | None] = mapped_column(String(20))
    subject_id: Mapped[str | None] = mapped_column(String(32))
    sources: Mapped[list[dict[str, Any]] | None] = mapped_column(JSON)
    status: Mapped[str] = mapped_column(String(20), default="waiting")  # waiting | answered
    answer: Mapped[str | None] = mapped_column(Text)
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=_now)
    decided_at: Mapped[datetime | None] = mapped_column(UTCDateTime)
