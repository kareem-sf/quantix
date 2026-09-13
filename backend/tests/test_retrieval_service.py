"""Meaning-first hybrid retrieval and the Manager's package map and whole-document tools."""

import asyncio
import json

from quantix.office_tools import OfficeContext, source_tools
from quantix.package_analysis import ensure_schema, source_fingerprint
from quantix.repository import Repository
from quantix.retrieval_service import hybrid_search
from quantix.tool_policy import dispatch


def _workspace(tmp_path, passages):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Retrieval Tender")
    run = repo.create_run(tender["id"], "manager", "Find the bid bond")
    repo.update_run(run["id"], status="running")
    artifact, _ = repo.register_artifact(tender["id"], "Conditions/general.pdf", "c" * 64, 100, {
        "kind": "pdf", "status": "extracted",
        "segments": [{"locator": f"page:{index + 1}", "text": text, "page": index + 1} for index, text in enumerate(passages)],
    })
    evidence = repo.artifact_evidence(tender["id"], artifact["id"], limit=50)
    return repo, tender, run, artifact, evidence


class FakeMeaning:
    """Returns chosen evidence in a fixed meaning order with chosen scores."""

    def __init__(self, repo, tender_id, ranked):
        self.repo, self.tender_id, self.ranked = repo, tender_id, ranked

    def search(self, tender_id, query, limit=20, **_kwargs):
        rows = {row["id"]: row for row in self.repo.artifact_evidence(tender_id, self.repo.list_artifacts(tender_id)[0]["id"], limit=50)}
        return [
            rows[evidence_id] | {"artifact_name": "general.pdf", "relative_path": "Conditions/general.pdf", "score": score,
                                 "metadata": {"semantic_match": {"start": 0, "end": 20, "semantic_score": score,
                                                                  "structure": {"heading": "Securities"}}}}
            for evidence_id, score in self.ranked
        ][:limit]


def test_meaning_and_word_matches_are_fused_and_labelled(tmp_path):
    repo, tender, _run, _artifact, evidence = _workspace(tmp_path, [
        "The tenderer shall provide a bid bond of two percent.",
        "Tender security must remain valid for 120 days.",
        "Concrete grade C30/37 for foundations.",
    ])
    meaning = FakeMeaning(repo, tender["id"], [(evidence[1]["id"], 0.86), (evidence[0]["id"], 0.82), (evidence[2]["id"], 0.70)])
    hits, info = hybrid_search(repo, tender["id"], "bid bond", 3, semantic=meaning)
    assert info["meaning"] == "ready"
    by_id = {hit["id"]: hit for hit in hits}
    assert by_id[evidence[0]["id"]]["found_by"] == "meaning+words"  # found both ways ranks first
    assert hits[0]["id"] == evidence[0]["id"]
    assert by_id[evidence[1]["id"]]["found_by"] == "meaning"  # different wording, still found
    assert not any(hit["weak_match"] for hit in hits)
    assert by_id[evidence[1]["id"]]["meaning_span"]["heading"] == "Securities"


def test_flat_meaning_only_results_are_weak_matches(tmp_path):
    repo, tender, _run, _artifact, evidence = _workspace(tmp_path, ["Roof waterproofing membrane.", "Reinforcing steel B500B."])
    meaning = FakeMeaning(repo, tender["id"], [(evidence[0]["id"], 0.717), (evidence[1]["id"], 0.716)])
    hits, info = hybrid_search(repo, tender["id"], "orbital period of Neptune", 5, semantic=meaning)
    assert hits and all(hit["weak_match"] for hit in hits)
    assert info["weak_results"] is True


def test_scope_is_applied_before_ranking(tmp_path):
    repo, tender, _run, _artifact, evidence = _workspace(tmp_path, [f"bid bond clause {n}" for n in range(12)])
    permitted = evidence[11]["id"]
    hits, _info = hybrid_search(repo, tender["id"], "bid bond", 2, mode="exact", allowed=lambda eid: eid == permitted)
    assert [hit["id"] for hit in hits] == [permitted]


def test_exact_mode_never_uses_meaning(tmp_path):
    repo, tender, _run, _artifact, evidence = _workspace(tmp_path, ["Clause 14.7 advance payment guarantee."])

    class Refuse:
        def search(self, *_args, **_kwargs):
            raise AssertionError("exact search must not embed the query")

    hits, info = hybrid_search(repo, tender["id"], "14.7", 5, mode="exact", semantic=Refuse())
    assert [hit["found_by"] for hit in hits] == ["words"] and info["meaning"] == "not_requested"


def _call(context, name, payload):
    definition = next(item for item in source_tools() if item.name == name)
    return json.loads(asyncio.run(dispatch("nested", definition, context, payload, invocation_id=f"{name}-call")))


def test_whole_document_reading_pages_through_the_document_and_counts_as_read(tmp_path):
    repo, tender, run, artifact, evidence = _workspace(tmp_path, ["A" * 7000, "B" * 7000, "C" * 3000])
    context = OfficeContext(repo, tender["id"], run["id"])
    first = _call(context, "read_whole_document", {"artifact_id": artifact["id"]})
    assert [passage["id"] for passage in first["passages"]] == [evidence[0]["id"]]
    assert first["next_offset"] == 1 and first["document_finished"] is False
    second = _call(context, "read_whole_document", {"artifact_id": artifact["id"], "offset": 1})
    assert [passage["id"] for passage in second["passages"]] == [evidence[1]["id"], evidence[2]["id"]] or \
        [passage["id"] for passage in second["passages"]] == [evidence[1]["id"]]
    assert all(context.has_seen_source(row["id"]) for row in evidence[:2])


def test_package_map_tool_reports_the_saved_map(tmp_path):
    repo, tender, run, artifact, _evidence = _workspace(tmp_path, ["Invitation to tender."])
    context = OfficeContext(repo, tender["id"], run["id"])
    assert _call(context, "read_package_map", {})["available"] is False
    ensure_schema(repo)
    data = {"overview": "Road works package.", "gaps": ["No BOQ"], "identity": {"name": "Harbour Road"},
            "documents": [{"document_id": artifact["id"], "relative_path": "Conditions/general.pdf",
                           "document_type": "conditions_of_contract", "brief": "General conditions."}]}
    with repo.db.connect(write=True) as conn:
        conn.execute("INSERT INTO package_maps VALUES(?,?,?,?)",
                     (tender["id"], json.dumps(data), source_fingerprint(repo.list_artifacts(tender["id"])), "now"))
    mapped = _call(context, "read_package_map", {})
    assert mapped["current"] is True and mapped["gaps"] == ["No BOQ"]
    assert mapped["documents"][0]["type"] == "conditions of contract"
