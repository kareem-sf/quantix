"""Work products retain exact Tender evidence dependencies."""

from __future__ import annotations

import hashlib

import pytest

from quantix.execution_context import engineer_identity
from quantix.repository import Repository
from quantix.work_product_models import WorkProductDraft
from quantix.work_products import WorkProductService


def _register(repo, tender_id, text, path="Sources/pump.pdf"):
    artifact, _ = repo.register_artifact(
        tender_id,
        path,
        hashlib.sha256(text).hexdigest(),
        len(text),
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [{"locator": "page:1", "text": text.decode()}],
        },
    )
    return artifact


def test_work_product_rejects_web_refs_and_derives_source_revision_impact(tmp_path):
    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic work-product dependency Tender")
    artifact = _register(repo, tender["id"], b"Specified pump duty is 65 m3 per hour.")
    evidence = repo.artifact_evidence(tender["id"], artifact["id"])[0]
    identity = engineer_identity(tender["id"])
    service = WorkProductService(repo)
    saved = service.save_draft(
        identity,
        WorkProductDraft(
            kind="comparison",
            title="Pump comparison",
            rows=[],
            content="Compare the specified pump duty.",
            source_refs=[evidence["id"]],
            method_refs=[],
            idempotency_key="product-dependencies",
        ),
    )
    assert saved.dependency_state == "current"
    assert saved.review_reasons == []

    with pytest.raises(ValueError, match="Tender evidence IDs"):
        service.save_draft(
            identity,
            WorkProductDraft(
                kind="note",
                title="Unsafe URL",
                content="This must not save.",
                source_refs=["https://model-invented.example/not-cited"],
                idempotency_key="unsafe-public-url",
            ),
        )

    _register(
        repo, tender["id"], b"Specified pump duty is 80 m3 per hour.", artifact["relative_path"]
    )
    impacted = service.get(tender["id"], saved.product_id, saved.version)
    assert impacted.dependency_state == "needs_review"
    assert impacted.review_reasons == ["source_revision_changed"]
