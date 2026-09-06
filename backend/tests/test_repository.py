"""Behaviour contract for persistent, Tender-scoped project knowledge."""

import importlib

import pytest


@pytest.fixture
def repo(tmp_path):
    try:
        module = importlib.import_module("quantix.repository")
    except ModuleNotFoundError:
        pytest.fail("The persistent Tender repository has not been implemented")
    return module.Repository(tmp_path)


def test_tender_creation_survives_reopen(repo):
    tender = repo.create_tender("Pier foundations")
    other = type(repo)(repo.home)
    assert other.get_tender(tender["id"])["name"] == "Pier foundations"
    assert [row["id"] for row in other.list_tenders()] == [tender["id"]]


def test_blank_tender_name_is_rejected(repo):
    with pytest.raises(ValueError):
        repo.create_tender("   ")


def test_finding_cannot_cite_an_unknown_source(repo):
    tender = repo.create_tender("Tender A")
    with pytest.raises(ValueError, match="source"):
        repo.add_finding(
            tender["id"], "Concrete strength", "Proposed finding", "requirement", ["invented"]
        )
    assert repo.list_findings(tender["id"]) == []


def test_plan_requires_explicit_engineer_approval(repo):
    tender = repo.create_tender("Tender A")
    plan = repo.create_plan(
        tender["id"],
        "Review the package",
        [
            {
                "title": "Review scope",
                "description": "Identify missing documents",
                "role": "Tender analyst",
                "source_ids": [],
            }
        ],
    )
    assert plan["status"] == "proposed"
    assert repo.list_tasks(tender["id"])[0]["status"] == "awaiting_approval"
    with pytest.raises(ValueError):
        repo.approve_plan(tender["id"], plan["id"], "")
    approved = repo.approve_plan(tender["id"], plan["id"], "Proceed with scope review")
    assert approved["status"] == "approved"
    assert repo.list_tasks(tender["id"])[0]["status"] == "ready"


def test_recovery_does_not_claim_running_jobs_completed(repo):
    tender = repo.create_tender("Tender A")
    run = repo.create_run(tender["id"], "manager", "Review documents")
    repo.update_run(run["id"], status="running", progress=10)
    repo.recover_interrupted_runs()
    restored = repo.get_run(run["id"])
    assert restored["status"] == "interrupted"
    assert restored["progress"] < 100


def test_empty_search_is_safe_and_tender_scoped(repo):
    tender = repo.create_tender("Tender A")
    assert repo.search(tender["id"], "") == []
    assert repo.search(tender["id"], '" OR * :') == []
    with pytest.raises(KeyError):
        repo.search("missing", "concrete")


def source(
    repo, tender_id, path="Civil/spec.pdf", digest="a" * 64, body="Concrete strength 30 MPa"
):
    artifact, changed = repo.register_artifact(
        tender_id,
        path,
        digest,
        100,
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [{"locator": "Page 1", "text": body, "page": 1}],
        },
    )
    return artifact, repo.artifact_evidence(tender_id, artifact["id"])[0], changed


def test_sources_are_scoped_and_duplicates_are_searchable_once(repo):
    a, b = repo.create_tender("A"), repo.create_tender("B")
    artifact, evidence, _ = source(repo, a["id"])
    source(repo, a["id"], "Arch/spec.pdf")
    assert len(repo.search(a["id"], "30 concrete")) == 1
    assert repo.search(b["id"], "concrete") == []
    with pytest.raises(KeyError):
        repo.get_evidence(b["id"], evidence["id"])
    with pytest.raises(ValueError):
        repo.add_finding(b["id"], "Strength", "30 MPa", "requirement", [evidence["id"]])
    again, _, changed = source(repo, a["id"])
    assert not changed
    assert again["id"] == artifact["id"]


def test_revision_preserves_old_evidence_and_flags_affected_findings(repo):
    tender = repo.create_tender("A")
    old, evidence, _ = source(repo, tender["id"])
    finding = repo.add_finding(tender["id"], "Concrete", "30 MPa", "requirement", [evidence["id"]])
    new, _, _ = source(repo, tender["id"], digest="b" * 64, body="Concrete strength 40 MPa")
    assert new["version"] == 2
    assert repo.get_evidence(tender["id"], evidence["id"])["text"] == "Concrete strength 30 MPa"
    assert not repo.get_artifact(tender["id"], old["id"])["is_current"]
    assert repo.list_findings(tender["id"])[0]["is_stale"]
    with pytest.raises(ValueError, match="changed"):
        repo.decide_finding(tender["id"], finding["id"], "accept", "Checked")


def test_area_filter_is_applied_before_duplicate_ranking(repo):
    tender = repo.create_tender("A")
    source(repo, tender["id"], "Building A/spec.pdf")
    source(repo, tender["id"], "Building B/spec.pdf")
    hits = repo.search(tender["id"], "concrete", limit=1, area="Building B")
    assert len(hits) == 1
    assert hits[0]["relative_path"] == "Building B/spec.pdf"
