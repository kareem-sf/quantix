"""Analyzing tender package: local stages, cached AI package map, naming and summary."""

import threading

import pytest

from quantix import package_analysis, structured_ai
from quantix.package_analysis import (
    BriefBatch,
    DocumentBrief,
    PackageOverview,
    apply_analysis,
    package_map,
    run_analysis,
)
from quantix.project_identity import ProjectIdentity
from quantix.repository import Repository


def _tender(tmp_path, *, name=None):
    repo = Repository(tmp_path / "home")
    tender = repo.create_tender(name)
    evidence = [
        ("01 ITT/Invitation to Tender.pdf", "pdf", "Invitation to tender for the Harbour Road Upgrade, Alexandria."),
        ("02 Specs/Concrete.pdf", "pdf", "Section 03 30 00 cast-in-place concrete grade C30/37."),
    ]
    for index, (path, kind, text) in enumerate(evidence):
        repo.register_artifact(tender["id"], path, f"{index:064x}", len(text), {
            "kind": kind, "status": "extracted", "metadata": {"page_count": 1}, "warnings": [],
            "segments": [{"locator": "page:1", "text": text, "page": 1, "kind": "text", "metadata": {}}],
        })
    return repo, tender


class _Stub:
    """Stands in for the AI: returns briefs for every document and records calls."""

    def __init__(self):
        self.calls = []

    async def __call__(self, repo, tender_id, run_id, prompt, output_type, *, operation):
        self.calls.append(operation)
        usage = {"requests": 1, "input_tokens": 1000, "output_tokens": 200, "cached_input_tokens": 400}
        if output_type is BriefBatch:
            ids = [artifact["id"] for artifact in repo.list_artifacts(tender_id) if artifact["id"] in prompt]
            return BriefBatch(briefs=[
                DocumentBrief(document_id=identifier, document_type="specification", brief="Concrete requirements.")
                for identifier in ids
            ]), usage
        return PackageOverview(
            identity=ProjectIdentity(name="Harbour Road Upgrade", client="Alexandria Port Authority",
                                     submission_deadline="12 October 2026"),
            overview="Road and quay works tender.", gaps=["No bill of quantities was found."],
        ), usage


@pytest.fixture
def local_only(monkeypatch):
    class NoIndex:
        def __init__(self, repo):
            pass

        def index(self, *_args):
            raise ValueError("synthetic: no model in this test")

    monkeypatch.setattr("quantix.semantic.SemanticService", NoIndex)


async def test_local_stages_complete_and_the_map_waits_for_ai(tmp_path, local_only):
    repo, tender = _tender(tmp_path)
    run = repo.create_run(tender["id"], "analysis")
    result = await run_analysis(repo, tender["id"], run["id"], threading.Event(), ai_ready=lambda: "No AI chosen.")
    assert [key for key in result.stages] == ["register", "recognise", "index", "structure", "map"]
    assert result.stages["index"]["state"] == "completed"
    assert "background" in result.stages["index"]["detail"]
    assert result.stages["structure"]["state"] == "completed"
    assert result.stages["map"]["state"] == "waiting"
    labels = [event["data"]["label"] for event in repo.run_events(run["id"]) if event["kind"] == "analysis_stage"]
    assert "Indexing tender evidence" in labels and "Mapping the tender package" in labels


async def test_package_map_names_the_project_and_briefs_are_cached(tmp_path, local_only, monkeypatch):
    repo, tender = _tender(tmp_path)
    stub = _Stub()
    monkeypatch.setattr(structured_ai, "ask_structured", stub)
    run = repo.create_run(tender["id"], "analysis")
    result = await run_analysis(repo, tender["id"], run["id"], threading.Event(), ai_ready=lambda: True)
    assert stub.calls == ["package_briefs", "package_map"]
    assert result.stages["map"]["state"] == "completed"
    assert result.usage["cached_input_tokens"] == 800
    with repo.atomic():
        applied = apply_analysis(repo, result)
    assert applied["mapped_documents"] == 2
    saved = repo.get_tender(tender["id"])
    assert (saved["name"], saved["name_source"]) == ("Harbour Road Upgrade", "ai")
    mapped = package_map(repo, tender["id"])
    assert mapped["current"] and len(mapped["documents"]) == 2
    assert mapped["gaps"] == ["No bill of quantities was found."]
    summary = repo.messages(tender["id"])[-1]["content"]
    assert "**Harbour Road Upgrade**" in summary and "Submission deadline: 12 October 2026" in summary

    # A second analysis of the same files reuses every brief: only the short package pass is asked.
    again = _Stub()
    monkeypatch.setattr(structured_ai, "ask_structured", again)
    rerun = repo.create_run(tender["id"], "analysis")
    second = await run_analysis(repo, tender["id"], rerun["id"], threading.Event(), ai_ready=lambda: True)
    assert again.calls == ["package_map"]
    assert second.stages["map"]["from_cache"] == 2


async def test_engineer_named_tenders_keep_their_name(tmp_path, local_only, monkeypatch):
    repo, tender = _tender(tmp_path, name="Corniche Works")
    monkeypatch.setattr(structured_ai, "ask_structured", _Stub())
    run = repo.create_run(tender["id"], "analysis")
    result = await run_analysis(repo, tender["id"], run["id"], threading.Event(), ai_ready=lambda: True)
    with repo.atomic():
        apply_analysis(repo, result)
    assert repo.get_tender(tender["id"])["name"] == "Corniche Works"


def test_recognition_summary_counts_pages_needing_a_second_reading():
    artifacts = [{
        "relative_path": "scans/drawing.pdf", "metadata": {"page_count": 4, "ocr_pages": 3},
        "warnings": [{"code": "ocr_low_confidence", "locator": "page:2"}, {"code": "pdf_no_text", "locator": "page:4"}],
    }]
    summary = package_analysis.recognition_summary(artifacts)
    assert (summary["pages"], summary["recognised_pages"], summary["uncertain_pages"], summary["unreadable_pages"]) == (4, 3, 1, 1)
    assert summary["attention"] == ["scans/drawing.pdf page:2", "scans/drawing.pdf page:4"]


async def test_brief_cache_is_bound_to_tender_and_complete_source_context(tmp_path, local_only, monkeypatch):
    repo, tender = _tender(tmp_path)
    stub = _Stub()
    monkeypatch.setattr(structured_ai, "ask_structured", stub)
    async def analyze(tid):
        run = repo.create_run(tid, "analysis")
        return await run_analysis(repo, tid, run["id"], threading.Event(), ai_ready=lambda: True)
    await analyze(tender["id"])
    other = repo.create_tender("Separate package")
    for artifact in repo.list_artifacts(tender["id"]):
        repo.register_artifact(other["id"], artifact["relative_path"], artifact["content_hash"], 20, {
            "kind": "pdf", "status": "extracted", "segments": [{"locator": "page:1", "text": "Different opening context."}],
        })
    stub.calls.clear()
    await analyze(other["id"])
    assert stub.calls == ["package_briefs", "package_map"]
    stub.calls.clear()
    artifact = repo.list_artifacts(tender["id"])[0]
    with repo.db.connect(write=True) as conn:
        conn.execute("UPDATE evidence SET text='Corrected extraction for another project.' WHERE artifact_id=?", (artifact["id"],))
    await analyze(tender["id"])
    assert stub.calls == ["package_briefs", "package_map"]


async def test_staff_read_the_same_package_map_as_the_manager(tmp_path, monkeypatch):
    import json

    from quantix.ai_tools import ToolContext
    from quantix.office_tools import OfficeContext, source_tools

    repo, tender = _tender(tmp_path)
    artifact = repo.list_artifacts(tender["id"])[0]
    saved_map = {"current": True, "identity": {"client": "Synthetic employer"}, "overview": "Two-storey school",
                 "gaps": [], "readability": {}, "documents": [{"document_id": artifact["id"], "brief": "Specification"}]}
    monkeypatch.setattr(package_analysis, "package_map", lambda *_: saved_map)
    run = repo.create_run(tender["id"], "manager", "Review")
    definition = next(tool for tool in source_tools() if tool.name == "read_package_map")
    manager = OfficeContext(repo, tender["id"], run["id"])
    staff = OfficeContext(repo, tender["id"], run["id"], actor_id="staff-1", assignment_id="assignment-1")
    for context in (manager, staff):
        response = json.loads(await definition.function.__wrapped__(ToolContext(context)))
        assert response["available"] is True
        assert response["overview"] == "Two-storey school"


def test_overlong_orientation_text_is_trimmed_not_rejected():
    brief = DocumentBrief.model_validate({
        "document_id": "d1", "document_type": "addendum_or_clarification", "title": None,
        "brief": "b" * 900, "key_locations": ["page 1 onward: " + "x" * 300], "key_topics": ["t"] * 12,
    })
    assert len(brief.brief) == 600 and len(brief.key_locations[0]) == 120 and len(brief.key_topics) == 8
    assert brief.title is None
