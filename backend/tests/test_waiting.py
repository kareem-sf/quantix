"""What is waiting on the engineer is collected from real pending records only."""

from quantix.repository import Repository
from quantix.waiting import WaitingService
from quantix.work_brief import WorkBriefService
from quantix.work_brief_models import BriefQuestion, BriefStep, WorkBriefDraft


def test_nothing_waits_on_a_new_tender(tmp_path):
    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Quiet Tender")
    state = WaitingService(repo).list(tender["id"])
    assert state.items == [] and state.total == 0


def test_proposed_plan_findings_and_engineer_questions_wait(tmp_path):
    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Busy Tender")
    plan = repo.create_plan(
        tender["id"],
        "Review the package",
        [
            {
                "title": "Map the package",
                "description": "Read every document.",
                "role": "Tender Manager",
            }
        ],
    )
    repo.add_finding(
        tender["id"], "Visit date differs", "Schedule 21 Sep, clarification 23 Sep", "risk", []
    )
    repo.add_finding(tender["id"], "Use BOQ units?", "Units differ from drawings", "question", [])
    accepted = repo.add_finding(tender["id"], "Already decided", "x", "observation", [])
    repo.decide_finding(tender["id"], accepted["id"], "accept", "fine")
    run = repo.create_run(tender["id"], "manager")
    WorkBriefService(repo).save(
        tender["id"],
        run["id"],
        "manager",
        WorkBriefDraft(
            outcome="Review the package",
            status="in_progress",
            steps=[BriefStep(title="Map the package", state="done", owner="Tender Manager")],
            open_questions=[
                BriefQuestion(text="Include the guard house?", owner="engineer", affects="Scope"),
                BriefQuestion(text="Which sheet is current?", owner="colleague"),
            ],
        ),
        expected_version=0,
        inspected_source_ids=set(),
        idempotency_key="brief-1",
    )

    state = WaitingService(repo).list(tender["id"])
    titles = [(item.kind, item.title) for item in state.items]
    assert titles == [
        ("plan", "Approve the work plan"),
        ("question", "Use BOQ units?"),
        ("finding", "Decide on 1 finding"),
        ("question", "Include the guard house?"),
    ]
    assert state.items[0].target_id == plan["id"]
    assert state.total == 4
