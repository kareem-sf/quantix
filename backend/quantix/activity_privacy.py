"""Scrub before persistence, preserving complete permitted application content."""

import base64
import dataclasses
import json
import re
from collections.abc import Mapping
from datetime import date, datetime
from decimal import Decimal
from pathlib import Path

_PRIVATE = {
    "apikey",
    "accesstoken",
    "refreshtoken",
    "idtoken",
    "authorization",
    "password",
    "secret",
    "clientsecret",
    "credentials",
    "cookie",
    "setcookie",
    "signature",
    "thoughtsignature",
    "encryptedcontent",
    "rawcontent",
    "providerdetails",
    "accounthome",
    "reasoningcontent",
}
_PATTERNS = (
    re.compile(r"\b(?:sk-|xai-)[A-Za-z0-9_-]{8,}\b"),
    re.compile(r"\bAIza[A-Za-z0-9_-]{20,}\b"),
    re.compile(r"(?i)\bBearer\s+[A-Za-z0-9._~+/-]+=*"),
    re.compile(
        r"(?i)(?:api[_-]?key|access[_-]?token|refresh[_-]?token|password|client[_-]?secret)\s*[=:]\s*[^\s&\"',}]+"
    ),
)


def sanitize(value, secrets=()):
    """Return safe value, whether redacted, and explicit unavailable field paths."""
    changed = False
    unavailable = []
    known = tuple(secret for secret in secrets if isinstance(secret, str) and len(secret) >= 4)

    def walk(item, path="", depth=0):
        nonlocal changed
        if depth > 80:
            unavailable.append(path or "content")
            return "[structure exceeds inspection depth]"
        if isinstance(item, str):
            text = item
            for secret in known:
                text = text.replace(secret, "[credential]")
            for pattern in _PATTERNS:
                text = pattern.sub("[credential]", text)
            # Tool results are frequently JSON strings. Scrub their keys too,
            # without changing their type or serialisation when no redaction occurs.
            if text.lstrip().startswith(("{", "[")):
                try:
                    decoded = json.loads(text)
                except (ValueError, RecursionError):
                    pass
                else:
                    before = changed
                    cleaned = walk(decoded, path, depth + 1)
                    if cleaned != decoded:
                        text = json.dumps(cleaned, ensure_ascii=False)
                    changed = changed or before
            changed = changed or text != item
            return text
        if isinstance(item, Mapping):
            result = {}
            thinking = item.get("part_kind") == "thinking" or item.get("type") in {
                "thinking",
                "redacted_thinking",
            }
            for key, content in item.items():
                name = str(key)
                normalized = re.sub(r"[^a-z]", "", name.lower())
                child_path = f"{path}.{name}" if path else name
                if normalized in _PRIVATE or thinking and name in {"content", "thinking", "data"}:
                    result[name] = "[not retained]"
                    unavailable.append(child_path)
                    changed = True
                else:
                    result[name] = walk(content, child_path, depth + 1)
            return result
        if isinstance(item, (list, tuple, set)):
            return [walk(child, f"{path}[{index}]", depth + 1) for index, child in enumerate(item)]
        if item is None or isinstance(item, (bool, int, float)):
            return item
        if isinstance(item, bytes):
            if any(secret.encode() in item for secret in known):
                changed = True
                unavailable.append(path or "binary content")
                return "[binary content containing a credential was not retained]"
            return {
                "encoding": "base64",
                "data": base64.b64encode(item).decode("ascii"),
                "bytes": len(item),
            }
        if isinstance(item, (datetime, date)):
            return item.isoformat()
        if isinstance(item, Decimal):
            return str(item)
        if isinstance(item, Path):
            unavailable.append(path)
            return "[local path]"
        if dataclasses.is_dataclass(item) and not isinstance(item, type):
            return walk(dataclasses.asdict(item), path, depth + 1)
        if callable(getattr(item, "model_dump", None)):
            return walk(item.model_dump(mode="json"), path, depth + 1)
        unavailable.append(path or "content")
        return f"[unavailable {type(item).__name__}]"

    result = walk(value)
    return result, changed, list(dict.fromkeys(unavailable))
