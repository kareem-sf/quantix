"""Read-only access to engineer-approved reusable notes, separate from Tender evidence."""

import json

from agents import RunContextWrapper, function_tool

from .knowledge import KnowledgeService
from .knowledge_models import KnowledgeCategory
from .office_tools import OfficeContext, redact_text


def _clean(value):
    if isinstance(value, str):
        return redact_text(value)
    if isinstance(value, dict):
        return {key: _clean(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_clean(item) for item in value]
    return value


def knowledge_tools():
    @function_tool(failure_error_function=None)
    async def list_reusable_notes(
        ctx: RunContextWrapper[OfficeContext],
        category: KnowledgeCategory | None,
        offset: int,
        limit: int,
    ) -> str:
        """List engineer-approved reusable guidance. These notes are not current Tender facts. Price/tax notes always require fresh research."""
        if not 0 <= offset or not 1 <= limit <= 20:
            raise ValueError("Read at most 20 reusable notes at a time using a nonnegative offset.")
        notes = KnowledgeService(ctx.context.repo).list(
            category=category, offset=offset, limit=limit
        )
        return json.dumps(
            _clean(
                {
                    "current_tender_evidence": False,
                    "notes": [
                        {
                            key: note[key]
                            for key in (
                                "id",
                                "title",
                                "category",
                                "approved_at",
                                "status",
                                "needs_recheck",
                                "commercial_revalidation_required",
                                "revalidation_reasons",
                            )
                        }
                        for note in notes
                    ],
                    "next_offset": offset + limit if len(notes) == limit else None,
                    "instruction": "Read relevant notes before using them. Apply current engineer instructions and approved scope first. Supporting source IDs remain in the original Tender and are not citations for this Tender.",
                }
            ),
            ensure_ascii=False,
        )

    @function_tool(failure_error_function=None)
    async def read_reusable_note(
        ctx: RunContextWrapper[OfficeContext], knowledge_id: str, offset: int = 0, limit: int = 8000
    ) -> str:
        """Inspect approved or withdrawn reusable guidance, with current provenance/revalidation flags and bounded full-text pages."""
        if not 0 <= offset or not 1 <= limit <= 8000:
            raise ValueError(
                "Read up to 8,000 note characters at a time using a nonnegative offset."
            )
        note = _clean(KnowledgeService(ctx.context.repo).get(knowledge_id))
        content = note["content"]
        note.update(
            content=content[offset : offset + limit],
            text_offset=offset,
            total_characters=len(content),
            next_offset=offset + limit if offset + limit < len(content) else None,
            current_tender_evidence=False,
        )
        ctx.context.repo.event(
            ctx.context.run_id,
            "reusable_note_read",
            "An approved reusable note was inspected.",
            {
                "knowledge_id": knowledge_id,
                "status": note["status"],
                "needs_recheck": note["needs_recheck"],
                "offset": offset,
            },
        )
        return json.dumps(note, ensure_ascii=False)

    return [list_reusable_notes, read_reusable_note]
