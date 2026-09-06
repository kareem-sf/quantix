"""Read-only tools expose only the active Tender's indexed material."""

import json
import re
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any

from agents import RunContextWrapper, function_tool

if TYPE_CHECKING:
    from .repository import Repository


def redact_text(value: Any) -> str:
    text = str(value or "")
    text = re.sub(r"(?i)\b[a-z]:[\\/][^\s\"<>]*", "[local path]", text)
    text = re.sub(r"\\\\[^\s\"<>]+", "[local path]", text)
    text = re.sub(r"\bsk-[A-Za-z0-9_-]{8,}\b", "[credential]", text)
    return text


def safe_text(value: Any, limit: int = 12000) -> str:
    return redact_text(value)[:limit]


@dataclass
class OfficeContext:
    repo: "Repository"
    tender_id: str
    run_id: str
    seen_sources: set[str] = field(default_factory=set)
    specialist_calls: int = 0
    approved_scope: dict | None = None
    item_bases: dict[str, str] = field(default_factory=dict)
    trusted_recipients: set[str] = field(default_factory=set)
    source_recipients: dict[str, set[str]] = field(default_factory=dict)
    semantic_service: Any = None
    standing_preferences: str = ""

    def source(self, evidence_id: str, offset: int = 0, limit: int = 12000) -> dict:
        evidence = self.repo.get_evidence(self.tender_id, evidence_id)
        artifact = self.repo.get_artifact(self.tender_id, evidence["artifact_id"])
        text = str(evidence.get("text") or "")
        if offset < 0 or offset > len(text) or not 1 <= limit <= 12000:
            raise ValueError("Choose an available source offset and a text limit from 1 to 12000.")
        self.seen_sources.add(evidence["id"])
        metadata = evidence.get("metadata") or {}
        cells = metadata.get("cells") or []
        from .office_business import recipient_addresses

        visible_text = safe_text(text[offset : offset + limit])
        visible_sender = safe_text(metadata.get("sender"), 1000)
        self.source_recipients.setdefault(evidence_id, set()).update(
            recipient_addresses(visible_text + " " + visible_sender)
        )
        return {
            "id": evidence["id"],
            "artifact_id": evidence["artifact_id"],
            "artifact_name": safe_text(evidence.get("artifact_name"), 250),
            "version": artifact["version"],
            "is_current": artifact["is_current"],
            "locator": safe_text(evidence.get("locator"), 250),
            "page": evidence.get("page"),
            "sheet": safe_text(evidence.get("sheet"), 250),
            "cell_range": evidence.get("cell_range"),
            "kind": evidence.get("kind"),
            "text": visible_text,
            "text_is_partial": offset > 0 or offset + limit < len(text),
            "text_offset": offset,
            "text_length": len(text),
            "next_offset": offset + limit if offset + limit < len(text) else None,
            "metadata": {
                "cells": [
                    {
                        key: safe_text(cell[key], 1000) if isinstance(cell[key], str) else cell[key]
                        for key in (
                            "coordinate",
                            "value",
                            "cached_value",
                            "formula",
                            "number_format",
                            "data_type",
                        )
                        if key in cell
                    }
                    for cell in cells[:100]
                ],
                "cells_are_partial": len(cells) > 100,
                **{
                    key: safe_text(metadata[key], 1000)
                    for key in (
                        "sender",
                        "received_at",
                        "date_header",
                        "date_basis",
                        "message_id",
                        "quote_id",
                        "origin",
                        "measurement_id",
                        "status",
                        "method",
                        "source_version",
                        "source_hash",
                    )
                    if metadata.get(key) is not None
                },
            },
        }

    def validate_sources(self, source_ids: list[str]) -> None:
        for evidence_id in source_ids:
            if evidence_id not in self.seen_sources:
                raise ValueError(
                    "The response cites evidence that was not read in this Tender run."
                )
            evidence = self.repo.get_evidence(self.tender_id, evidence_id)
            artifact = self.repo.get_artifact(self.tender_id, evidence["artifact_id"])
            if not artifact["is_current"]:
                raise ValueError(
                    "The response cites a superseded source. Review the current evidence."
                )


def source_tools() -> list:
    @function_tool(failure_error_function=None)
    async def view_document_page(
        ctx: RunContextWrapper[OfficeContext],
        artifact_id: str,
        page: int,
        x: float,
        y: float,
        width: float,
        height: float,
    ) -> list:
        """Inspect a PDF page or zoomed region. Coordinates are top-left fractions; use 0,0,1,1 for the whole page. Cite the returned source ID."""
        from .visual_sources import visual_source

        return await visual_source(ctx.context, artifact_id, page, [x, y, width, height])

    @function_tool(failure_error_function=None)
    async def search_sources(ctx: RunContextWrapper[OfficeContext], query: str, limit: int) -> str:
        """Search this Tender's extracted evidence; return IDs and exact source locations."""
        if not query.strip() or len(query) > 1000 or not 1 <= limit <= 20:
            raise ValueError("Use a search query under 1000 characters and a limit from 1 to 20.")
        hits = ctx.context.repo.search(ctx.context.tender_id, query, limit=limit)
        sources = [ctx.context.source(hit["id"]) for hit in hits]
        ctx.context.repo.event(
            ctx.context.run_id,
            "sources_read",
            "Tender evidence searched.",
            {"source_ids": [source["id"] for source in sources]},
        )
        return json.dumps(sources, ensure_ascii=False)

    @function_tool(failure_error_function=None)
    async def read_source(
        ctx: RunContextWrapper[OfficeContext], source_id: str, offset: int = 0, limit: int = 12000
    ) -> str:
        """Read an indexed Tender source at a character offset. Follow next_offset to inspect the rest."""
        source = ctx.context.source(source_id, offset, limit)
        ctx.context.repo.event(
            ctx.context.run_id,
            "sources_read",
            "Tender source inspected.",
            {"source_ids": [source_id]},
        )
        return json.dumps(source, ensure_ascii=False)

    @function_tool(failure_error_function=None)
    async def read_document(
        ctx: RunContextWrapper[OfficeContext], artifact_id: str, offset: int, limit: int
    ) -> str:
        """Read a bounded page of indexed evidence from a Tender document."""
        if offset < 0 or not 1 <= limit <= 20:
            raise ValueError("Use a nonnegative offset and a limit from 1 to 20.")
        rows = ctx.context.repo.artifact_evidence(
            ctx.context.tender_id, artifact_id, offset=offset, limit=limit
        )
        return json.dumps([ctx.context.source(row["id"]) for row in rows], ensure_ascii=False)

    @function_tool(failure_error_function=None)
    async def list_documents(ctx: RunContextWrapper[OfficeContext]) -> str:
        """List registered Tender documents and their extraction status, without claiming analysis."""
        artifacts = ctx.context.repo.list_artifacts(ctx.context.tender_id)
        return json.dumps(
            {
                "total": len(artifacts),
                "documents": [
                    {
                        key: safe_text(row.get(key), 300)
                        for key in ("id", "name", "kind", "status", "area")
                    }
                    for row in artifacts[:200]
                ],
                "listing_is_partial": len(artifacts) > 200,
            },
            ensure_ascii=False,
        )

    from .office_business import business_tools
    from .office_knowledge import knowledge_tools
    from .office_project import project_tools
    from .office_measurement import measurement_tools

    return [
        search_sources,
        read_source,
        read_document,
        list_documents,
        view_document_page,
        *business_tools(),
        *knowledge_tools(),
        *project_tools(),
        *measurement_tools(),
    ]
