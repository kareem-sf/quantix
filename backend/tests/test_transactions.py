import pytest

from quantix.office import prepare_result, publish_prepared
from quantix.office_research import ResearchRecord
from quantix.office_tools import OfficeContext
from quantix.office_types import OfficeOutput
from quantix.repository import Repository


def test_office_result_is_published_as_one_transaction(tmp_path, monkeypatch):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Test")
    run = repo.create_run(tender["id"], "manager")
    artifact, _ = repo.register_artifact(
        tender["id"],
        "spec.pdf",
        "a" * 64,
        10,
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [{"locator": "Page 1", "text": "Concrete grade C30."}],
        },
    )
    source = repo.artifact_evidence(tender["id"], artifact["id"])[0]
    context = OfficeContext(repo, tender["id"], run["id"])
    context.source(source["id"])
    output = OfficeOutput.model_validate(
        {
            "summary": "Review the concrete requirement.",
            "source_ids": [source["id"]],
            "findings": [
                {
                    "title": "Concrete",
                    "detail": "C30 specified",
                    "kind": "requirement",
                    "source_ids": [source["id"]],
                }
            ],
            "plan": {
                "title": "Review",
                "tasks": [
                    {
                        "title": "Check",
                        "description": "Check concrete",
                        "role": "Engineer",
                        "source_ids": [],
                    }
                ],
            },
        }
    )

    def fail_plan(*args, **kwargs):
        raise OSError("Simulated storage failure")

    monkeypatch.setattr(repo, "create_plan", fail_plan)
    prepared = prepare_result(output, context, {}, ResearchRecord(context))
    with pytest.raises(OSError):
        with repo.atomic():
            publish_prepared(repo, prepared)
    assert repo.list_findings(tender["id"]) == []
    assert repo.messages(tender["id"]) == []
