"""Attributable deterministic calculations and intermediate draft artifacts."""

from __future__ import annotations

import hashlib
import json
import re

from .ai_connections import AIConnectionService
from .ai_tools import ToolArgumentError, ToolContext, tool
from .calculation_models import CalculationCheckRequest, CalculationRequest
from .calculations import CalculationService
from .capability_models import validate_result_value
from .db import dump, now
from .execution_context import identity_from_office_context
from .office_tools import OfficeContext
from .staff_models import OfficeConflict
from .work_product_models import WorkProductDraft, WorkProductKind
from .work_products import WorkProductService


def _sources(office, references):
    from .research_service import ResearchService

    identity = identity_from_office_context(office)
    ResearchService(office.repo).validate_work_product_refs(identity, references)
    for reference in references:
        if reference.startswith("public_citation:"):
            continue
        office.ensure_evidence_allowed(reference, tool_id=None)
        if reference not in office.seen_sources:
            raise ValueError("Read each source before using it in a work product or calculation.")


def _work(ctx, capability, payload, action, event, *, source_refs=()):
    office = ctx.context
    if not isinstance(office, OfficeContext) or not ctx.invocation_id:
        raise ValueError(
            "Engineering work requires an active office context and trusted invocation identity."
        )
    validate_result_value(payload)
    identity = identity_from_office_context(office, invocation_id=ctx.invocation_id)
    receipt_actor = f"{identity.actor_id}:{office.assignment_id or 'manager'}"
    payload_hash = hashlib.sha256(dump(payload).encode()).hexdigest()
    with AIConnectionService(office.repo).authority_guard(), office.repo.atomic() as conn:
        # Stop, source revisions and grant changes serialize with local writes.
        # No await or provider operation occurs while this guard is held.
        if office.repo.get_run(office.run_id)["status"] not in {"queued", "running"}:
            raise InterruptedError("This Tender run is no longer active.")
        office.require_tool(capability)
        office._active_root()
        office.ensure_scope_current()
        _sources(office, source_refs)
        conn.execute("""CREATE TABLE IF NOT EXISTS engineering_work_receipts (
            root_id TEXT NOT NULL, actor_id TEXT NOT NULL, invocation_id TEXT NOT NULL,
            capability TEXT NOT NULL, payload_hash TEXT NOT NULL, result_json TEXT NOT NULL,
            created_at TEXT NOT NULL, PRIMARY KEY(root_id,actor_id,invocation_id))""")
        previous = conn.execute(
            "SELECT * FROM engineering_work_receipts WHERE root_id=? AND actor_id=? AND invocation_id=?",
            (office.run_id, receipt_actor, ctx.invocation_id),
        ).fetchone()
        if previous:
            if previous["payload_hash"] != payload_hash or previous["capability"] != capability:
                raise OfficeConflict("This engineering operation already saved different inputs.")
            return previous["result_json"]
        result = action(identity)
        if hasattr(result, "model_dump"):
            result = result.model_dump(mode="json")
        encoded = dump(result)
        validate_result_value(json.loads(encoded))
        conn.execute(
            "INSERT INTO engineering_work_receipts VALUES(?,?,?,?,?,?,?)",
            (
                office.run_id,
                receipt_actor,
                ctx.invocation_id,
                capability,
                payload_hash,
                encoded,
                now(),
            ),
        )
        office.repo.event(
            office.run_id,
            event,
            "Engineering work saved as a draft.",
            {
                "actor_id": identity.actor_id,
                "assignment_id": office.assignment_id,
                "calculation_id": result.get("calculation_id", result.get("id"))
                if event.startswith("calculation_")
                else None,
                "reproducible": result.get("reproducible"),
                "product_id": result.get("product_id"),
                "version_id": result.get("id") if event == "saved_work_product" else None,
                "version": result.get("version"),
            },
        )
        return encoded


def engineering_tools():
    @tool(read_only=False, idempotent=True, requires_invocation_id=True)
    async def calculate_engineering(
        ctx: ToolContext[OfficeContext],
        method: str,
        inputs: dict,
        units: dict | None = None,
        precision: str = "0.01",
        source_refs: list[str] | None = None,
    ) -> str:
        """Calculate product, sum, difference or add_units using decimal values and checked unit conversion. convert_unit takes inputs.value and units.from/units.to. Pass numbers as strings. This saves a draft without changing accepted quantities or prices."""
        if not re.fullmatch(r"1|0\.0{0,11}1", precision):
            raise ValueError("Choose decimal precision from 1 through 0.000000000001.")
        references = source_refs or []
        request = CalculationRequest(
            method_id=method,
            method_version="1",
            inputs=inputs,
            units=units or {},
            precision=precision,
            idempotency_key=ctx.invocation_id,
        )

        def calculate(identity):
            result = (
                CalculationService(ctx.context.repo)
                .calculate(identity, request)
                .model_dump(mode="json")
            )
            return {**result, "source_refs": references}

        return _work(
            ctx,
            "calculate_engineering",
            {"request": request.model_dump(), "source_refs": references},
            calculate,
            "calculation_completed",
            source_refs=references,
        )

    @tool(read_only=False, idempotent=True, requires_invocation_id=True)
    async def check_engineering_calculation(
        ctx: ToolContext[OfficeContext], calculation_id: str
    ) -> str:
        """Independently recompute a saved calculation from its stored inputs and compare its result. A reproducible calculation is still subject to engineering source review."""

        def calculate(identity):
            with ctx.context.repo.db.connect() as conn:
                original = conn.execute(
                    "SELECT result_json FROM engineering_work_receipts WHERE root_id=? AND capability='calculate_engineering' AND json_extract(result_json,'$.id')=? LIMIT 1",
                    (ctx.context.run_id, calculation_id),
                ).fetchone()
            if original is None:
                raise ValueError(
                    "Choose a calculation produced in this work root, or reproduce its inputs as a new calculation."
                )
            references = json.loads(original["result_json"]).get("source_refs", [])
            for reference in references:
                if not reference.startswith("public_citation:"):
                    ctx.context.ensure_evidence_allowed(reference, tool_id=None)
            result = CalculationService(ctx.context.repo).check(
                identity,
                CalculationCheckRequest(
                    calculation_id=calculation_id, method_id="recompute", method_version="1"
                ),
            )
            return {
                "calculation_id": calculation_id,
                "reproducible": result["reproducible"],
                "source_review_required": True,
                "source_refs": references,
                "record": result["record"].model_dump(mode="json"),
            }

        return _work(
            ctx,
            "check_engineering_calculation",
            {"calculation_id": calculation_id},
            calculate,
            "calculation_checked",
        )

    @tool(read_only=False, idempotent=True, requires_invocation_id=True)
    async def save_work_product(
        ctx: ToolContext[OfficeContext],
        kind: WorkProductKind,
        title: str,
        rows: list[dict] | None = None,
        content: str = "",
        source_refs: list[str] | None = None,
        product_id: str | None = None,
        expected_version: int | None = None,
    ) -> str:
        """Save an intermediate draft table, comparison, chart, calculation sheet, note, timeline or document. Cite inspected evidence IDs or validated public_citation references. A new product is created unless its ID and current version are explicitly supplied."""
        references = source_refs or []
        draft = WorkProductDraft(
            kind=kind,
            title=title,
            rows=rows or [],
            content=content,
            source_refs=references,
            product_id=product_id,
            expected_version=expected_version,
            idempotency_key=f"{ctx.context.run_id}:{ctx.invocation_id}",
        )
        return _work(
            ctx,
            "save_work_product",
            draft.model_dump(),
            lambda identity: WorkProductService(ctx.context.repo).save_draft(identity, draft),
            "saved_work_product",
            source_refs=references,
        )

    @tool
    async def read_work_product(
        ctx: ToolContext[OfficeContext],
        product_id: str,
        version: int,
        offset: int = 0,
        limit: int = 50,
    ) -> str:
        """Read a specific saved work-product version and a bounded page of its rows. Inspect its cited sources separately before adopting its findings."""
        office = ctx.context
        office.require_tool("read_work_product")
        office._active_root()
        office.ensure_scope_current()
        if offset < 0 or not 1 <= limit <= 100:
            raise ValueError("Choose a non-negative row offset and a limit of 1–100.")
        result = WorkProductService(office.repo).get(office.tender_id, product_id, version)
        for reference in result.source_refs:
            if not reference.startswith("public_citation:"):
                office.ensure_evidence_allowed(reference, tool_id=None)
        return dump(
            {
                **result.model_dump(mode="json"),
                "rows": result.rows[offset : offset + limit],
                "content": result.sanitized_content[:16000],
                "sanitized_content": result.sanitized_content[:16000],
                "total_rows": len(result.rows),
                "next_offset": offset + limit if offset + limit < len(result.rows) else None,
            }
        )

    @tool
    async def list_work_products(
        ctx: ToolContext[OfficeContext], offset: int = 0, limit: int = 20
    ) -> str:
        """List saved work products newest first: ID, title, kind, current version and whether a source behind it changed. Use read_work_product for the content."""
        office = ctx.context
        office.require_tool("list_work_products")
        office._active_root()
        office.ensure_scope_current()
        if offset < 0 or not 1 <= limit <= 50:
            raise ToolArgumentError("Choose a non-negative offset and a limit of 1–50.")
        page = WorkProductService(office.repo).list(office.tender_id, offset=offset, limit=limit)
        items = [
            item
            for item in page.items
            if not office.is_staff
            or all(
                office.evidence_allowed(reference)
                for reference in item.source_refs
                if not reference.startswith("public_citation:")
            )
        ]
        return dump(
            {
                "items": [
                    {
                        "product_id": item.product_id,
                        "title": item.title,
                        "kind": item.kind,
                        "version": item.version,
                        "row_count": item.row_count,
                        "dependency_state": item.dependency_state,
                        "created_at": item.created_at,
                    }
                    for item in items
                ],
                "withheld_count": len(page.items) - len(items),
                "next_offset": page.next_offset,
                "total": page.total,
            }
        )

    return [
        calculate_engineering,
        check_engineering_calculation,
        save_work_product,
        read_work_product,
        list_work_products,
    ]
