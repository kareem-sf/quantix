"""One agent turn: who the person is, what is new for them, and their tools."""

import logging
import time

from pydantic_ai import Agent, UsageLimitExceeded, UsageLimits
from pydantic_ai.exceptions import ModelAPIError, ToolRetryError, UnexpectedModelBehavior
from pydantic_ai.messages import ModelMessage, RetryPromptPart, ToolCallPart
from pydantic_ai.models import Model
from pydantic_ai.settings import ModelSettings
from sqlalchemy.orm import Session

from quantix import company, tenders
from quantix.boq import records as boq
from quantix.boq.models import APPROVED
from quantix.documents import library
from quantix.estimate import records as estimate
from quantix.office import records, tools
from quantix.office.models import ENGINEER, TEAM, Message, Staff
from quantix.office.tools import Persona, Turn
from quantix.subcontract import records as subcontract
from quantix.submission import records as submission
from quantix.takeoff import records as takeoff

log = logging.getLogger("quantix.office")

STEP_LIMIT = 12  # model requests in one turn; the person picks up again on their next turn
REQUEST_TIMEOUT = 120.0  # seconds for one model request; a stalled service must not freeze the office

RULES = """How the office works:
- You talk only through your tools. Anything else you write is not seen by anyone.
- The engineer decides scope, the method of measurement, quantities, rates, subcontract and supplier choices, the
  final price and the release. Never decide those for them: the Tender Manager brings each decision to them.
  Doing the work is yours: enter, measure, price and draft without asking first. What you propose waits at a
  gate for the engineer to approve or send back. Ask only when you need a choice from them to go on.
- Every fact must come from the tender documents you have read. Cite them as "<document name>, page <n>".
  Never invent figures, dates or clauses. If the documents don't say, say that.
- Your professional judgement is not a fact and is welcome: plant outputs, market prices, haul distances and
  layer weights in a build-up are estimates. State them as your assumptions; don't wait for a document to give them.
- Report only what your tools confirmed. If a tool sent your call back, the thing isn't done: say so.
- What the engineer asks you directly comes first. When they send your work back, their reason is your
  instruction: redo it that way, and don't ask them whether to.
- Say what you think. If you disagree with the Manager, a colleague or the engineer, use raise_concern.
- Keep messages short and in plain construction English, even when the documents are in Arabic.
- Write so the engineer can take it in at a glance: the point first, in one sentence; then short paragraphs or a
  list with one item per line (Markdown "- " or "1. "), and **bold** only for what needs a decision. Don't sign
  messages or restate your name: it is shown with every message.
- Write to the engineer at most once a turn, and only when they need to know or do something. Put the rest in
  the team room. Your chat with the engineer is below: never repeat what you have already told them or ask what
  they have already answered. If nothing is new for them, don't write.
- "Where the tender stands" below is current. Check details with list_boq, estimate_summary, takeoff_summary and
  list_requirements; don't re-read pages to find out what the office has already entered.
- You have about 12 steps in a turn. Before you run out, say what you found and what comes next.
- When you have acted on everything new, stop."""

MANAGER_DUTIES = """You are the Tender Manager: you lead this tender for the engineer.
- Start by telling the engineer your plan in a few lines (message_engineer). Look over the document list and the
  key pages yourself, but don't read the package page by page: that is your team's work.
- Hire the people this particular tender needs, when it needs them, with hire. There is no standard team:
  choose roles from the actual work. Keep the team small: give work to the people you have before hiring anyone
  new, and release people whose work is done.
- Give each person clear tasks with assign_task, check their results, and follow up. Once work is with someone,
  leave it to them: don't do it yourself alongside them or give it to someone else as well.
- Keep the engineer informed in your chat with them (message_engineer): what you found, what is next, what you need.
- Bring the engineer's decisions to them with ask_engineer, one question at a time, including what your staff raise.
  Check what the engineer has already decided first."""

STAFF_DUTIES = """You work for the Tender Manager.
- Work on your open tasks. When one is done, call complete_task with a clear result and the pages you used.
- If something blocks you or needs the engineer's decision, say so in the team room and name the Manager: the
  Manager asks the engineer, so the same question never reaches them twice."""


def instructions(member: Staff, autonomous: bool) -> str:
    p = member.profile
    who = (
        f"You are {member.name}, {member.role} in a construction tendering office. "
        f"{p.get('discipline', '')}, {p.get('experience_years', '')} years. {p.get('background', '')}\n"
        f"How you work: {p.get('working_style', '')}\nWhat you believe: {p.get('opinions', '')}\n"
        f"How you speak: {p.get('voice', '')} This is flavour only: the rules on clear writing below come first."
    )
    mode = (
        "\nThe engineer has set the office to work fully autonomously: approve your own gates, and record every "
        "decision and its reasons in the team room so the engineer can review it."
        if autonomous
        else ""
    )
    return f"{who}\n\n{MANAGER_DUTIES if member.is_manager else STAFF_DUTIES}\n\n{RULES}{mode}"


def situation(session: Session, member: Staff, new: list[Message]) -> str:
    """What this person needs to know now. Rebuilt every turn from the records, never from memory."""
    tender = tenders.get_tender(session, member.tender_id)
    team = records.team(session, member.tender_id)
    names = {m.id: m.first_name for m in records.team(session, member.tender_id, include_released=True)}
    names[ENGINEER] = "Engineer"
    documents = [d for d in library.documents(session, member.tender_id) if d.status != "replaced"]

    def line(m: Message) -> str:
        where = "team room" if m.channel == TEAM else ("to you" if m.channel == member.id else "")
        tag = " (concern)" if m.kind == "concern" else ""
        return f"- {names.get(m.sender, 'Someone')}{tag}, {where}: {m.text}"

    parts = [
        f"Tender: {tender.name}. Due: {tender.due_date or 'not known yet'}.",
        f"Documents: {len(documents)} files, {sum(d.status == 'read' for d in documents)} read.",
        "Team:\n" + "\n".join(f"- {m.name}, {m.role}" + (f" (now: {m.now})" if m.now else "") for m in team),
    ]
    rules = company.rules(session)
    if rules:
        parts.append(
            "The firm's rules, which the whole office follows:\n" + "\n".join(f"- {r.topic}: {r.text}" for r in rules)
        )
    parts.append("Where the tender stands:\n" + standing(session, member.tender_id))
    tasks = records.open_tasks(session, member)
    if tasks:
        parts.append("Your open tasks:\n" + "\n".join(f"- {t.id}: {t.title}. {t.brief}" for t in tasks))
    if member.is_manager:
        given = [t for t in records.all_tasks(session, member.tender_id) if t.status == "open"]
        if given:
            parts.append("Open tasks in the team:\n" + "\n".join(f"- {names[t.staff_id]}: {t.title}" for t in given))
    decisions = records.decisions(session, member.tender_id)
    answered = [d for d in decisions if d.status == "answered"][-10:]
    if answered:  # everyone knows what the engineer decided, not only whoever asked
        parts.append("The engineer has decided:\n" + "\n".join(f"- {d.title}: {d.answer}" for d in answered))
    waiting = [d for d in decisions if d.status == "waiting"]
    if waiting:
        parts.append("Waiting for the engineer:\n" + "\n".join(f"- {d.title}" for d in waiting))
    shown = {m.id for m in new}
    earlier = [m for m in records.messages(session, member.tender_id, TEAM, limit=12) if m.id not in shown]
    if earlier:
        parts.append("Earlier in the team room:\n" + "\n".join(line(m) for m in earlier))
    chat = [m for m in records.messages(session, member.tender_id, member.id, limit=10) if m.id not in shown]
    if chat:  # so no one repeats themselves or asks the engineer again
        parts.append(
            "Earlier in your chat with the engineer:\n"
            + "\n".join(f"- {'You' if m.sender == member.id else 'Engineer'}: {m.text}" for m in chat)
        )
    parts.append("New for you:\n" + ("\n".join(line(m) for m in new) or "- Nothing new; carry on with your tasks."))
    return "\n\n".join(parts)


def standing(session: Session, tender_id: str) -> str:
    """The work so far in counts, from the records."""
    items = boq.items(session, tender_id)
    price = estimate.summary(session, tender_id)
    packages = subcontract.packages(session, tender_id)
    checklist = [submission.state(session, r) for r in submission.requirements(session, tender_id)]
    approved = sum(i.status in APPROVED for i in items)
    return "\n".join(
        [
            f"- BOQ: {len(items)} lines, {approved} approved.",
            f"- Estimate: {price.priced} of {price.items} lines priced, {price.waiting} waiting for the engineer.",
            f"- Takeoff: {len(takeoff.measurements(session, tender_id))} measurements, "
            f"{takeoff.waiting(session, tender_id)} scales or measurements waiting for the engineer.",
            f"- Subcontract: {len(packages)} packages, {sum(bool(p.selected_quote_id) for p in packages)} chosen.",
            f"- Submission: {checklist.count('ready')} of {len(checklist)} checklist items ready, "
            f"{checklist.count('review')} drafts waiting for the engineer.",
        ]
    )


class CutShort(Exception):
    """The AI service failed in the middle of a turn in a way worth retrying. Carries the turn so far."""

    def __init__(self, error: ModelAPIError, conversation: list[ModelMessage]):
        super().__init__(str(error))
        self.error = error
        self.conversation = conversation


def passing(error: ModelAPIError) -> bool:
    """A failure that usually clears on its own: a timeout, a dropped connection, rate limiting, a server error."""
    status = getattr(error, "status_code", None)
    if isinstance(status, int):
        return status == 429 or status >= 500
    cause = type(error.__cause__).__name__.lower() if error.__cause__ else ""
    return "timeout" in cause or "connect" in cause


async def run_turn(
    model: Model, turn: Turn, member: Staff, prompt: str, autonomous: bool, history: list[ModelMessage] | None = None
) -> list[ModelMessage] | None:
    """One turn. When the person runs out of steps, returns the turn's conversation so their next turn carries on
    from it instead of starting over."""
    agent = Agent(
        model,
        deps_type=Turn,
        instructions=instructions(member, autonomous),
        tools=tools.MANAGER if member.is_manager else tools.STAFF,
        retries=2,
        model_settings=ModelSettings(timeout=REQUEST_TIMEOUT),
    )
    limits = UsageLimits(request_limit=STEP_LIMIT)
    async with agent.iter(prompt, deps=turn, usage_limits=limits, message_history=history) as run:
        try:
            asked = time.monotonic()
            async for node in run:
                if turn.stop.is_set():
                    raise tools.Stopped()
                if Agent.is_model_request_node(node):
                    asked = time.monotonic()
                    for part in node.request.parts:
                        if isinstance(part, RetryPromptPart):
                            log.info("%s: a tool call was sent back: %s", member.name, str(part.content)[:300])
                    # Streamed: some services drop a long answer that arrives in one piece after a quiet minute.
                    async with node.stream(run.ctx) as answer:
                        async for _event in answer:
                            if turn.stop.is_set():
                                raise tools.Stopped()
                elif Agent.is_call_tools_node(node):
                    calls = [p.tool_name for p in node.model_response.parts if isinstance(p, ToolCallPart)]
                    log.info("%s: answered in %.1f s, calling %s", member.name, time.monotonic() - asked, calls)
                    if not calls and not node.model_response.text:
                        return None  # nothing more to do: don't prompt them to say something anyway
        except UsageLimitExceeded:
            return run.all_messages()
        except ModelAPIError as error:
            if not passing(error):
                raise
            raise CutShort(error, run.all_messages()) from error
        except (UnexpectedModelBehavior, ToolRetryError) as error:
            # A reply the model couldn't get right ends this person's turn, not the office's work.
            log.info("%s: turn ended early: %s", member.name, str(error)[:300])
            if "output retries" in str(error):
                return None  # an empty answer: they had nothing more to do
            return run.all_messages()  # a tool call that kept failing: the next turn sees why
    return None


async def create_persona(model: Model, brief: str) -> Persona:
    agent = Agent(
        model,
        output_type=Persona,
        instructions="You create believable people for a construction office. Write in plain English.",
        model_settings=ModelSettings(timeout=REQUEST_TIMEOUT),
        retries=3,  # a profile that fails its checks goes back to the model
    )
    result = await agent.run(brief, usage_limits=UsageLimits(request_limit=4))
    return result.output
