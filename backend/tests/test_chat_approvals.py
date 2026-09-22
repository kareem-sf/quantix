import hashlib

import pytest

from quantix.chat_approvals import ApprovalDecision, decide
from quantix.office_types import EngineerQuestion, ManagerAnswer
from quantix.repository import Repository


@pytest.fixture
def job(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic chat approvals")
    text = "Site visit forms are due 48 hours before the visit."
    artifact, _ = repo.register_artifact(
        tender["id"],
        "Visit notice.pdf",
        hashlib.sha256(text.encode()).hexdigest(),
        len(text),
        {"kind": "pdf", "status": "extracted", "segments": [{"locator": "page:1", "text": text}]},
    )
    source_id = repo.artifact_evidence(tender["id"], artifact["id"])[0]["id"]
    run = repo.create_run(tender["id"], "manager", "What must we submit before the visit?")
    finding = repo.add_finding(
        tender["id"],
        "Forms due 48 hours before the visit",
        "Form 01 or 02 with a copy of the ID.",
        "requirement",
        [source_id],
        origin="agent",
        run_id=run["id"],
    )
    repo.add_message(tender["id"], "manager", "Forms are due 48 hours before.", run_id=run["id"])
    return repo, tender["id"], run["id"], finding["id"]


def test_the_reply_carries_what_its_job_needs_approved_and_the_question_it_asked(job):
    repo, tender_id, run_id, finding_id = job
    question = EngineerQuestion(
        text="Who should send the visit forms?",
        choices=[
            {"label": "I'll ask the client for the email", "recommended": True},
            {"label": "Send them to Mohammed Al-Rumaih"},
        ],
    )
    repo.event(run_id, "engineer_question", question.text, question.model_dump(mode="json"))

    reply = next(item for item in repo.messages(tender_id) if item["role"] == "manager")
    assert reply["question"]["text"] == "Who should send the visit forms?"
    assert reply["question"]["choices"][0]["recommended"] is True
    assert reply["approvals"] == [
        {
            "kind": "finding",
            "id": finding_id,
            "title": "Forms due 48 hours before the visit",
            "detail": "Form 01 or 02 with a copy of the ID.",
            "state": "waiting",
            "can_reject": True,
        }
    ]


def test_accepting_and_rejecting_in_the_chat_uses_the_records_own_decision(job):
    repo, tender_id, _run_id, finding_id = job
    accepted = decide(
        repo, tender_id, ApprovalDecision(kind="finding", id=finding_id, decision="accept")
    )
    assert accepted["state"] == "accepted"
    assert repo.list_findings(tender_id)[0]["state"] == "accepted"
    with pytest.raises(ValueError, match="change the plan"):
        decide(repo, tender_id, ApprovalDecision(kind="plan", id="any", decision="reject"))


def test_one_item_opens_in_full_with_its_sources(job):
    repo, tender_id, _run_id, finding_id = job
    from quantix.chat_approvals import approval_detail

    detail = approval_detail(repo, tender_id, "finding", finding_id)
    assert detail["title"] == "Forms due 48 hours before the visit"
    assert detail["detail"] == "Form 01 or 02 with a copy of the ID."
    assert detail["facts"] == ["Requirement"]
    assert len(detail["source_ids"]) == 1
    assert detail["state"] == "waiting"
    with pytest.raises(KeyError):
        approval_detail(repo, tender_id, "finding", "missing")


def test_a_question_needs_two_to_four_choices():
    with pytest.raises(ValueError):
        ManagerAnswer(
            summary="Next step", question={"text": "Pick one", "choices": [{"label": "Only"}]}
        )
    answer = ManagerAnswer(
        summary="Next step",
        question={"text": "Pick one", "choices": [{"label": "A"}, {"label": "B"}]},
    )
    assert answer.question.choices[1].recommended is False


def test_boq_rows_the_team_read_are_confirmed_or_set_aside_in_the_chat(tmp_path):
    from quantix.chat_approvals import approvals_for_run
    from quantix.estimates import EstimateService

    repo = Repository(tmp_path)
    tid = repo.create_tender("BOQ rows in chat")["id"]
    content = "A1 Concrete foundations 12.5 m3\nA2 Formwork 30 m2"
    digest = hashlib.sha256(content.encode()).hexdigest()
    (repo.objects / digest).write_bytes(content.encode())
    artifact, _ = repo.register_artifact(
        tid,
        "Bill.pdf",
        digest,
        len(content.encode()),
        {
            "kind": "pdf",
            "status": "extracted",
            "metadata": {"page_count": 1},
            "segments": [{"locator": "page:1", "page": 1, "text": content}],
        },
    )
    source = repo.artifact_evidence(tid, artifact["id"], 0, 10)[0]
    run = repo.create_run(tid, "manager", "Read the BOQ")
    service = EstimateService(repo)
    first = service.propose_source_row(
        tid,
        {
            "source_id": source["id"],
            "row_reference": "A1",
            "source_excerpt": "A1 Concrete foundations 12.5 m3",
            "description": "Concrete foundations",
            "unit": "m3",
            "quantity": "12.5",
        },
        run_id=run["id"],
    )
    second = service.propose_source_row(
        tid,
        {
            "source_id": source["id"],
            "row_reference": "A2",
            "source_excerpt": "A2 Formwork 30 m2",
            "description": "Formwork",
            "unit": "m2",
            "quantity": "30",
        },
        run_id=run["id"],
    )
    with repo.db.connect() as conn:
        items = approvals_for_run(repo, conn, tid, run["id"])
    assert [(item["kind"], item["title"], item["state"]) for item in items] == [
        ("boq_row", "A1 Concrete foundations", "waiting"),
        ("boq_row", "A2 Formwork", "waiting"),
    ]
    assert items[0]["detail"] == "12.5 m3 · Bill.pdf · page 1"

    accepted = decide(
        repo, tid, ApprovalDecision(kind="boq_row", id=first["id"], decision="accept")
    )
    rejected = decide(
        repo, tid, ApprovalDecision(kind="boq_row", id=second["id"], decision="reject")
    )
    assert (accepted["state"], rejected["state"]) == ("accepted", "rejected")
    service.refresh(tid)
    rows = {item["id"]: item for item in service.view(tid)["items"]}
    assert rows[first["id"]]["confirmed"] is True
    assert second["id"] not in rows

    # A slip of the finger: both decisions can be changed while the row is unpriced.
    switched = decide(
        repo, tid, ApprovalDecision(kind="boq_row", id=first["id"], decision="reject")
    )
    restored = decide(
        repo, tid, ApprovalDecision(kind="boq_row", id=second["id"], decision="accept")
    )
    assert (switched["state"], restored["state"]) == ("rejected", "accepted")
    rows = {item["id"]: item for item in service.view(tid)["items"]}
    assert first["id"] not in rows
    assert rows[second["id"]]["confirmed"] is True


def test_a_boq_row_title_does_not_repeat_its_item_number():
    from quantix.chat_approvals import _row_title

    assert (
        _row_title("2.3.4", "Structure 2.3 - 2.3.4 Solid slab")
        == "Structure 2.3 - 2.3.4 Solid slab"
    )
    assert _row_title("A1", "Concrete foundations") == "A1 Concrete foundations"


def test_a_built_up_rate_shows_its_total():
    from quantix.chat_approvals import _rate_amount

    assert (
        _rate_amount(
            {
                "components": [
                    {"quantity": "1", "unit_rate": "62000"},
                    {"quantity": "1", "unit_rate": "33000"},
                ]
            }
        )
        == "95,000"
    )
    assert _rate_amount({"unit_rate": "330.5"}) == "330.5"
