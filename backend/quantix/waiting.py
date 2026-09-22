"""Everything that is waiting on the engineer for one Tender, in one list.

Only items that genuinely need an engineer's decision or answer are listed:
proposed plans, proposed findings, proposed quantities and rates, proposed
submission requirements, and open questions the Manager has put to the
engineer. Informational records are not.
"""

from __future__ import annotations

from typing import Literal

from pydantic import Field

from .models import ApiModel

WaitingKind = Literal["plan", "finding", "quantities", "rates", "requirements", "question"]


class WaitingItem(ApiModel):
    id: str
    kind: WaitingKind
    title: str
    detail: str = ""
    count: int = 1
    # Where the engineer decides it: a workspace record view and optional id.
    target_view: str
    target_id: str | None = None


class WaitingState(ApiModel):
    items: list[WaitingItem] = Field(default_factory=list)
    total: int = 0


def _pending(conn, table: str, tender_id: str) -> int:
    # Estimate tables are created when the estimate is first used.
    if not conn.execute(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?", (table,)
    ).fetchone():
        return 0
    return conn.execute(
        f"SELECT count(*) FROM {table} WHERE tender_id=? AND status='proposed'", (tender_id,)
    ).fetchone()[0]


def _plural(count: int, one: str, many: str) -> str:
    return f"{count} {one if count == 1 else many}"


class WaitingService:
    def __init__(self, repo):
        self.repo = repo

    def list(self, tender_id: str) -> WaitingState:
        self.repo.get_tender(tender_id)
        items: list[WaitingItem] = []
        with self.repo.db.connect() as conn:
            plan = conn.execute(
                "SELECT id,title FROM plans WHERE tender_id=? AND status='proposed' "
                "ORDER BY version DESC LIMIT 1",
                (tender_id,),
            ).fetchone()
            if plan:
                items.append(
                    WaitingItem(
                        id=f"plan:{plan['id']}",
                        kind="plan",
                        title="Approve the work plan",
                        detail=plan["title"],
                        target_view="plan-review",
                        target_id=plan["id"],
                    )
                )

            findings = conn.execute(
                "SELECT id,title,kind FROM findings WHERE tender_id=? AND state='proposed' "
                "AND is_stale=0 ORDER BY created_at,id",
                (tender_id,),
            ).fetchall()
            for finding in [row for row in findings if row["kind"] == "question"]:
                items.append(
                    WaitingItem(
                        id=f"finding:{finding['id']}",
                        kind="question",
                        title=finding["title"],
                        target_view="finding",
                        target_id=finding["id"],
                    )
                )
            others = [row for row in findings if row["kind"] != "question"]
            if others:
                items.append(
                    WaitingItem(
                        id="findings",
                        kind="finding",
                        title=f"Decide on {_plural(len(others), 'finding', 'findings')}",
                        detail="; ".join(row["title"] for row in others[:2]),
                        count=len(others),
                        target_view="finding",
                        target_id=others[0]["id"],
                    )
                )

            quantities = _pending(conn, "quantity_proposals", tender_id)
            if quantities:
                items.append(
                    WaitingItem(
                        id="quantities",
                        kind="quantities",
                        title=f"Review {_plural(quantities, 'proposed quantity', 'proposed quantities')}",
                        count=quantities,
                        target_view="estimate",
                    )
                )
            rates = _pending(conn, "rate_proposals", tender_id)
            if rates:
                items.append(
                    WaitingItem(
                        id="rates",
                        kind="rates",
                        title=f"Review {_plural(rates, 'proposed rate', 'proposed rates')}",
                        count=rates,
                        target_view="estimate",
                    )
                )

        from .tender_requirements import RequirementService

        requirements = [
            row
            for row in RequirementService(self.repo).list(tender_id, limit=100)
            if row.get("status") == "proposed"
        ]
        if requirements:
            items.append(
                WaitingItem(
                    id="requirements",
                    kind="requirements",
                    title=(
                        "Approve "
                        + _plural(
                            len(requirements), "submission requirement", "submission requirements"
                        )
                    ),
                    detail="; ".join(str(row.get("title", "")) for row in requirements[:2]),
                    count=len(requirements),
                    target_view="submission",
                )
            )

        from .work_brief import WorkBriefService

        brief = WorkBriefService(self.repo).current(tender_id)
        for index, question in enumerate(brief.open_questions if brief else []):
            if question.owner != "engineer":
                continue
            items.append(
                WaitingItem(
                    id=f"brief-question:{index}",
                    kind="question",
                    title=question.text,
                    detail=f"Affects {question.affects}" if question.affects else "",
                    target_view="manager",
                )
            )
        return WaitingState(items=items, total=sum(item.count for item in items))
