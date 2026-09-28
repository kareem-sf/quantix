"""Which tools each person has in a turn. Everyone has a small core. The rest come in packs, one per kind of work:
the Tender Manager gives each hire the packs for their work, and those are loaded every turn; any other pack stays
hidden, as its name and one line, until the person loads it. The Manager's packs hold only what he needs to check
work, never tools that produce records. Fewer tools in view means a cheaper model picks the right one."""

from collections.abc import Callable
from dataclasses import dataclass, field

from pydantic_ai.capabilities import Toolset
from pydantic_ai.toolsets import FunctionToolset

from quantix.office import tools as t
from quantix.office.models import Staff


@dataclass
class Pack(Toolset[t.Turn]):
    """A kind of work: its tools, and how the office does that work."""

    method: str = ""

    def get_instructions(self) -> str:
        return self.method


@dataclass
class Work:
    about: str  # one line, shown while the pack is hidden
    method: str
    reads: list[Callable] = field(default_factory=list)  # for anyone who loads it
    produces: list[Callable] = field(default_factory=list)  # staff only
    leads: list[Callable] = field(default_factory=list)  # the Tender Manager only


WORK: dict[str, Work] = {
    "documents": Work(
        "Reading the package in depth: spreadsheets by rows, what changed between copies, the package map and how "
        "much of it has been read.",
        "Reading the package: read spreadsheets with read_sheet, a part at a time. When a newer copy of a document "
        "comes in, see what changed with compare_copies before relying on older work. Describe documents you have "
        "looked at with describe_documents, so the whole office and the engineer see the package map.",
        reads=[t.read_sheet, t.compare_copies, t.coverage],
        produces=[t.describe_documents],
    ),
    "boq": Work(
        "Entering the client's BOQ and the facts pricing depends on (method of measurement, currency, VAT).",
        "Entering the BOQ: enter lines exactly as the client's bill states them, with the page and a quote that has "
        "the item number and the quantity, 40 at a time. Give each line its bill as the section, so equal item "
        "numbers in different bills stay apart. Record the method of measurement, currency and VAT as facts from "
        "the clause that states them.",
        reads=[t.list_boq],
        produces=[t.propose_boq_items, t.propose_fact],
    ),
    "takeoff": Work(
        "Measuring quantities on drawings: scales, lengths, areas and counts, cut and fill from levels, and the "
        "takeoff against the BOQ.",
        "Measuring: set a sheet's scale from a printed dimension, and measure a second known dimension before you "
        "rely on it. Take each point from what you see: look at the sheet, then zoom in on each corner. Link each "
        "measurement to its BOQ line. Work out cut and fill from levels with earthwork_volumes, citing the levels.",
        reads=[t.find_on_page, t.takeoff_summary, t.earthwork_volumes],
        produces=[t.set_scale, t.measure],
    ),
    "drawings": Work(
        "CAD drawings (DWG and DXF): what their layers and blocks hold, rooms, takeoff from their own objects, "
        "checking them and the BOQ, and tender queries for missing items and conflicts.",
        "CAD drawings: start with drawing_overview. Set the units with set_drawing_units from what the drawing says. "
        "Work out what each layer and block is from what it holds, not from its name alone, and propose the layer "
        "map. Take off by rule with measure_drawing (count a block's copies, take the length or area of what is on "
        "a layer, or a room's area and perimeter); try each rule with query_drawing first, and never work out a "
        "quantity yourself. Check a drawing, and the BOQ, with find_problems. Raise a tender query for work that is "
        "drawn or specified but in no BOQ line or preamble, and for documents that disagree, with every source and "
        "which document governs; put repeats of one problem into one query.",
        reads=[t.drawing_overview, t.query_drawing, t.view_drawing, t.find_problems],
        produces=[t.set_drawing_units, t.measure_drawing, t.propose_layer_map, t.raise_query],
    ),
    "pricing": Work(
        "Pricing BOQ lines and markups: build-ups, the rate library, earlier tenders, market prices, how a rate "
        "compares, where the money is and what a change would do.",
        "Pricing: build a rate up from labour, plant, material and subcontract per unit of the line, with outputs "
        "and prices you state as your assumptions, or take it from a quote, the library or a market price you read. "
        "Check a rate against the library, earlier tenders, similar lines and quotes with check_rate before you "
        "propose it. Reuse an approved build-up for the same work with apply_buildup. Never work out an amount "
        "yourself: Quantix computes rates, amounts and totals.",
        reads=[
            t.priced_boq,
            t.estimate_summary,
            t.price_breakdown,
            t.what_if,
            t.check_rate,
            t.search_library,
            t.search_past_tenders,
        ],
        produces=[t.propose_rate, t.apply_buildup, t.propose_markups],
        leads=[t.suggest_library],
    ),
    "subcontract": Work(
        "Subcontract and supply packages: the directory of firms, enquiries, quotes, levelling and the recommendation.",
        "Subcontract and supply: group lines to price from outside into packages, draft enquiries for the engineer "
        "to send, record each quote from its pages with what it excludes, and recommend a quote from Quantix's "
        "levelling, the gaps and exclusions and your view of the firm.",
        reads=[t.list_packages, t.levelling, t.search_directory],
        produces=[t.add_company, t.create_package, t.draft_enquiry, t.record_quote, t.recommend_quote],
    ),
    "submission": Work(
        "The submission: the checklist of what the tender asks for, drafted documents, the work schedule and the "
        "priced BOQ's columns.",
        "The submission: list every document the tender asks the bidder to submit, each with its clause. Draft each "
        "for the client in client-ready words, from the tender documents and the approved figures, with signatures "
        "left as blanks. Draft the work schedule from the BOQ quantities with your outputs and crews.",
        reads=[t.list_requirements],
        produces=[t.add_requirements, t.draft_document, t.draft_work_schedule, t.set_pricing_columns],
    ),
}
# What files or finishes work, or tells someone about it: a turn that called none of these stopped silently
FILES_OR_SAYS = {tool.__name__ for w in WORK.values() for tool in w.produces} | {
    tool.__name__ for tool in (t.complete_task, t.withdraw, t.message_engineer, t.post_to_team, t.raise_concern)
}
# The Manager's hiring tools, loaded while the team is small and on request after that
TEAM = Work(
    "Hiring and releasing staff, and looking back through the conversation.",
    "The team: hire the people this tender needs, when it needs them, giving each the kinds of work they will do. "
    "Keep the team small and release people whose work is done.",
    reads=[t.search_conversation],
    leads=[t.hire, t.release],
)
SMALL_TEAM = 2  # staff below which the Manager's hiring tools stay loaded

EVERYONE: list[Callable] = [
    t.message_engineer,
    t.post_to_team,
    t.raise_concern,
    t.open_record,
    t.find_records,
    t.list_documents,
    t.search_documents,
    t.read_page,
    t.view_page,
    t.search_web,
    t.read_web_page,
    t.calculate,
    t.complete_task,
]
STAFF: list[Callable] = [*EVERYONE, t.precheck, t.withdraw]
MANAGER: list[Callable] = [
    *EVERYONE,
    t.what_changed,
    t.estimate_summary,
    t.review_queue,
    t.review,
    t.escalate,
    t.assign_task,
    t.ask_engineer,
    t.set_due_date,
    t.audit_tender,
]
# left out for an AI that can't read images
SEEING = {t.view_page, t.find_on_page, t.set_scale, t.measure, t.view_drawing}


def work_of(member: Staff) -> list[str]:
    """The kinds of work a person was hired for. Staff hired before packs existed do every kind."""
    chosen = [w for w in (member.profile or {}).get("work") or [] if w in WORK]
    return chosen or list(WORK)


def loadout(member: Staff, team_size: int, sees_images: bool) -> tuple[list[Callable], list[Pack]]:
    """The person's core tools, and their packs: loaded for their own work, hidden until loaded for the rest."""

    core = MANAGER if member.is_manager else STAFF

    def seen(tools: list[Callable]) -> list[Callable]:
        return [tool for tool in tools if sees_images or tool not in SEEING]

    def extra(tools: list[Callable]) -> list[Callable]:
        return [tool for tool in seen(tools) if tool not in core]  # a tool in view once, however it came

    if member.is_manager:
        packs = [
            Pack(
                toolset=FunctionToolset(extra([*w.reads, *w.leads])),
                id=name,
                description=f"Checking the team's work: {w.about[0].lower()}{w.about[1:]}",
                defer_loading=True,
                method=w.method,
            )
            for name, w in WORK.items()
            if w.reads or w.leads
        ]
        packs.append(
            Pack(
                toolset=FunctionToolset(extra([*TEAM.reads, *TEAM.leads])),
                id="team",
                description=TEAM.about,
                defer_loading=team_size >= SMALL_TEAM,
                method=TEAM.method,
            )
        )
        return seen(core), packs
    mine = work_of(member)
    packs = [
        Pack(
            toolset=FunctionToolset(extra([*w.reads, *w.produces])),
            id=name,
            description=w.about,
            defer_loading=name not in mine,
            method=w.method,
        )
        for name, w in WORK.items()
    ]
    return seen(core), packs


def every_tool() -> set[str]:
    """Every tool anyone can have, for checks."""
    found = {tool.__name__ for tool in [*STAFF, *MANAGER, *TEAM.reads, *TEAM.leads]}
    for w in WORK.values():
        found |= {tool.__name__ for tool in [*w.reads, *w.produces, *w.leads]}
    return found
