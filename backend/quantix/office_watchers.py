"""Bounded local watchers with meaningful-change deduplication and no implicit sends."""

from __future__ import annotations

import hashlib
import json

from .db import new_id, now
from .execution_context import OfficeExecutionIdentity
from .staff_models import OfficeConflict
from .watcher_models import WatchActivation, WatchDraft, WatchRunReceipt, WatchSpec

_SCHEMA = (
    """
    CREATE TABLE IF NOT EXISTS office_watches (
        id TEXT PRIMARY KEY,
        tender_id TEXT NOT NULL,
        scope TEXT NOT NULL,
        trigger TEXT NOT NULL,
        schedule TEXT NOT NULL,
        timezone TEXT NOT NULL,
        stop_condition TEXT NOT NULL,
        budget INTEGER NOT NULL,
        remaining INTEGER NOT NULL,
        notification_policy TEXT NOT NULL,
        state TEXT NOT NULL,
        fingerprint TEXT NOT NULL,
        last_observation TEXT,
        last_successful_check TEXT,
        missed_checks INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL
    )
    """,
    """
    CREATE TABLE IF NOT EXISTS office_watch_notifications (
        id TEXT PRIMARY KEY,
        watch_id TEXT NOT NULL,
        observation_hash TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE(watch_id, observation_hash)
    )
    """,
)


def _fingerprint(payload: dict) -> str:
    return hashlib.sha256(
        json.dumps(payload, ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode()
    ).hexdigest()


class OfficeWatcherService:
    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            for statement in _SCHEMA:
                conn.execute(statement)

    def create(self, ctx: OfficeExecutionIdentity, draft: WatchDraft) -> WatchSpec:
        if not ctx.tender_id:
            raise ValueError("Watches require a selected Tender.")
        payload = draft.model_dump(mode="json")
        fingerprint = _fingerprint(payload)
        identifier, stamp = new_id(), now()
        with self.repo.atomic() as conn:
            conn.execute(
                """
                INSERT INTO office_watches(
                    id,tender_id,scope,trigger,schedule,timezone,stop_condition,budget,remaining,
                    notification_policy,state,fingerprint,missed_checks,created_at
                ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,0,?)
                """,
                (
                    identifier,
                    ctx.tender_id,
                    draft.scope,
                    draft.trigger,
                    draft.schedule,
                    draft.timezone,
                    draft.stop_condition,
                    draft.budget,
                    draft.budget,
                    draft.notification_policy,
                    "draft",
                    fingerprint,
                    stamp,
                ),
            )
        return self.get(identifier)

    def activate(self, ctx: OfficeExecutionIdentity, watch_id: str, request: WatchActivation) -> WatchSpec:
        with self.repo.atomic() as conn:
            row = conn.execute(
                "SELECT * FROM office_watches WHERE id=? AND tender_id=?",
                (watch_id, ctx.tender_id),
            ).fetchone()
            if row is None:
                raise KeyError("This watch is not in the selected Tender.")
            if row["fingerprint"] != request.fingerprint:
                raise OfficeConflict("Activate the exact displayed watch.")
            conn.execute("UPDATE office_watches SET state='active' WHERE id=?", (watch_id,))
        return self.get(watch_id)

    def pause(self, watch_id: str) -> WatchSpec:
        with self.repo.atomic() as conn:
            conn.execute("UPDATE office_watches SET state='paused' WHERE id=?", (watch_id,))
        return self.get(watch_id)

    def stop(self, watch_id: str) -> WatchSpec:
        with self.repo.atomic() as conn:
            conn.execute("UPDATE office_watches SET state='stopped' WHERE id=?", (watch_id,))
        return self.get(watch_id)

    def record_sleep(self, watch_id: str, missed: int) -> WatchSpec:
        with self.repo.atomic() as conn:
            conn.execute(
                "UPDATE office_watches SET missed_checks=missed_checks+? WHERE id=?",
                (missed, watch_id),
            )
        return self.get(watch_id)

    def run_due(self, watch_id: str, observations: list[str], *, sleeping: bool = False) -> WatchRunReceipt:
        spec = self.get(watch_id)
        if spec.state in {"paused", "stopped", "exhausted"}:
            return WatchRunReceipt(
                watch_id=watch_id,
                notifications=0,
                missed_checks=spec.missed_checks,
                commercial_sends=0,
                catch_up=0,
                provider_calls=0,
            )
        notifications = 0
        catch_up = 0
        with self.repo.atomic() as conn:
            if sleeping:
                conn.execute(
                    "UPDATE office_watches SET missed_checks=missed_checks+? WHERE id=?",
                    (len(observations), watch_id),
                )
            remaining = conn.execute(
                "SELECT remaining, last_observation FROM office_watches WHERE id=?",
                (watch_id,),
            ).fetchone()
            if remaining["remaining"] <= 0:
                conn.execute("UPDATE office_watches SET state='exhausted' WHERE id=?", (watch_id,))
                return WatchRunReceipt(
                    watch_id=watch_id,
                    notifications=0,
                    missed_checks=self.get(watch_id).missed_checks,
                    commercial_sends=0,
                    catch_up=0,
                    provider_calls=0,
                )
            unique = []
            for item in observations:
                digest = _fingerprint({"observation": item})
                if digest not in unique:
                    unique.append(digest)
            for digest in unique:
                try:
                    conn.execute(
                        """
                        INSERT INTO office_watch_notifications(id,watch_id,observation_hash,created_at)
                        VALUES(?,?,?,?)
                        """,
                        (new_id(), watch_id, digest, now()),
                    )
                    notifications += 1
                except Exception:
                    continue
            if spec.missed_checks and not sleeping:
                catch_up = 1
            conn.execute(
                """
                UPDATE office_watches
                SET remaining=remaining-1, last_successful_check=?, last_observation=?
                WHERE id=?
                """,
                (now(), unique[0] if unique else None, watch_id),
            )
        latest = self.get(watch_id)
        return WatchRunReceipt(
            watch_id=watch_id,
            notifications=notifications,
            missed_checks=latest.missed_checks,
            commercial_sends=0,
            catch_up=catch_up,
            provider_calls=0 if spec.state != "active" else 1,
        )

    def get(self, watch_id: str) -> WatchSpec:
        with self.repo.atomic() as conn:
            row = conn.execute("SELECT * FROM office_watches WHERE id=?", (watch_id,)).fetchone()
        if row is None:
            raise KeyError("This watch is not in the selected Tender.")
        return WatchSpec(
            id=row["id"],
            scope=row["scope"],
            trigger=row["trigger"],
            schedule=row["schedule"],
            timezone=row["timezone"],
            last_successful_check=row["last_successful_check"],
            next_due=None,
            stop_condition=row["stop_condition"],
            budget=row["budget"],
            notification_policy=row["notification_policy"],
            state=row["state"],
            missed_checks=row["missed_checks"],
            fingerprint=row["fingerprint"],
        )
