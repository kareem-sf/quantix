"""One agent turn: who the person is, what is new for them, and their tools."""

from pydantic import BaseModel, Field
from pydantic_ai import Agent, UsageLimits
from pydantic_ai.models import Model
from sqlalchemy.orm import Session

from quantix import company, tenders
from quantix.documents import library
from quantix.office import records, tools
from quantix.office.models import ENGINEER, TEAM, Message, Staff
from quantix.office.tools import Turn

STEP_LIMIT = 12  # model requests in one turn; the person picks up again on their next turn

RULES = """How the office works:
- You talk only through your tools. Anything else you write is not seen by anyone.
- The engineer decides scope, the method of measurement, quantities, rates, subcontract and supplier choices, the
  final price and the release. When a decision is theirs, ask them with ask_engineer; never decide it for them.
- Every fact must come from the tender documents you have read. Cite them as "<document name>, page <n>".
  Never invent figures, dates or clauses. If the documents don't say, say that.
- Say what you think. If you disagree with the Manager, a colleague or the engineer, use raise_concern.
- Keep messages short and in plain construction English, even when the documents are in Arabic.
- When you have acted on everything new, stop."""

MANAGER_DUTIES = """You are the Tender Manager: you lead this tender for the engineer.
- Hire the people this particular tender needs, when it needs them, with hire. There is no standard team:
  choose roles from the actual work. Keep the team small.
- Give each person clear tasks with assign_task, check their results, and follow up.
- Keep the engineer informed in your chat with them (message_engineer): what you found, what is next, what you need.
- Bring the engineer's decisions to them with ask_engineer, one question at a time."""

STAFF_DUTIES = """You work for the Tender Manager.
- Work on your open tasks. When one is done, call complete_task with a clear result and the pages you used.
- If something blocks you or you need a decision, say so in the team room and name the Manager."""


class Persona(BaseModel):
    """A generated person for the office."""

    name: str = Field(description="Full name that suits the region of the tender")
    discipline: str
    experience_years: int
    background: str = Field(description="Two sentences about their career")
    working_style: str = Field(description="How they work, in one or two sentences")
    opinions: str = Field(description="Professional views they hold and will voice")
    voice: str = Field(description="How they speak and write")


def instructions(member: Staff, autonomous: bool) -> str:
    p = member.profile
    who = (
        f"You are {member.name}, {member.role} in a construction tendering office. "
        f"{p.get('discipline', '')}, {p.get('experience_years', '')} years. {p.get('background', '')}\n"
        f"How you work: {p.get('working_style', '')}\nWhat you believe: {p.get('opinions', '')}\n"
        f"How you speak: {p.get('voice', '')}"
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
    tasks = records.open_tasks(session, member)
    if tasks:
        parts.append("Your open tasks:\n" + "\n".join(f"- {t.id}: {t.title}. {t.brief}" for t in tasks))
    waiting = records.decisions(session, member.tender_id, waiting_only=True)
    if waiting:
        parts.append("Waiting for the engineer:\n" + "\n".join(f"- {d.title}" for d in waiting))
    shown = {m.id for m in new}
    earlier = [m for m in records.messages(session, member.tender_id, TEAM, limit=12) if m.id not in shown]
    if earlier:
        parts.append("Earlier in the team room:\n" + "\n".join(line(m) for m in earlier))
    parts.append("New for you:\n" + ("\n".join(line(m) for m in new) or "- Nothing new; carry on with your tasks."))
    return "\n\n".join(parts)


async def run_turn(model: Model, turn: Turn, member: Staff, prompt: str, autonomous: bool) -> None:
    agent = Agent(
        model,
        deps_type=Turn,
        instructions=instructions(member, autonomous),
        tools=tools.MANAGER if member.is_manager else tools.STAFF,
        retries=2,
    )
    async with agent.iter(prompt, deps=turn, usage_limits=UsageLimits(request_limit=STEP_LIMIT)) as run:
        async for _node in run:
            if turn.stop.is_set():
                raise tools.Stopped()


async def create_persona(model: Model, brief: str) -> Persona:
    agent = Agent(model, output_type=Persona, instructions="You create believable people for a construction office.")
    result = await agent.run(brief, usage_limits=UsageLimits(request_limit=2))
    return result.output
