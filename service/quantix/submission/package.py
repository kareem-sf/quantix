"""The submission's documents as content, from Quantix's own records: the office's approved drafts, the work
programme from its computed durations, and the priced bill of quantities from Quantix's figures."""

import re
from datetime import datetime
from decimal import Decimal
from pathlib import Path

from sqlalchemy.orm import Session

from quantix import company
from quantix.boq.models import BoqItem
from quantix.documents.models import Document as Source
from quantix.estimate import records as estimate
from quantix.submission import records
from quantix.submission.content import Document, Heading, Letterhead, Paragraph, Run, Table, from_markdown
from quantix.submission.models import Draft

_DURATION = re.compile(
    r"^- (?P<reference>.+?), (?P<description>.*): (?P<quantity>[\d,.]+) (?P<unit>\S+) at (?P<output>[\d,.]+) \S+ a day "
    r"× (?P<crews>\d+) crews? = (?P<days>\d+) days?$"
)


def letterhead(home: Path, tender: str) -> Letterhead:
    firm = company.profile(home)
    registration = " · ".join(
        f"{label} {value}" for label, value in (("CR", firm.cr_number), ("VAT", firm.vat_number)) if value
    )
    lines = [line for line in (*firm.address.splitlines(), registration) if line.strip()]
    return Letterhead(firm.name, lines, firm.logo, tender)


def facts(head: Letterhead, now: datetime) -> list[tuple[str, str]]:
    """The title block every document opens with."""
    found = [("Tender", head.tender)]
    if head.firm:
        found.append(("Submitted by", head.firm))
    return [*found, ("Date", f"{now:%d %B %Y}".lstrip("0"))]


def _amount(value: Decimal | None) -> str:
    return "" if value is None else f"{value:,.2f}"


def _quantity(value: Decimal | None) -> str:
    return "" if value is None else f"{value:,.3f}".rstrip("0").rstrip(".")


def bill(session: Session, item: BoqItem) -> str:
    """The client's bill a line comes from: the name of the workbook or document it was read from."""
    return Path(session.get(Source, item.document_id).name).stem


def draft(session: Session, draft: Draft, opening: list[tuple[str, str]]) -> Document:
    """An approved draft as a document: the office's Markdown read into headings, lists and tables. A work schedule
    is laid out from the durations Quantix worked out, so its figures come from Quantix, not from the text."""
    if draft.schedule:
        found = _programme(session, draft, opening)
        if found is not None:
            return found
    return Document(draft.title, from_markdown(draft.body, draft.title), opening)


def _programme(session: Session, draft: Draft, opening: list[tuple[str, str]]) -> Document | None:
    lines = draft.body.splitlines()
    durations = [(index, match) for index, line in enumerate(lines) if (match := _DURATION.match(line.strip()))]
    schedule = draft.schedule["lines"]
    if len(durations) != len(schedule):
        return None  # not the layout Quantix wrote: read it as the office's text
    rows = []
    for (_, match), line in zip(durations, schedule, strict=True):
        item = session.get(BoqItem, line["item_id"])
        rows.append(
            [
                bill(session, item),
                item.item or f"Row {records.row_of(item.quote) or ''}".strip(),
                item.description,
                _quantity(item.quantity),
                item.unit,
                match["output"],
                match["crews"],
                str(line["days"]),
            ]
        )
    after = durations[-1][0] + 1
    sequence = "\n".join(line for line in lines[after:] if not line.startswith("Overall duration:")).strip()
    blocks = [
        Paragraph([Run("Each activity's duration comes from its bill quantity and the output assumed for each crew.")]),
        Heading("Durations"),
        Table(
            ["Bill", "Item", "Description", "Quantity", "Unit", "Output a day", "Crews", "Days"],
            rows,
            widths=[1.4, 0.8, 3.1, 1.0, 0.6, 1.0, 0.8, 0.65],
        ),
    ]
    if sequence:
        blocks += [Heading("Sequence"), *from_markdown(sequence)]
    overall = f"{draft.schedule['overall_days']} working days"
    blocks.append(Paragraph([Run("Overall duration: ", bold=True), Run(overall)]))
    return Document(draft.title, blocks, opening)


def priced(
    session: Session,
    items: list[BoqItem],
    rates: dict[str, Decimal],
    summary: estimate.Summary,
    opening: list[tuple[str, str]],
    spread: bool,
) -> Document:
    """The priced bill of quantities, bill by bill with a subtotal, then the summary with VAT. The total is the sum of
    the amounts shown, and VAT is worked out on it, so the document adds up as the client will check it."""
    blocks: list = [Paragraph([Run(basis(summary, spread))])]
    subtotals: list[tuple[str, Decimal]] = []
    for name, lines in grouped(session, items):
        subtotal, rows = Decimal(0), []
        for item in lines:
            rate = rates.get(item.id)
            amount = estimate.money(item.quantity * rate) if rate is not None else None
            subtotal += amount or Decimal(0)
            row = records.row_of(item.quote)
            rows.append([str(row or ""), item.item, item.description, item.unit, _quantity(item.quantity),
                         _amount(rate), _amount(amount)])  # fmt: skip
        blocks += [
            Heading(name),
            Table(
                ["Row", "Item", "Description", "Unit", "Quantity", "Rate", "Amount"],
                rows,
                widths=[0.6, 0.9, 4.2, 0.7, 1.2, 1.1, 1.5],
                totals=[["", "", f"Subtotal, {name}", "", "", "", _amount(subtotal)]],
            ),
        ]
        subtotals.append((name, subtotal))
    total = sum((s for _, s in subtotals), Decimal(0))
    rows = [[name, _amount(subtotal)] for name, subtotal in subtotals]
    if not spread and summary.total != summary.net:
        rows.append(["Preliminaries, overheads and profit", _amount(summary.total - summary.net)])
        total += summary.total - summary.net
    totals = [["Total before VAT" if summary.vat_rate is not None else "Total", _amount(total)]]
    if summary.vat_rate is not None:
        vat = estimate.money(total * summary.vat_rate)
        totals += [[vat_label(summary.vat_rate), _amount(vat)], ["Total with VAT", _amount(total + vat)]]
    blocks += [Heading("Summary"), Table(["", "Amount"], rows, widths=[4, 1.5], totals=totals)]
    return Document("Priced bill of quantities", blocks, opening)


def grouped(session: Session, items: list[BoqItem]) -> list[tuple[str, list[BoqItem]]]:
    """The lines bill by bill, each bill in the client's own row order."""
    bills: dict[str, list[BoqItem]] = {}
    for item in items:
        bills.setdefault(item.document_id, []).append(item)
    return [
        (bill(session, lines[0]), sorted(lines, key=lambda i: (i.page, records.row_of(i.quote) or 0, i.position)))
        for lines in bills.values()
    ]


def basis(summary: estimate.Summary, spread: bool) -> str:
    """What the prices are in, and what the rates include."""
    currency = summary.currency or "the tender currency"
    return f"Rates and amounts in {currency}" + (", including preliminaries, overheads and profit." if spread else ".")


def vat_label(rate: Decimal) -> str:
    return f"VAT {(rate * 100).normalize():f}%"
