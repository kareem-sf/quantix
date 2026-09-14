"""Read-only business tools and validation/publication of proposed commercial work."""

import json
import re
from typing import Literal

from .ai_tools import ToolContext
from .correspondence import QuoteService
from .db import record
from .estimates import EstimateService
from .office_tools import OfficeContext, redact_text, safe_text, scoped_tool

RecordType = Literal["findings", "decisions", "tasks", "runs", "messages", "takeoff"]
RECORD_TABLES = {name: name for name in ("findings", "decisions", "tasks", "runs", "messages")} | {
    "takeoff": "takeoff_lines"
}


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


def validate_business(output, context, web_sources):
    """Check commercial proposals. ``web_sources`` is None while the run is still searching;
    web-dependent checks then wait for the final check."""
    if not output.quote_drafts and not output.unit_rate_proposals:
        return
    read = [
        context.repo.get_evidence(context.tender_id, source_id)
        for source_id in context.seen_sources
    ]
    recipients = set(context.trusted_recipients)
    for finding in output.web_findings:
        if finding.urls and (
            web_sources is None or all(url in web_sources for url in finding.urls)
        ):
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
            if web_sources is not None and any(
                url not in web_sources for url in proposal.provenance.urls
            ):
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
        """List findings, decisions, tasks, runs, messages or saved takeoff lines. Use read_tender_record for complete records; summaries are not source evidence."""
        page(offset, limit)
        table = RECORD_TABLES[record_type]
        ctx.context.repo.get_tender(ctx.context.tender_id)
        if record_type == "takeoff":
            from .takeoff import TakeoffService

            lines = TakeoffService(ctx.context.repo).list(ctx.context.tender_id)
            fields = (
                "id",
                "description",
                "location",
                "unit",
                "quantity",
                "boq_item_id",
                "boq",
                "comparison",
                "difference",
                "difference_percent",
                "status",
                "is_current",
                "author",
            )
            payload = {
                "record_type": record_type,
                "total": len(lines),
                "records": [
                    line.model_dump(include=set(fields)) for line in lines[offset : offset + limit]
                ],
                "next_offset": offset + limit if offset + limit < len(lines) else None,
            }
            return json.dumps(_clean(payload), ensure_ascii=False)
        with ctx.context.repo.db.connect() as conn:
            rows = [
                record(row)
                for row in conn.execute(
                    f"SELECT * FROM {table} WHERE tender_id=? ORDER BY rowid LIMIT ? OFFSET ?",
                    (ctx.context.tender_id, limit + 1, offset),
                )
            ]
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
        selected = []
        for row in rows[:limit]:
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
            "next_offset": offset + limit if len(rows) > limit else None,
        }
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
        page(offset, limit)
        estimates = EstimateService(ctx.context.repo)
        view = estimates.view(ctx.context.tender_id)
        selected = []
        for item in view["items"][offset : offset + limit]:
            basis = estimates.rate_basis(ctx.context.tender_id, item["id"])
            ctx.context.stage_item_base(item["id"], basis["fingerprint"])
            selected.append(
                {
                    "item": item,
                    "basis_fingerprint": basis["fingerprint"],
                    "source": ctx.context.source(item["source_id"], tool_id="inspect_estimate"),
                }
            )
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
        page(offset, limit)
        rows = QuoteService(ctx.context.repo).list_drafts(ctx.context.tender_id)
        selected = []
        for row in rows[offset : offset + limit]:
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
            "total": len(rows),
            "next_offset": offset + limit if offset + limit < len(rows) else None,
        }
        return json.dumps(_clean(result), ensure_ascii=False)

    @scoped_tool
    async def read_quote_replies(
        ctx: ToolContext[OfficeContext], quote_id: str, offset: int, limit: int
    ) -> str:
        """Read supplier reply source evidence. Receipt is not acceptance of a rate; read remaining source IDs separately when a reply is long."""
        page(offset, limit)
        rows = QuoteService(ctx.context.repo).replies(ctx.context.tender_id, quote_id)
        selected = [
            {
                **{key: row.get(key) for key in ("id", "sender", "received_at", "subject")},
                "sources": [
                    ctx.context.source(source_id, tool_id="read_quote_replies")
                    for source_id in row["source_ids"][:2]
                ],
                "remaining_source_ids": row["source_ids"][2:],
            }
            for row in rows[offset : offset + limit]
        ]
        result = {
            "replies": selected,
            "total": len(rows),
            "next_offset": offset + limit if offset + limit < len(rows) else None,
        }
        return json.dumps(_clean(result), ensure_ascii=False)

    return [
        inspect_tender_records,
        read_tender_record,
        inspect_estimate,
        inspect_quote_requests,
        read_quote_replies,
    ]
