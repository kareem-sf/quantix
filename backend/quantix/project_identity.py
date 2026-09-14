"""Identify the project from its package: its name, client, place and key facts.

One bounded, no-tools AI pass over the file list and the opening text of the
documents most likely to state them. It runs on the Tender's approved AI route
and budget, right after a package import, so the engineer never types a name.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import PurePosixPath
from typing import TYPE_CHECKING

from pydantic import BaseModel, ConfigDict

from .clipped_fields import OptionalText, Text, TextList
from .office_tools import redact_text, safe_text

if TYPE_CHECKING:
    from .repository import Repository

PLACEHOLDER_NAME = "New tender"
_MAX_PATHS = 150
_MAX_DOCUMENTS = 6
_EXCERPT_CHARS = 2000
# Words that mark the documents that usually state the project's identity.
_TELLING_WORDS = (
    "invitation",
    "itt",
    "rfp",
    "rfq",
    "tender",
    "bid",
    "instruction",
    "cover",
    "letter",
    "form of",
    "conditions",
    "particular",
    "contract",
    "scope",
    "specification",
    "bill of quantities",
    "boq",
    "project",
    "summary",
)
_READABLE = {".pdf": 3, ".docx": 3, ".doc": 2, ".txt": 2, ".xlsx": 1, ".xlsm": 1, ".xls": 1}


class ProjectIdentity(BaseModel):
    """Facts stated by the package. Unknown facts stay null; nothing is guessed."""

    model_config = ConfigDict(extra="forbid")

    name: Text(
        120,
        description="The project's own short name as the documents state it, without tender "
        "numbers or words like 'Tender documents'. If no document names the project, a short "
        "descriptive name from the package folder name.",
    )
    client: OptionalText(200, description="Employer or client organisation.")
    location: OptionalText(200, description="Site, city or region.")
    country: OptionalText(80)
    reference: OptionalText(120, description="Tender or contract reference number.")
    contract_type: OptionalText(120, description="For example FIDIC Red Book, lump sum.")
    currencies: TextList(8, 5, description="ISO 4217 codes the documents require for pricing.")
    submission_deadline: OptionalText(120, description="As written, with time zone if stated.")
    measurement_method: OptionalText(120, description="For example POMI, CESMM4, NRM2.")
    summary: Text(400, default="", description="One sentence describing the works.")
    sources: TextList(300, 8, description="Relative paths of the documents that state these facts.")


@dataclass(frozen=True)
class PreparedIdentityResult:
    tender_id: str
    run_id: str
    output: ProjectIdentity
    usage: dict


def package_name(source_path: str) -> str:
    """A readable provisional Tender name from a package folder or ZIP file."""

    stem = PurePosixPath(str(source_path).replace("\\", "/").rstrip("/")).name
    if stem.lower().endswith(".zip"):
        stem = stem[:-4]
    stem = " ".join(stem.replace("_", " ").split())
    return stem[:200] or PLACEHOLDER_NAME


def _score(artifact: dict) -> int:
    path = artifact["relative_path"].lower()
    words = sum(word in path for word in _TELLING_WORDS)
    return words * 3 + _READABLE.get(PurePosixPath(path).suffix, 0) - path.count("/")


def package_digest(repo: "Repository", tender_id: str) -> dict:
    """The file list and opening text of the documents most likely to identify the project."""

    artifacts = repo.list_artifacts(tender_id)
    paths = [safe_text(artifact["relative_path"], 200) for artifact in artifacts[:_MAX_PATHS]]
    excerpts = []
    with repo.db.connect() as conn:
        for artifact in sorted(artifacts, key=_score, reverse=True):
            if len(excerpts) == _MAX_DOCUMENTS:
                break
            rows = conn.execute(
                "SELECT text FROM evidence WHERE artifact_id=? ORDER BY COALESCE(page,0), rowid LIMIT 12",
                (artifact["id"],),
            ).fetchall()
            text = " ".join(" ".join(row[0].split()) for row in rows if row[0])
            if text.strip():
                excerpts.append(
                    {
                        "document": safe_text(artifact["relative_path"], 300),
                        "opening_text": safe_text(text, _EXCERPT_CHARS),
                    }
                )
    return {"file_count": len(artifacts), "files": paths, "excerpts": excerpts}


def _prompt(repo: "Repository", tender_id: str) -> str:
    tender = repo.get_tender(tender_id)
    digest = package_digest(repo, tender_id)
    return (
        "Identify this construction project from its Tender package so Quantix can name it and "
        "record its key facts. Use only the supplied file names and opening text; do not use "
        "outside knowledge. Leave a field null when the package does not state it, and never infer "
        "a country or currency from language alone. Give the project's own name as written, "
        "keeping its original language; drop tender numbers and generic words such as 'Tender "
        "documents'. When no document names the project, write a short descriptive name from "
        "the package folder name. List in sources only documents whose text states a returned "
        "fact.\n\n"
        + redact_text(
            json.dumps(
                {"package_folder_name": tender["name"], "package": digest}, ensure_ascii=False
            )
        )
    )


async def run_identification(
    repo: "Repository", tender_id: str, run_id: str
) -> PreparedIdentityResult:
    """One bounded model pass on the Tender's approved Manager route, with no tools."""

    from .structured_ai import ask_structured

    output, usage = await ask_structured(
        repo, tender_id, run_id, _prompt(repo, tender_id), ProjectIdentity, operation="identify"
    )
    if not output.name.strip():
        raise ValueError("The identification pass returned no project name.")
    return PreparedIdentityResult(tender_id=tender_id, run_id=run_id, output=output, usage=usage)


def apply_identity(
    repo: "Repository", prepared: PreparedIdentityResult, *, announce: bool = True
) -> dict:
    """Name the Tender provisionally and retain proposed facts for source review."""

    identity, tender_id = prepared.output, prepared.tender_id
    tender = repo.get_tender(tender_id)
    renamed = tender["name_source"] != "engineer" and identity.name.strip() != ""
    if renamed:
        repo.rename_tender(tender_id, identity.name.strip(), source="ai")
    facts = [
        ("Client", identity.client),
        ("Location", ", ".join(filter(None, [identity.location, identity.country]))),
        ("Reference", identity.reference),
        ("Contract", identity.contract_type),
        ("Currency", ", ".join(identity.currencies)),
        ("Submission deadline", identity.submission_deadline),
        ("Measurement", identity.measurement_method),
    ]
    lines = [
        f"I identified this project as **{identity.name.strip()}**.",
        "These proposed project facts need source review before use in the Tender profile.",
    ]
    if identity.summary.strip():
        lines.append(identity.summary.strip())
    lines += [f"- {label}: {value}" for label, value in facts if value]
    if identity.sources:
        lines.append("Sources: " + ", ".join(identity.sources[:5]))
    if announce:
        repo.add_message(tender_id, "manager", "\n".join(lines), run_id=prepared.run_id)
    return {
        "kind": "identify",
        "name": identity.name.strip(),
        "renamed": renamed,
        "profile_fields": [],
        "identity": identity.model_dump(mode="json"),
        "usage": prepared.usage,
    }
