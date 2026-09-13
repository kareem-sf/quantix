"""Benchmark scorer fixtures are explicitly synthetic, never live-model evidence."""

import json
from decimal import Decimal

import pytest

from quantix.benchmarks.cases import CASES
from quantix.benchmarks.live import LiveJobExecutor, instruction_for
from quantix.benchmarks.models import CaseResult, ObservedRun, RegressionReview
from quantix.benchmarks.runner import (
    compare_reports,
    deterministic_checks,
    digest,
    evaluate,
    new_report,
    validate_dataset,
)
from quantix.benchmarks.scoring import score


def literal_first_case_observation():
    return ObservedRun(
        status="completed",
        numbers={
            "quantity": {
                "value": "10",
                "unit": "m3",
                "evidence": ["boq-01-A"],
                "calculation_id": "synthetic-calculation-a",
            },
            "difference": {
                "value": "-2",
                "unit": "m3",
                "evidence": ["boq-01-A", "boq-01-B"],
                "calculation_id": "synthetic-calculation-b",
            },
        },
        findings=[
            {
                "code": "quantity_discrepancy",
                "description": "The calculated volume is below the BOQ volume.",
                "evidence": ["boq-01-A", "boq-01-B"],
            }
        ],
        read_evidence=["boq-01-A", "boq-01-B"],
        events=["calculation_completed", "saved_work_product"],
        calculation_ids=["synthetic-calculation-a", "synthetic-calculation-b"],
        calculation_values=["10.00", "-2.00"],
        calculation_outputs={
            "synthetic-calculation-a": ["10.00"],
            "synthetic-calculation-b": ["-2.00"],
        },
        calculation_evidence={
            "synthetic-calculation-a": ["boq-01-A"],
            "synthetic-calculation-b": ["boq-01-A", "boq-01-B"],
        },
        calculations_verified=True,
        artifact_ids=["synthetic-artifact"],
        requests=1,
        input_tokens=100,
        output_tokens=100,
        estimated_cost_usd="0.0003",
        usage_complete=True,
    )


def test_dataset_has_24_explicit_cases_and_no_golden_answers_in_task_prompt():
    report = validate_dataset()
    assert report["case_count"] == 24
    assert set(report["categories"].values()) == {4}
    assert CASES[0].numbers["quantity"].value == "10"
    assert CASES[-1].numbers["budget_overrun"].value == "3000"
    prompt = json.loads(
        instruction_for(CASES[0], [{"source_id": "actual-source-id", "document": "boq-01-A"}])
    )
    assert "expected" not in prompt
    assert prompt["required_numeric_fields"] == {
        "quantity": {"unit": "m3"},
        "difference": {"unit": "m3"},
    }
    assert "-2" not in json.dumps(prompt)


def test_scorer_requires_real_artifacts_calculation_and_read_evidence():
    observed = literal_first_case_observation()
    assert score(CASES[0], observed).task_completed
    observed.artifact_ids = []
    observed.calculation_ids = []
    observed.read_evidence = []
    observed.requests = None
    result = score(CASES[0], observed)
    assert not result.task_completed
    assert result.numerical_accuracy == 1
    assert result.evidence_recall == 0
    assert "saved_artifact_missing" in result.violations
    assert "provider_request_missing" in result.violations


def test_wrong_numbers_forged_evidence_and_unrelated_calculations_fail():
    observed = literal_first_case_observation()
    observed.numbers["quantity"].value = "12"
    observed.numbers["difference"].evidence.append("fabricated-source")
    observed.calculation_values = ["2"]
    observed.calculation_outputs = {"unrelated-calculation": ["2"]}
    result = score(CASES[0], observed)
    assert not result.task_completed
    assert result.numerical_accuracy == 0.5
    assert result.evidence_precision < 1
    assert "calculation_does_not_support_number:quantity" in result.violations


def test_planted_error_self_check_cannot_count_as_independent_review():
    from quantix.benchmarks.models import IndependentCheck

    case = next(item for item in CASES if item.id == "integrated-03")
    source = "integrated-03-A"
    observed = ObservedRun(
        status="completed",
        numbers={
            "correct_amount": {
                "value": "7500",
                "unit": "EGP",
                "evidence": [source],
                "calculation_id": "calc-a",
            },
            "understatement": {
                "value": "500",
                "unit": "EGP",
                "evidence": [source],
                "calculation_id": "calc-b",
            },
        },
        findings=[
            {
                "code": "planted_error_found",
                "description": "The original worksheet understates the amount.",
                "evidence": [source],
            }
        ],
        read_evidence=[source],
        events=["calculation_completed", "calculation_checked", "saved_work_product"],
        artifact_ids=["artifact"],
        calculation_ids=["calc-a", "calc-b"],
        checked_calculation_ids=["calc-a", "calc-b"],
        calculation_outputs={"calc-a": ["7500"], "calc-b": ["500"]},
        calculation_evidence={"calc-a": [source], "calc-b": [source]},
        calculations_verified=True,
        completed_assignments=1,
        requests=1,
        usage_complete=True,
    )
    for identifier in observed.calculation_ids:
        observed.independent_checks[identifier] = IndependentCheck(
            creator_actor_id="same-actor",
            reviewer_actor_id="same-actor",
            reviewer_assignment_id="unrelated-assignment",
            reviewer_result_id="unrelated-result",
            evidence=[source],
        )
    result = score(case, observed)
    assert result.numerical_accuracy == 1 and not result.task_completed
    assert "independent_review_missing:correct_amount" in result.violations
    for identifier in observed.calculation_ids:
        observed.independent_checks[identifier].reviewer_actor_id = "actual-other-reviewer"
    assert score(case, observed).task_completed


def test_real_deterministic_probes_preserve_literal_expected_values(tmp_path):
    report = deterministic_checks(tmp_path)
    assert report.mode == "deterministic_checks"
    assert len(report.deterministic_checks) == 24
    assert all(item["passed"] for item in report.deterministic_checks)
    assert not report.overall_completed
    assert len({item["calculation_id"] for item in report.deterministic_checks}) == 24
    assert report.cases == []


@pytest.mark.asyncio
async def test_pydantic_evals_retains_all_three_attempts_including_failure(tmp_path):
    async def synthetic_executor(_case, repetition):
        if repetition == 2:
            raise RuntimeError("Synthetic injected failure")
        return literal_first_case_observation()

    report = await evaluate(tmp_path, synthetic_executor, cases=CASES[:1])
    assert report.mode == "synthetic_executor"
    assert [item.repetition for item in report.cases] == [1, 2, 3]
    assert report.cases[1].observed.status == "failed"
    assert not report.overall_completed
    assert len(list((tmp_path / "benchmarks" / report.id / "attempts").glob("*.json"))) == 3
    stored = json.loads((tmp_path / "benchmarks" / report.id / "pydantic-evals.json").read_text())
    assert len(stored["cases"]) == 3
    with pytest.raises(ValueError, match="real approved"):
        await evaluate(tmp_path, synthetic_executor, mode="live", cases=CASES[:1])


def test_critical_regression_gate_requires_exact_report_review():
    baseline = new_report("synthetic_executor", repetitions=3)
    for case in CASES:
        for repetition in range(1, 4):
            observed = literal_first_case_observation()
            baseline.cases.append(
                CaseResult(
                    case_id=case.id,
                    category=case.category,
                    repetition=repetition,
                    critical=True,
                    case_hash=digest(case.model_dump(mode="json")),
                    observed=observed,
                    scores=score(case, observed),
                )
            )
    candidate = baseline.model_copy(deep=True)
    candidate.id = "changed-report"
    candidate.cases[0].scores.task_completed = False
    assert compare_reports(baseline, candidate)["unreviewed_regressions"] == ["boq-01"]
    reviewed = RegressionReview(
        baseline_report_id=baseline.id,
        candidate_report_id=candidate.id,
        case_ids=["boq-01"],
        engineer_confirmed=True,
        rationale="Synthetic reviewer explicitly accepted this test regression.",
    )
    assert not compare_reports(baseline, candidate, reviewed)["passed"]
    candidate.cases.pop()
    assert not compare_reports(baseline, candidate, reviewed)["passed"]
    already_incomplete = baseline.model_copy(deep=True)
    already_incomplete.cases[0].scores.task_completed = False
    worse = already_incomplete.model_copy(deep=True)
    worse.id = "worse-numerical-report"
    worse.cases[0].scores.numerical_accuracy = 0.5
    assert compare_reports(already_incomplete, worse)["unreviewed_regressions"] == ["boq-01"]


@pytest.mark.asyncio
async def test_job_driver_uses_real_import_review_tools_and_receipts_with_synthetic_model(
    tmp_path, monkeypatch
):
    from test_catalog_authority import configured_office

    from quantix.office_types import OfficeOutput
    from quantix.tool_policy import dispatch

    repo, original_tender, _, connection, model, *_ = configured_office(tmp_path, monkeypatch)

    async def synthetic_provider(
        route, account, credentials, context, prompt, output_type, **options
    ):
        definitions = {item.name: item for item in options["definitions"]}
        references = {}
        for artifact in context.repo.list_artifacts(context.tender_id):
            references[artifact["relative_path"].split(".")[0]] = context.repo.artifact_evidence(
                context.tender_id, artifact["id"]
            )[0]["id"]
        all_refs = list(references.values())
        reservation = await options["before_request"](100, 100)
        for index, reference in enumerate(all_refs):
            await dispatch(
                "direct",
                definitions["read_source"],
                context,
                {"source_id": reference},
                invocation_id=f"source-{index}",
            )
        first_calculation = json.loads(
            await dispatch(
                "direct",
                definitions["calculate_engineering"],
                context,
                {
                    "method": "product",
                    "inputs": {"quantity": "50", "factor": "0.2"},
                    "units": {"quantity": "m2", "factor": "m"},
                    "source_refs": all_refs,
                },
                invocation_id="quantity",
            )
        )
        second_calculation = json.loads(
            await dispatch(
                "direct",
                definitions["calculate_engineering"],
                context,
                {
                    "method": "difference",
                    "inputs": {"left": "10", "right": "12"},
                    "units": {"left": "m3", "right": "m3"},
                    "source_refs": all_refs,
                },
                invocation_id="difference",
            )
        )
        rows = [
            {
                "kind": "number",
                "key": "quantity",
                "value": "10",
                "unit": "m3",
                "source_ids": [references["boq-01-A"]],
                "calculation_id": first_calculation["id"],
            },
            {
                "kind": "number",
                "key": "difference",
                "value": "-2",
                "unit": "m3",
                "source_ids": all_refs,
                "calculation_id": second_calculation["id"],
            },
            {
                "kind": "finding",
                "key": "quantity_discrepancy",
                "value": "Calculated slab volume is below the BOQ quantity.",
                "source_ids": all_refs,
            },
        ]
        await dispatch(
            "direct",
            definitions["save_work_product"],
            context,
            {
                "kind": "table",
                "title": "Benchmark result boq-01",
                "rows": rows,
                "source_refs": all_refs,
            },
            invocation_id="table",
        )
        usage = {
            "requests": 1,
            "input_tokens": 100,
            "output_tokens": 100,
            "web_search_calls": 0,
            "usage_complete": True,
        }
        await options["on_response"](usage, reservation)
        return {
            "output": OfficeOutput(
                summary="The synthetic benchmark is saved.", source_ids=all_refs
            ),
            "usage": usage,
            "web_sources": [],
        }

    monkeypatch.setattr("quantix.ai_execution.execute_api", synthetic_provider)
    driver = LiveJobExecutor(
        repo.home,
        connection_id=connection["id"],
        model_id=model["model_id"],
        total_budget=Decimal("20"),
    )
    # A genuine CLI admission can test this known-blocked configuration; ordinary
    # work cannot borrow its marker. The provider below is explicitly synthetic.
    from test_benchmark_adoption import _block

    service, _ = _block(repo, driver.route.model_dump(mode="json"), driver.manager_version)
    observed = await driver(CASES[0], 1)
    assert observed.status == "completed", observed.detail
    assert observed.tender_id != original_tender["id"]
    assert score(CASES[0], observed).task_completed, score(CASES[0], observed).violations
    assert observed.requests == 1
    assert observed.artifact_ids and len(observed.calculation_ids) == 2
    assert repo.list_runs(original_tender["id"]) == []
    assert service.list()[0].state == "critical_block"  # A synthetic pilot grants no acceptance.
    with pytest.raises(ValueError, match="recorded benchmark critical"):
        service.assert_allowed(
            observed.tender_id, observed.root_run_id, driver.route.model_dump(mode="json")
        )
