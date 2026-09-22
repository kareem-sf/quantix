"""What each job did, counted from the facts its tools recorded when they
finished, for the Activity list's summary line ("read 23 documents · hired 7
staff"). Documents and colleagues are named so attempts of one job can be
combined without counting the same document twice."""

from __future__ import annotations

import json
from collections import defaultdict

from pydantic import Field

from .models import ApiModel


class JobSummary(ApiModel):
    run_id: str
    documents: list[str] = Field(default_factory=list)
    searches: int = 0
    page_views: int = 0
    staff_hired: int = 0
    colleagues_asked: list[str] = Field(default_factory=list)
    drafts_saved: int = 0
    proposals: int = 0


class JobSummaryList(ApiModel):
    jobs: list[JobSummary] = Field(default_factory=list)


def summarise(run_id: str, facts: list[dict]) -> JobSummary:
    by_kind: dict[str, list[dict]] = defaultdict(list)
    for fact in facts:
        by_kind[str(fact.get("kind") or "")].append(fact)

    def named(kind: str) -> list[str]:
        return list(
            dict.fromkeys(
                str(fact.get("subject") or f"{kind} {index}")
                for index, fact in enumerate(by_kind[kind])
            )
        )

    return JobSummary(
        run_id=run_id,
        documents=named("read"),
        searches=len(by_kind["search"]),
        page_views=len(by_kind["view"]),
        staff_hired=len(by_kind["hire"]),
        colleagues_asked=named("assign"),
        drafts_saved=len(by_kind["save"]),
        proposals=len(by_kind["propose"]),
    )


class JobSummaryService:
    def __init__(self, repo):
        self.repo = repo

    def list(self, tender_id: str) -> JobSummaryList:
        self.repo.get_tender(tender_id)
        facts: dict[str, list[dict]] = defaultdict(list)
        with self.repo.db.connect() as conn:
            rows = conn.execute(
                """SELECT e.run_id, json_extract(e.data_json,'$.fact')
                FROM run_events e JOIN runs r ON r.id=e.run_id
                WHERE r.tender_id=? AND json_extract(e.data_json,'$.phase')='completed'
                AND json_extract(e.data_json,'$.fact.kind') IS NOT NULL
                ORDER BY e.id""",
                (tender_id,),
            ).fetchall()
        for run_id, raw in rows:
            try:
                fact = json.loads(raw)
            except (TypeError, ValueError):
                continue
            if isinstance(fact, dict) and fact.get("state", "done") == "done":
                facts[run_id].append(fact)
        return JobSummaryList(jobs=[summarise(run_id, items) for run_id, items in facts.items()])
