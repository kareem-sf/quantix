"""Preserve cited qualifications; lexical checks do not replace engineering review."""

import re

from .ai_tools import ToolArgumentError

_CONDITION = re.compile(
    r"\b(?:if|unless|provided that|only when|where applicable|subject to)\b|إذا|اذا|في حال|بشرط",
    re.I,
)
_EXCEPTION = re.compile(
    r"\b(?:except|unless|exempt(?:ion)?|instead of)\b|باستثناء|إلا إذا|الا اذا|يعفى|معفى|بدلا|بدلاً",
    re.I,
)


def _phrases(pattern, text, *, keep_marker):
    for match in pattern.finditer(text):
        start = match.start() if keep_marker else match.end()
        tail = text[start:]
        phrase = re.split(r"[,،;؛\n.!?]", tail, maxsplit=1)[0].strip()
        if phrase:
            yield phrase


def validate_qualification(repo, tender_id, request, *, manager=False):
    quote = request.source_quote
    if not quote:
        if manager or request.applicability != "unknown" or request.condition or request.exceptions:
            raise ToolArgumentError(
                "Copy the exact complete source clause into source_quote, including any conditions and exceptions."
            )
        return
    passages = [repo.get_evidence(tender_id, sid)["text"] for sid in request.source_ids]
    matching = [text for text in passages if quote in text]
    if not matching:
        raise ToolArgumentError(
            "The source quote must match a passage in the cited Tender evidence."
        )
    # Inspect the enclosing line too, so quoting only the duty after 'If ...'
    # cannot hide a qualifier on that line. Other formulations still need review.
    context = []
    for text in matching:
        start = text.index(quote)
        end = start + len(quote)
        left = text.rfind("\n", 0, start) + 1
        right = text.find("\n", end)
        context.append(text[left : right if right >= 0 else len(text)])
    qualified = "\n".join(context)
    if _CONDITION.search(qualified) and (
        request.applicability != "conditional" or not request.condition
    ):
        raise ToolArgumentError(
            "Preserve the source condition: set applicability to conditional and copy its condition verbatim."
        )
    if request.applicability == "conditional" and not request.condition:
        raise ToolArgumentError("Record the condition for this conditional requirement.")
    if request.condition and request.condition not in quote:
        raise ToolArgumentError(
            "Include the full condition in the source quote and copy it verbatim into condition."
        )
    for condition in _phrases(_CONDITION, qualified, keep_marker=True):
        if condition not in quote or condition not in request.condition:
            raise ToolArgumentError(
                "Include the actual source condition in both the complete quote and condition; duty text cannot replace it."
            )
    if _EXCEPTION.search(qualified) and not request.exceptions:
        raise ToolArgumentError(
            "Preserve the source exception in exceptions; do not present it as a universal duty."
        )
    if any(exception not in quote for exception in request.exceptions):
        raise ToolArgumentError("Copy each exception verbatim from the full source quote.")
    for exception in _phrases(_EXCEPTION, qualified, keep_marker=False):
        if exception not in quote or not any(exception in saved for saved in request.exceptions):
            raise ToolArgumentError(
                "Include the actual source exception in the complete quote and exceptions; duty text cannot replace it."
            )
    if manager and request.applicability == "unknown":
        raise ToolArgumentError(
            "State whether the quoted requirement is unconditional or conditional. Keep its condition unresolved for engineer review."
        )
