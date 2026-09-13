"""Work products retain exact local and cited-public dependencies."""

from __future__ import annotations

import hashlib

import pytest

from quantix.execution_context import engineer_identity
from quantix.repository import Repository
from quantix.research_models import PublicFetchCommand, ResearchCitationCommand
from quantix.research_service import ResearchService, TransportResponse
from quantix.work_product_models import WorkProductDraft
from quantix.work_products import WorkProductService


class _PublicText:
    def get(self, _url, _max_bytes, _addresses):
        return TransportResponse(
            status=200,
            headers={"content-type": "text/plain; charset=utf-8"},
            body=b"Synthetic pump output is 70 m3 per hour.",
        )


def test_work_product_validates_public_refs_and_derives_source_revision_impact(tmp_path):
    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Synthetic work-product dependency Tender")
    text = b"Specified pump duty is 65 m3 per hour."
    artifact, _ = repo.register_artifact(
        tender["id"],
        "Sources/pump.pdf",
        hashlib.sha256(text).hexdigest(),
        len(text),
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [{"locator": "page:1", "text": text.decode()}],
        },
    )
    evidence = repo.artifact_evidence(tender["id"], artifact["id"])[0]
    identity = engineer_identity(tender["id"])
    research = ResearchService(
        repo,
        test_mode=True,
        resolver=lambda _host, _port: ["93.184.216.34"],
        transport=_PublicText(),
    )
    receipt = research.fetch(
        identity,
        PublicFetchCommand(
            url="https://public.example/pump",
            idempotency_key="product-public-fetch",
        ),
    )
    citation = research.cite(
        identity,
        ResearchCitationCommand(
            receipt_id=receipt.id,
            passage_ids=[receipt.passages[0].id],
            purpose="Support the public pump comparison.",
            idempotency_key="product-public-citation",
        ),
    )
    assert research.get_citation(tender["id"], citation.id) == citation
    service = WorkProductService(repo)
    saved = service.save_draft(
        identity,
        WorkProductDraft(
            kind="comparison",
            title="Pump comparison",
            rows=[],
            content="Compare the specified and public pump duties.",
            source_refs=[evidence["id"], citation.work_product_reference],
            method_refs=[],
            idempotency_key="product-dependencies",
        ),
    )
    assert saved.dependency_state == "current"
    assert saved.review_reasons == []
    assert saved.public_citation_refs == [citation.work_product_reference]

    with pytest.raises(ValueError, match="saved public_citation"):
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

    changed = b"Specified pump duty is 80 m3 per hour."
    repo.register_artifact(
        tender["id"],
        artifact["relative_path"],
        hashlib.sha256(changed).hexdigest(),
        len(changed),
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [{"locator": "page:1", "text": changed.decode()}],
        },
    )
    impacted = service.get(tender["id"], saved.product_id, saved.version)
    assert impacted.dependency_state == "needs_review"
    assert impacted.review_reasons == ["source_revision_changed"]
