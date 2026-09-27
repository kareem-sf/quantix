"""What a document says, apart from the file it becomes. Word and PDF are drawn from the same blocks, so both carry
the same headings, lists and tables. The office writes drafts in Markdown; Quantix reads them into these blocks."""

import re
from dataclasses import dataclass, field
from pathlib import Path

from markdown_it import MarkdownIt

_ARABIC = re.compile(r"[؀-ۿݐ-ݿﭐ-﷿ﹰ-﻿]")
_NUMBER = re.compile(r"^[-+(]?[\d,]+(\.\d+)?\)?%?$")


@dataclass
class Run:
    text: str
    bold: bool = False


@dataclass
class Paragraph:
    runs: list[Run]


@dataclass
class Heading:
    text: str
    level: int = 1  # 1 to 3, under the document's own title


@dataclass
class Item:
    runs: list[Run]
    numbered: bool = False
    level: int = 0


@dataclass
class ListBlock:
    items: list[Item]
    start: int = 1  # the first number of a numbered list


@dataclass
class Table:
    header: list[str]
    rows: list[list[str]]
    widths: list[float] | None = None  # shares of the width; even when None
    totals: list[list[str]] = field(default_factory=list)  # subtotal and total rows, in bold

    @property
    def numeric(self) -> set[int]:
        """Columns that hold only numbers, aligned to the right."""
        found = set()
        for column in range(len(self.header)):
            cells = [row[column] for row in self.rows if column < len(row) and row[column].strip()]
            if cells and all(_NUMBER.match(c.strip()) for c in cells):
                found.add(column)
        return found


Block = Paragraph | Heading | ListBlock | Table


@dataclass
class Document:
    title: str
    blocks: list[Block]
    facts: list[tuple[str, str]] = field(default_factory=list)  # the title block: ("Tender", ...), ("Date", ...)


def is_arabic(text: str) -> bool:
    """Mostly Arabic, so it reads right to left."""
    letters = [c for c in text if c.isalpha()]
    return bool(letters) and sum(bool(_ARABIC.match(c)) for c in letters) * 2 > len(letters)


def plain(runs: list[Run]) -> str:
    return "".join(r.text for r in runs)


def from_markdown(text: str, title: str = "") -> list[Block]:
    """The office's draft as blocks: headings, paragraphs, bulleted and numbered lists, and tables. A heading that
    repeats the document's title is dropped, since the title is written above it anyway."""
    tokens = MarkdownIt("commonmark", {"breaks": True}).enable("table").parse(text)
    blocks: list[Block] = []
    lists: list[bool] = []  # the lists being read, outermost first: numbered or not
    current: ListBlock | None = None
    table: Table | None = None
    row: list[str] | None = None
    heading_level = 0
    for token in tokens:
        kind = token.type
        if kind in ("bullet_list_open", "ordered_list_open"):
            if not lists:  # a nested list continues its parent's block, one level in
                # "2. Time schedule" after a bulleted list starts a new list that counts on from 2
                current = ListBlock([], start=int(token.attrGet("start") or 1))
                blocks.append(current)
            lists.append(kind == "ordered_list_open")
        elif kind in ("bullet_list_close", "ordered_list_close"):
            lists.pop()
        elif kind == "heading_open":
            heading_level = int(token.tag[1])
        elif kind == "table_open":
            table = Table([], [])
            blocks.append(table)
        elif kind == "table_close":
            table = None
        elif kind == "tr_open":
            row = []
        elif kind == "tr_close" and table is not None and row is not None:
            if table.header:
                table.rows.append(row)
            else:
                table.header = row
            row = None
        elif kind == "inline":
            runs = _runs(token.children or [])
            if row is not None:
                row.append(plain(runs).strip())
            elif heading_level:
                heading = plain(runs).strip()
                if heading and heading.lower() != title.strip().lower():
                    blocks.append(Heading(heading, min(max(heading_level - 1, 1), 3)))
            elif lists and current is not None:
                current.items.append(Item(runs, numbered=lists[-1], level=len(lists) - 1))
            elif plain(runs).strip():
                blocks.append(Paragraph(runs))
        elif kind == "heading_close":
            heading_level = 0
        elif kind in ("fence", "code_block"):
            blocks += [Paragraph([Run(line)]) for line in token.content.splitlines() if line.strip()]
    first = blocks[0] if blocks else None
    if isinstance(first, Paragraph) and title.strip():
        opening, _, rest = plain(first.runs).strip().partition("\n")
        if opening.strip().lower() == title.strip().lower():  # the title again, as the draft's first line
            if rest.strip():
                blocks[0] = _without_first_line(first)
            else:
                blocks.pop(0)
    return blocks


def _without_first_line(paragraph: Paragraph) -> Paragraph:
    """The paragraph from its second line, keeping which runs are bold."""
    runs, dropped = [], False
    for run in paragraph.runs:
        if not dropped:
            if "\n" in run.text:
                dropped, text = True, run.text.split("\n", 1)[1]
                if text:
                    runs.append(Run(text, run.bold))
            continue
        runs.append(run)
    return Paragraph(runs)


def _runs(children) -> list[Run]:
    runs: list[Run] = []
    bold = 0
    for child in children:
        if child.type == "strong_open":
            bold += 1
        elif child.type == "strong_close":
            bold -= 1
        elif child.type in ("text", "code_inline") and child.content:
            if runs and runs[-1].bold == bool(bold):
                runs[-1].text += child.content
            else:
                runs.append(Run(child.content, bool(bold)))
        elif child.type in ("softbreak", "hardbreak"):
            runs.append(Run("\n", bool(bold)))
    return runs


@dataclass
class Letterhead:
    """The firm the document comes from, and the tender it is for: the header and footer of every page."""

    firm: str
    lines: list[str]  # address, registration numbers
    logo: Path | None
    tender: str
