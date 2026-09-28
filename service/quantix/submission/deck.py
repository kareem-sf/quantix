"""The tender summary deck, for the firm's own bid review; it is never sent to the client. Every figure comes from
Quantix's records: the price and its build-up, the bills, where the money sits, the programme, the subcontract and
supply choices, the open points and where the submission stands."""

from decimal import Decimal
from pathlib import Path

from pptx import Presentation
from pptx.chart.data import CategoryChartData
from pptx.dml.color import RGBColor
from pptx.enum.chart import XL_CHART_TYPE, XL_LABEL_POSITION
from pptx.enum.text import PP_ALIGN
from pptx.oxml.ns import qn
from pptx.util import Emu, Inches, Pt
from sqlalchemy.orm import Session

from quantix.boq import records as boq
from quantix.estimate import records as estimate
from quantix.review import audit
from quantix.review import records as reviews
from quantix.subcontract import records as subcontract
from quantix.subcontract.models import Company, Quote
from quantix.submission import package, records
from quantix.submission.content import Letterhead

FONT = "Arial"
INK = RGBColor(0x1F, 0x29, 0x37)
MUTED = RGBColor(0x6B, 0x72, 0x80)
ACCENT = RGBColor(0xC2, 0x41, 0x0C)
SHADE = RGBColor(0xF1, 0xF2, 0xF4)
RULE = "D5D8DD"
WIDTH, HEIGHT = Inches(13.333), Inches(7.5)
LEFT = Inches(0.7)


def _money(value: Decimal | None) -> str:
    return "" if value is None else f"{value:,.2f}"


class _Deck:
    def __init__(self, head: Letterhead):
        self.head = head
        self.deck = Presentation()
        self.deck.slide_width, self.deck.slide_height = WIDTH, HEIGHT

    def slide(self, title: str):
        slide = self.deck.slides.add_slide(self.deck.slide_layouts[6])  # blank
        self.text(slide, LEFT, Inches(0.5), WIDTH - 2 * LEFT, Inches(0.7), title, 26, bold=True)
        number = len(self.deck.slides)
        footer = f"{self.head.tender} · Tender summary · Internal"
        self.text(slide, LEFT, HEIGHT - Inches(0.55), Inches(10), Inches(0.3), footer, 9, color=MUTED)
        self.text(slide, WIDTH - LEFT - Inches(1), HEIGHT - Inches(0.55), Inches(1), Inches(0.3), str(number), 9,
                  color=MUTED, align=PP_ALIGN.RIGHT)  # fmt: skip
        return slide

    def text(self, slide, x, y, w, h, text: str, size: float, bold=False, color=INK, align=PP_ALIGN.LEFT):
        box = slide.shapes.add_textbox(x, y, w, h)
        frame = box.text_frame
        frame.word_wrap = True
        for index, line in enumerate(text.split("\n")):
            paragraph = frame.paragraphs[0] if index == 0 else frame.add_paragraph()
            paragraph.alignment = align
            run = paragraph.add_run()
            run.text = line
            run.font.name, run.font.size, run.font.bold, run.font.color.rgb = FONT, Pt(size), bold, color
        return box

    def stat(self, slide, x, y, label: str, value: str, accent=False) -> None:
        self.text(slide, x, y, Inches(3.8), Inches(0.35), label, 12, color=MUTED)
        self.text(slide, x, y + Inches(0.35), Inches(3.8), Inches(0.6), value, 26, bold=True,
                  color=ACCENT if accent else INK)  # fmt: skip

    def table(self, slide, x, y, widths: list[float], header: list[str], rows: list[list[str]], right: set[int],
              bold_last: bool = False) -> None:  # fmt: skip
        shape = slide.shapes.add_table(len(rows) + 1, len(header), x, y, Inches(sum(widths)), Inches(0.35))
        table = shape.table
        table.first_row, table.horz_banding = True, False
        for column, width in enumerate(widths):
            table.columns[column].width = Inches(width)
        for r, values in enumerate([header, *rows]):
            last = bold_last and r == len(rows)
            for column, value in enumerate(values):
                cell = table.cell(r, column)
                cell.fill.solid()
                cell.fill.fore_color.rgb = SHADE if r == 0 else RGBColor(0xFF, 0xFF, 0xFF)
                cell.margin_top = cell.margin_bottom = Emu(45720)
                frame = cell.text_frame
                frame.word_wrap = True
                paragraph = frame.paragraphs[0]
                paragraph.alignment = PP_ALIGN.RIGHT if column in right else PP_ALIGN.LEFT
                run = paragraph.add_run()
                run.text = value
                run.font.name, run.font.size = FONT, Pt(12)
                run.font.bold = r == 0 or last
                run.font.color.rgb = INK
                _underline(cell)


def _underline(cell) -> None:
    """A thin grey rule under the cell, the table's only lines."""
    properties = cell._tc.get_or_add_tcPr()
    for side in ("a:lnL", "a:lnR", "a:lnT"):
        line = properties.makeelement(qn(side), {"w": "0"})
        line.append(line.makeelement(qn("a:noFill"), {}))
        properties.append(line)
    line = properties.makeelement(qn("a:lnB"), {"w": "9525"})
    fill = line.makeelement(qn("a:solidFill"), {})
    fill.append(fill.makeelement(qn("a:srgbClr"), {"val": RULE}))
    line.append(fill)
    properties.append(line)


def write(session: Session, home: Path, tender_id: str, head: Letterhead, facts: list[tuple[str, str]], path: Path):
    summary = estimate.summary(session, tender_id)
    markups = estimate.current_markups(session, tender_id)
    items = boq.items(session, tender_id)
    net: dict[str, Decimal] = {}
    for item in items:
        rate = estimate.current_rate(session, item.id)
        if rate is not None and item.quantity is not None:
            net[item.id] = estimate.money(item.quantity * estimate.rate_of(rate))
    currency = summary.currency or ""
    deck = _Deck(head)

    # The title
    slide = deck.deck.slides.add_slide(deck.deck.slide_layouts[6])
    deck.text(slide, LEFT, Inches(2.3), Inches(11), Inches(0.5), "Tender summary · for the bid review", 16,
              color=MUTED)  # fmt: skip
    deck.text(slide, LEFT, Inches(2.8), Inches(11.5), Inches(1.6), head.tender, 36, bold=True)
    lines = [f"{label}: {value}" for label, value in facts if label != "Tender"]
    deck.text(slide, LEFT, Inches(4.6), Inches(11), Inches(1), "\n".join(lines), 14, color=MUTED)
    deck.text(slide, LEFT, HEIGHT - Inches(1), Inches(11), Inches(0.4),
              "Internal: the firm's own figures. Not for the client.", 11, color=ACCENT)  # fmt: skip

    # The price
    slide = deck.slide("The price")
    deck.stat(slide, LEFT, Inches(1.5), f"Total before VAT, {currency}".strip(", "), _money(summary.total), True)
    if summary.vat is not None:
        deck.stat(slide, LEFT + Inches(4.1), Inches(1.5), "VAT", _money(summary.vat))
        deck.stat(slide, LEFT + Inches(8.2), Inches(1.5), "Total with VAT", _money(summary.total_with_vat))
    rows = [["Net cost of the work", _money(summary.net), _share(summary.net, summary.total)],
            ["Preliminaries", _money(summary.preliminaries), _share(summary.preliminaries, summary.total)]]  # fmt: skip
    if markups is not None:
        rows += [
            [
                f"Overheads, {markups.overheads:.1%}",
                _money(summary.overheads),
                _share(summary.overheads, summary.total),
            ],
            [f"Profit, {markups.profit:.1%}", _money(summary.profit), _share(summary.profit, summary.total)],
        ]
    if summary.adjustment:
        rows.append(["Adjustment", _money(summary.adjustment), _share(summary.adjustment, summary.total)])
    rows.append(["Total before VAT", _money(summary.total), "100%"])
    deck.table(slide, LEFT, Inches(3.1), [6.4, 2.6, 1.6], ["Build-up", currency, "Share"], rows, {1, 2}, True)

    # By bill
    slide = deck.slide("The price by bill")
    bills = []
    for name, lines_ in package.grouped(session, items):
        amount = sum((net.get(i.id, Decimal(0)) for i in lines_), Decimal(0))
        priced = sum(1 for i in lines_ if i.id in net)
        bills.append([name, f"{priced} of {len(lines_)}", _money(amount), _share(amount, summary.net)])
    bills.append(["All bills", f"{len(net)} of {len(items)}", _money(summary.net), "100%"])
    deck.table(slide, LEFT, Inches(1.5), [5.6, 1.8, 2.6, 1.6], ["Bill", "Lines priced", "Net cost", "Share"], bills,
               {1, 2, 3}, True)  # fmt: skip
    deck.text(slide, LEFT, Inches(1.5) + Inches(0.42) * (len(bills) + 1) + Inches(0.3), Inches(11), Inches(0.4),
              "Net cost: quantity × rate, before preliminaries, overheads and profit.", 11, color=MUTED)  # fmt: skip

    # Where the money is
    slide = deck.slide("Where the money is")
    ranked = sorted(items, key=lambda i: net.get(i.id, Decimal(0)), reverse=True)[:8]
    if ranked and summary.net:
        chart_data = CategoryChartData()
        chart_data.categories = [f"{boq.reference(i)} {_clip(i.description, 30)}" for i in reversed(ranked)]
        chart_data.add_series("Net cost", [float(net.get(i.id, 0)) for i in reversed(ranked)])
        chart = slide.shapes.add_chart(XL_CHART_TYPE.BAR_CLUSTERED, LEFT, Inches(1.4), Inches(11.9), Inches(5.2),
                                       chart_data).chart  # fmt: skip
        chart.has_legend = chart.has_title = False
        chart.font.name, chart.font.size = FONT, Pt(11)
        plot = chart.plots[0]
        plot.gap_width = 60
        plot.has_data_labels = True
        plot.data_labels.number_format, plot.data_labels.number_format_is_linked = "#,##0", False
        plot.data_labels.position = XL_LABEL_POSITION.OUTSIDE_END
        series = plot.series[0]
        series.format.fill.solid()
        series.format.fill.fore_color.rgb = INK
        chart.value_axis.visible = False
        chart.value_axis.has_major_gridlines = False
        top = sum((net.get(i.id, Decimal(0)) for i in ranked), Decimal(0))
        note = f"These {len(ranked)} lines carry {_share(top, summary.net)} of the net cost."
        deck.text(slide, LEFT, Inches(6.55), Inches(11), Inches(0.35), note, 11, color=MUTED)

    # The programme
    schedule = _schedule(session, tender_id)
    if schedule is not None:
        slide = deck.slide("The programme")
        deck.stat(slide, LEFT, Inches(1.5), "Overall duration", f"{schedule['overall_days']} working days", True)
        longest = sorted(schedule["lines"], key=lambda line: line["days"], reverse=True)[:6]
        rows = [[line["reference"], str(line["days"])] for line in longest]
        deck.table(slide, LEFT, Inches(2.9), [6.4, 1.6], ["Longest activities", "Days"], rows, {1})

    # Subcontract and supply
    packages = subcontract.packages(session, tender_id)
    if packages:
        slide = deck.slide("Subcontract and supply")
        rows = []
        for found in packages:
            received = subcontract.quotes(session, found.id)
            chosen = session.get(Quote, found.selected_quote_id) if found.selected_quote_id else None
            choice = session.get(Company, chosen.company_id).name if chosen else "Priced at our own rates"
            rows.append([found.name, found.kind.capitalize(), str(len(received)), choice])
        deck.table(slide, LEFT, Inches(1.5), [4.2, 1.8, 1.4, 4.4], ["Package", "Kind", "Quotes", "Choice"], rows, {2})

    # Open points
    slide = deck.slide("Open points")
    requirements = records.requirements(session, tender_id)
    queries = [r.title for r in requirements if r.section.strip().lower() == "correspondence"]
    settled = reviews.accepted(session, tender_id)
    found = audit.findings(session, home, tender_id)
    points = [["Client query", title] for title in queries]
    points += [["Blocker", f.message] for f in found if f.severity == audit.BLOCKER]
    points += [["Accepted warning", f"{f.message} Reason: {settled[f.key].reason}"] for f in found
               if f.severity == audit.WARNING and f.key in settled]  # fmt: skip
    points += [["Warning", f.message] for f in found if f.severity == audit.WARNING and f.key not in settled]
    if points:
        rows = [[kind, _clip(point, 240)] for kind, point in points[:9]]
        deck.table(slide, LEFT, Inches(1.5), [2.4, 9.5], ["", "Point"], rows, set())
    else:
        deck.text(slide, LEFT, Inches(1.6), Inches(11), Inches(0.5), "Nothing is open.", 16)

    # The submission
    slide = deck.slide("The submission")
    states = {"ready": "Ready", "review": "Waiting for you", "manager": "With the Tender Manager", "missing": "Missing"}
    rows = [[r.section, _clip(r.title, 90), states[records.state(session, r)]] for r in requirements]
    ready = sum(1 for r in requirements if records.state(session, r) == "ready")
    complete = ready == len(requirements)
    deck.stat(slide, LEFT, Inches(1.4), "Checklist", f"{ready} of {len(requirements)} ready", complete)
    if rows:
        deck.table(slide, LEFT, Inches(2.8), [2.2, 7.6, 2.1], ["Section", "Requirement", "State"], rows[:10], set())
    deck.deck.save(path)


def _clip(text: str, limit: int) -> str:
    """At most `limit` characters, cut at a word."""
    return text if len(text) <= limit else text[:limit].rsplit(" ", 1)[0].rstrip(",;:") + "…"


def _share(part: Decimal, whole: Decimal) -> str:
    return f"{part / whole:.1%}" if whole else ""


def _schedule(session: Session, tender_id: str) -> dict | None:
    """The newest approved work schedule."""
    for requirement in records.requirements(session, tender_id):
        current = records.current_draft(session, requirement.id)
        if current is not None and current.schedule and current.status in ("approved", "office_approved"):
            return current.schedule
    return None
