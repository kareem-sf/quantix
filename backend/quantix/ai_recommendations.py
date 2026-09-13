"""Conservative, source-backed suggestions; an existing model is never silently replaced."""

_PREFERRED = (
    "gpt-6-astra", "claude-fable-5-1", "claude-opus-5", "claude-sonnet-5",
    "grok-4.6", "gemini-3.8-flash", "gemini-3.1-pro-preview", "gemini-2.5-pro",
)
SOURCES = {
    "openai": "https://developers.openai.com/api/docs/models/gpt-6-astra",
    "anthropic": "https://platform.claude.com/docs/en/models/overview",
    "google": "https://ai.google.dev/gemini-api/docs/models",
    "xai": "https://docs.x.ai/developers/models",
}


def suggest_model(models):
    candidates = [model for model in models if model["capabilities"].get("tools") is not False
                  and not any(word in model["model_id"].lower() for word in ("embedding", "tts", "image-generation", "transcribe", "whisper", "moderation"))]
    for preferred in _PREFERRED:
        candidate = next((m for m in candidates if m["model_id"] == preferred), None)
        if candidate:
            return candidate["model_id"], "Recommended for demanding work from the models available to this account. You can change it. Model guidance reviewed 7 September 2026."
    candidates.sort(key=lambda m: (m["capabilities"].get("tools") is True,
                                   m["capabilities"].get("images") is True,
                                   m["capabilities"].get("context_window") or 0), reverse=True)
    if candidates:
        return candidates[0]["model_id"], "Suggested from the available document and tool support. Comparative model quality is not established for this account; review or change this choice."
    return None, "No suitable model has been confirmed for this account yet."
