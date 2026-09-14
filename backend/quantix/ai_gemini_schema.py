"""Gemini-compatible JSON Schemas for tool and structured-output declarations.

The Gemini API rejects requests whose declared schemas carry length, item-count
or numeric bounds with ``400 INVALID_ARGUMENT`` ("Request contains an invalid
argument"). This was isolated against ``gemini-3.8-flash`` with the Tender
Manager's ``OfficeOutput`` schema: removing ``pattern``, ``format``,
``prefixItems``, ``default``, ``additionalProperties``, nullable unions or
``$defs`` did not help, while removing only the bounds made the same request
valid.

Quantix validates every model result with the original Pydantic model before
anything is published, so the bounds remain enforced locally. They are moved
into the field description so the model still sees them as guidance.
"""

from __future__ import annotations

from typing import Any

from pydantic_ai.profiles.google import GoogleJsonSchemaTransformer

_BOUND_HINTS: dict[str, str] = {
    "minLength": "at least {value} characters",
    "maxLength": "at most {value} characters",
    "minItems": "at least {value} items",
    "maxItems": "at most {value} items",
    "minimum": "minimum {value}",
    "maximum": "maximum {value}",
}


class GeminiSchemaTransformer(GoogleJsonSchemaTransformer):
    """Google's transformer plus relocation of bounds Gemini refuses."""

    def transform(self, schema: dict[str, Any]) -> dict[str, Any]:
        schema = super().transform(schema)
        hints = [
            template.format(value=schema.pop(key))
            for key, template in _BOUND_HINTS.items()
            if key in schema
        ]
        if hints:
            note = "; ".join(hints)
            description = schema.get("description")
            schema["description"] = (
                f"{description} ({note})" if description else note[:1].upper() + note[1:]
            )
        return schema
