"""Read-only evidence coverage and revision tools.

They report what extraction actually produced, how a revised document's text
changed, and which saved records still rest on the replaced version. None of
them reads a source into citable evidence, changes a record or approves work.
"""

from __future__ import annotations

import difflib
import json

from .ai_tools import ToolArgumentError, ToolContext
from .db import record
from .office_tools import OfficeContext, safe_text, scoped_tool

_COVERAGE_NOTE = (
    "Extracted text is not analysed or reviewed coverage. Pages without text may hold drawings, "
    "scans or tables that need OCR or visual review."
)
_COMPARE_NOTE = (
    "Text is compared page by page (or sheet range by range). Moved text shows as removed and added. "
    "Graphic-only drawing changes are not detected. Read the current source before citing it."
)
_IMPACT_NOTE = (
    "Records are matched by the replaced version's evidence IDs. A listed record may still be correct; "
    "it needs checking against the current document. Nothing was changed."
)
_PER_KIND = 20


def _document_coverage(conn, artifact: dict) -> dict:
    metadata = artifact.get("metadata") or {}
    warnings = artifact.get("warnings") or []
    no_text = sorted(
        {
            item.get("locator")
            for item in warnings
            if item.get("code") == "pdf_no_text" and item.get("locator")
        }
    )
    ocr_pages = conn.execute(
        "SELECT COUNT(*) FROM evidence WHERE artifact_id=? AND COALESCE(is_current,1)=1 AND json_extract(metadata_json,'$.method')='ocr'",
        (artifact["id"],),
    ).fetchone()[0]
    other = {}
    for item in warnings:
        if item.get("code") == "pdf_no_text":
            continue
        entry = other.setdefault(
            item.get("code") or "warning",
            {"count": 0, "message": safe_text(item.get("message"), 300)},
        )
        entry["count"] += 1
    row = {
        "artifact_id": artifact["id"],
        "name": safe_text(artifact["name"], 300),
        "kind": artifact["kind"],
        "status": artifact["status"],
        "version": artifact["version"],
        "page_count": metadata.get("page_count"),
        "pages_processed": metadata.get("pages_processed"),
        "segment_count": metadata.get("segment_count"),
        "pages_without_text": no_text[:50],
        "pages_without_text_count": len(no_text),
        "ocr_pages": int(ocr_pages),
        "other_warnings": [{"code": code, **entry} for code, entry in sorted(other.items())][:10],
    }
    return {key: value for key, value in row.items() if value not in (None, [], {})} | {
        "status": artifact["status"]
    }


def _has_exception(row: dict) -> bool:
    return (
        row["status"] != "extracted"
        or row.get("pages_without_text_count", 0) > 0
        or bool(row.get("other_warnings"))
        or (
            row.get("page_count") is not None
            and row.get("pages_processed") is not None
            and row["pages_processed"] < row["page_count"]
        )
    )


def _segments(conn, artifact_id: str) -> dict[str, dict]:
    segments: dict[str, dict] = {}
    for row in conn.execute(
        "SELECT id,locator,page,sheet,cell_range,text FROM evidence WHERE artifact_id=? AND COALESCE(is_current,1)=1 ORDER BY rowid",
        (artifact_id,),
    ):
        key = row["locator"]
        if key in segments:
            segments[key]["text"] += "\n" + row["text"]
        else:
            segments[key] = {"id": row["id"], "locator": key, "text": row["text"]}
    return segments


def _change_excerpt(before: str, after: str, limit: int = 1200) -> str:
    lines = difflib.unified_diff(before.splitlines(), after.splitlines(), lineterm="", n=1)
    kept = [line for line in lines if not line.startswith(("---", "+++"))]
    text = "\n".join(kept)
    return text if len(text) <= limit else text[:limit] + "\n…"


def _walk_strings(value, found: set[str]) -> None:
    if isinstance(value, str):
        found.add(value)
    elif isinstance(value, dict):
        for item in value.values():
            _walk_strings(item, found)
    elif isinstance(value, list):
        for item in value:
            _walk_strings(item, found)


def _mentions(raw: str | None, old_ids: set[str]) -> bool:
    if not raw:
        return False
    try:
        value = json.loads(raw)
    except (TypeError, ValueError):
        return False
    strings: set[str] = set()
    _walk_strings(value, strings)
    return not old_ids.isdisjoint(strings)


def _table_exists(conn, name: str) -> bool:
    return (
        conn.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?", (name,)
        ).fetchone()
        is not None
    )


def evidence_tools():
    @scoped_tool
    async def inspect_extraction_coverage(
        ctx: ToolContext[OfficeContext],
        offset: int = 0,
        limit: int = 20,
        exceptions_only: bool = True,
    ) -> str:
        """Check what document processing actually covered: per document, its status, pages processed, pages with no readable text, OCR pages and warnings. By default lists only documents with exceptions. Use it before claiming a package was fully read."""
        office = ctx.context
        if offset < 0 or not 1 <= limit <= 50:
            raise ToolArgumentError("Choose a non-negative offset and a limit of 1–50.")
        artifacts = office.repo.list_artifacts(office.tender_id)
        with office.repo.db.connect() as conn:
            rows = [_document_coverage(conn, artifact) for artifact in artifacts]
        totals = {
            "documents": len(rows),
            "extracted": sum(row["status"] == "extracted" for row in rows),
            "needs_attention": sum(row["status"] == "needs_attention" for row in rows),
            "unsupported": sum(row["status"] == "unsupported" for row in rows),
            "failed": sum(row["status"] == "failed" for row in rows),
            "not_processed": sum(row["status"] == "registered" for row in rows),
            "pages": sum(row.get("page_count") or 0 for row in rows),
            "pages_without_text": sum(row.get("pages_without_text_count", 0) for row in rows),
            "ocr_pages": sum(row.get("ocr_pages", 0) for row in rows),
            "documents_with_exceptions": sum(_has_exception(row) for row in rows),
        }
        listed = [row for row in rows if _has_exception(row)] if exceptions_only else rows
        page = listed[offset : offset + limit]
        return json.dumps(
            {
                "totals": totals,
                "documents": page,
                "next_offset": offset + limit if offset + limit < len(listed) else None,
                "limitation": _COVERAGE_NOTE,
            },
            ensure_ascii=False,
        )

    @scoped_tool
    async def compare_source_versions(
        ctx: ToolContext[OfficeContext],
        artifact_id: str,
        previous_version: int | None = None,
        offset: int = 0,
        limit: int = 10,
    ) -> str:
        """Compare a revised document's text with an earlier version of the same file (the one before it by default): changed, added and removed pages or sheet ranges with short change excerpts and the current evidence IDs to read."""
        office = ctx.context
        if offset < 0 or not 1 <= limit <= 30:
            raise ToolArgumentError("Choose a non-negative offset and a limit of 1–30.")
        current = office.ensure_artifact_allowed(artifact_id)
        wanted = current["version"] - 1 if previous_version is None else previous_version
        if wanted < 1 or wanted >= current["version"]:
            raise ToolArgumentError(
                f"This document is version {current['version']}. Choose an earlier version from 1 to {current['version'] - 1}."
                if current["version"] > 1
                else "This document has no earlier version to compare."
            )
        with office.repo.db.connect() as conn:
            earlier_row = conn.execute(
                "SELECT * FROM artifacts WHERE tender_id=? AND relative_path=? AND version=?",
                (office.tender_id, current["relative_path"], wanted),
            ).fetchone()
            if earlier_row is None:
                raise ToolArgumentError(f"Version {wanted} of this document is not saved.")
            earlier = record(earlier_row)
            before, after = _segments(conn, earlier["id"]), _segments(conn, current["id"])
        changes = []
        for locator, segment in after.items():
            old = before.get(locator)
            if old is None:
                changes.append(
                    {
                        "change": "added",
                        "locator": locator,
                        "evidence_id": segment["id"],
                        "excerpt": safe_text(segment["text"], 600),
                    }
                )
            elif old["text"].split() != segment["text"].split():
                changes.append(
                    {
                        "change": "changed",
                        "locator": locator,
                        "evidence_id": segment["id"],
                        "excerpt": _change_excerpt(old["text"], segment["text"]),
                    }
                )
        for locator, segment in before.items():
            if locator not in after:
                changes.append(
                    {
                        "change": "removed",
                        "locator": locator,
                        "excerpt": safe_text(segment["text"], 600),
                    }
                )
        office.emit_event(
            "source_versions_compared",
            "Two versions of a Tender document were compared.",
            {
                "artifact_id": current["id"],
                "from_version": wanted,
                "to_version": current["version"],
                "changes": len(changes),
            },
        )
        return json.dumps(
            {
                "document": safe_text(current["name"], 300),
                "from_version": wanted,
                "to_version": current["version"],
                "summary": {
                    "changed": sum(item["change"] == "changed" for item in changes),
                    "added": sum(item["change"] == "added" for item in changes),
                    "removed": sum(item["change"] == "removed" for item in changes),
                    "unchanged": len(after) - sum(item["change"] != "removed" for item in changes),
                },
                "changes": changes[offset : offset + limit],
                "next_offset": offset + limit if offset + limit < len(changes) else None,
                "limitation": _COMPARE_NOTE,
            },
            ensure_ascii=False,
        )

    @scoped_tool
    async def trace_change_impact(ctx: ToolContext[OfficeContext], artifact_id: str) -> str:
        """List saved work that still rests on earlier versions of a revised document: findings, tasks, BOQ rows, quantity and rate proposals, measurements, submission requirements, project map items, generated documents, working notes, work products and the working brief. It changes nothing."""
        office = ctx.context
        current = office.ensure_artifact_allowed(artifact_id)
        tender_id = office.tender_id
        with office.repo.db.connect() as conn:
            earlier = [
                record(row)
                for row in conn.execute(
                    "SELECT * FROM artifacts WHERE tender_id=? AND relative_path=? AND version<? ORDER BY version",
                    (tender_id, current["relative_path"], current["version"]),
                )
            ]
            if not earlier:
                raise ToolArgumentError(
                    "This document has no earlier version, so nothing rests on a replaced version."
                )
            earlier_ids = {item["id"] for item in earlier}
            old_ids = {
                row[0]
                for artifact in earlier
                for row in conn.execute(
                    "SELECT id FROM evidence WHERE artifact_id=?", (artifact["id"],)
                )
            }
            match_ids = old_ids | earlier_ids
            affected: dict[str, list[dict]] = {}

            def add(kind, item):
                affected.setdefault(kind, []).append(item)

            for row in conn.execute(
                "SELECT id,title,state,is_stale,source_ids_json FROM findings WHERE tender_id=?",
                (tender_id,),
            ):
                if _mentions(row["source_ids_json"], match_ids):
                    add(
                        "findings",
                        {
                            "id": row["id"],
                            "title": safe_text(row["title"], 200),
                            "state": row["state"],
                            "marked_stale": bool(row["is_stale"]),
                        },
                    )
            for row in conn.execute(
                "SELECT id,title,status,source_ids_json,result_json FROM tasks WHERE tender_id=?",
                (tender_id,),
            ):
                if _mentions(row["source_ids_json"], match_ids) or _mentions(
                    row["result_json"], match_ids
                ):
                    add(
                        "tasks",
                        {
                            "id": row["id"],
                            "title": safe_text(row["title"], 200),
                            "status": row["status"],
                        },
                    )
            if _table_exists(conn, "boq_items"):
                for row in conn.execute(
                    "SELECT id,active,data_json,source_id,artifact_id FROM boq_items WHERE tender_id=?",
                    (tender_id,),
                ):
                    if row["source_id"] in old_ids or row["artifact_id"] in earlier_ids:
                        data = json.loads(row["data_json"] or "{}")
                        add(
                            "boq_items",
                            {
                                "id": row["id"],
                                "description": safe_text(data.get("description"), 200),
                                "active": bool(row["active"]),
                            },
                        )
            if _table_exists(conn, "quantity_proposals"):
                for row in conn.execute(
                    "SELECT id,item_id,status,data_json FROM quantity_proposals WHERE tender_id=?",
                    (tender_id,),
                ):
                    if _mentions(row["data_json"], match_ids):
                        add(
                            "quantity_proposals",
                            {"id": row["id"], "item_id": row["item_id"], "status": row["status"]},
                        )
            if _table_exists(conn, "rate_proposals"):
                for row in conn.execute(
                    "SELECT id,item_id,status,source_ids_json,basis_json FROM rate_proposals WHERE tender_id=?",
                    (tender_id,),
                ):
                    if _mentions(row["source_ids_json"], match_ids) or _mentions(
                        row["basis_json"], match_ids
                    ):
                        add(
                            "rate_proposals",
                            {"id": row["id"], "item_id": row["item_id"], "status": row["status"]},
                        )
            if _table_exists(conn, "measurements"):
                for row in conn.execute(
                    "SELECT id,source_id,artifact_id FROM measurements WHERE tender_id=?",
                    (tender_id,),
                ):
                    if row["source_id"] in old_ids or row["artifact_id"] in earlier_ids:
                        add("measurements", {"id": row["id"]})
            for table, kind, title_keys in (
                ("submission_requirements", "submission_requirements", ("title",)),
                ("project_nodes", "project_map_items", ("name", "title")),
            ):
                if not _table_exists(conn, table):
                    continue
                for row in conn.execute(
                    f"SELECT id,payload_json FROM {table} WHERE tender_id=?", (tender_id,)
                ):
                    if _mentions(row["payload_json"], match_ids):
                        payload = json.loads(row["payload_json"] or "{}")
                        title = next(
                            (payload.get(key) for key in title_keys if payload.get(key)), ""
                        )
                        add(kind, {"id": row["id"], "title": safe_text(title, 200)})
            if _table_exists(conn, "generated_outputs"):
                for row in conn.execute(
                    "SELECT id,record_json FROM generated_outputs WHERE tender_id=?", (tender_id,)
                ):
                    if _mentions(row["record_json"], match_ids):
                        payload = json.loads(row["record_json"] or "{}")
                        add(
                            "generated_documents",
                            {"id": row["id"], "filename": safe_text(payload.get("filename"), 200)},
                        )
            if _table_exists(conn, "research_dependencies"):
                owners = conn.execute(
                    f"SELECT DISTINCT owner_kind,owner_id FROM research_dependencies WHERE tender_id=? AND artifact_id IN ({','.join('?' * len(earlier_ids))})",
                    (tender_id, *sorted(earlier_ids)),
                ).fetchall()
                labels = {
                    "work_product_version": "work_products",
                    "work_brief_version": "working_brief",
                }
                for owner in owners:
                    add(
                        labels.get(owner["owner_kind"], owner["owner_kind"]),
                        {"id": owner["owner_id"]},
                    )
        result = {
            "document": safe_text(current["name"], 300),
            "current_version": current["version"],
            "replaced_versions": [item["version"] for item in earlier],
            "affected_counts": {kind: len(items) for kind, items in sorted(affected.items())},
            "affected": {kind: items[:_PER_KIND] for kind, items in sorted(affected.items())},
            "lists_are_partial": any(len(items) > _PER_KIND for items in affected.values()),
            "limitation": _IMPACT_NOTE,
        }
        office.emit_event(
            "change_impact_traced",
            "Saved work resting on an earlier document version was listed.",
            {"artifact_id": current["id"], "affected_counts": result["affected_counts"]},
        )
        return json.dumps(result, ensure_ascii=False)

    return [inspect_extraction_coverage, compare_source_versions, trace_change_impact]
