"""Length-limited text for AI orientation notes that should be trimmed, not rejected.

Package briefs and project identity are guidance, never citable evidence. A
reply with one sentence over its limit is still useful, so overlong text and
lists are cut to the declared limit before validation. The limit stays in the
JSON schema so the model is still asked to keep within it.
"""

from __future__ import annotations

from typing import Annotated, Any

from pydantic import BeforeValidator, Field


def _clip_text(limit: int):
    def clip(value: Any) -> Any:
        return value.strip()[:limit].rstrip() if isinstance(value, str) else value

    return clip


def _clip_list(limit: int):
    def clip(value: Any) -> Any:
        return value[:limit] if isinstance(value, list) else value

    return clip


def Text(limit: int, **field: Any):  # noqa: N802 - reads like a type in model declarations
    return Annotated[str, BeforeValidator(_clip_text(limit)), Field(max_length=limit, **field)]


def OptionalText(limit: int, **field: Any):  # noqa: N802
    return Annotated[
        Annotated[str, Field(max_length=limit)] | None,
        BeforeValidator(_clip_text(limit)),
        Field(default=None, **field),
    ]


def TextList(item_limit: int, count: int, **field: Any):  # noqa: N802
    return Annotated[
        list[Annotated[str, BeforeValidator(_clip_text(item_limit)), Field(max_length=item_limit)]],
        BeforeValidator(_clip_list(count)),
        Field(default_factory=list, max_length=count, **field),
    ]
