"""Preserve cited qualifications; lexical checks do not replace engineering review."""

import re

from .ai_tools import ToolArgumentError
from .source_boq import matchable

_CONDITION = re.compile(
    r"\b(?:if|unless|provided that|only when|where applicable|subject to)\b|إذا|اذا|في حال|بشرط",
    re.I,
)
_EXCEPTION = re.compile(
    r"\b(?:except|unless|exempt(?:ion)?|instead of)\b|باستثناء|إلا إذا|الا اذا|يعفى|معفى|بدلا|بدلاً",
    re.I,
)


# Where one qualifier ends: punctuation, or the next numbered item on a line a PDF
# stored as one run ("... تفاديا -2 تعبئة وتوقيع ...").
_PHRASE_END = re.compile(r"[,،;؛\n.!?]|\s[-–—]\s*\d+[.)]?\s|\s\d+[-.)]\s")
MAX_PHRASE = 200
# The start of a numbered item in a list a PDF stored as one line.
_ITEM_START = re.compile(r"(?:^|\s)[-–—]?\s*\d+\s*[-.)]\s|(?:^|\s)[-–—]\s*\d+\s")
# The end of one sentence, line or numbered item.
_BREAK = re.compile(r"[.!?؟]\s|[\n]|(?:^|\s)[-–—]?\s*\d+\s*[-.)]\s|(?:^|\s)[-–—]\s*\d+\s")


def _phrases(pattern, text, *, keep_marker):
    for match in pattern.finditer(text):
        start = match.start() if keep_marker else match.end()
        tail = text[start:]
        phrase = _PHRASE_END.split(tail, maxsplit=1)[0].strip()
        if len(phrase) > MAX_PHRASE:
            # A qualifier this long is a paragraph, not a clause; keep its opening
            # words, which is what a citation must preserve.
            phrase = phrase[:MAX_PHRASE].rsplit(" ", 1)[0]
        if phrase:
            yield phrase


def _sentence_start(text: str, position: int) -> int:
    """Where the quoted sentence begins: after the nearest sentence end, line break
    or numbered item before it."""

    marks = [match.end() for match in _BREAK.finditer(text, 0, position)]
    return max(marks) if marks else 0


def _sentence_end(text: str, position: int) -> int:
    match = _BREAK.search(text, position)
    return match.start() + 1 if match else len(text)


def _widest(phrases) -> list[str]:
    """Only the fullest of overlapping phrases: "except" and "exempt" on one line
    each start a phrase, and the shorter one is part of the longer."""

    found: list[str] = []
    for phrase in sorted(phrases, key=len, reverse=True):
        if not any(_holds(phrase, kept) for kept in found):
            found.append(phrase)
    return found


def _holds(part: str, whole: str) -> bool:
    """Is this text inside that one, as a reader sees them? Copied Arabic loses the
    direction marks and doubled spaces a PDF stores."""

    return matchable(part) in matchable(whole)


def validate_qualification(repo, tender_id, request, *, manager=False):
    quote = request.source_quote
    if not quote:
        if manager or request.applicability != "unknown" or request.condition or request.exceptions:
            raise ToolArgumentError(
                "Copy the exact complete source clause into source_quote, including any conditions and exceptions."
            )
        return
    passages = [repo.get_evidence(tender_id, sid)["text"] for sid in request.source_ids]
    matching = [text for text in passages if _holds(quote, text)]
    if not matching:
        from .source_boq import _first_difference

        detail = _first_difference(matchable(quote), matchable(passages[0])) if passages else ""
        raise ToolArgumentError(
            "The source quote must match a passage in the cited Tender evidence." + detail
        )
    # Inspect the sentence the quote sits in, so quoting only the duty after 'If ...'
    # cannot hide its qualifier, while a neighbouring item's qualifier in the same
    # stored paragraph is not attached to this requirement.
    context = []
    for text in matching:
        plain, found = matchable(text), matchable(quote)
        start = plain.index(found)
        end = start + len(found)
        context.append(plain[_sentence_start(plain, start) : _sentence_end(plain, end)])
    qualified = "\n".join(context)
    conditions = _widest(_phrases(_CONDITION, qualified, keep_marker=True))
    exceptions = _widest(_phrases(_EXCEPTION, qualified, keep_marker=False))
    if conditions and request.applicability == "unconditional":
        raise ToolArgumentError(
            f'The source line carries a condition ("{conditions[0]}"), so this requirement is not '
            "unconditional. Set applicability to conditional."
        )
    if exceptions and not request.exceptions:
        raise ToolArgumentError(
            f'The source line carries an exception ("{exceptions[0]}"); it is not a universal duty. '
            "Record it in exceptions."
        )
    # Quantix copies each qualifier out of the cited line itself. Asking a model to
    # echo text that a PDF wrapped or reordered only costs corrections, and the saved
    # record has to carry the source's own words either way.
    if conditions:
        request.applicability = "conditional"
        if not all(_holds(condition, request.condition or "") for condition in conditions):
            request.condition = "; ".join(conditions)[:3000]
    if exceptions:
        # The source's own words, never a duty sentence offered in their place.
        request.exceptions = exceptions[:20]
    if (conditions or exceptions) and not all(
        _holds(phrase, request.source_quote) for phrase in (*conditions, *exceptions)
    ):
        # The quote is widened to the cited line, so it holds every qualifier on it.
        request.source_quote = qualified[:6000]
    if manager and request.applicability == "unknown":
        raise ToolArgumentError(
            "State whether the quoted requirement is unconditional or conditional. Keep its condition unresolved for engineer review."
        )
