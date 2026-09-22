"""Arabic text from PDF character boxes, and search normalisation.

Many Arabic PDFs store words in drawing order, so reading the stored text gives lines backwards. Rebuilding each
line from where every character is drawn gives the reading order whatever the storage order: Arabic words right to
left, runs of English words and numbers left to right, and vowel marks kept with their letter.
"""

import re
import unicodedata
from dataclasses import dataclass
from statistics import median

ARABIC = re.compile(r"[؀-ۿݐ-ݿࢠ-ࣿﭐ-﷿ﹰ-﻿]")
_MARKS = re.compile(r"[ً-ٰٟۖ-ۭ]")


@dataclass(frozen=True)
class Char:
    text: str
    left: float
    bottom: float
    right: float
    top: float

    @property
    def x(self) -> float:
        return (self.left + self.right) / 2

    @property
    def y(self) -> float:
        return (self.bottom + self.top) / 2


def has_arabic(text: str) -> bool:
    return bool(ARABIC.search(text))


def lines_from_boxes(chars: list[Char]) -> list[str]:
    """Reading-order lines, top of the page first."""
    visible = [c for c in chars if c.text not in ("\r", "\n", "")]
    lines: list[list[Char]] = []
    for char in sorted(visible, key=lambda c: -c.y):
        if lines and abs(_centre(lines[-1]) - char.y) <= _height(lines[-1] + [char]) / 2:
            lines[-1].append(char)
        else:
            lines.append([char])
    return [text for text in (_line_text(line) for line in lines) if text]


def _centre(line: list[Char]) -> float:
    return sum(c.y for c in line) / len(line)


def _height(chars: list[Char]) -> float:
    return max(max(c.top - c.bottom for c in chars), 1.0)


def _line_text(line: list[Char]) -> str:
    words = _words(sorted(line, key=lambda c: c.x))
    ordered = [_word_text(w) for w in words]
    if any(has_arabic(w) for w in ordered):
        ordered = _right_to_left(ordered)
    return unicodedata.normalize("NFKC", " ".join(ordered)).strip()


def _words(visual: list[Char]) -> list[list[Char]]:
    solid = [c for c in visual if not c.text.isspace()]
    if not solid:
        return []
    gap = 0.3 * median(max(c.right - c.left, 0.1) for c in solid)
    words: list[list[Char]] = [[]]
    previous: Char | None = None
    for char in visual:
        if char.text.isspace():
            words.append([])
        elif previous is not None and char.left - previous.right > gap and not _is_mark(char):
            words.append([char])
        else:
            words[-1].append(char)
        if not char.text.isspace():
            previous = char
    return [w for w in words if w]


def _is_mark(char: Char) -> bool:
    return unicodedata.category(char.text[0]) == "Mn"


def _word_text(word: list[Char]) -> str:
    if not has_arabic("".join(c.text for c in word)):
        return "".join(c.text for c in sorted(word, key=lambda c: c.x))
    bases = sorted((c for c in word if not _is_mark(c)), key=lambda c: -c.x)
    marks = [c for c in word if _is_mark(c)]
    if not bases:
        return "".join(c.text for c in marks)
    seated: list[list[str]] = [[b.text] for b in bases]
    for mark in marks:
        nearest = min(range(len(bases)), key=lambda i: abs(bases[i].x - mark.x))
        seated[nearest].append(mark.text)
    return "".join("".join(parts) for parts in seated)


def _right_to_left(words: list[str]) -> list[str]:
    """Visual left-to-right words to reading order: reverse, keeping runs of non-Arabic words left to right."""
    result: list[str] = []
    run: list[str] = []
    for word in reversed(words):
        if has_arabic(word):
            result.extend(reversed(run))
            run = []
            result.append(word)
        else:
            # "1%" is drawn "%1" in Arabic text: put the sign back after the number.
            run.append(word[1:] + word[0] if word[:1] in "%٪" and word[1:2].isdigit() else word)
    result.extend(reversed(run))
    return result


def searchable(text: str) -> str:
    """Text as the search index sees it: no vowel marks or tatweel, one alef, ya and ta marbuta form, lower case."""
    text = unicodedata.normalize("NFKC", text)
    text = _MARKS.sub("", text).replace("ـ", "")
    text = re.sub("[أإآٱ]", "ا", text).replace("ى", "ي").replace("ة", "ه")
    return text.lower()
