"""What one Manager job produced that needs the engineer's OK, decided in the chat.

The engineer reads a short list under the Manager's reply and accepts or rejects
each item there. Each decision goes through the record's own service, so the
same checks apply as anywhere else.
"""

from __future__ import annotations

from typing import Literal

from pydantic import Field

from .models import ApiModel

ApprovalKind = Literal["finding", "plan", "requirement", "boq_row", "takeoff", "quantity", "rate"]
ApprovalState = Literal["waiting", "accepted", "rejected", "closed"]


class ApprovalItem(ApiModel):
    kind: ApprovalKind
    id: str
    title: str
    detail: str = ""
    state: ApprovalState
    # A plan is changed by asking the Manager, not rejected.
    can_reject: bool = True


class ApprovalDetail(ApprovalItem):
    """One item in full, for reading before deciding."""

    facts: list[str] = Field(default_factory=list)
    source_ids: list[str] = Field(default_factory=list)


class ApprovalDecision(ApiModel):
    kind: ApprovalKind
    id: str = Field(min_length=1, max_length=100)
    decision: Literal["accept", "reject"]
    note: str = Field(default="", max_length=2000)


def _tables(conn) -> set[str]:
    return {row[0] for row in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")}


def _rate_amount(payload: dict) -> str:
    """A proposed rate as one number: the direct rate, or its build-up added up."""

    from decimal import Decimal, InvalidOperation

    try:
        if payload.get("unit_rate") is not None:
            amount = Decimal(str(payload["unit_rate"]))
        elif payload.get("components"):
            amount = sum(
                Decimal(str(part.get("quantity") or 0)) * Decimal(str(part.get("unit_rate") or 0))
                for part in payload["components"]
            )
        else:
            return "No rate"
    except InvalidOperation:
        return "Rate"
    return f"{amount:,.2f}".rstrip("0").rstrip(".")


def _row_title(reference, description) -> str:
    """A BOQ row's item number and description, without the number twice when
    the description already starts with it."""

    reference, description = str(reference or "").strip(), str(description or "").strip()
    if reference and reference in description[: len(reference) + 60]:
        return description
    return " ".join(part for part in (reference, description) if part)


def _locator_words(locator) -> str:
    """ "page:12" as "page 12" and "sheet:BOQ/row:14" as "BOQ, row 14"."""

    words = []
    for part in str(locator or "").split("/"):
        key, _, value = part.partition(":")
        if not value:
            words.append(part)
        elif key in {"sheet", "range"}:
            words.append(value)
        elif key != "characters":
            words.append(f"{key} {value}")
    return ", ".join(word for word in words if word)


def _short(value, limit=600) -> str:
    text = " ".join(str(value or "").split())
    return text if len(text) <= limit else text[: limit - 1].rstrip() + "…"


def approvals_for_run(repo, conn, tender_id: str, run_id: str) -> list[dict]:
    """Records saved by this run, in the order the engineer should see them."""

    import json

    tables = _tables(conn)
    items: list[dict] = []

    def add(kind, identifier, title, detail, state, can_reject=True):
        items.append(
            ApprovalItem(
                kind=kind,
                id=identifier,
                title=_short(title, 200),
                detail=_short(detail),
                state=state,
                can_reject=can_reject,
            ).model_dump()
        )

    for row in conn.execute(
        "SELECT id,title,status FROM plans WHERE tender_id=? AND run_id=? ORDER BY rowid",
        (tender_id, run_id),
    ):
        state = {"proposed": "waiting", "approved": "accepted"}.get(row["status"], "closed")
        add("plan", row["id"], row["title"], "", state, can_reject=False)

    for row in conn.execute(
        "SELECT id,title,detail,state,is_stale FROM findings WHERE tender_id=? AND run_id=? ORDER BY rowid",
        (tender_id, run_id),
    ):
        state = {
            "proposed": "closed" if row["is_stale"] else "waiting",
            "accepted": "accepted",
            "resolved": "accepted",
            "rejected": "rejected",
        }.get(row["state"], "closed")
        add("finding", row["id"], row["title"], row["detail"], state)

    if "submission_requirements" in tables:
        from .tender_requirements import RequirementService

        service = RequirementService(repo)
        for row in conn.execute(
            "SELECT id FROM submission_requirements WHERE tender_id=? "
            "AND json_extract(payload_json,'$.run_id')=? ORDER BY rowid",
            (tender_id, run_id),
        ):
            requirement = service.get(tender_id, row["id"])
            state = {
                "proposed": "waiting",
                "approved": "accepted",
                "withdrawn": "rejected",
            }.get(requirement.get("status"), "closed")
            add(
                "requirement",
                row["id"],
                requirement.get("title", "Submission requirement"),
                requirement.get("description") or requirement.get("detail") or "",
                state,
            )

    if "boq_items" in tables:
        for row in conn.execute(
            "SELECT id,active,data_json FROM boq_items WHERE tender_id=? "
            "AND json_extract(data_json,'$.run_id')=? ORDER BY rowid",
            (tender_id, run_id),
        ):
            data = json.loads(row["data_json"])
            state = (
                "rejected"
                if data.get("rejected")
                else "accepted"
                if data.get("confirmed")
                else "waiting"
                if row["active"]
                else "closed"
            )
            quantity = " ".join(
                filter(None, [str(data.get("supplied_quantity") or ""), data.get("unit")])
            )
            add(
                "boq_row",
                row["id"],
                _row_title(data.get("row_reference"), data.get("description")),
                " · ".join(
                    filter(
                        None, [quantity, data.get("document"), _locator_words(data.get("locator"))]
                    )
                ),
                state,
            )

    if "takeoff_lines" in tables:
        for row in conn.execute(
            "SELECT id,status,data_json FROM takeoff_lines WHERE tender_id=? AND run_id=? ORDER BY rowid",
            (tender_id, run_id),
        ):
            data = json.loads(row["data_json"])
            quantity = " ".join(filter(None, [str(data.get("quantity") or ""), data.get("unit")]))
            add(
                "takeoff",
                row["id"],
                data.get("description", "Takeoff line"),
                " · ".join(filter(None, [quantity, data.get("location")])),
                {"proposed": "waiting", "accepted": "accepted", "rejected": "rejected"}.get(
                    row["status"], "closed"
                ),
            )

    if "quantity_proposals" in tables:
        for row in conn.execute(
            """SELECT q.id,q.status,q.data_json,b.data_json AS item_json FROM quantity_proposals q
            JOIN boq_items b ON b.id=q.item_id
            WHERE q.tender_id=? AND json_extract(q.data_json,'$.run_id')=? ORDER BY q.rowid""",
            (tender_id, run_id),
        ):
            data, item = json.loads(row["data_json"]), json.loads(row["item_json"])
            add(
                "quantity",
                row["id"],
                f"Quantity for {item.get('description', 'a BOQ row')}",
                f"{data.get('quantity', '')} {item.get('unit', '')}".strip(),
                {"proposed": "waiting", "approved": "accepted", "rejected": "rejected"}.get(
                    row["status"], "closed"
                ),
            )

    if "rate_proposals" in tables:
        for row in conn.execute(
            """SELECT r.id,r.status,r.payload_json,b.data_json AS item_json FROM rate_proposals r
            JOIN boq_items b ON b.id=r.item_id
            WHERE r.tender_id=? AND r.run_id=? ORDER BY r.rowid""",
            (tender_id, run_id),
        ):
            payload, item = json.loads(row["payload_json"]), json.loads(row["item_json"])
            add(
                "rate",
                row["id"],
                f"Rate for {item.get('description', 'a BOQ row')}",
                f"{_rate_amount(payload)} {payload.get('currency', '')} per {item.get('unit', 'unit')}".strip(),
                {"proposed": "waiting", "approved": "accepted", "rejected": "rejected"}.get(
                    row["status"], "closed"
                ),
            )
    return items


def decide(repo, tender_id: str, request: ApprovalDecision, *, carry_out_plan=None) -> dict:
    """Apply one chat decision through the record's own service."""

    accept = request.decision == "accept"
    rationale = request.note.strip() or (
        "Accepted in the chat." if accept else "Rejected in the chat."
    )
    if request.kind == "finding":
        repo.decide_finding(tender_id, request.id, request.decision, rationale)
    elif request.kind == "plan":
        if not accept:
            raise ValueError("Ask the Tender Manager in the chat to change the plan.")
        repo.approve_plan(tender_id, request.id, rationale)
        if carry_out_plan is not None:
            carry_out_plan(tender_id, request.id)
    elif request.kind == "requirement":
        from .tender_requirements import RequirementService

        RequirementService(repo).decide(
            tender_id,
            request.id,
            {
                "engineer_confirmed": True,
                "rationale": rationale,
                "decision": "approve" if accept else "withdraw",
                # The engineer read the requirement in the chat before deciding.
                "applicability_reviewed": True,
            },
        )
    elif request.kind == "boq_row":
        from .estimates import EstimateService

        service = EstimateService(repo)
        if accept:
            # Accepting a row rejected earlier brings it back first.
            service.restore_source_row(tender_id, request.id, rationale)
            service.update_item(
                tender_id,
                request.id,
                {"engineer_confirmed": True, "rationale": rationale, "confirm_source": True},
            )
        else:
            service.reject_source_row(tender_id, request.id, rationale)
    elif request.kind == "takeoff":
        from .takeoff import TakeoffService
        from .takeoff_models import TakeoffReview

        TakeoffService(repo).review(
            tender_id,
            request.id,
            TakeoffReview(decision="accepted" if accept else "rejected", note=request.note.strip()),
        )
    elif request.kind == "quantity":
        from .estimates import EstimateService

        service = EstimateService(repo)
        if accept:
            service.approve_quantity(
                tender_id, request.id, {"engineer_confirmed": True, "rationale": rationale}
            )
        else:
            service.reject_quantity(tender_id, request.id, rationale)
    elif request.kind == "rate":
        from .estimates import EstimateService

        service = EstimateService(repo)
        if accept:
            service.approve_rate(
                tender_id,
                request.id,
                {"engineer_confirmed": True, "rationale": rationale, "confirm_source": False},
            )
        else:
            service.reject_rate(tender_id, request.id, rationale)
    with repo.db.connect() as conn:
        run = conn.execute(
            _RUN_OF[request.kind],
            (request.id, tender_id),
        ).fetchone()
        items = approvals_for_run(repo, conn, tender_id, run[0]) if run and run[0] else []
    return next(
        (item for item in items if item["kind"] == request.kind and item["id"] == request.id),
        {"kind": request.kind, "id": request.id, "title": "", "state": "closed"},
    )


def approval_detail(repo, tender_id: str, kind: str, record_id: str) -> dict:
    """The full text, key facts and cited sources of one item waiting in the chat."""

    import json

    if kind not in _RUN_OF:
        raise KeyError("Unknown kind of record.")
    repo.get_tender(tender_id)
    with repo.db.connect() as conn:
        run = conn.execute(_RUN_OF[kind], (record_id, tender_id)).fetchone()
        if run is None:
            raise KeyError("This item is not in the tender.")
        listed = approvals_for_run(repo, conn, tender_id, run[0]) if run[0] else []
        item = next(
            (entry for entry in listed if entry["kind"] == kind and entry["id"] == record_id),
            None,
        )
        facts: list[str] = []
        detail = ""
        sources: list[str] = []
        if kind == "finding":
            row = conn.execute(
                "SELECT title,detail,kind,state,source_ids_json FROM findings WHERE id=?",
                (record_id,),
            ).fetchone()
            detail = row["detail"]
            sources = json.loads(row["source_ids_json"] or "[]")
            facts.append(
                {
                    "requirement": "Requirement",
                    "risk": "Risk",
                    "question": "Question for the client",
                    "assumption": "Assumption",
                    "observation": "Observation",
                    "exclusion": "Exclusion",
                }.get(row["kind"], row["kind"].title())
            )
        elif kind == "plan":
            tasks = conn.execute(
                "SELECT title,description,role,source_ids_json FROM tasks WHERE plan_id=? ORDER BY rowid",
                (record_id,),
            ).fetchall()
            detail = "\n".join(
                f"- **{task['title']}** ({task['role']}): {task['description']}" for task in tasks
            )
            for task in tasks:
                sources += json.loads(task["source_ids_json"] or "[]")
            facts.append(f"{len(tasks)} steps")
        elif kind == "requirement":
            from .tender_requirements import RequirementService

            requirement = RequirementService(repo).get(tender_id, record_id)
            detail = requirement.get("detail") or ""
            if requirement.get("source_quote"):
                detail += f"\n\n> {requirement['source_quote']}"
            if requirement.get("due_date"):
                facts.append(f"Due {requirement['due_date']}")
            if requirement.get("condition"):
                facts.append(f"Applies when: {requirement['condition']}")
            sources = [source["source_id"] for source in requirement.get("sources", [])]
        elif kind == "boq_row":
            row = conn.execute(
                "SELECT source_id,data_json FROM boq_items WHERE id=?", (record_id,)
            ).fetchone()
            data = json.loads(row["data_json"])
            if data.get("source_excerpt"):
                detail = f"> {data['source_excerpt']}"
            quantity = " ".join(
                filter(None, [str(data.get("supplied_quantity") or ""), data.get("unit")])
            )
            facts += [value for value in (quantity, data.get("document")) if value]
            sources = [row["source_id"]]
        elif kind == "takeoff":
            row = conn.execute(
                "SELECT data_json FROM takeoff_lines WHERE id=?", (record_id,)
            ).fetchone()
            data = json.loads(row["data_json"])
            detail = data.get("working", "")
            quantity = " ".join(filter(None, [str(data.get("quantity") or ""), data.get("unit")]))
            facts += [value for value in (quantity, data.get("location")) if value]
            if data.get("boq"):
                facts.append(
                    f"BOQ: {data['boq'].get('quantity') or '—'} {data['boq'].get('unit', '')}".strip()
                )
            sources = data.get("source_ids", [])
        elif kind == "quantity":
            row = conn.execute(
                "SELECT data_json FROM quantity_proposals WHERE id=?", (record_id,)
            ).fetchone()
            data = json.loads(row["data_json"])
            detail = data.get("calculation", "")
            facts.append(f"Proposed quantity {data.get('quantity', '')}")
            sources = data.get("source_ids", [])
        elif kind == "rate":
            row = conn.execute(
                "SELECT payload_json,source_ids_json FROM rate_proposals WHERE id=?", (record_id,)
            ).fetchone()
            payload = json.loads(row["payload_json"])
            provenance = payload.get("provenance") or {}
            parts = [
                f"- {component.get('name', '')}: {component.get('quantity', '')} × "
                f"{component.get('unit_rate', '')} {payload.get('currency', '')}"
                for component in payload.get("components") or []
            ]
            detail = "\n\n".join(
                text for text in (provenance.get("conditions", ""), "\n".join(parts)) if text
            )
            dated = provenance.get("observed_on")
            facts += [
                value
                for value in (
                    f"{_rate_amount(payload)} {payload.get('currency', '')}".strip(),
                    provenance.get("geography") or provenance.get("location"),
                    dated
                    and (
                        f"Estimated {dated}"
                        if provenance.get("basis") == "estimated"
                        else f"Observed {dated}"
                    ),
                )
                if value
            ]
            sources = json.loads(row["source_ids_json"] or "[]")
    if item is None:
        item = {
            "kind": kind,
            "id": record_id,
            "title": "",
            "state": "closed",
            "can_reject": kind != "plan",
        }
    return ApprovalDetail(
        **{**item, "detail": detail or item.get("detail", "")},
        facts=[_short(fact, 200) for fact in facts],
        source_ids=list(dict.fromkeys(sources))[:50],
    ).model_dump()


_RUN_OF = {
    "finding": "SELECT run_id FROM findings WHERE id=? AND tender_id=?",
    "plan": "SELECT run_id FROM plans WHERE id=? AND tender_id=?",
    "requirement": "SELECT json_extract(payload_json,'$.run_id') FROM submission_requirements WHERE id=? AND tender_id=?",
    "boq_row": "SELECT json_extract(data_json,'$.run_id') FROM boq_items WHERE id=? AND tender_id=?",
    "takeoff": "SELECT run_id FROM takeoff_lines WHERE id=? AND tender_id=?",
    "quantity": "SELECT json_extract(data_json,'$.run_id') FROM quantity_proposals WHERE id=? AND tender_id=?",
    "rate": "SELECT run_id FROM rate_proposals WHERE id=? AND tender_id=?",
}
