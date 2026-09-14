"""Thinking levels the engineer can choose for a Tender route, for every provider.

A model record's own reasoning list, reported by the provider or its original
client, always wins. Direct API models without a recorded list are offered the
levels their vendor documents for current models:

- Google Gemini 3 Flash and Pro: thinking_level low, medium, high
  (https://ai.google.dev/gemini-api/docs/thinking, checked 2026-09-12).
- xAI grok-4.5 and grok-4.6: reasoning_effort low, medium, high
  (https://docs.x.ai/developers/model-capabilities/text/reasoning, checked 2026-09-12).
- OpenAI reasoning models: reasoning_effort low, medium, high.
- Anthropic: effort low, medium, high with adaptive thinking, or thinking disabled.

Codex, Grok Build and OpenAI-compatible endpoints report their own lists, so
nothing is assumed for them. A provider that still refuses a level rejects the
request before processing and its budget hold is released.
"""

from __future__ import annotations

_DIRECT_LEVELS = {
    "google": ("low", "medium", "high"),
    "xai": ("low", "medium", "high"),
    "openai": ("low", "medium", "high"),
    "anthropic": ("disabled", "low", "medium", "high"),
}

_ORDER = (
    "none",
    "disabled",
    "minimal",
    "low",
    "medium",
    "high",
    "xhigh",
    "max",
    "ultra",
    "adaptive",
)

_TEXT = {
    "none": ("Off", "Answers straight away. Uses the least."),
    "disabled": ("Off", "Answers straight away. Uses the least."),
    "minimal": ("Minimal", "Very quick thinking for simple requests."),
    "low": ("Low", "Quick thinking. Good for questions about the documents."),
    "medium": ("Medium", "Balanced. Recommended for most Tender work."),
    "high": ("High", "Careful thinking for plans and checks. Uses noticeably more."),
    "xhigh": ("Extra high", "Much more usage. Only for hard problems."),
    "max": ("Maximum", "Uses the most. Rarely needed."),
    "ultra": ("Ultra", "Uses the most. Rarely needed."),
    "adaptive": ("Adaptive", "The model decides how much to think."),
}

_OFF = frozenset({"none", "disabled"})


def _rank(level: str) -> int:
    return _ORDER.index(level) if level in _ORDER else len(_ORDER)


def thinking_levels(connection: dict, model: dict) -> list[str]:
    """Return the levels this route can use, lightest first."""

    from .ai_connections import is_direct_profile

    recorded = [
        level
        for level in (model.get("capabilities") or {}).get("reasoning") or []
        if isinstance(level, str) and level and not level.startswith("budget:")
    ]
    levels = recorded or (
        list(_DIRECT_LEVELS.get(connection.get("provider_id"), ()))
        if is_direct_profile(connection)
        else []
    )
    return sorted(dict.fromkeys(levels), key=_rank)


def allows_level(connection: dict, model: dict, level: str | None) -> bool:
    if level is None:
        return True
    recorded = (model.get("capabilities") or {}).get("reasoning") or []
    return level in recorded or level in thinking_levels(connection, model)


def recommended_level(connection: dict, model: dict) -> str | None:
    """Medium where offered: xhigh by default burned subscription limits fastest."""

    levels = thinking_levels(connection, model)
    for preferred in ("medium", "high", "low"):
        if preferred in levels:
            return preferred
    return None


def light_level(connection: dict, model: dict) -> str | None:
    """The lightest thinking for routing and short replies, never off-by-guess."""

    levels = [level for level in thinking_levels(connection, model) if level != "adaptive"]
    return levels[0] if levels else None


def describe(level: str | None) -> dict:
    if level is None:
        return {
            "value": None,
            "label": "Default",
            "detail": "The AI's own default setting.",
            "off": False,
        }
    label, detail = _TEXT.get(
        level, (level.replace("_", " ").capitalize(), "A level reported by this AI.")
    )
    return {"value": level, "label": label, "detail": detail, "off": level in _OFF}
