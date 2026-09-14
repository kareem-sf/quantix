"""Corpus installation preserves locators so labels can bind to live evidence."""

from quantix.repository import Repository

from .install import annotate_hits, apply_pump_revision, install_corpus


def test_install_binds_labels_and_keyword_hits(tmp_path):
    repo = Repository(tmp_path / "home")
    tender = repo.create_tender("Synthetic retrieval corpus")
    mapping = install_corpus(repo, tender["id"])
    assert mapping["reinforcement-ar"]["evidence"]["page:1"]
    assert mapping["clauses-en"]["evidence"]["clause:14.7.1"]
    hits = annotate_hits(repo.search(tender["id"], "B500D", limit=10), mapping, tender["id"])
    documents = {hit.get("document") for hit in hits}
    assert "reinforcement-ar" in documents or "grades-en" in documents
    assert all(hit.get("tender_id") == tender["id"] for hit in hits)


def test_revision_replaces_the_current_pump_source(tmp_path):
    repo = Repository(tmp_path / "home")
    tender = repo.create_tender("Revision corpus")
    mapping = install_corpus(repo, tender["id"])
    old = mapping["pump-en"]["evidence"]["page:1"]
    mapping = apply_pump_revision(repo, tender["id"], mapping)
    new = mapping["pump-en"]["evidence"]["page:1"]
    assert new != old
    current = repo.artifact_evidence(tender["id"], mapping["pump-en"]["artifact_id"])[0]
    assert "82 cubic metres" in current["text"]
    hits = annotate_hits(repo.search(tender["id"], "82 cubic", limit=5), mapping, tender["id"])
    assert any(hit.get("document") == "pump-en" and hit["id"] == new for hit in hits)
