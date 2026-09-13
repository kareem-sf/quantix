"""Analyzing tender package: the automatic work that runs after documents are registered.

Stages, in engineering terms the engineer sees:
  1. Registering documents               (the import run itself)
  2. Recognising scanned pages           (OCR coverage and uncertain pages)
  3. Indexing tender evidence            (keyword index plus local meaning vectors)
  4. Extracting BOQ, schedules and tables
  5. Mapping the tender package          (one-time AI: document types, briefs, project identity)

Local stages never need AI. Document briefs are reused only within the same
Tender and unchanged document/extraction context. Every stage
failure is isolated and reported in plain English.
"""

from __future__ import annotations

import hashlib
import json
from collections import Counter
from dataclasses import dataclass, field
from pathlib import PurePosixPath
from typing import TYPE_CHECKING, Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field

from .db import dump, now
from .office_tools import redact_text, safe_text
from .project_identity import ProjectIdentity

if TYPE_CHECKING:
    from .repository import Repository

BRIEF_VERSION = 2
OPENING_CHARS = 1200
BATCH_CHARS = 24_000
BATCH_DOCUMENTS = 25

STAGES = {
    "register": "Registering documents",
    "recognise": "Recognising scanned pages",
    "index": "Indexing tender evidence",
    "structure": "Extracting BOQ, schedules and tables",
    "map": "Mapping the tender package",
}

DocumentType = Literal[
    "invitation_to_tender", "instructions_to_tenderers", "conditions_of_contract", "particular_conditions",
    "specification", "bill_of_quantities", "drawing", "schedule", "addendum_or_clarification", "tender_form",
    "programme", "correspondence", "report", "other",
]
DOCUMENT_TYPE_LABELS = {
    "invitation_to_tender": "invitation to tender", "instructions_to_tenderers": "instructions to tenderers",
    "conditions_of_contract": "conditions of contract", "particular_conditions": "particular conditions",
    "specification": "specification", "bill_of_quantities": "bill of quantities", "drawing": "drawing",
    "schedule": "schedule", "addendum_or_clarification": "addendum or clarification", "tender_form": "tender form",
    "programme": "programme", "correspondence": "correspondence", "report": "report", "other": "other document",
}


class DocumentBrief(BaseModel):
    model_config = ConfigDict(extra="forbid")

    document_id: str = Field(max_length=64)
    document_type: DocumentType
    title: str | None = Field(default=None, max_length=200, description="The document's own title as written.")
    discipline: str | None = Field(default=None, max_length=80, description="For example civil, structural, MEP.")
    brief: str = Field(max_length=600, description="Two or three sentences: what this document is and covers.")
    key_topics: list[Annotated[str, Field(max_length=80)]] = Field(default_factory=list, max_length=8)
    key_locations: list[Annotated[str, Field(max_length=120)]] = Field(
        default_factory=list, max_length=6, description="Where key content is, for example 'page 3: form of tender'.")
    related_document_ids: list[Annotated[str, Field(max_length=64)]] = Field(default_factory=list, max_length=8)


class BriefBatch(BaseModel):
    model_config = ConfigDict(extra="forbid")

    briefs: list[DocumentBrief] = Field(max_length=BATCH_DOCUMENTS)


class PackageOverview(BaseModel):
    model_config = ConfigDict(extra="forbid")

    identity: ProjectIdentity
    overview: str = Field(max_length=1500, description="What this tender package is for and what it contains.")
    gaps: list[Annotated[str, Field(max_length=300)]] = Field(
        default_factory=list, max_length=10,
        description="Important documents or facts a tender package normally has but this one lacks.")


@dataclass
class PreparedAnalysisResult:
    tender_id: str
    run_id: str
    stages: dict[str, dict] = field(default_factory=dict)
    briefs: dict[str, dict] = field(default_factory=dict)
    overview: PackageOverview | None = None
    usage: dict = field(default_factory=dict)


_SCHEMA = (
    """CREATE TABLE IF NOT EXISTS document_briefs(
        content_hash TEXT NOT NULL, brief_version INTEGER NOT NULL, data_json TEXT NOT NULL,
        model TEXT NOT NULL, created_at TEXT NOT NULL, PRIMARY KEY(content_hash, brief_version))""",
    """CREATE TABLE IF NOT EXISTS package_maps(
        tender_id TEXT PRIMARY KEY, data_json TEXT NOT NULL, source_fingerprint TEXT NOT NULL, updated_at TEXT NOT NULL)""",
)


def ensure_schema(repo: "Repository") -> None:
    with repo.db.connect(write=True) as conn:
        for statement in _SCHEMA:
            conn.execute(statement)


def source_fingerprint(artifacts: list[dict]) -> str:
    return hashlib.sha256(dump(sorted(artifacts, key=lambda a: a["id"])).encode()).hexdigest()


def package_map(repo: "Repository", tender_id: str) -> dict | None:
    ensure_schema(repo)
    with repo.db.connect() as conn:
        row = conn.execute("SELECT data_json, source_fingerprint FROM package_maps WHERE tender_id=?", (tender_id,)).fetchone()
    if row is None:
        return None
    data = json.loads(row[0])
    data["current"] = row[1] == source_fingerprint(repo.list_artifacts(tender_id))
    return data


def _stage(repo, run_id, key, state, detail="", **data):
    label = STAGES[key]
    repo.event(run_id, "analysis_stage", f"{label}: {detail}" if detail else label,
               {"stage": key, "label": label, "state": state, "detail": detail, **data})
    if state == "running":
        repo.update_run(run_id, detail=f"{label}…")


def _opening_text(repo: "Repository", artifact: dict) -> str:
    with repo.db.connect() as conn:
        rows = conn.execute(
            "SELECT text FROM evidence WHERE artifact_id=? ORDER BY COALESCE(page,0), rowid LIMIT 16", (artifact["id"],)
        ).fetchall()
    return safe_text(" ".join(" ".join((row[0] or "").split()) for row in rows), OPENING_CHARS)


def recognition_summary(artifacts: list[dict]) -> dict:
    pages = recognised = uncertain = unreadable = 0
    attention: list[str] = []
    for artifact in artifacts:
        metadata, warnings = artifact.get("metadata") or {}, artifact.get("warnings") or []
        pages += int(metadata.get("page_count") or 0)
        recognised += int(metadata.get("ocr_pages") or 0)
        for warning in warnings:
            if warning.get("code") == "ocr_low_confidence":
                uncertain += 1
                attention.append(f"{artifact['relative_path']} {warning.get('locator', '')}".strip())
            elif warning.get("code") == "pdf_no_text":
                unreadable += 1
                attention.append(f"{artifact['relative_path']} {warning.get('locator', '')}".strip())
    return {"pages": pages, "recognised_pages": recognised, "uncertain_pages": uncertain,
            "unreadable_pages": unreadable, "attention": attention[:40]}


def _brief_prompt(documents: list[dict]) -> str:
    return (
        "You are mapping a construction tender package. For each document below, identify its type, "
        "its own title and discipline when stated, and write a short brief of what it is and covers. Note "
        "where key content is (pages, sections, sheets) and which other listed documents it relates to, "
        "using their ids. Use only the supplied file names and opening text; never invent content. When the "
        "opening text is empty, classify from the file name and say that the text was not readable. Return "
        "exactly one brief per document id.\n\n"
        + redact_text(json.dumps({"documents": documents}, ensure_ascii=False))
    )


def _package_prompt(repo: "Repository", tender_id: str, briefs: dict[str, dict], recognition: dict) -> str:
    from .project_identity import package_digest

    documents = [
        {"path": brief["relative_path"], "type": brief["document_type"], "title": brief.get("title"),
         "brief": brief["brief"]}
        for brief in briefs.values()
    ]
    return (
        "You are the Tender Manager's analyst. From the document briefs and opening text of this construction "
        "tender package, identify the project and describe the package. Use only the supplied material; leave "
        "a fact null when the package does not state it and never infer a country or currency from language "
        "alone. The project name is the project's own name as written in the documents, never a folder or "
        "file name unless no document names the project. List important gaps a tender package normally "
        "covers but this one does not appear to, such as a missing bill of quantities or submission "
        "instructions.\n\n"
        + redact_text(json.dumps({"documents": documents, "readability": recognition,
                                  "opening_text": package_digest(repo, tender_id)["excerpts"]}, ensure_ascii=False))
    )


async def run_analysis(repo: "Repository", tender_id: str, run_id: str, cancelled, *, ai_ready) -> PreparedAnalysisResult:
    """Run the local stages, then the AI package map when the Tender's AI is usable."""

    import asyncio

    from .estimates import EstimateService
    from .semantic import SemanticService
    from .structured_ai import add_usage, ask_structured

    ensure_schema(repo)
    result = PreparedAnalysisResult(tender_id, run_id)
    artifacts = repo.list_artifacts(tender_id)
    if not artifacts:
        raise ValueError("Add the tender package before analysing it.")

    def finish(key, state, detail="", **data):
        result.stages[key] = {"state": state, "detail": detail, **data}
        _stage(repo, run_id, key, state, detail, **data)

    finish("register", "completed", f"{len(artifacts)} documents registered", documents=len(artifacts))

    _stage(repo, run_id, "recognise", "running")
    recognition = recognition_summary(artifacts)
    finish("recognise", "completed",
           f"{recognition['recognised_pages']} scanned pages recognised; "
           f"{recognition['uncertain_pages'] + recognition['unreadable_pages']} pages need a second reading",
           **recognition)

    _stage(repo, run_id, "index", "running")
    repo.update_run(run_id, progress=10)

    def progress(percent, detail):
        repo.update_run(run_id, progress=10 + int(min(99, max(0, percent)) * 0.5),
                        detail=f"{STAGES['index']}… {detail}")

    try:
        indexed = await asyncio.to_thread(SemanticService(repo).index, tender_id, cancelled.is_set, progress)
        finish("index", "completed", "Keyword and meaning search are ready for every readable passage",
               chunks=getattr(indexed, "chunk_count", None))
    except InterruptedError:
        raise
    except Exception as error:  # noqa: BLE001 - reported to the engineer, later stages continue
        finish("index", "failed", f"Meaning search could not be prepared: {safe_text(error, 300)}")

    _stage(repo, run_id, "structure", "running")
    repo.update_run(run_id, progress=62)
    try:
        estimate = await asyncio.to_thread(EstimateService(repo).refresh, tender_id)
        finish("structure", "completed", f"{len(estimate['items'])} BOQ items extracted", boq_items=len(estimate["items"]))
    except Exception as error:  # noqa: BLE001
        finish("structure", "failed", f"BOQ extraction needs attention: {safe_text(error, 300)}")

    if cancelled.is_set():
        raise InterruptedError("Analysis stopped at your request.")
    _stage(repo, run_id, "map", "running")
    repo.update_run(run_id, progress=70)
    readiness = ai_ready()
    if readiness is not True:
        finish("map", "waiting", "Choose the AI for this Tender to map the package and identify the project",
               reason=safe_text(readiness, 300))
        return result

    inputs = {
        artifact["id"]: {"id": artifact["id"], "path": safe_text(artifact["relative_path"], 300), "format": artifact["kind"],
                         "pages": (artifact.get("metadata") or {}).get("page_count"),
                         "sheets": ((artifact.get("metadata") or {}).get("sheets") or [])[:12],
                         "opening_text": _opening_text(repo, artifact)}
        for artifact in artifacts
    }
    basis = hashlib.sha256(dump({"tender_id": tender_id, "sources": source_fingerprint(artifacts),
                               "documents": inputs}).encode()).hexdigest()
    keys = {identifier: hashlib.sha256(f"{basis}:{identifier}".encode()).hexdigest() for identifier in inputs}
    with repo.db.connect() as conn:
        cached = {
            row[0]: json.loads(row[1])
            for row in conn.execute(
                f"SELECT content_hash, data_json FROM document_briefs WHERE brief_version=? AND content_hash IN "
                f"({','.join('?' * len(keys))})", (BRIEF_VERSION, *keys.values()))
        }
    pending = [artifact for artifact in artifacts if keys[artifact["id"]] not in cached]
    batches, batch, size = [], [], 0
    for artifact in pending:
        item = inputs[artifact["id"]]
        cost = len(json.dumps(item, ensure_ascii=False))
        if batch and (size + cost > BATCH_CHARS or len(batch) >= BATCH_DOCUMENTS):
            batches.append(batch)
            batch, size = [], 0
        batch.append(item)
        size += cost
    if batch:
        batches.append(batch)
    try:
        for number, documents in enumerate(batches, start=1):
            if cancelled.is_set():
                raise InterruptedError("Analysis stopped at your request.")
            repo.update_run(run_id, progress=70 + int(20 * number / max(1, len(batches))),
                            detail=f"{STAGES['map']}… documents {number} of {len(batches)} batches")
            output, usage = await ask_structured(repo, tender_id, run_id, _brief_prompt(documents), BriefBatch,
                                                 operation="package_briefs")
            result.usage = add_usage(result.usage, usage)
            valid = {document["id"] for document in documents}
            with repo.db.connect(write=True) as conn:
                for brief in output.briefs:
                    if brief.document_id not in valid:
                        continue
                    artifact = next(a for a in artifacts if a["id"] == brief.document_id)
                    data = brief.model_dump(mode="json")
                    data["related_document_ids"] = [identifier for identifier in data["related_document_ids"]
                                                    if identifier in valid and identifier != brief.document_id]
                    cached[keys[artifact["id"]]] = data
                    # Briefs are cached as soon as they are paid for, so a retry never repeats them.
                    conn.execute("INSERT OR REPLACE INTO document_briefs VALUES(?,?,?,?,?)",
                                 (keys[artifact["id"]], BRIEF_VERSION, dump(data),
                                  str((usage.get("request_details") or [{}])[0].get("model") or ""), now()))
        for artifact in artifacts:
            if keys[artifact["id"]] in cached:
                brief = dict(cached[keys[artifact["id"]]])
                brief.update(document_id=artifact["id"], relative_path=artifact["relative_path"])
                result.briefs[artifact["id"]] = brief
        repo.update_run(run_id, progress=92, detail=f"{STAGES['map']}… identifying the project")
        overview, usage = await ask_structured(repo, tender_id, run_id,
                                               _package_prompt(repo, tender_id, result.briefs, recognition),
                                               PackageOverview, operation="package_map")
        result.usage = add_usage(result.usage, usage)
        if not overview.identity.name.strip():
            raise ValueError("The package map returned no project name.")
        result.overview = overview
        finish("map", "completed", f"{len(result.briefs)} documents mapped; project identified",
               mapped=len(result.briefs), from_cache=len(artifacts) - len(pending))
    except InterruptedError:
        raise
    except Exception as error:  # noqa: BLE001
        finish("map", "failed", f"The package map could not be completed: {safe_text(error, 400)}")
    return result


def apply_analysis(repo: "Repository", prepared: PreparedAnalysisResult) -> dict:
    """Save the package map, name the Tender and post the package summary."""

    from .project_identity import PreparedIdentityResult, apply_identity

    ensure_schema(repo)
    tender_id = prepared.tender_id
    artifacts = repo.list_artifacts(tender_id)
    recognition = prepared.stages.get("recognise", {})
    identity_result = None
    if prepared.overview is not None:
        identity_result = apply_identity(
            repo, PreparedIdentityResult(tender_id, prepared.run_id, prepared.overview.identity, prepared.usage),
            announce=False)
    data = {
        "stages": prepared.stages,
        "documents": list(prepared.briefs.values()),
        "overview": prepared.overview.overview if prepared.overview else None,
        "gaps": prepared.overview.gaps if prepared.overview else [],
        "identity": prepared.overview.identity.model_dump(mode="json") if prepared.overview else None,
        "readability": {key: recognition.get(key) for key in
                        ("pages", "recognised_pages", "uncertain_pages", "unreadable_pages", "attention")},
    }
    if prepared.briefs or prepared.overview:
        with repo.db.connect(write=True) as conn:
            conn.execute("INSERT OR REPLACE INTO package_maps VALUES(?,?,?,?)",
                         (tender_id, dump(data), source_fingerprint(artifacts), now()))
    repo.add_message(tender_id, "manager", summary_message(prepared, artifacts), run_id=prepared.run_id)
    return {"kind": "analysis", "stages": prepared.stages, "mapped_documents": len(prepared.briefs),
            "identity": identity_result, "usage": prepared.usage}


def summary_message(prepared: PreparedAnalysisResult, artifacts: list[dict]) -> str:
    stages = prepared.stages
    lines: list[str] = []
    if prepared.overview is not None:
        identity = prepared.overview.identity
        lines.append(f"I analysed the tender package for **{identity.name.strip()}**.")
        lines.append("These proposed project facts need source review before use in the Tender profile.")
        if prepared.overview.overview.strip():
            lines.append(prepared.overview.overview.strip())
        facts = [("Client", identity.client), ("Location", ", ".join(filter(None, [identity.location, identity.country]))),
                 ("Reference", identity.reference), ("Contract", identity.contract_type),
                 ("Currency", ", ".join(identity.currencies)), ("Submission deadline", identity.submission_deadline),
                 ("Measurement", identity.measurement_method)]
        lines += [f"- {label}: {value}" for label, value in facts if value]
    else:
        lines.append(f"I registered and indexed the tender package ({len(artifacts)} documents).")
    if prepared.briefs:
        counts = Counter(DOCUMENT_TYPE_LABELS.get(brief["document_type"], "other document")
                         for brief in prepared.briefs.values())
        lines.append("**Documents:** " + ", ".join(f"{count} {label}{'s' if count > 1 and not label.endswith('s') else ''}"
                                                   for label, count in counts.most_common()))
    recognition = stages.get("recognise", {})
    second = (recognition.get("uncertain_pages") or 0) + (recognition.get("unreadable_pages") or 0)
    if second:
        examples = "; ".join(PurePosixPath(item).name if "/" in item else item for item in recognition.get("attention", [])[:4])
        lines.append(f"**Needs a second reading:** {second} pages could not be read reliably ({examples}). "
                     "I will not rely on them without checking.")
    if prepared.overview is not None and prepared.overview.gaps:
        lines.append("**Gaps to check:** " + "; ".join(prepared.overview.gaps[:5]))
    for key in ("index", "structure", "map"):
        stage = stages.get(key) or {}
        if stage.get("state") in {"failed", "waiting"}:
            lines.append(f"**{STAGES[key]}:** {stage.get('detail')}")
    lines.append("Ask me anything about this tender, or tell me what to prepare first.")
    return "\n\n".join(lines)
