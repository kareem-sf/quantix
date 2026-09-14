"""Read-only business tools and validation/publication of proposed commercial work."""

import asyncio
import json
import re
from typing import Literal

from .ai_tools import ToolContext
from .correspondence import QuoteService
from .db import record
from .estimates import EstimateService
from .office_tools import OfficeContext, redact_text, safe_text, scoped_tool

RecordType = Literal["findings", "decisions", "tasks", "runs", "messages"]
RECORD_TABLES = {name: name for name in ("findings", "decisions", "tasks", "runs", "messages")}


def recipient_addresses(text):
    return {
        address.rstrip(".").casefold()
        for address in re.findall(r"[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9.-]+", text)
    }


def _clean(value, *, _source_container=False):
    if isinstance(value, str):
        # ``OfficeContext.source`` has already redacted and bounded the
        # requested original range. Preserve that complete transformed text;
        # applying the ordinary metadata bound here would drop its suffix
        # while leaving its original text_length/next_offset claims intact.
        return redact_text(value) if _source_container else safe_text(value, 12000)
    if isinstance(value, dict):
        return {
            key: _clean(
                item,
                _source_container=(_source_container and key == "text")
                or key in {"source", "sources"},
            )
            for key, item in value.items()
        }
    if isinstance(value, list):
        return [_clean(item, _source_container=_source_container) for item in value]
    return value


def _redact_record(value):
    if isinstance(value, str):
        return redact_text(value)
    if isinstance(value, dict):
        return {key: _redact_record(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_redact_record(item) for item in value]
    return value


def _record_source_ids(value):
    """Collect every persisted evidence reference from one business record."""

    found = []

    def collect(item):
        if isinstance(item, dict):
            for key, child in item.items():
                if key in {"source_ids", "source_ids_read", "supporting_source_ids"} and isinstance(
                    child, list
                ):
                    found.extend(source_id for source_id in child if isinstance(source_id, str))
                elif key == "source_id" and isinstance(child, str):
                    found.append(child)
                else:
                    collect(child)
        elif isinstance(item, list):
            for child in item:
                collect(child)

    collect(value)
    return list(dict.fromkeys(found))


def _record_artifact_ids(value):
    found = []

    def collect(item):
        if isinstance(item, dict):
            for key, child in item.items():
                if key == "artifact_id" and isinstance(child, str):
                    found.append(child)
                elif key in {
                    "artifact_ids",
                    "attachment_ids",
                    "related_artifact_ids",
                } and isinstance(child, list):
                    found.extend(identifier for identifier in child if isinstance(identifier, str))
                else:
                    collect(child)
        elif isinstance(item, list):
            for child in item:
                collect(child)

    collect(value)
    return list(dict.fromkeys(found))


def _record_allowed(context, row, record_type=None):
    if not context.is_staff:
        return True
    if record_type in {"messages", "runs"}:
        return False
    source_ids = _record_source_ids(row)
    artifact_ids = _record_artifact_ids(row)
    return (
        bool(source_ids or artifact_ids)
        and all(context.evidence_allowed(source_id) for source_id in source_ids)
        and all(_artifact_allowed(context, artifact_id) for artifact_id in artifact_ids)
    )


def _artifact_allowed(context, artifact_id):
    try:
        context.ensure_artifact_allowed(artifact_id)
    except (KeyError, ValueError):
        return False
    return True


def _page_rows(context, rows, offset, limit, predicate=lambda row: True, *, pre_paged=False):
    """Filter provenance before pagination for scoped actors."""

    if not context.is_staff:
        if pre_paged:
            return rows[:limit], len(rows) > limit
        return rows[offset : offset + limit], offset + limit < len(rows)
    else:
        visible = [row for row in rows if predicate(row)]
    return visible[offset : offset + limit], offset + limit < len(visible)


def validate_business(output, context, web_sources):
    context.ensure_scope_current()
    if not output.quote_drafts and not output.unit_rate_proposals:
        return
    read = [
        context.repo.get_evidence(context.tender_id, source_id)
        for source_id in context.seen_sources
    ]
    recipients = set(context.trusted_recipients)
    for finding in output.web_findings:
        if finding.urls and all(url in web_sources for url in finding.urls):
            recipients.update(recipient_addresses(finding.detail))
    for source in read:
        if context.repo.get_artifact(context.tender_id, source["artifact_id"])["is_current"]:
            recipients.update(context.source_recipients.get(source["id"], set()))
    read_artifacts = {source["artifact_id"] for source in read}
    for draft in output.quote_drafts:
        context.validate_sources(draft.source_ids)
        if any(address.casefold() not in recipients for address in [*draft.to, *draft.cc]):
            raise ValueError(
                "A proposed supplier recipient was not present in read Tender evidence, attributed web findings, or the engineer's request."
            )
        for artifact_id in draft.attachment_ids:
            artifact = context.ensure_artifact_allowed(artifact_id)
            if not artifact["is_current"] or artifact_id not in read_artifacts:
                raise ValueError(
                    "A proposed attachment must be a current Tender original that was inspected."
                )
            context.repo.object_path(context.tender_id, artifact_id)
    if output.unit_rate_proposals:
        estimates = EstimateService(context.repo)
        for proposal in output.unit_rate_proposals:
            if proposal.item_id not in context.item_bases:
                raise ValueError("The proposed rate's BOQ item was not read in this run.")
            payload = proposal.model_dump(mode="json", exclude={"item_id"})
            _, basis = estimates.validate_rate(
                context.tender_id, proposal.item_id, payload, context.item_bases[proposal.item_id]
            )
            context.validate_sources([basis["source_id"], *proposal.provenance.source_ids])
            if any(url not in web_sources for url in proposal.provenance.urls):
                raise ValueError(
                    "A proposed rate cites a URL not returned by this run's web research."
                )


def publish_business(output, context):
    """Called only by publish_prepared in the job owner's final transaction."""
    drafts, rates = [], []
    if output.quote_drafts:
        quotes = QuoteService(context.repo)
        drafts = [
            quotes.create_draft(context.tender_id, draft.model_dump())
            for draft in output.quote_drafts
        ]
    if output.unit_rate_proposals:
        estimates = EstimateService(context.repo)
        rates = [
            estimates.propose_rate(
                context.tender_id,
                proposal.item_id,
                proposal.model_dump(mode="json", exclude={"item_id"}),
                expected_basis=context.item_bases[proposal.item_id],
                run_id=context.run_id,
            )
            for proposal in output.unit_rate_proposals
        ]
    return {"quote_drafts": drafts, "unit_rate_proposals": rates}


def business_tools():
    def page(offset, limit):
        if offset < 0 or not 1 <= limit <= 20:
            raise ValueError("Use a nonnegative offset and a limit from 1 to 20.")

    @scoped_tool
    async def inspect_tender_records(
        ctx: ToolContext[OfficeContext], record_type: RecordType, offset: int, limit: int
    ) -> str:
        """List findings, decisions, tasks, runs or messages. Use read_tender_record for complete records; summaries are not source evidence."""
        ctx.context.require_tool("inspect_tender_records")
        ctx.context.ensure_scope_current()
        page(offset, limit)
        table = RECORD_TABLES[record_type]
        ctx.context.repo.get_tender(ctx.context.tender_id)
        with ctx.context.repo.db.connect() as conn:
            if ctx.context.is_staff:
                initial_total = conn.execute(
                    f"SELECT COUNT(*) FROM {table} WHERE tender_id=?",
                    (ctx.context.tender_id,),
                ).fetchone()[0]
                rows, raw_offset = [], 0
                batch_size = 200
                while raw_offset < initial_total and len(rows) < offset + limit + 1:
                    batch = [
                        record(row)
                        for row in conn.execute(
                            f"SELECT * FROM {table} WHERE tender_id=? ORDER BY rowid LIMIT ? OFFSET ?",
                            (
                                ctx.context.tender_id,
                                min(batch_size, initial_total - raw_offset),
                                raw_offset,
                            ),
                        )
                    ]
                    raw_offset += len(batch)
                    if not batch:
                        break
                    rows.extend(
                        row for row in batch if _record_allowed(ctx.context, row, record_type)
                    )
            else:
                rows = [
                    record(row)
                    for row in conn.execute(
                        f"SELECT * FROM {table} WHERE tender_id=? ORDER BY rowid LIMIT ? OFFSET ?",
                        (ctx.context.tender_id, limit + 1, offset),
                    )
                ]
        visible_rows, has_more = _page_rows(
            ctx.context,
            rows,
            offset,
            limit,
            lambda row: _record_allowed(ctx.context, row, record_type),
            pre_paged=True,
        )
        selected = []
        fields = (
            "id",
            "title",
            "kind",
            "state",
            "is_stale",
            "status",
            "role",
            "origin",
            "target_type",
            "target_id",
            "decision",
            "plan_id",
            "source_ids",
            "created_at",
            "updated_at",
        )
        for row in visible_rows:
            preview = next(
                (
                    row[key]
                    for key in ("content", "detail", "rationale", "description", "instruction")
                    if isinstance(row.get(key), str)
                ),
                "",
            )
            selected.append(
                {
                    **{key: row[key] for key in fields if key in row},
                    "preview": safe_text(preview, 1000),
                    "preview_is_partial": len(preview) > 1000,
                }
            )
        payload = {
            "record_type": record_type,
            "records": selected,
            "next_offset": offset + limit if has_more else None,
        }
        if ctx.context.is_staff:
            payload.update(
                {
                    "scope_filter_applied": True,
                    "withheld_reason": "Records without complete permitted source provenance are outside this assignment's reviewed scope.",
                }
            )
        return json.dumps(_clean(payload), ensure_ascii=False)

    @scoped_tool
    async def read_tender_record(
        ctx: ToolContext[OfficeContext],
        record_type: RecordType,
        record_id: str,
        offset: int = 0,
        limit: int = 8000,
    ) -> str:
        """Read full stored record JSON in bounded character pages. Follow next_offset; source references still need source-tool reading before citation."""
        ctx.context.require_tool("read_tender_record")
        ctx.context.ensure_scope_current()
        if offset < 0 or not 1 <= limit <= 12000:
            raise ValueError("Use a nonnegative record offset and a limit from 1 to 12000.")
        table = RECORD_TABLES[record_type]
        with ctx.context.repo.db.connect() as conn:
            row = record(
                conn.execute(
                    f"SELECT * FROM {table} WHERE id=? AND tender_id=?",
                    (record_id, ctx.context.tender_id),
                ).fetchone()
            )
        if ctx.context.is_staff and not _record_allowed(ctx.context, row, record_type):
            return json.dumps(
                {
                    "available": False,
                    "record_type": record_type,
                    "record_id": record_id,
                    "withheld_reason": "This record combines material outside the assignment's reviewed source scope and cannot be shown.",
                },
                ensure_ascii=False,
            )
        text = json.dumps(_redact_record(row), ensure_ascii=False)
        if offset > len(text):
            raise ValueError("The requested offset is beyond this record.")
        return json.dumps(
            {
                "record_type": record_type,
                "record_id": record_id,
                "state": row.get("state", row.get("status")),
                "is_stale": row.get("is_stale"),
                "json_text": text[offset : offset + limit],
                "text_offset": offset,
                "total_characters": len(text),
                "next_offset": offset + limit if offset + limit < len(text) else None,
            },
            ensure_ascii=False,
        )

    @scoped_tool
    async def inspect_estimate(ctx: ToolContext[OfficeContext], offset: int, limit: int) -> str:
        """Read current BOQ quantities, installed rates and incomplete pricing state. Rate proposals cannot alter these values."""
        ctx.context.require_tool("inspect_estimate")
        ctx.context.ensure_scope_current()
        page(offset, limit)
        estimates = EstimateService(ctx.context.repo)
        view = estimates.view(ctx.context.tender_id)
        all_items = view["items"]
        if ctx.context.is_staff:
            all_items = [item for item in all_items if _record_allowed(ctx.context, item)]
        selected = []
        for item in all_items[offset : offset + limit]:
            basis = estimates.rate_basis(ctx.context.tender_id, item["id"])
            ctx.context.stage_item_base(item["id"], basis["fingerprint"])
            selected.append(
                {
                    "item": item,
                    "basis_fingerprint": basis["fingerprint"],
                    "source": ctx.context.source(item["source_id"], tool_id="inspect_estimate"),
                }
            )
        if ctx.context.is_staff:
            result = {
                "items": selected,
                "next_offset": offset + limit if offset + limit < len(all_items) else None,
                "scope_filter_applied": True,
            }
        else:
            result = {
                **{key: value for key, value in view.items() if key != "items"},
                "items": selected,
                "total_items": len(view["items"]),
                "next_offset": offset + limit if offset + limit < len(view["items"]) else None,
            }
        return json.dumps(_clean(result), ensure_ascii=False)

    @scoped_tool
    async def inspect_quote_requests(
        ctx: ToolContext[OfficeContext], offset: int, limit: int
    ) -> str:
        """Read this Tender's saved quotation request drafts. Quantix never sends them; the engineer sends each request from their own mail program."""
        ctx.context.require_tool("inspect_quote_requests")
        ctx.context.ensure_scope_current()
        page(offset, limit)
        rows = QuoteService(ctx.context.repo).list_drafts(ctx.context.tender_id)
        visible_rows, has_more = _page_rows(
            ctx.context,
            rows,
            offset,
            limit,
            lambda row: _record_allowed(ctx.context, row),
        )
        selected = []
        for row in visible_rows:
            ctx.context.add_trusted_recipients(
                {address.casefold() for address in [*row["to"], *row["cc"]]}
            )
            selected.append(
                {
                    **{
                        key: row.get(key)
                        for key in (
                            "id",
                            "to",
                            "cc",
                            "subject",
                            "status",
                            "message_id",
                            "source_ids",
                        )
                    },
                    "body": row["body"][:3000],
                    "body_is_partial": len(row["body"]) > 3000,
                }
            )
        result = {
            "quotes": selected,
            "next_offset": offset + limit if has_more else None,
        }
        if ctx.context.is_staff:
            result["scope_filter_applied"] = True
        else:
            result["total"] = len(rows)
        return json.dumps(_clean(result), ensure_ascii=False)

    @scoped_tool
    async def read_quote_replies(
        ctx: ToolContext[OfficeContext], quote_id: str, offset: int, limit: int
    ) -> str:
        """Read supplier reply source evidence. Receipt is not acceptance of a rate; read remaining source IDs separately when a reply is long."""
        ctx.context.require_tool("read_quote_replies")
        ctx.context.ensure_scope_current()
        page(offset, limit)
        rows = QuoteService(ctx.context.repo).replies(ctx.context.tender_id, quote_id)
        visible_rows, has_more = _page_rows(
            ctx.context,
            rows,
            offset,
            limit,
            lambda row: _record_allowed(ctx.context, row),
        )
        selected = []
        for row in visible_rows:
            selected.append(
                {
                    **{
                        key: row.get(key)
                        for key in (
                            "id",
                            "sender",
                            "received_at",
                            "subject",
                        )
                    },
                    "sources": [
                        ctx.context.source(source_id, tool_id="read_quote_replies")
                        for source_id in row["source_ids"][:2]
                    ],
                    "remaining_source_ids": row["source_ids"][2:],
                }
            )
        result = {
            "replies": selected,
            "next_offset": offset + limit if has_more else None,
        }
        if ctx.context.is_staff:
            result["scope_filter_applied"] = True
        else:
            result["total"] = len(rows)
        return json.dumps(_clean(result), ensure_ascii=False)

    @scoped_tool
    async def search_semantic_sources(
        ctx: ToolContext[OfficeContext], query: str, limit: int
    ) -> str:
        """Search the local semantic index. Unavailable indexes return explicit status; this tool never substitutes keyword search."""
        ctx.context.require_tool("search_semantic_sources")
        ctx.context.ensure_scope_current()
        from .retrieval_service import retrieve
        from .semantic import SemanticService
        from .semantic_models import SemanticUnavailable

        if not query.strip() or len(query) > 2000 or not 1 <= limit <= 20:
            raise ValueError("Use a bounded semantic query and a result limit from 1 to 20.")
        semantic_service = ctx.context.semantic_service
        if semantic_service is None:
            semantic_service = SemanticService(ctx.context.repo)
        artifact_ids = list(ctx.context.reviewed_artifacts) if ctx.context.is_staff else None
        try:
            response = await asyncio.to_thread(
                retrieve,
                ctx.context.repo,
                ctx.context.tender_id,
                query,
                mode="meaning",
                limit=limit,
                semantic=semantic_service,
                artifact_ids=artifact_ids,
            )
        except SemanticUnavailable as exc:
            result = {
                "available": False,
                "status": exc.code,
                "detail": safe_text(str(exc)),
                "sources": [],
            }
            if ctx.context.is_staff:
                result["scope_filter_applied"] = True
            return json.dumps(result)
        sources = []
        for hit in response.hits:
            match = (hit.metadata or {}).get("semantic_match") or {}
            start = hit.spans[0].start if hit.spans else int(match.get("start") or 0)
            end = hit.spans[0].end if hit.spans else int(match.get("end") or start)
            source = ctx.context.source(
                hit.id,
                start,
                min(12000, max(1, end - start)),
                tool_id="search_semantic_sources",
            )
            source["score"] = hit.score
            source["found_by"] = hit.found_by
            source["semantic_match"] = {
                "start": start,
                "end": end,
                "structure": match.get("structure"),
                "language": match.get("language"),
                "source_version": hit.source_version,
            }
            sources.append(source)
        if ctx.context.semantic_service is None:
            ctx.context.set_semantic_service(semantic_service)
        result = {
            "available": True,
            "status": "ready",
            "sources": sources,
            "actual_mode": response.actual_mode,
            "limitations": response.limitations,
        }
        if ctx.context.is_staff:
            result.update(
                {
                    "partial_results": response.coverage.truncated,
                    "scope_filter_applied": True,
                    "limitation": None,
                }
            )
        return json.dumps(result, ensure_ascii=False)

    return [
        inspect_tender_records,
        read_tender_record,
        inspect_estimate,
        inspect_quote_requests,
        read_quote_replies,
        search_semantic_sources,
    ]
