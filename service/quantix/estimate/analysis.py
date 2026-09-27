"""Figures Quantix works out about the price on request: where the money is, what a change would do to the price
without changing any record, and how a rate compares with the firm's library, its earlier tenders, similar lines
and the quotes. The model asks; Quantix computes."""

import re
from datetime import date
from decimal import Decimal

from pydantic import BaseModel, Field
from sqlalchemy.orm import Session

from quantix import company
from quantix.boq import records as boq
from quantix.boq.models import BoqItem
from quantix.core.calculate import plain
from quantix.documents.arabic import searchable
from quantix.estimate import records as estimate
from quantix.estimate.models import Rate
from quantix.subcontract import records as subcontract
from quantix.subcontract.models import Company

STALE_MONTHS = 12  # a library rate older than this is flagged: check it is still current before relying on it
TOP_SHARE = Decimal("0.8")  # the lines listed as making up most of the price, until they reach this share
_FILLER = {"and", "the", "for", "with", "including", "incl", "not", "less", "than", "of", "to", "up", "in", "on", "a"}


def _percent(part: Decimal, whole: Decimal) -> str:
    return f"{part / whole:.1%}" if whole else "-"


def _priced(session: Session, tender_id: str) -> list[tuple[BoqItem, Rate, Decimal]]:
    found = []
    for item in boq.items(session, tender_id):
        rate = estimate.current_rate(session, item.id)
        if rate is not None and item.quantity is not None:
            found.append((item, rate, estimate.money(item.quantity * estimate.rate_of(rate))))
    return found


def breakdown(session: Session, tender_id: str) -> str:
    """Where the money is: the net by bill, the lines that make up most of it, and the kinds of cost in the
    build-ups."""
    s = estimate.summary(session, tender_id)
    priced = _priced(session, tender_id)
    if not priced:
        return "Nothing is priced yet."
    sections: dict[str, Decimal] = {}
    kinds = {kind: Decimal(0) for kind in (*estimate.KINDS, "unit rates")}
    for item, rate, amount in priced:
        sections[item.section or "(no bill)"] = sections.get(item.section or "(no bill)", Decimal(0)) + amount
        if rate.lines:
            for line in rate.lines:
                kinds[line["kind"]] += estimate.money(item.quantity * estimate.line_cost(line))
        else:
            kinds["unit rates"] += amount
    lines = [f"Net {s.net:,} {s.currency}; with markups {s.total:,} (Quantix's figures).", "By bill:"]
    lines += [f"- {name}: {value:,} ({_percent(value, s.net)})" for name, value in sorted(sections.items())]
    lines.append("The lines that make up most of the net:")
    running = Decimal(0)
    for item, _, amount in sorted(priced, key=lambda p: p[2], reverse=True):
        if s.net and running / s.net >= TOP_SHARE:
            break
        running += amount
        lines.append(f"- {boq.reference(item)} {item.description[:60]}: {amount:,} ({_percent(amount, s.net)})")
    lines.append("By kind of cost, from the build-ups:")
    lines += [f"- {kind}: {value:,} ({_percent(value, s.net)})" for kind, value in kinds.items() if value]
    if s.unpriced:
        lines.append(f"Not priced yet: {len(s.unpriced)} lines.")
    return "\n".join(lines)


class Change(BaseModel):
    """One change to try on the price. Nothing is saved."""

    item: str | None = Field(default=None, description='A BOQ line, e.g. "Earthwork / C.1.2"; leave out for every line')
    resource: str | None = Field(default=None, description="Words in the build-up lines to change, e.g. 'fill'")
    kind: str | None = Field(default=None, description="labour, plant, material or subcontract")
    factor: Decimal = Field(gt=0, description="What to multiply those costs by: 1.2 is 20% more, 0.9 is 10% less")


def _hits(line: dict, change: Change) -> bool:
    return (not change.kind or line["kind"] == change.kind) and (
        not change.resource or all(w in searchable(line["resource"]) for w in searchable(change.resource).split())
    )


def _new_rate(rate: Rate, changes: list[Change]) -> Decimal:
    """The rate with the changes that apply to it: a unit rate moves only with a change to the whole rate."""
    if not rate.lines:
        value = estimate.rate_of(rate)
        for change in changes:
            value *= change.factor if not (change.resource or change.kind) else 1
        return estimate.money(value)
    total = Decimal(0)
    for line in rate.lines:
        cost = estimate.line_cost(line)
        for change in changes:
            cost *= change.factor if _hits(line, change) else 1
        total += cost
    return estimate.money(total)


def what_if(session: Session, tender_id: str, changes: list[Change]) -> str:
    """The price if these changes were made, against the price now. No record changes."""
    s = estimate.summary(session, tender_id)
    targets = {c.item: boq.find_item(session, tender_id, c.item).id for c in changes if c.item}
    net, moved = Decimal(0), []
    for item, rate, amount in _priced(session, tender_id):
        applying = [c for c in changes if not c.item or targets[c.item] == item.id]
        value = _new_rate(rate, applying)
        new = estimate.money(item.quantity * value)
        net += new
        if new != amount:
            moved.append((new - amount, item, estimate.rate_of(rate), value))
    if not moved:
        return "No priced line is affected: check the item, the resource words or the kind."
    markups = estimate.current_markups(session, tender_id)
    total = net
    if markups:
        overheads = estimate.money((net + s.preliminaries) * markups.overheads)
        profit = estimate.money((net + s.preliminaries + overheads) * markups.profit)
        total = net + s.preliminaries + overheads + profit + markups.adjustment
    lines = [
        f"Net {s.net:,} would be {net:,} ({net - s.net:+,}, {_percent(net - s.net, s.net)}).",
        f"With markups {s.total:,} would be {total:,} ({total - s.total:+,}). Nothing was changed.",
        f"{len(moved)} lines move, largest first:",
    ]
    for diff, item, old, new in sorted(moved, key=lambda m: abs(m[0]), reverse=True)[:12]:
        lines.append(f"- {boq.reference(item)}: rate {old} → {new} per {item.unit}, amount {diff:+,}")
    return "\n".join(lines)


def _words(description: str) -> str:
    words = [w for w in re.findall(r"[^\W_]+", description.lower()) if w not in _FILLER and len(w) > 2]
    return " ".join(words[:5])


def _versus(ours: Decimal, theirs: Decimal) -> str:
    if not theirs:
        return ""
    diff = (ours - theirs) / theirs
    if abs(diff) < Decimal("0.005"):
        return "the same as ours"
    return f"ours is {abs(diff):.0%} {'above' if diff > 0 else 'below'}"


def _months(dated: date) -> int:
    today = date.today()
    return (today.year - dated.year) * 12 + today.month - dated.month


def compare_rate(session: Session, tender_id: str, reference: str) -> str:
    """A line's rate beside the firm's library, its earlier tenders, similar lines here and any quote for it."""
    item = boq.find_item(session, tender_id, reference)
    rate = estimate.current_rate(session, item.id)
    if rate is None:
        return f"{boq.reference(item)} isn't priced yet."
    ours = estimate.rate_of(rate)
    words = _words(item.description)
    unit = searchable(item.unit)
    lines = [f"{boq.reference(item)} {item.description[:80]}: ours {ours} per {item.unit} ({rate.basis})."]
    library = [r for r in estimate.library(session, words) if r.kind == "unit_rate" and searchable(r.unit) == unit]
    for r in library[:5]:
        age = _months(r.dated)
        stale = f", {age} months old: check it is still current" if age > STALE_MONTHS else ""
        where = f"{r.name[:60]} {plain(r.rate)} per {r.unit}, dated {r.dated:%b %Y}{stale}"
        lines.append(f"- Library: {where}: {_versus(ours, r.rate)}")
    for p in [p for p in company.past_rates(session, tender_id, words) if searchable(p.unit) == unit][:5]:
        where = f"{p.tender} ({p.outcome}, {p.dated:%b %Y}): {p.item} {p.rate} per {p.unit}"
        lines.append(f"- {where}: {_versus(ours, p.rate)}")
    own = set(words.split())
    for other in boq.items(session, tender_id):
        other_rate = estimate.current_rate(session, other.id)
        shared = own & set(_words(other.description).split())
        if other.id != item.id and other_rate and searchable(other.unit) == unit and len(shared) >= 2:
            value = estimate.rate_of(other_rate)
            lines.append(f"- Here: {boq.reference(other)} {other.description[:50]} {value}: {_versus(ours, value)}")
    for package in subcontract.packages(session, tender_id):
        if item.id in package.items:
            for quote in subcontract.quotes(session, package.id):
                for line in quote.lines:
                    if line["boq_item_id"] == item.id:
                        firm = session.get(Company, quote.company_id).name
                        lines.append(f"- Quote from {firm}: {line['rate']}: {_versus(ours, Decimal(line['rate']))}")
    if len(lines) == 1:
        lines.append("Nothing in the library, earlier tenders, this tender or the quotes to compare it with.")
    return "\n".join(lines)
