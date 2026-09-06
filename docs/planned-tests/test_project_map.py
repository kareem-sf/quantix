"""Source-backed project relationships and explicit engineer review coverage."""

import importlib

import pytest

from quantix.repository import Repository


def service(repo):
    return importlib.import_module("quantix.project_map").ProjectMapService(repo)


def source(repo, tid, digest="a" * 64, path="Building/Scope.txt"):
    artifact, _ = repo.register_artifact(
        tid,
        path,
        digest,
        40,
        {
            "kind": "other",
            "status": "extracted",
            "metadata": {},
            "segments": [
                {"locator": "paragraph:1", "text": "Foundation scope", "kind": "text"},
                {"locator": "paragraph:2", "text": "Drainage scope", "kind": "text"},
            ],
        },
    )
    return artifact, repo.artifact_evidence(tid, artifact["id"])


@pytest.fixture
def workspace(tmp_path):
    repo = Repository(tmp_path)
    tid = repo.create_tender("School")["id"]
    artifact, evidence = source(repo, tid)
    return repo, tid, artifact, evidence


def node(sources, **values):
    return {
        "kind": "building",
        "title": "Teaching building",
        "detail": "Foundation works described in the source",
        "source_ids": sources,
        **values,
    }


def decision(**values):
    return {
        "engineer_confirmed": True,
        "rationale": "Checked this scope against the source",
        **values,
    }


def test_map_proposal_preserves_source_and_run_but_never_approves_itself(workspace):
    repo, tid, artifact, evidence = workspace
    run = repo.create_run(tid, "manager")
    maps = service(repo)
    saved = maps.propose(tid, node([evidence[0]["id"]]), origin="agent", run_id=run["id"])
    assert saved["state"] == "proposed" and saved["origin"] == "agent"
    assert saved["run_id"] == run["id"] and saved["approval_valid"] is False
    assert saved["source_manifest"][0]["content_hash"] == artifact["content_hash"]
    assert saved["source_manifest"][0]["locator"] == "paragraph:1"
    approved = maps.decide(tid, saved["id"], decision(decision="approve"))
    assert approved["state"] == "approved" and approved["approval_valid"] is True


def test_related_documents_findings_and_boq_follow_actual_shared_sources(workspace):
    from quantix.estimates import EstimateService
    from test_estimates import seed

    repo, tid, _, _ = workspace
    seed(repo, tid)
    estimate = EstimateService(repo)
    item = estimate.refresh(tid)["items"][0]
    finding = repo.add_finding(
        tid, "Concrete scope", "Check concrete specification", "requirement", [item["source_id"]]
    )
    saved = service(repo).propose(
        tid, node([item["source_id"]], kind="work_item"), origin="engineer"
    )
    assert saved["related_artifact_ids"] == [item["artifact_id"]]
    assert saved["related_finding_ids"] == [finding["id"]]
    assert saved["related_boq_item_ids"] == [item["id"]]


def test_source_revision_invalidates_node_and_descendant_approval(workspace):
    repo, tid, _, evidence = workspace
    maps = service(repo)
    parent = maps.propose(tid, node([evidence[0]["id"]]), origin="engineer")
    _, other = source(repo, tid, path="Other/Drainage.txt")
    child = maps.propose(
        tid,
        node([other[0]["id"]], title="Drainage", kind="discipline", parent_id=parent["id"]),
        origin="agent",
    )
    maps.decide(tid, parent["id"], decision(decision="approve"))
    maps.decide(tid, child["id"], decision(decision="approve"))
    source(repo, tid, digest="b" * 64)
    rows = maps.view(tid)["nodes"]
    assert all(not row["is_current"] and not row["approval_valid"] for row in rows)
    assert all(row["state"] == "approved" for row in rows)
    with pytest.raises(ValueError, match="current|changed"):
        maps.decide(tid, parent["id"], decision(decision="approve"))


def test_parent_and_run_are_tender_scoped_and_withdrawn_parent_is_unusable(workspace):
    repo, tid, _, evidence = workspace
    maps = service(repo)
    parent = maps.propose(tid, node([evidence[0]["id"]]), origin="engineer")
    other = repo.create_tender("Other")["id"]
    _, other_sources = source(repo, other)
    with pytest.raises(KeyError):
        maps.propose(other, node([other_sources[0]["id"]], parent_id=parent["id"]), origin="agent")
    with pytest.raises(ValueError, match="Tender"):
        maps.propose(
            tid,
            node([evidence[0]["id"]]),
            origin="agent",
            run_id=repo.create_run(other, "manager")["id"],
        )
    maps.decide(tid, parent["id"], decision(decision="withdraw"))
    with pytest.raises(ValueError, match="withdrawn|current"):
        maps.propose(tid, node([evidence[0]["id"]], parent_id=parent["id"]), origin="agent")


def test_sources_are_required_current_and_tender_scoped(workspace):
    repo, tid, _, evidence = workspace
    maps = service(repo)
    with pytest.raises(ValueError):
        maps.propose(tid, node([]), origin="engineer")
    other = repo.create_tender("Other")["id"]
    with pytest.raises(KeyError):
        maps.propose(other, node([evidence[0]["id"]]), origin="engineer")
    source(repo, tid, digest="b" * 64)
    with pytest.raises(ValueError, match="current|changed"):
        maps.propose(tid, node([evidence[0]["id"]]), origin="agent")


def test_reading_or_citing_sources_never_implies_engineer_review(workspace):
    repo, tid, artifact, evidence = workspace
    maps = service(repo)
    repo.get_evidence(tid, evidence[0]["id"])
    repo.add_finding(tid, "Formation", "Inspect formation", "requirement", [evidence[0]["id"]])
    repo.add_finding(tid, "Repeated reference", "Same passage", "observation", [evidence[0]["id"]])
    coverage = maps.view(tid)["coverage"]
    assert coverage["registered_files"] == 1
    assert coverage["extracted_files"] == 1 and coverage["extracted_evidence"] == 2
    assert coverage["evidence_cited_in_findings"] == 1
    assert coverage["current_review_scopes"] == 0 and coverage["reviewed_artifacts_in_full"] == 0
    reviewed = maps.review(
        tid,
        decision(
            artifact_id=artifact["id"],
            scope_type="locator",
            locator="paragraph:1",
            scope_label="Formation requirement only",
        ),
    )
    assert reviewed["is_current"] is True and reviewed["whole_document_reviewed"] is False
    coverage = maps.view(tid)["coverage"]
    assert coverage["current_review_scopes"] == 1 and coverage["reviewed_artifacts_in_full"] == 0


def test_full_document_review_is_explicit_and_revision_makes_it_historical(workspace):
    repo, tid, artifact, _ = workspace
    maps = service(repo)
    with pytest.raises(ValueError):
        maps.review(
            tid,
            decision(
                artifact_id=artifact["id"], scope_type="artifact", scope_label="Whole specification"
            ),
        )
    maps.review(
        tid,
        decision(
            artifact_id=artifact["id"],
            scope_type="artifact",
            scope_label="Whole specification",
            whole_document_reviewed=True,
        ),
    )
    assert maps.view(tid)["coverage"]["reviewed_artifacts_in_full"] == 1
    source(repo, tid, digest="b" * 64)
    state = maps.view(tid)
    assert state["review_scopes"][0]["is_current"] is False
    assert state["coverage"]["reviewed_artifacts_in_full"] == 0


def test_review_rejects_missing_locator_and_other_tender_artifact(workspace):
    repo, tid, artifact, _ = workspace
    maps = service(repo)
    with pytest.raises(ValueError, match="locator|location"):
        maps.review(
            tid,
            decision(
                artifact_id=artifact["id"],
                scope_type="locator",
                locator="not present",
                scope_label="Foundation",
            ),
        )
    with pytest.raises(KeyError):
        maps.review(
            repo.create_tender("Other")["id"],
            decision(
                artifact_id=artifact["id"],
                scope_type="artifact",
                whole_document_reviewed=True,
                scope_label="Whole document",
            ),
        )


def test_page_review_checks_actual_pdf_page_count(workspace):
    import hashlib
    import pypdfium2 as pdfium

    repo, tid, _, _ = workspace
    document = pdfium.PdfDocument.new()
    page = document.new_page(300, 400)
    page.close()
    temporary = repo.home / "fixture.pdf"
    document.save(temporary)
    document.close()
    payload = temporary.read_bytes()
    digest = hashlib.sha256(payload).hexdigest()
    (repo.objects / digest).write_bytes(payload)
    artifact, _ = repo.register_artifact(
        tid,
        "Drawing.pdf",
        digest,
        len(payload),
        {"kind": "pdf", "status": "needs_attention", "metadata": {"pages": 999}, "segments": []},
    )
    maps = service(repo)
    with pytest.raises(ValueError, match="page"):
        maps.review(
            tid,
            decision(
                artifact_id=artifact["id"],
                scope_type="page",
                page=2,
                scope_label="Foundation details",
            ),
        )
    record = maps.review(
        tid,
        decision(
            artifact_id=artifact["id"], scope_type="page", page=1, scope_label="Foundation details"
        ),
    )
    assert record["page"] == 1 and record["whole_document_reviewed"] is False


def test_map_routes_exclude_agent_control_of_engineer_decisions(workspace):
    from fastapi import FastAPI
    from fastapi.testclient import TestClient

    repo, tid, _, evidence = workspace
    app = FastAPI()
    app.include_router(importlib.import_module("quantix.map_routes").create_router(repo))
    client = TestClient(app)
    saved = client.post(f"/api/tenders/{tid}/project-map/nodes", json=node([evidence[0]["id"]]))
    assert saved.status_code == 200 and saved.json()["origin"] == "engineer"
    assert (
        client.post(
            f"/api/tenders/{tid}/project-map/nodes/{saved.json()['id']}/decision",
            json={"decision": "approve"},
        ).status_code
        == 422
    )
    assert (
        client.post(
            f"/api/tenders/{tid}/project-map/nodes", json=node([evidence[0]["id"]], origin="agent")
        ).status_code
        == 422
    )
    assert "MapView" in app.openapi()["components"]["schemas"]
