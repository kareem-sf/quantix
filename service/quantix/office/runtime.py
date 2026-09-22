"""The office at work: wakes the people who have something new, one turn at a time, on a background thread."""

import asyncio
import logging
import threading
from collections.abc import Callable
from pathlib import Path

from pydantic_ai import UsageLimitExceeded
from pydantic_ai.models import Model
from sqlalchemy import select
from sqlalchemy.orm import Session, sessionmaker

from quantix import settings, tenders
from quantix.ai import connections, providers
from quantix.documents import library
from quantix.office import agents, records
from quantix.office.models import ENGINEER, TEAM, Message, Staff
from quantix.office.tools import Stopped, Turn
from quantix.tenders import Tender

log = logging.getLogger("quantix.office")

OFFICE = "office"  # the sender of the office's own notices; never used for anything an agent says
TURN_BUDGET = 40  # turns without hearing from the engineer before the office pauses


def office_model(home: Path) -> Model | None:
    chosen = settings.load(home)["office_ai"]
    connection = chosen and connections.get(home, chosen["connection_id"])
    if not connection:
        return None
    return providers.build_model(connection["provider"], chosen["model"], connection["api_key"], connection["base_url"])


class Office:
    def __init__(self, home: Path, sessions: sessionmaker[Session], model: Callable[[], Model | None] | None = None):
        self.home = home
        self.sessions = sessions
        self.model = model or (lambda: office_model(home))
        self._wake = threading.Event()
        self._closing = threading.Event()
        self._thread = threading.Thread(target=self._run, name="quantix-office", daemon=True)
        self._stops: dict[str, threading.Event] = {}
        self._turns: dict[str, int] = {}
        self._paused: set[str] = set()
        self._working: str | None = None  # the tender being worked on right now
        self._again: set[str] = set()  # people whose last turn ran out of steps with work open

    # The engineer's side -------------------------------------------------------------------------------------

    def start(self) -> None:
        self._thread.start()
        self.wake()

    def close(self) -> None:
        self._closing.set()
        for stop in self._stops.values():
            stop.set()
        self._wake.set()
        self._thread.join(timeout=15)

    def wake(self) -> None:
        self._wake.set()

    def engineer_spoke(self, tender_id: str) -> None:
        """A message from the engineer resumes a stopped or paused office."""
        self._stop_event(tender_id).clear()
        self._turns[tender_id] = 0
        self._paused.discard(tender_id)
        self.wake()

    def stop(self, tender_id: str) -> None:
        self._stop_event(tender_id).set()
        self._paused.add(tender_id)

    def status(self, tender_id: str) -> str:
        """working | paused | idle"""
        if self._working == tender_id:
            return "working"
        return "paused" if tender_id in self._paused else "idle"

    def _stop_event(self, tender_id: str) -> threading.Event:
        return self._stops.setdefault(tender_id, threading.Event())

    # The work ---------------------------------------------------------------------------------------------------

    def _run(self) -> None:
        while not self._closing.is_set():
            self._wake.wait()
            self._wake.clear()
            try:
                asyncio.run(self._work())
            except Exception:
                log.exception("The office loop failed")

    async def _work(self) -> None:
        with self.sessions() as session:
            tender_ids = list(session.scalars(select(Tender.id)))
        busy = True
        while busy and not self._closing.is_set():
            busy = False
            for tender_id in tender_ids:
                if tender_id not in self._paused and not self._stop_event(tender_id).is_set():
                    self._working = tender_id
                    try:
                        busy = await self._work_on(tender_id) or busy
                    finally:
                        self._working = None

    async def _work_on(self, tender_id: str) -> bool:
        """One pass over the tender's people. True if anyone worked."""
        with self.sessions() as session:
            if not self._has_manager(session, tender_id) and not self._engineer_has_spoken(session, tender_id):
                return False
        model = self.model()
        if model is None:
            return False
        try:
            with self.sessions() as session:
                if not self._has_manager(session, tender_id):
                    await self._appoint_manager(session, model, tender_id)
            worked = False
            with self.sessions() as session:
                people = [m.id for m in records.team(session, tender_id)]
            for staff_id in people:
                if self._turns.get(tender_id, 0) >= TURN_BUDGET:
                    self._pause(
                        tender_id, "The office paused after a long stretch of work. Send a message to carry on."
                    )
                    return False
                if await self._turn(model, tender_id, staff_id):
                    worked = True
            return worked
        except Stopped:
            return False
        except Exception as error:
            log.warning("Office work on %s failed: %s", tender_id, type(error).__name__)
            self._pause(tender_id, f"The office stopped: {providers.explain(error)} Send a message to try again.")
            return False

    async def _turn(self, model: Model, tender_id: str, staff_id: str) -> bool:
        with self.sessions() as session:
            member = session.get(Staff, staff_id)
            new = records.inbox(session, member)
            if not new and staff_id not in self._again:
                return False
            self._again.discard(staff_id)
            prompt = agents.situation(session, member, new)
            records.mark_read(session, member)
            session.commit()
            session.expunge(member)
        autonomous = settings.load(self.home)["office_mode"] == "autonomous"
        turn = Turn(self.home, self.sessions, tender_id, staff_id, autonomous, self._stop_event(tender_id))
        self._turns[tender_id] = self._turns.get(tender_id, 0) + 1
        try:
            await agents.run_turn(model, turn, member, prompt, autonomous)
        except UsageLimitExceeded:
            with self.sessions() as session:
                if records.open_tasks(session, session.get(Staff, staff_id)):
                    self._again.add(staff_id)
        finally:
            with self.sessions() as session:
                person = session.get(Staff, staff_id)
                if not person.is_manager and not records.open_tasks(session, person):
                    person.now = None
                session.commit()
        return True

    async def _appoint_manager(self, session: Session, model: Model, tender_id: str) -> None:
        tender = tenders.get_tender(session, tender_id)
        names = [d.path for d in library.documents(session, tender_id)][:40]
        brief = (
            f"Create the Tender Manager who will lead this construction tender: {tender.name}.\n"
            f"The package includes: {', '.join(names) or 'no documents yet'}.\n"
            "A senior person who has led many tenders like this one, with their own opinions and way of speaking."
        )
        persona = await agents.create_persona(model, brief)
        profile = persona.model_dump(exclude={"name"})
        member = records.hire(session, tender_id, persona.name, "Tender Manager", profile, is_manager=True)
        member.now = "Getting to know the tender"
        records.post(session, tender_id, OFFICE, TEAM, f"{member.name} is the Tender Manager for this tender.", "note")
        session.commit()

    def _pause(self, tender_id: str, notice: str) -> None:
        self._paused.add(tender_id)
        with self.sessions() as session:
            records.post(session, tender_id, OFFICE, TEAM, notice, "note")
            session.commit()

    @staticmethod
    def _has_manager(session: Session, tender_id: str) -> bool:
        return records.manager(session, tender_id) is not None

    @staticmethod
    def _engineer_has_spoken(session: Session, tender_id: str) -> bool:
        query = select(Message.id).where(Message.tender_id == tender_id, Message.sender == ENGINEER).limit(1)
        return session.scalars(query).first() is not None
