"""One line per tender for the sidebar: a readable name, due date, live work and waiting count."""

from __future__ import annotations

import re

from pydantic import Field

from .models import ApiModel

_LATIN = re.compile(r"[A-Za-z]")


class TenderSummary(ApiModel):
    tender_id: str
    short_name: str
    full_name: str
    due: str | None = None
    working: bool = False
    waiting: int = 0
    documents: int = 0
    document_problems: int = 0


class TenderSummaryList(ApiModel):
    tenders: list[TenderSummary] = Field(default_factory=list)


def short_name(full_name: str, identity: dict | None) -> str:
    """The AI's short English name when it gave one, else the English part of the title."""

    stated = (identity or {}).get("short_name")
    if isinstance(stated, str) and stated.strip():
        return stated.strip()[:60]
    parts = [part.strip() for part in re.split(r"\s+[-–—]\s+", full_name) if part.strip()]
    english = [part for part in parts if _LATIN.search(part) and not re.search(r"[؀-ۿ]", part)]
    if english:
        return english[-1][:60]
    bracketed = re.findall(r"\(([^)]*[A-Za-z][^)]*)\)", full_name)
    if bracketed:
        return bracketed[0].strip()[:60]
    return full_name[:60]


class TenderSummaryService:
    def __init__(self, repo):
        self.repo = repo

    def list(self) -> TenderSummaryList:
        from .package_analysis import package_map
        from .waiting import WaitingService

        waiting = WaitingService(self.repo)
        summaries = []
        for tender in self.repo.list_tenders():
            mapped = package_map(self.repo, tender["id"]) or {}
            identity = mapped.get("identity") if isinstance(mapped.get("identity"), dict) else None
            with self.repo.db.connect() as conn:
                working = conn.execute(
                    "SELECT 1 FROM runs WHERE tender_id=? AND status IN ('queued','running') LIMIT 1",
                    (tender["id"],),
                ).fetchone()
                files = conn.execute(
                    "SELECT count(*), sum(status IN ('needs_attention','failed','unsupported')) "
                    "FROM artifacts WHERE tender_id=? AND is_current=1",
                    (tender["id"],),
                ).fetchone()
            pending = tender.get("name_source") == "pending"
            summaries.append(
                TenderSummary(
                    tender_id=tender["id"],
                    short_name="Analyzing tender package…"
                    if pending
                    else short_name(tender["name"], identity),
                    full_name=tender["name"],
                    due=(identity or {}).get("submission_deadline") or None,
                    working=bool(working),
                    waiting=waiting.list(tender["id"]).total,
                    documents=files[0] or 0,
                    document_problems=files[1] or 0,
                )
            )
        return TenderSummaryList(tenders=summaries)
