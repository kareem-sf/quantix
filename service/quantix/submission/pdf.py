"""Documents as PDF, drawn from the same blocks as the Word files and in the same house style, and the whole
submission as one PDF with a cover and contents. Noto Sans with its Arabic companion, shaped, so Arabic text reads
right."""

from pathlib import Path

from fpdf import FPDF, FontFace
from fpdf.enums import TableBordersLayout

from quantix.submission.content import Document, Heading, Letterhead, ListBlock, Paragraph, Run, Table, is_arabic

FONTS = Path(__file__).parent / "fonts"
INK = (31, 41, 55)
MUTED = (107, 114, 128)
RULE = (201, 205, 211)
SHADE = (241, 242, 244)
MARGIN = 20
SIZES = {1: 13, 2: 11.5, 3: 10.5}


class _Pages(FPDF):
    def __init__(self, head: Letterhead):
        super().__init__(format="A4")
        self.head, self.title_now, self.cover_page = head, "", 0
        self.add_font("Noto", "", FONTS / "NotoSans-Regular.ttf")
        self.add_font("Noto", "B", FONTS / "NotoSans-Bold.ttf")
        self.add_font("Arabic", "", FONTS / "NotoSansArabic-Regular.ttf")
        self.add_font("Arabic", "B", FONTS / "NotoSansArabic-Bold.ttf")
        self.set_fallback_fonts(["Arabic"])
        self.set_text_shaping(True)
        self.set_margins(MARGIN, 32, MARGIN)
        self.set_auto_page_break(True, margin=20)
        self.set_draw_color(*RULE)
        self.set_text_color(*INK)

    def header(self):
        if self.page_no() == self.cover_page:
            return
        head = self.head
        if head.logo is not None:
            self.image(head.logo, x=self.w - MARGIN - 36, y=10, w=36, h=13, keep_aspect_ratio=True)
        self.set_xy(MARGIN, 10)
        self._text(head.firm or head.tender, 9.5, bold=True)
        for line in head.lines:
            self._text(line, 8, color=MUTED)
        self.line(MARGIN, 26, self.w - MARGIN, 26)
        self.set_y(32)

    def footer(self):
        if self.page_no() == self.cover_page:
            return
        self.set_y(-14)
        self.set_font("Noto", size=8)
        self.set_text_color(*MUTED)
        width = self.w - 2 * MARGIN
        self.cell(width * 0.75, 5, f"{self.head.tender} · {self.title_now}")
        self.cell(width * 0.25, 5, f"Page {self.page_no()} of {{nb}}", align="R")
        self.set_text_color(*INK)

    def _text(self, text: str, size: float, bold: bool = False, color=INK, align: str = "L") -> None:
        self.set_font("Noto", "B" if bold else "", size)
        self.set_text_color(*color)
        self.multi_cell(0, size * 0.48, text, align="R" if is_arabic(text) else align, new_x="LMARGIN", new_y="NEXT")
        self.set_text_color(*INK)


def _markdown(runs: list[Run]) -> str:
    """Runs as fpdf2's own markup: bold between double asterisks."""
    return "".join(f"**{r.text}**" if r.bold and r.text.strip() else r.text for r in runs)


def _paragraph(pdf: _Pages, runs: list[Run]) -> None:
    pdf.set_font("Noto", size=10)
    text = "".join(r.text for r in runs)
    pdf.multi_cell(0, 5, _markdown(runs), markdown=True, align="R" if is_arabic(text) else "L",
                   new_x="LMARGIN", new_y="NEXT")  # fmt: skip
    pdf.ln(1.6)


def _list(pdf: _Pages, block: ListBlock) -> None:
    counts: dict[int, int] = {0: block.start - 1}
    pdf.set_font("Noto", size=10)
    for item in block.items:
        counts = {level: n for level, n in counts.items() if level <= item.level}
        counts[item.level] = counts.get(item.level, 0) + 1
        marker = f"{counts[item.level]}." if item.numbered else ("•" if item.level == 0 else "–")
        pdf.set_x(MARGIN + 2 + item.level * 7)
        pdf.cell(6, 5, marker)
        align = "R" if is_arabic("".join(r.text for r in item.runs)) else "L"  # fpdf2 would justify
        pdf.multi_cell(0, 5, _markdown(item.runs), markdown=True, align=align, new_x="LMARGIN", new_y="NEXT")
        pdf.ln(0.8)
    pdf.ln(1.2)


def _table(pdf: _Pages, block: Table, facts: bool = False) -> None:
    numeric = block.numeric
    align = tuple("RIGHT" if column in numeric else "LEFT" for column in range(len(block.header)))
    pdf.set_font("Noto", size=8.5)
    with pdf.table(
        col_widths=tuple(block.widths or [1.0] * len(block.header)),
        text_align=align,
        borders_layout=TableBordersLayout.HORIZONTAL_LINES,
        headings_style=FontFace(emphasis="BOLD", fill_color=SHADE),
        first_row_as_headings=not facts,
        line_height=4.6,
        padding=(1.2, 1.6),
    ) as table:
        if not facts:
            table.row(block.header)
        for values in block.rows:
            row = table.row()
            for column, value in enumerate(values):
                label = facts and column == 0
                row.cell(value, style=FontFace(color=MUTED, fill_color=SHADE) if label else None)
        for values in block.totals:
            table.row(values, style=FontFace(emphasis="BOLD"))
    pdf.ln(4)


def _document(pdf: _Pages, document: Document, section: bool = False, new_page: bool = True) -> None:
    if new_page:
        pdf.add_page()
    pdf.title_now = document.title  # after the page break, so the page before keeps its own title in the footer
    if section:
        pdf.start_section(document.title)
    pdf._text(document.title, 18, bold=True)
    pdf.ln(2)
    if document.facts:
        _table(pdf, Table(["", ""], [list(f) for f in document.facts], widths=[1, 3]), facts=True)
    for block in document.blocks:
        if isinstance(block, Heading):
            pdf.ln(3 if block.level == 1 else 2)
            pdf._text(block.text, SIZES[block.level], bold=True)
            pdf.ln(1)
        elif isinstance(block, Paragraph):
            _paragraph(pdf, block.runs)
        elif isinstance(block, ListBlock):
            _list(pdf, block)
        elif isinstance(block, Table):
            _table(pdf, block)


def write(document: Document, head: Letterhead, path: Path) -> None:
    pdf = _Pages(head)
    _document(pdf, document)
    pdf.output(path)


def write_package(documents: list[Document], head: Letterhead, cover: list[tuple[str, str]], path: Path) -> None:
    """The whole submission as one PDF: a cover, the contents with their pages, then each document in turn."""
    pdf = _Pages(head)
    pdf.cover_page = 1
    pdf.add_page()
    pdf.set_y(24)
    if head.logo is not None:
        pdf.image(head.logo, x=MARGIN, y=24, h=18, keep_aspect_ratio=True)
        pdf.set_y(46)
    pdf._text(head.firm, 11, bold=True)
    for line in head.lines:
        pdf._text(line, 9, color=MUTED)
    pdf.set_y(92)
    pdf._text("Tender submission", 12, color=MUTED)
    pdf._text(head.tender, 24, bold=True)
    pdf.ln(6)
    _table(pdf, Table(["", ""], [list(f) for f in cover], widths=[1, 3]), facts=True)
    pdf.add_page()
    pdf.title_now = "Contents"
    pdf._text("Contents", 18, bold=True)
    pdf.ln(2)
    pdf.insert_toc_placeholder(_contents, pages=1)  # it breaks the page itself
    for index, document in enumerate(documents):
        _document(pdf, document, section=True, new_page=index > 0)
    pdf.output(path)


def _contents(pdf: _Pages, outline) -> None:
    pdf.set_font("Noto", size=10)
    width = pdf.w - 2 * MARGIN
    for entry in outline:
        pdf.set_x(MARGIN)
        pdf.cell(width - 20, 7, entry.name)
        pdf.cell(20, 7, str(entry.page_number), align="R", new_x="LMARGIN", new_y="NEXT")
        pdf.line(MARGIN, pdf.get_y(), pdf.w - MARGIN, pdf.get_y())
