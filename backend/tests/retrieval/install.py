"""Load the labeled corpus into an isolated Repository. No production ranker change."""

from __future__ import annotations

import hashlib
from typing import Any

from .dataset import DOCUMENTS

REVISED_PUMP = (
    "The concrete pump shall provide a minimum output of 82 cubic metres per hour "
    "at the placing point."
)


def _digest(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def install_corpus(repo, tender_id: str) -> dict[str, Any]:
    """Register current labeled documents. Returns document -> locator -> evidence."""

    mapping: dict[str, Any] = {}
    for document in DOCUMENTS:
        body = (
            "\n".join(segment.get("text") or "" for segment in document["segments"])
            or document["id"]
        )
        extraction = {
            "kind": document["kind"],
            "status": document["status"],
            "segments": [
                {
                    "locator": segment["locator"],
                    "text": segment.get("text") or "",
                    "page": segment.get("page"),
                    "sheet": segment.get("sheet"),
                    "cell_range": segment.get("cell_range"),
                    "kind": segment.get("block_kind") or document["kind"],
                    "metadata": {
                        key: value
                        for key, value in segment.items()
                        if key in {"heading", "block_kind"}
                    },
                }
                for segment in document["segments"]
            ],
        }
        artifact, _created = repo.register_artifact(
            tender_id,
            document["path"],
            _digest(body),
            max(len(body.encode("utf-8")), 1),
            extraction,
        )
        locators = {}
        for evidence in repo.artifact_evidence(tender_id, artifact["id"], limit=50):
            locators[evidence["locator"]] = evidence["id"]
        mapping[document["id"]] = {
            "artifact_id": artifact["id"],
            "path": document["path"],
            "evidence": locators,
        }
    return mapping


def apply_pump_revision(repo, tender_id: str, mapping: dict[str, Any]) -> dict[str, Any]:
    """Replace the current pump-en source. Previous evidence becomes non-current."""

    old = mapping["pump-en"]
    artifact, _created = repo.register_artifact(
        tender_id,
        "Plant/pump-en.pdf",
        _digest(REVISED_PUMP),
        len(REVISED_PUMP.encode("utf-8")),
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [{"locator": "page:1", "text": REVISED_PUMP, "page": 1}],
        },
    )
    evidence = repo.artifact_evidence(tender_id, artifact["id"])[0]
    mapping["pump-en"] = {
        "artifact_id": artifact["id"],
        "path": "Plant/pump-en.pdf",
        "evidence": {"page:1": evidence["id"]},
        "previous_artifact_id": old["artifact_id"],
        "previous_evidence": old["evidence"],
    }
    return mapping


def annotate_hits(hits: list[dict], mapping: dict[str, Any], tender_id: str) -> list[dict]:
    """Attach stable document/locator keys used by the evaluator."""

    current: dict[str, tuple[str, str]] = {}
    previous: dict[str, tuple[str, str]] = {}
    for document_id, record in mapping.items():
        for locator, evidence_id in record.get("evidence", {}).items():
            current[evidence_id] = (document_id, locator)
        for locator, evidence_id in record.get("previous_evidence", {}).items():
            previous[evidence_id] = (document_id, locator)
    annotated = []
    for hit in hits:
        row = dict(hit)
        row.setdefault("tender_id", tender_id)
        if hit["id"] in current:
            row["document"], row["locator"] = current[hit["id"]]
            row["is_current"] = True
        elif hit["id"] in previous:
            row["document"], row["locator"] = previous[hit["id"]]
            row["is_current"] = False
            row["superseded"] = True
        annotated.append(row)
    return annotated
