"""The tender's documents in the groups the package analysis chose.

Groups come from the AI package map, never from a fixed list. An engineer can
move a document to another group; that choice is stored separately and always
wins, including after the package is analysed again.
"""

from __future__ import annotations

from pydantic import Field

from .db import now
from .models import ApiModel

UNGROUPED = "Not grouped yet"
MAX_GROUP_NAME = 60

_SCHEMA = """CREATE TABLE IF NOT EXISTS document_group_overrides(
    tender_id TEXT NOT NULL, artifact_id TEXT NOT NULL, group_name TEXT NOT NULL,
    updated_at TEXT NOT NULL, PRIMARY KEY(tender_id, artifact_id))"""


class GroupedDocument(ApiModel):
    artifact_id: str
    name: str
    label: str = ""
    pages: int | None = None
    kind: str = ""
    # Only real problems: unreadable pages or extraction failures.
    problem: str = ""
    moved_by_engineer: bool = False


class DocumentGroupView(ApiModel):
    name: str
    documents: list[GroupedDocument] = Field(default_factory=list)


class DocumentGroupsState(ApiModel):
    groups: list[DocumentGroupView] = Field(default_factory=list)
    total: int = 0
    problems: int = 0
    # False until the package analysis has grouped this tender's documents.
    grouped: bool = False


class MoveDocumentRequest(ApiModel):
    group: str = Field(min_length=1, max_length=MAX_GROUP_NAME)


def _problem(artifact: dict) -> str:
    status = artifact.get("status")
    if status in {"failed", "unsupported"}:
        return "This file could not be read."
    if status == "needs_attention":
        warnings = artifact.get("warnings") or []
        first = warnings[0] if warnings else None
        message = first.get("message") if isinstance(first, dict) else None
        return message or "Some pages need a second reading."
    return ""


class DocumentGroupService:
    def __init__(self, repo):
        self.repo = repo

    def _ensure(self):
        with self.repo.db.connect(write=True) as conn:
            conn.execute(_SCHEMA)

    def list(self, tender_id: str) -> DocumentGroupsState:
        from .package_analysis import package_map

        self.repo.get_tender(tender_id)
        self._ensure()
        artifacts = [item for item in self.repo.list_artifacts(tender_id) if item.get("is_current")]
        mapped = package_map(self.repo, tender_id) or {}
        briefs = {
            brief.get("document_id"): brief
            for brief in mapped.get("documents") or []
            if isinstance(brief, dict)
        }
        assigned: dict[str, str] = {}
        order: list[str] = []
        for group in mapped.get("groups") or []:
            name = str(group.get("name") or "").strip()[:MAX_GROUP_NAME]
            if not name:
                continue
            if name not in order:
                order.append(name)
            for identifier in group.get("document_ids") or []:
                assigned.setdefault(identifier, name)
        with self.repo.db.connect() as conn:
            overrides = {
                row[0]: row[1]
                for row in conn.execute(
                    "SELECT artifact_id, group_name FROM document_group_overrides WHERE tender_id=?",
                    (tender_id,),
                )
            }
        groups: dict[str, list[GroupedDocument]] = {}
        for artifact in sorted(artifacts, key=lambda item: item["name"].lower()):
            brief = briefs.get(artifact["id"]) or {}
            name = overrides.get(artifact["id"]) or assigned.get(artifact["id"]) or UNGROUPED
            label = brief.get("label") or brief.get("title") or ""
            metadata = artifact.get("metadata") or {}
            groups.setdefault(name, []).append(
                GroupedDocument(
                    artifact_id=artifact["id"],
                    name=artifact["name"],
                    label=str(label)[:120],
                    pages=metadata.get("page_count") if isinstance(metadata, dict) else None,
                    kind=str(artifact.get("kind") or ""),
                    problem=_problem(artifact),
                    moved_by_engineer=artifact["id"] in overrides,
                )
            )
        names = [name for name in order if name in groups]
        names += sorted(name for name in groups if name not in names and name != UNGROUPED)
        if UNGROUPED in groups:
            names.append(UNGROUPED)
        views = [DocumentGroupView(name=name, documents=groups[name]) for name in names]
        return DocumentGroupsState(
            groups=views,
            total=len(artifacts),
            problems=sum(1 for view in views for document in view.documents if document.problem),
            grouped=bool(order),
        )

    def move(self, tender_id: str, artifact_id: str, group: str) -> DocumentGroupsState:
        self.repo.get_tender(tender_id)
        name = " ".join(group.split())[:MAX_GROUP_NAME]
        if not name:
            raise ValueError("Choose a group name.")
        self.repo.get_artifact(tender_id, artifact_id)
        self._ensure()
        with self.repo.db.connect(write=True) as conn:
            conn.execute(
                "INSERT OR REPLACE INTO document_group_overrides VALUES(?,?,?,?)",
                (tender_id, artifact_id, name, now()),
            )
        return self.list(tender_id)
