import hashlib

import pytest
from docx import Document
from test_estimates import approval, seed

from quantix.estimates import EstimateService
from quantix.outputs import OutputService
from quantix.repository import Repository


@pytest.fixture
def report_setup(tmp_path):
    repo = Repository(tmp_path)
    tender_id = repo.create_tender("School foundations")["id"]
    source_bytes = b"Controlled synthetic BOQ source"
    digest = hashlib.sha256(source_bytes).hexdigest()
    seed(repo, tender_id, digest=digest)
    (repo.objects / digest).write_bytes(source_bytes)
    estimates = EstimateService(repo)
    estimates.refresh(tender_id)
    return repo, tender_id, estimates, OutputService(repo)


def save_engineering_analysis(repo, tender_id, content, source_ids, *, kind="manager", status="completed"):
    run = repo.create_run(tender_id, kind, "Review the Tender documents")
    repo.update_run(run["id"], status="running")
    repo.add_message(tender_id, "manager", content, source_ids, run_id=run["id"])
    repo.update_run(
        run["id"],
        status=status,
        result={"summary": content, "source_ids": source_ids},
    )
    return run


def save_conversation_reply(repo, tender_id, content):
    run = repo.create_run(tender_id, "conversation", content)
    repo.update_run(run["id"], status="running")
    repo.add_message(tender_id, "manager", content, run_id=run["id"])
    repo.update_run(
        run["id"],
        status="completed",
        result={"kind": "conversation", "reply": content, "next_action": "Review the Tender documents."},
    )
    return run


def document_text(outputs, tender_id, record):
    document = Document(outputs.path(tender_id, record["id"]))
    return "\n".join(paragraph.text for paragraph in document.paragraphs)


def test_report_uses_completed_conversation_run_that_entered_engineering_after_later_greeting(
    report_setup,
):
    repo, tender_id, estimates, outputs = report_setup
    source_id = estimates.view(tender_id)["items"][0]["source_id"]
    analysis = "Allow for groundwater monitoring before excavation."
    save_engineering_analysis(
        repo,
        tender_id,
        analysis,
        [source_id],
        kind="conversation",
    )
    save_conversation_reply(repo, tender_id, "Good morning. How can I help?")

    record = outputs.generate(tender_id, approval(kind="analysis_docx"))
    text = document_text(outputs, tender_id, record)

    assert analysis in text
    assert "Good morning. How can I help?" not in text
    assert record["source_ids"] == [source_id]


def test_report_ignores_a_later_failed_engineering_run(report_setup):
    repo, tender_id, estimates, outputs = report_setup
    source_id = estimates.view(tender_id)["items"][0]["source_id"]
    completed = "The completed review identified a temporary works risk."
    failed = "Uncommitted text from a failed later review."
    save_engineering_analysis(repo, tender_id, completed, [source_id])
    save_engineering_analysis(repo, tender_id, failed, [source_id], status="failed")

    record = outputs.generate(tender_id, approval(kind="analysis_docx"))
    text = document_text(outputs, tender_id, record)

    assert completed in text
    assert failed not in text


def test_report_states_when_no_attributable_analysis_exists(report_setup):
    repo, tender_id, _estimates, outputs = report_setup
    repo.add_message(tender_id, "manager", "Historical Manager text with no run link.")
    unknown_run = repo.create_run(tender_id, "conversation", "Historical request")
    repo.add_message(
        tender_id,
        "manager",
        "Historical text linked to a run with no recorded outcome.",
        run_id=unknown_run["id"],
    )
    repo.update_run(unknown_run["id"], status="completed", result={})

    record = outputs.generate(tender_id, approval(kind="analysis_docx"))
    text = document_text(outputs, tender_id, record)

    assert "Historical Manager text with no run link." not in text
    assert "Historical text linked to a run with no recorded outcome." not in text
    assert "No completed Tender Manager engineering analysis with recorded run provenance is available." in text
    assert "Review limitation" in text
    assert any("recorded run provenance" in warning for warning in record["metadata"]["warnings"])


def test_source_revision_still_blocks_a_report_with_attributable_analysis(report_setup):
    repo, tender_id, estimates, outputs = report_setup
    source_id = estimates.view(tender_id)["items"][0]["source_id"]
    save_engineering_analysis(repo, tender_id, "Review based on the first source revision.", [source_id])
    record = outputs.generate(tender_id, approval(kind="analysis_docx"))

    revised_bytes = b"Controlled synthetic BOQ source revision two"
    revised_digest = hashlib.sha256(revised_bytes).hexdigest()
    seed(repo, tender_id, digest=revised_digest)
    (repo.objects / revised_digest).write_bytes(revised_bytes)

    with pytest.raises(ValueError, match="source|working records|current"):
        outputs.check_current(tender_id, record)


def test_later_greeting_does_not_make_an_existing_report_stale(report_setup):
    repo, tender_id, estimates, outputs = report_setup
    source_id = estimates.view(tender_id)["items"][0]["source_id"]
    save_engineering_analysis(repo, tender_id, "Completed foundation review.", [source_id])
    record = outputs.generate(tender_id, approval(kind="analysis_docx"))

    save_conversation_reply(repo, tender_id, "Hello again.")

    captured = outputs.check_current(tender_id, record)
    assert captured["basis_fingerprint"] == record["metadata"]["basis_fingerprint"]
