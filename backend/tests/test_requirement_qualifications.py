"""A citation cannot turn a conditional source clause into an unconditional duty."""

import pytest
from test_estimates import approval
from test_source_boq import document

from quantix.repository import Repository
from quantix.tender_requirements import RequirementService


def setup_clause(tmp_path, text):
    repo = Repository(tmp_path)
    tid = repo.create_tender("Synthetic qualifications")["id"]
    _, source = document(repo, tid, text)
    run = repo.create_run(tid, "manager", "Prepare requirements")
    service = RequirementService(repo)
    proposal = {
        "title": "Separate offers",
        "detail": "Provide separate offers.",
        "source_ids": [source["id"]],
        "deliverable_kind": "registers_xlsx",
    }
    return repo, tid, run, service, proposal


@pytest.mark.parametrize(
    "clause",
    [
        "If the invitation requires separate offers, provide two PDF files.",
        "إذا اشترطت المناقصة فصل العرضين، يجب تقديم ملفين منفصلين.",
    ],
)
def test_dropped_condition_is_rejected_but_exact_condition_is_preserved(tmp_path, clause):
    _, tid, run, service, proposal = setup_clause(tmp_path, clause)
    with pytest.raises(ValueError, match="condition"):
        service.propose(
            tid,
            proposal | {"source_quote": clause, "applicability": "unconditional"},
            origin="manager",
            run_id=run["id"],
        )
    saved = service.propose(
        tid,
        proposal | {"source_quote": clause, "applicability": "conditional", "condition": clause},
        origin="manager",
        run_id=run["id"],
    )
    assert saved["condition"] == clause
    assert saved["applicability"] == "conditional"
    with pytest.raises(ValueError, match="applicability"):
        service.decide(tid, saved["id"], approval(decision="approve"))
    approved = service.decide(
        tid, saved["id"], approval(decision="approve", applicability_reviewed=True)
    )
    assert approved["status"] == "approved"
    assert approved["condition"] == clause


def test_cherry_picked_quote_does_not_drop_prefix_condition(tmp_path):
    _, tid, run, service, proposal = setup_clause(tmp_path, "If requested, provide two PDF files.")
    with pytest.raises(ValueError, match="condition"):
        service.propose(
            tid,
            proposal | {"source_quote": "provide two PDF files.", "applicability": "unconditional"},
            origin="manager",
            run_id=run["id"],
        )
    # Duty text cannot pass as the condition: Quantix records the source's own
    # words and widens the quote to the line they sit on.
    saved = service.propose(
        tid,
        proposal
        | {
            "source_quote": "provide two PDF files.",
            "applicability": "conditional",
            "condition": "provide two PDF files.",
        },
        origin="manager",
        run_id=run["id"],
    )
    assert saved["condition"] == "If requested"
    assert saved["source_quote"] == "If requested, provide two PDF files."


def test_nonempty_duty_text_cannot_substitute_for_an_exception(tmp_path):
    clause = "Provide two PDF files, except exempt bidders may submit one."
    _, tid, run, service, proposal = setup_clause(tmp_path, clause)
    saved = service.propose(
        tid,
        proposal
        | {
            "source_quote": "Provide two PDF files",
            "applicability": "unconditional",
            "exceptions": ["Provide two PDF files"],
        },
        origin="manager",
        run_id=run["id"],
    )
    # The duty sentence is replaced by the source's own exception.
    assert saved["exceptions"] == ["exempt bidders may submit one"]
    assert saved["source_quote"] == clause


def test_exception_and_quote_attribution_are_required_for_manager(tmp_path):
    clause = "Provide a guarantee, except small enterprises may submit an exemption certificate."
    _, tid, run, service, proposal = setup_clause(tmp_path, clause)
    with pytest.raises(ValueError, match="clause|quote"):
        service.propose(tid, proposal, origin="manager", run_id=run["id"])
    with pytest.raises(ValueError, match="exception"):
        service.propose(
            tid,
            proposal | {"source_quote": clause, "applicability": "unconditional"},
            origin="manager",
            run_id=run["id"],
        )
    saved = service.propose(
        tid,
        proposal
        | {
            "source_quote": clause,
            "applicability": "unconditional",
            "exceptions": ["small enterprises may submit an exemption certificate"],
        },
        origin="manager",
        run_id=run["id"],
    )
    assert saved["exceptions"] == ["small enterprises may submit an exemption certificate"]


def test_unqualified_historical_proposals_remain_visible_and_require_review(tmp_path):
    repo, tid, _, service, proposal = setup_clause(tmp_path, "Provide two PDF files.")
    saved = service.propose(tid, proposal)
    with repo.db.connect() as conn:
        before = conn.execute(
            "SELECT payload_json FROM submission_requirements WHERE id=?", (saved["id"],)
        ).fetchone()[0]
    assert saved["applicability"] == "unknown"
    assert any("applicability" in warning.lower() for warning in saved["warnings"])
    with pytest.raises(ValueError, match="applicability"):
        service.decide(tid, saved["id"], approval(decision="approve"))
    with repo.db.connect() as conn:
        assert (
            conn.execute(
                "SELECT payload_json FROM submission_requirements WHERE id=?", (saved["id"],)
            ).fetchone()[0]
            == before
        )


def test_a_clause_copied_without_the_pdf_direction_marks_still_matches(tmp_path):
    # A reader copying Arabic from a page does not reproduce the marks a PDF stores.
    stored = "إذا‏ اشترطت المناقصة فصل  العرضين، يجب تقديم ملفين منفصلين."
    copied = "إذا اشترطت المناقصة فصل العرضين، يجب تقديم ملفين منفصلين."
    _, tid, run, service, proposal = setup_clause(tmp_path, stored)
    saved = service.propose(
        tid,
        proposal | {"source_quote": copied, "applicability": "conditional", "condition": copied},
        origin="manager",
        run_id=run["id"],
    )
    assert saved["applicability"] == "conditional"


def test_a_condition_stops_at_the_next_numbered_item_on_a_merged_line(tmp_path):
    from quantix.requirement_qualifications import _CONDITION, _phrases

    # The PDF stored two numbered duties as one line with no punctuation between.
    line = (
        "1- تقديم العرض المالي باللغة العربية في حال تم رفعه باللغة الإنجليزية يجب إرفاق خطاب "
        "باللغة العربية -2 تعبئة وتوقيع وختم جميع صفحات كراسة المواصفات الفنية المرفقة"
    )
    condition = next(_phrases(_CONDITION, line, keep_marker=True))
    assert condition.startswith("في حال تم رفعه")
    assert "تعبئة وتوقيع" not in condition

    _, tid, run, service, proposal = setup_clause(tmp_path, line)
    saved = service.propose(
        tid,
        proposal | {"source_quote": line, "applicability": "conditional", "condition": condition},
        origin="manager",
        run_id=run["id"],
    )
    assert saved["condition"] == condition


def test_a_neighbouring_items_condition_is_not_attached_to_this_one(tmp_path):
    # One stored line, three numbered duties; only the first carries a condition.
    line = (
        "-1 رفع العرض المالي باللغة العربية فقط، وفي حال تم رفعه باللغة الإنجليزية يجب إرفاق "
        "خطاب باللغة العربية -2 تعبئة وتوقيع وختم جميع صفحات كراسة المواصفات الفنية المرفقة "
        "-3 تعبئة وتوقيع وختم نموذج العطاء وصيغة التعهد"
    )
    _, tid, run, service, proposal = setup_clause(tmp_path, line)
    second = "تعبئة وتوقيع وختم جميع صفحات كراسة المواصفات الفنية المرفقة"
    saved = service.propose(
        tid,
        proposal | {"source_quote": second, "applicability": "unconditional"},
        origin="manager",
        run_id=run["id"],
    )
    assert saved["applicability"] == "unconditional" and not saved["condition"]

    first = (
        "رفع العرض المالي باللغة العربية فقط، وفي حال تم رفعه باللغة الإنجليزية يجب "
        "إرفاق خطاب باللغة العربية"
    )
    with pytest.raises(ValueError, match="condition"):
        service.propose(
            tid,
            proposal
            | {"title": "Arabic offer", "source_quote": first, "applicability": "unconditional"},
            origin="manager",
            run_id=run["id"],
        )


def test_a_condition_in_a_neighbouring_sentence_is_not_this_requirements(tmp_path):
    # One stored paragraph, two sentences; the condition belongs to the first.
    text = (
        "في حال كانت الشركة اجنبية ذات سجل تجاري سعودي فيجب ان تكون نسبة الملكية للمواطنين فوق 51%. "
        "تعبئة وتوقيع وختم نموذج العطاء وصيغة التعهد."
    )
    _, tid, run, service, proposal = setup_clause(tmp_path, text)
    saved = service.propose(
        tid,
        proposal
        | {
            "source_quote": "تعبئة وتوقيع وختم نموذج العطاء وصيغة التعهد",
            "applicability": "unconditional",
        },
        origin="manager",
        run_id=run["id"],
    )
    assert saved["applicability"] == "unconditional" and not saved["condition"]
