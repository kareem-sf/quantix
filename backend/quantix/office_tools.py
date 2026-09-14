"""Read-only tools expose only the active Tender's indexed material."""

import json
import re
from contextlib import contextmanager
from contextvars import ContextVar
from dataclasses import dataclass, field
from functools import wraps
from typing import TYPE_CHECKING, Any

from .ai_tools import ToolArgumentError, ToolContext, tool
from .db import dump, now

if TYPE_CHECKING:
    from .repository import Repository


@dataclass
class _ReadDraft:
    """Per-invocation provenance staged outside the shared context."""

    seen_sources: set[str] = field(default_factory=set)
    item_bases: dict[str, str] = field(default_factory=dict)
    trusted_recipients: set[str] = field(default_factory=set)
    source_recipients: dict[str, set[str]] = field(default_factory=dict)
    events: list[tuple[str, str, dict]] = field(default_factory=list)
    evidence_rows: list[tuple] = field(default_factory=list)
    returned_reads: set[tuple[str, int, int]] = field(default_factory=set)
    semantic_service: Any = None
    semantic_service_set: bool = False


_READ_DRAFT: ContextVar[tuple["OfficeContext", _ReadDraft] | None] = ContextVar(
    "quantix_read_draft", default=None
)
_READ_COMMIT_DEFERRED: ContextVar[Any] = ContextVar("quantix_read_commit_deferred", default=None)
_DEFERRED_READ_DRAFT: ContextVar[Any] = ContextVar("quantix_deferred_read_draft", default=None)
_MAX_REDACTED_SOURCE_CHARS = 48000
# Every tool result is sent back to the model on each later step of the run,
# so reads return a focused slice and the model asks for more when needed.
SEARCH_EXCERPT_CHARS = 700
READ_SOURCE_CHARS = 4000
READ_SOURCE_MAX_CHARS = 12000
# One page of whole-document reading; the Manager follows next_offset for the rest.
WHOLE_DOCUMENT_CHARS = 12000
_TERMS = re.compile(r"[^\W_]+(?:[-.][^\W_]+)*", flags=re.UNICODE)


@contextmanager
def defer_read_commit(context):
    """Hold read provenance until the caller validates the complete result."""

    mode_token = _READ_COMMIT_DEFERRED.set(context)
    draft_token = _DEFERRED_READ_DRAFT.set(None)
    try:
        yield
    finally:
        _DEFERRED_READ_DRAFT.reset(draft_token)
        _READ_COMMIT_DEFERRED.reset(mode_token)


def commit_deferred_read(context) -> None:
    pending = _DEFERRED_READ_DRAFT.get()
    if pending is None or pending[0] is not context:
        return
    context._flush_read_draft(pending[1])
    _DEFERRED_READ_DRAFT.set(None)


def redact_text(value: Any) -> str:
    text = str(value or "")
    text = re.sub(r"(?i)\b[a-z]:[\\/][^\s\"<>]*", "[local path]", text)
    text = re.sub(r"\\\\[^\s\"<>]+", "[local path]", text)
    text = re.sub(r"\bsk-[A-Za-z0-9_-]{8,}\b", "[credential]", text)
    return text


def safe_text(value: Any, limit: int = 12000) -> str:
    return redact_text(value)[:limit]


def redact_prompt_data(value: Any) -> Any:
    """Redact string leaves before JSON encoding, preserving complete structure."""
    if isinstance(value, str):
        return redact_text(value)
    if isinstance(value, dict):
        return {key: redact_prompt_data(item) for key, item in value.items()}
    if isinstance(value, list):
        return [redact_prompt_data(item) for item in value]
    return value


@dataclass
class OfficeContext:
    """What one Manager or staff turn has read and may cite.

    Staff work inside the whole current tender, like the Manager. A staff
    context carries the colleague's id as ``actor_id`` and its assignment id.
    """

    repo: "Repository"
    tender_id: str
    run_id: str
    seen_sources: set[str] = field(default_factory=set)
    # Exact text ranges already returned to the model in this run.
    returned_reads: set[tuple[str, int, int]] = field(default_factory=set)
    approved_scope: dict | None = None
    item_bases: dict[str, str] = field(default_factory=dict)
    trusted_recipients: set[str] = field(default_factory=set)
    source_recipients: dict[str, set[str]] = field(default_factory=dict)
    semantic_service: Any = None
    standing_preferences: str = ""
    actor_id: str | None = None
    assignment_id: str | None = None
    # Records staged with the propose tool, published when the run finishes.
    proposals: dict = field(default_factory=dict)

    @property
    def is_staff(self) -> bool:
        return self.assignment_id is not None

    @property
    def staff_id(self) -> str | None:
        return self.actor_id if self.is_staff else None

    def _draft(self) -> _ReadDraft | None:
        current = _READ_DRAFT.get()
        return current[1] if current is not None and current[0] is self else None

    def _seen_source_ids(self) -> set[str]:
        draft = self._draft()
        if draft is None:
            return self.seen_sources
        return self.seen_sources | draft.seen_sources

    def has_seen_source(self, source_id: str) -> bool:
        return source_id in self._seen_source_ids()

    def add_seen_source(self, source_id: str) -> None:
        draft = self._draft()
        (self.seen_sources if draft is None else draft.seen_sources).add(source_id)

    def was_returned(self, key: tuple[str, int, int]) -> bool:
        draft = self._draft()
        return key in self.returned_reads or (draft is not None and key in draft.returned_reads)

    def add_returned(self, key: tuple[str, int, int]) -> None:
        # Staged like provenance: a result the model never received is not "sent".
        draft = self._draft()
        (self.returned_reads if draft is None else draft.returned_reads).add(key)

    def add_item_base(self, item_id: str, fingerprint: str) -> None:
        draft = self._draft()
        (self.item_bases if draft is None else draft.item_bases)[item_id] = fingerprint

    def stage_item_base(self, item_id: str, fingerprint: str) -> None:
        self.add_item_base(item_id, fingerprint)

    def add_trusted_recipients(self, recipients: set[str] | list[str] | tuple[str, ...]) -> None:
        draft = self._draft()
        (self.trusted_recipients if draft is None else draft.trusted_recipients).update(recipients)

    def add_source_recipients(self, source_id: str, recipients: set[str]) -> None:
        draft = self._draft()
        target = self.source_recipients if draft is None else draft.source_recipients
        target.setdefault(source_id, set()).update(recipients)

    def emit_event(self, kind: str, message: str, data: dict | None = None) -> None:
        draft = self._draft()
        if draft is None:
            self.repo.event(self.run_id, kind, message, data)
        else:
            draft.events.append((kind, message, data or {}))

    def stage_evidence(self, row: tuple) -> None:
        draft = self._draft()
        if draft is None:
            raise RuntimeError("Staged evidence requires an active tool read.")
        draft.evidence_rows.append(row)

    def set_semantic_service(self, service: Any) -> None:
        draft = self._draft()
        if draft is None:
            self.semantic_service = service
        else:
            draft.semantic_service = service
            draft.semantic_service_set = True

    @contextmanager
    def read_scope(self):
        """Stage one complete tool invocation until its return succeeds."""

        if self._draft() is not None:
            # Nested helper calls participate in the outer invocation.
            yield
            return
        draft = _ReadDraft()
        token = _READ_DRAFT.set((self, draft))
        try:
            yield
            if _READ_COMMIT_DEFERRED.get() is self:
                _DEFERRED_READ_DRAFT.set((self, draft))
            else:
                self._flush_read_draft(draft)
        finally:
            _READ_DRAFT.reset(token)

    def _flush_read_draft(self, draft: _ReadDraft) -> None:
        if draft.events or draft.evidence_rows:
            with self.repo.atomic() as conn:
                for row in draft.evidence_rows:
                    conn.execute(
                        """
                        INSERT INTO evidence(
                            id,artifact_id,locator,text,page,sheet,cell_range,kind,metadata_json
                        ) VALUES(?,?,?,?,?,?,?,?,?)
                        """,
                        row,
                    )
                if any(str(row[3] or "").strip() for row in draft.evidence_rows):
                    self.repo.advance_retrieval_generation(self.tender_id, conn)
                for kind, message, data in draft.events:
                    conn.execute(
                        "INSERT INTO run_events(run_id,kind,message,data_json,created_at) VALUES(?,?,?,?,?)",
                        (self.run_id, kind, message, dump(data or {}), now()),
                    )
        self.seen_sources.update(draft.seen_sources)
        self.returned_reads.update(draft.returned_reads)
        self.item_bases.update(draft.item_bases)
        self.trusted_recipients.update(draft.trusted_recipients)
        for source_id, recipients in draft.source_recipients.items():
            self.source_recipients.setdefault(source_id, set()).update(recipients)
        if draft.semantic_service_set:
            self.semantic_service = draft.semantic_service
        if any(str(row[3] or "").strip() for row in draft.evidence_rows):
            self.repo.notify_retrieval_generation(self.tender_id)

    def ensure_artifact_allowed(self, artifact_id: str, *, check_identity: bool = True) -> dict:
        if not isinstance(artifact_id, str) or not artifact_id:
            raise ValueError("Choose a source artifact from this Tender.")
        return self.repo.get_artifact(self.tender_id, artifact_id)

    def evidence_allowed(self, evidence_id: str) -> bool:
        try:
            evidence = self.repo.get_evidence(self.tender_id, evidence_id)
            self.repo.get_artifact(self.tender_id, evidence["artifact_id"])
        except (KeyError, ValueError):
            return False
        return True

    def ensure_evidence_allowed(
        self, evidence_id: str, *, tool_id: str | None = "read_source"
    ) -> dict:
        return self.repo.get_evidence(self.tender_id, evidence_id)

    def source(
        self,
        evidence_id: str,
        offset: int = 0,
        limit: int = 12000,
        *,
        tool_id: str = "read_source",
    ) -> dict:
        evidence = self.ensure_evidence_allowed(evidence_id, tool_id=tool_id)
        artifact = self.ensure_artifact_allowed(evidence["artifact_id"])
        text = str(evidence.get("text") or "")
        if offset < 0 or offset > len(text) or not 1 <= limit <= 12000:
            raise ToolArgumentError(
                "Choose an available source offset and a text limit from 1 to 12000."
            )
        metadata = evidence.get("metadata") or {}
        cells = metadata.get("cells") or []
        from .office_business import recipient_addresses

        original_excerpt = text[offset : offset + limit]
        visible_text = redact_text(original_excerpt)
        if len(visible_text) > _MAX_REDACTED_SOURCE_CHARS:
            raise ValueError(
                "The redacted source excerpt is too large; request a smaller original range."
            )
        visible_sender = safe_text(metadata.get("sender"), 1000)
        result = {
            "id": evidence["id"],
            "artifact_id": evidence["artifact_id"],
            "artifact_name": safe_text(evidence.get("artifact_name"), 250),
            "version": artifact["version"],
            "is_current": artifact["is_current"],
            "extraction_current": bool(evidence.get("extraction_current", True)),
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
        self.add_seen_source(evidence["id"])
        self.add_source_recipients(
            evidence_id, recipient_addresses(visible_text + " " + visible_sender)
        )
        return result

    def validate_sources(self, source_ids: list[str]) -> None:
        for evidence_id in source_ids:
            if not self.has_seen_source(evidence_id):
                raise ValueError(
                    "The response cites evidence that was not read in this Tender run."
                )
            evidence = self.ensure_evidence_allowed(evidence_id, tool_id=None)
            artifact = self.ensure_artifact_allowed(evidence["artifact_id"])
            if not artifact["is_current"]:
                raise ValueError(
                    "The response cites a superseded source. Review the current evidence."
                )


def scoped_tool(function=None, **metadata):
    """Wrap one read-only tool in a per-invocation staff provenance draft."""

    def decorate(fn):
        @wraps(fn)
        async def invoke(ctx, *args, **kwargs):
            with ctx.context.read_scope():
                result = await fn(ctx, *args, **kwargs)
                # Validate the actual provider-facing shape before the read
                # draft is flushed; serialization failures roll back all
                # staged provenance.
                json.dumps(result, ensure_ascii=False)
                return result

        return tool(invoke, **metadata)

    return decorate(function) if function is not None else decorate


IMAGE_TOOLS = frozenset({"view_document_page"})


def manager_source_tools() -> list:
    """The shared Tender tools a Manager turn can actually use."""

    return source_tools()


def usable_definitions(definitions, *, image_support: bool) -> list:
    """Offer image-only tools only to models whose image input is established."""

    return [
        definition
        for definition in definitions
        if image_support or definition.name not in IMAGE_TOOLS
    ]


def _excerpt_offset(text: str, query: str) -> int:
    """Start an excerpt a little before the first place the query matches."""

    folded = text.casefold()
    found = [
        position
        for position in (folded.find(term.casefold()) for term in _TERMS.findall(query)[:16])
        if position >= 0
    ]
    return max(0, min(found) - 200) if found else 0


FRUITLESS_SEARCH_LIMIT = 3


def _check_search_progress(context: "OfficeContext", passages: list[dict]) -> None:
    """Stop a run of searches that keep finding nothing new, once, with a way forward."""

    state = context.__dict__
    if any(not passage.get("already_returned") for passage in passages):
        state["_fruitless_searches"] = 0
        return
    state["_fruitless_searches"] = state.get("_fruitless_searches", 0) + 1
    if state["_fruitless_searches"] >= FRUITLESS_SEARCH_LIMIT:
        state["_fruitless_searches"] = 0
        raise ToolArgumentError(
            f"The last {FRUITLESS_SEARCH_LIMIT} searches found nothing new. Stop searching with similar "
            "terms: read a specific document with read_whole_document, check inspect_extraction_coverage for "
            "pages without readable text, or report what is still missing."
        )


def _brief(source: dict) -> dict:
    """Drop empty fields; the model pays for every character on every later step."""

    brief = {
        key: source[key]
        for key in ("id", "artifact_name", "page", "locator", "sheet", "cell_range", "kind", "text")
        if source.get(key) not in (None, "")
    }
    brief.update(text_offset=source["text_offset"], text_length=source["text_length"])
    if source["text_is_partial"]:
        brief["text_is_partial"] = True
    if source["next_offset"] is not None:
        brief["next_offset"] = source["next_offset"]
    if not source["is_current"]:
        brief["is_current"] = False
    metadata = {
        key: value
        for key, value in source["metadata"].items()
        if value is not None and value != "" and value != [] and value is not False
    }
    if metadata:
        brief["metadata"] = metadata
    return brief


def resolve_document_id(context: "OfficeContext", value: str) -> str:
    """Accept a document ID, or an exact file name or path.

    An unknown document is a correctable argument mistake rather than the end of the run.
    """

    if not isinstance(value, str) or not value.strip():
        return value
    artifacts = context.repo.list_artifacts(context.tender_id)
    if any(artifact["id"] == value for artifact in artifacts):
        return value
    wanted = value.strip().replace("\\", "/").casefold()
    matches = [
        a
        for a in artifacts
        if a["relative_path"].casefold() == wanted or a["name"].casefold() == wanted
    ]
    if len(matches) == 1:
        return matches[0]["id"]
    if _is_passage(context, value):
        raise ToolArgumentError(
            "This is a passage ID, not a document ID. Use read_source for a passage, or pass "
            "the document id from read_package_map or list_documents."
        )
    raise ToolArgumentError(
        "No document with this ID or file name exists in this Tender. Pass a document id from "
        "read_package_map or list_documents."
    )


def _is_passage(context: "OfficeContext", value: str) -> bool:
    try:
        context.repo.get_evidence(context.tender_id, value)
        return True
    except KeyError:
        return False


def _require_passage_id(context: "OfficeContext", source_id: str) -> None:
    """A wrong kind of ID is a correctable argument mistake, not the end of the run."""

    try:
        context.repo.get_evidence(context.tender_id, source_id)
        return
    except KeyError:
        pass
    if any(
        artifact["id"] == source_id for artifact in context.repo.list_artifacts(context.tender_id)
    ):
        raise ToolArgumentError(
            "This is a document ID, not a passage ID. Read the document with "
            f'read_whole_document(artifact_id="{source_id}"), or use search_sources to find the '
            "passage and pass the passage id it returns."
        )
    raise ToolArgumentError(
        "No passage with this ID exists in this Tender. Use a passage id returned by "
        "search_sources or read_whole_document in this run."
    )


def _passage(
    context: OfficeContext, evidence_id: str, offset: int, limit: int, tool_id: str
) -> dict:
    """Return a text range once per run; a repeat names the earlier result instead."""

    key = (evidence_id, offset, limit)
    if context.was_returned(key):
        context.ensure_evidence_allowed(evidence_id, tool_id=tool_id)
        return {"id": evidence_id, "text_offset": offset, "already_returned": True}
    passage = _brief(context.source(evidence_id, offset, limit, tool_id=tool_id))
    context.add_returned(key)
    return passage


def source_tools(context: OfficeContext | None = None) -> list:
    @scoped_tool
    async def view_document_page(
        ctx: ToolContext[OfficeContext],
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

    @scoped_tool
    async def search_sources(
        ctx: ToolContext[OfficeContext], query: str, limit: int = 8, exact: bool = False
    ) -> str:
        """Search this Tender's evidence by meaning (any wording, Arabic or English) fused with exact word matches. Returns a short excerpt of each passage with its evidence ID, how it was found (meaning, words or both) and weak_match when only a loose meaning match exists: a weak match is a place to look, not proof the answer exists. Set exact=true for identifiers, clause numbers, grades and quantities. Use read_source for more of a passage."""
        if not query.strip() or len(query) > 1000:
            raise ToolArgumentError("Use a search query under 1000 characters.")
        # An oversized request is served at the maximum, not refused.
        limit = max(1, min(int(limit), 20))
        from .retrieval_service import retrieve

        def excerpt(hit):
            span = hit.get("meaning_span")
            offset = (
                max(0, span["start"] - 100)
                if span
                else _excerpt_offset(str(hit.get("text") or ""), query)
            )
            passage = _passage(
                ctx.context, hit["id"], offset, SEARCH_EXCERPT_CHARS, "search_sources"
            )
            if not passage.get("already_returned"):
                passage["found_by"] = hit["found_by"]
                if span and span.get("heading"):
                    passage["heading"] = safe_text(span["heading"], 200)
            if hit.get("weak_match"):
                passage["weak_match"] = True
            return passage

        if ctx.context.semantic_service is None:
            from .semantic import SemanticService

            try:
                ctx.context.set_semantic_service(SemanticService(ctx.context.repo))
            except Exception:  # noqa: BLE001 - keyword search still answers
                pass
        response = retrieve(
            ctx.context.repo,
            ctx.context.tender_id,
            query,
            mode="words" if exact else "auto",
            limit=limit,
            semantic=ctx.context.semantic_service,
        )
        hits = []
        for item in response.hits:
            row = item.model_dump()
            if item.spans:
                row["meaning_span"] = {
                    "start": item.spans[0].start,
                    "end": item.spans[0].end,
                    "heading": item.spans[0].heading,
                }
            hits.append(row)
        result = [excerpt(hit) for hit in hits]
        _check_search_progress(ctx.context, result)
        ctx.context.emit_event(
            "sources_read",
            "Tender evidence searched.",
            {"source_ids": [source["id"] for source in result]},
        )
        return json.dumps(result, ensure_ascii=False)

    @scoped_tool
    async def read_source(
        ctx: ToolContext[OfficeContext],
        source_id: str,
        offset: int = 0,
        limit: int = READ_SOURCE_CHARS,
        reread: bool = False,
    ) -> str:
        """Read an indexed Tender source at a character offset. Follow next_offset to inspect the rest. A range already returned in this run is not sent again; pass reread=true only if you can no longer see it."""
        _require_passage_id(ctx.context, source_id)
        if reread:
            # One re-send per passage per run covers text a client compacted away;
            # repeated re-sends only repeat text the model still has.
            rereads = ctx.context.__dict__.setdefault("_rereads", set())
            if source_id in rereads:
                return json.dumps(
                    {
                        "id": source_id,
                        "text_offset": offset,
                        "already_returned": True,
                        "note": "This passage was already sent again in this run. Use the text above; do not request it again.",
                    },
                    ensure_ascii=False,
                )
            rereads.add(source_id)
            ctx.context.returned_reads.discard((source_id, offset, limit))
        source = _passage(ctx.context, source_id, offset, limit, "read_source")
        ctx.context.emit_event(
            "sources_read",
            "Tender source inspected.",
            {"source_ids": [source_id]},
        )
        return json.dumps(source, ensure_ascii=False)

    @scoped_tool
    async def read_package_map(ctx: ToolContext[OfficeContext]) -> str:
        """Read the package map made when the tender package was analysed: project identity, what the package contains, gaps, pages needing a second reading, and for each document its type, title, brief, where key content is and related documents. Use it first to decide which documents answer a request. It is orientation, not citable evidence."""
        from .package_analysis import DOCUMENT_TYPE_LABELS, package_map

        saved = package_map(ctx.context.repo, ctx.context.tender_id)
        if saved is None:
            return json.dumps(
                {
                    "available": False,
                    "detail": "The tender package has not been mapped yet; use list_documents and search_sources.",
                }
            )
        documents = saved.get("documents") or []
        return json.dumps(
            {
                "available": True,
                "current": saved.get("current"),
                "identity": saved.get("identity"),
                "overview": safe_text(saved.get("overview"), 1500),
                "gaps": saved.get("gaps") or [],
                "readability": saved.get("readability"),
                "documents": [
                    {
                        "document_id": doc.get("document_id"),
                        "path": safe_text(doc.get("relative_path"), 300),
                        "type": DOCUMENT_TYPE_LABELS.get(
                            doc.get("document_type"), doc.get("document_type")
                        ),
                        "title": safe_text(doc.get("title"), 200) or None,
                        "brief": safe_text(doc.get("brief"), 600),
                        "key_locations": doc.get("key_locations") or [],
                        "related": doc.get("related_document_ids") or [],
                    }
                    for doc in documents[:300]
                ],
                "listing_is_partial": len(documents) > 300,
            },
            ensure_ascii=False,
        )

    @scoped_tool
    async def read_whole_document(
        ctx: ToolContext[OfficeContext], artifact_id: str, offset: int = 0
    ) -> str:
        """Read one Tender document in full, in order, one page of about 12,000 characters at a time. Use it only when the request needs the whole document: listing every item, summarising or comparing a complete document, or when search keeps missing. Follow next_offset to continue. Everything returned counts as read and can be cited."""
        artifact_id = resolve_document_id(ctx.context, artifact_id)
        if offset < 0:
            raise ToolArgumentError("Use a nonnegative offset.")
        ctx.context.ensure_artifact_allowed(artifact_id)
        budget, passages, position = WHOLE_DOCUMENT_CHARS, [], offset
        while budget > 0:
            rows = ctx.context.repo.artifact_evidence(
                ctx.context.tender_id, artifact_id, offset=position, limit=20
            )
            if not rows:
                break
            for row in rows:
                length = len(row.get("text") or "")
                if passages and length > budget:
                    budget = 0
                    break
                passages.append(
                    _passage(
                        ctx.context,
                        row["id"],
                        0,
                        min(max(length, 1), READ_SOURCE_MAX_CHARS),
                        "read_whole_document",
                    )
                )
                budget -= length
                position += 1
                if budget <= 0:
                    break
        finished = not ctx.context.repo.artifact_evidence(
            ctx.context.tender_id, artifact_id, offset=position, limit=1
        )
        ctx.context.emit_event(
            "sources_read",
            "Tender document read in full.",
            {"source_ids": [p["id"] for p in passages]},
        )
        return json.dumps(
            {
                "passages": passages,
                "next_offset": None if finished else position,
                "document_finished": finished,
            },
            ensure_ascii=False,
        )

    @scoped_tool
    async def list_documents(ctx: ToolContext[OfficeContext]) -> str:
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

    from .engineering_tools import engineering_tools
    from .evidence_tools import evidence_tools
    from .office_business import business_tools
    from .office_knowledge import knowledge_tools
    from .office_measurement import measurement_tools
    from .office_project import project_tools
    from .package_tools import package_tools

    definitions = [
        search_sources,
        read_source,
        list_documents,
        read_package_map,
        read_whole_document,
        view_document_page,
        *business_tools(),
        *knowledge_tools(),
        *project_tools(),
        *measurement_tools(),
        *engineering_tools(),
        *evidence_tools(),
        *package_tools(),
    ]
    return definitions
