"""Scope is applied before ranking. Empty permission is never unrestricted."""

import hashlib

from quantix.repository import Repository
from quantix.retrieval_service import retrieve


def _source(repo, tender_id, path, text, *, kind="pdf"):
    body = text.encode()
    artifact, _ = repo.register_artifact(
        tender_id,
        path,
        hashlib.sha256(body).hexdigest(),
        len(body),
        {
            "kind": kind,
            "status": "extracted",
            "segments": [{"locator": "page:1", "text": text, "page": 1}],
        },
    )
    return artifact, repo.artifact_evidence(tender_id, artifact["id"])[0]


def test_permitted_hit_below_many_forbidden_copies_is_still_returned(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Scope")
    for index in range(40):
        _source(
            repo,
            tender["id"],
            f"Noise/bond-{index:02d}.pdf",
            "The tenderer shall provide a bid bond of two percent.",
        )
    permitted_artifact, permitted = _source(
        repo,
        tender["id"],
        "Permitted/conditions.pdf",
        "The contractor shall provide a bid bond of two percent of the contract sum in the form of a bank guarantee.",
    )
    response = retrieve(
        repo,
        tender["id"],
        "bid bond bank guarantee",
        mode="words",
        limit=2,
        artifact_ids=[permitted_artifact["id"]],
    )
    assert [hit.id for hit in response.hits] == [permitted["id"]]


def test_empty_permitted_scope_returns_no_candidates(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Empty")
    _source(repo, tender["id"], "Spec/a.pdf", "Bid bond two percent.")
    response = retrieve(repo, tender["id"], "bid bond", mode="words", artifact_ids=[])
    assert response.hits == []
    unrestricted = retrieve(repo, tender["id"], "bid bond", mode="words", artifact_ids=None)
    assert unrestricted.hits


def test_foreign_tender_evidence_is_excluded(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("One")
    other = repo.create_tender("Two")
    _source(repo, tender["id"], "A/spec.pdf", "Waterproof membrane to the roof.")
    foreign_artifact, foreign = _source(
        repo, other["id"], "A/spec.pdf", "Waterproof membrane to the roof."
    )
    response = retrieve(repo, tender["id"], "waterproof membrane", mode="words")
    assert all(hit.id != foreign["id"] for hit in response.hits)
    assert all(hit.artifact_id != foreign_artifact["id"] for hit in response.hits)


def test_superseded_revision_is_not_returned_as_current(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Rev")
    old_artifact, old = _source(
        repo, tender["id"], "Plant/pump.pdf", "Pump output 70 cubic metres per hour."
    )
    _source(repo, tender["id"], "Plant/pump.pdf", "Pump output 82 cubic metres per hour.")
    response = retrieve(repo, tender["id"], "cubic metres per hour", mode="words")
    ids = {hit.id for hit in response.hits}
    assert old["id"] not in ids
    assert old_artifact["id"] not in {hit.artifact_id for hit in response.hits}


def test_revoked_artifact_is_dropped_on_revalidation(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Revoke")
    kept_artifact, kept = _source(repo, tender["id"], "Keep/spec.pdf", "Concrete grade B500D.")
    gone_artifact, _gone = _source(
        repo, tender["id"], "Gone/spec.pdf", "Concrete grade B500D extra."
    )
    response = retrieve(
        repo,
        tender["id"],
        "B500D",
        mode="words",
        artifact_ids=[kept_artifact["id"], gone_artifact["id"]],
    )
    assert kept["id"] in {hit.id for hit in response.hits}
    revoked = retrieve(
        repo,
        tender["id"],
        "B500D",
        mode="words",
        artifact_ids=[kept_artifact["id"]],
    )
    assert [hit.artifact_id for hit in revoked.hits] == [kept_artifact["id"]]


def test_two_permitted_duplicate_occurrences_are_preserved(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Dup")
    first_artifact, first = _source(
        repo, tender["id"], "Civil/waterproof.pdf", "Provide a four millimetre SBS membrane."
    )
    second_artifact, second = _source(
        repo, tender["id"], "Architecture/waterproof.pdf", "Provide a four millimetre SBS membrane."
    )
    response = retrieve(
        repo,
        tender["id"],
        "SBS membrane",
        mode="words",
        limit=5,
        artifact_ids=[first_artifact["id"], second_artifact["id"]],
    )
    assert len(response.hits) == 1
    hit = response.hits[0]
    others = {hit.id, *(item.evidence_id for item in hit.duplicate_occurrences)}
    assert {first["id"], second["id"]} <= others
