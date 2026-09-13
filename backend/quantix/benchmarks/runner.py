"""Dataset validation, real deterministic probes and retained repeated reports."""

from __future__ import annotations

import asyncio
import hashlib
import json
import time
import uuid
from collections import Counter
from datetime import UTC, datetime
from decimal import Decimal
from pathlib import Path

from pydantic_evals import Case, Dataset

from ..sandbox_protocol import write_record
from .cases import CASES, DATASET_VERSION
from .models import BenchmarkReport, CaseResult, ObservedRun, RegressionReview
from .scoring import TenderEvaluator, score


def digest(value):
    return hashlib.sha256(
        json.dumps(
            value, sort_keys=True, ensure_ascii=False, separators=(",", ":"), allow_nan=False
        ).encode()
    ).hexdigest()


def validate_dataset(cases=CASES):
    from .fixtures import validate_fixtures

    if len(cases) != 24 or len({case.id for case in cases}) != 24:
        raise ValueError("The benchmark requires 24 distinct explicit cases.")
    categories = Counter(case.category for case in cases)
    if len(categories) != 6 or set(categories.values()) != {4}:
        raise ValueError("Each of the six benchmark categories requires four cases.")
    for case in cases:
        evidence = {source.id for source in case.sources}
        if len(evidence) != len(case.sources) or not case.numbers or not case.findings:
            raise ValueError(
                "Each case needs distinct evidence, literal numbers and mandatory findings."
            )
        for expected in [*case.numbers.values(), *case.findings]:
            if not expected.evidence or not set(expected.evidence) <= evidence:
                raise ValueError("Every expected claim must point to supplied synthetic evidence.")
        if any(Decimal(item.tolerance) < 0 for item in case.numbers.values()):
            raise ValueError("Numeric tolerances cannot be negative.")
        if not case.probe.expected:
            raise ValueError("Each case requires an independent literal calculation probe.")
    validate_fixtures(cases)
    return {
        "case_count": len(cases),
        "categories": dict(categories),
        "dataset_hash": digest([case.model_dump(mode="json") for case in cases]),
    }


def new_report(mode, *, repetitions=0):
    validation = validate_dataset()
    return BenchmarkReport(
        id=uuid.uuid4().hex,
        dataset_version=DATASET_VERSION,
        evaluator_version="2",
        dataset_hash=validation["dataset_hash"],
        mode=mode,
        created_at=datetime.now(UTC).isoformat(),
        repetitions=repetitions,
        validated_cases=24,
        detail="Only synthetic dataset structure and literal references were validated. No AI request ran."
        if mode == "dataset_validation"
        else "",
    )


def write_report(home: Path, report: BenchmarkReport):
    root = Path(home) / "benchmarks" / report.id
    root.mkdir(parents=True, exist_ok=True)
    write_record(root / "report.json", report.model_dump(mode="json"))
    write_record(
        root / "dataset.json",
        {"version": DATASET_VERSION, "cases": [case.model_dump(mode="json") for case in CASES]},
    )
    lines = [
        f"# Tender benchmark {report.id}",
        "",
        f"Mode: {report.mode}. {report.detail}",
        "",
        f"Dataset: {report.dataset_version}; {report.validated_cases} cases; {report.repetitions} repetitions.",
        "",
        "| Case | Repetition | Complete | Numeric accuracy | Evidence precision / recall | Seconds | Requests | Input / output tokens | Estimated USD | Provider-reported USD |",
        "| --- | ---: | --- | ---: | --- | ---: | ---: | --- | ---: | ---: |",
    ]
    details = []
    for item in report.cases:
        value, scores = item.observed, item.scores
        lines.append(
            f"| {item.case_id} | {item.repetition} | {scores.task_completed} | {scores.numerical_accuracy:.3f} | {scores.evidence_precision:.3f} / {scores.evidence_recall:.3f} | {value.latency_seconds:.2f} | {value.requests if value.requests is not None else 'unknown'} | {value.input_tokens if value.input_tokens is not None else 'unknown'} / {value.output_tokens if value.output_tokens is not None else 'unknown'} | {value.estimated_cost_usd if value.estimated_cost_usd is not None else 'unknown'} | {value.provider_reported_cost_usd if value.provider_reported_cost_usd is not None else 'unknown'}{' (partial)' if value.provider_cost_is_partial else ''} |"
        )
        if scores.violations:
            details.append(
                f"\n{item.case_id}, repetition {item.repetition}: {', '.join(scores.violations)}. {value.detail}\n"
            )
    lines.extend(details)
    if report.deterministic_checks:
        lines += [
            "",
            "Deterministic probes are calculation-service checks, not completed agent tasks.",
        ]
        for probe in report.deterministic_checks:
            lines.append(
                f"- {probe['case_id']}: {'passed' if probe['passed'] else 'failed'}; receipt {probe['calculation_id']}."
            )
    (root / "report.md").write_text("\n".join(lines), encoding="utf-8")
    return root


def deterministic_checks(home: Path):
    from ..calculation_models import CalculationRequest
    from ..calculations import CalculationService
    from ..execution_context import engineer_identity
    from ..repository import Repository

    report = new_report("deterministic_checks")
    isolated = Path(home) / "benchmarks" / report.id / "synthetic-calculation-store"
    repo = Repository(isolated)
    tender = repo.create_tender("Synthetic benchmark calculation probes")
    service = CalculationService(repo)
    for case in CASES:
        probe = case.probe
        record = service.calculate(
            engineer_identity(tender["id"]),
            CalculationRequest(
                method_id=probe.method,
                method_version="1",
                inputs=probe.inputs,
                units=probe.units,
                idempotency_key="benchmark-" + case.id,
            ),
        )
        report.deterministic_checks.append(
            {
                "case_id": case.id,
                "calculation_id": record.id,
                "passed": record.status == "calculated" and record.outputs == probe.expected,
                "expected": probe.expected,
                "actual": record.outputs,
                "formula_hash": record.formula_hash,
            }
        )
    report.detail = "Real CalculationService probes ran on a separate synthetic store. No model, evidence-reading journey or agent completion is claimed."
    write_report(home, report)
    return report


async def evaluate(home: Path, executor, *, mode="synthetic_executor", repetitions=3, cases=CASES):
    if mode not in {"live", "synthetic_executor", "synthetic_model"}:
        raise ValueError("Execution reports must identify the actual executor mode.")
    if mode == "live":
        from .live import LiveJobExecutor

        if not isinstance(executor, LiveJobExecutor):
            raise ValueError("Live results require the real approved Quantix job driver.")
    if repetitions != 3:
        raise ValueError("Acceptance retains exactly three repetitions per case.")
    report = new_report(mode, repetitions=repetitions)
    counters = Counter()
    folder = write_report(home, report)

    async def observed_task(case):
        counters[case.id] += 1
        repetition = counters[case.id]
        started = time.monotonic()
        try:
            output = ObservedRun.model_validate(await executor(case, repetition))
        except asyncio.CancelledError:
            output = ObservedRun(
                status="cancelled",
                detail="The benchmark was stopped; this attempt did not complete.",
            )
            _save(case, repetition, output, started)
            raise
        except Exception:
            output = ObservedRun(
                status="failed",
                detail="The executor failed. Inspect the saved synthetic job and tool receipts.",
            )
        _save(case, repetition, output, started)
        return output

    def _save(case, repetition, output, started):
        output.latency_seconds = round(time.monotonic() - started, 4)
        item = CaseResult(
            case_id=case.id,
            category=case.category,
            repetition=repetition,
            critical=case.critical,
            case_hash=digest(case.model_dump(mode="json")),
            observed=output,
            scores=score(case, output),
        )
        report.cases.append(item)
        write_record(
            folder / "attempts" / f"{case.id}-{repetition}.json", item.model_dump(mode="json")
        )
        write_report(home, report)

    dataset = Dataset(
        name="Quantix Tender benchmarks " + DATASET_VERSION,
        cases=[
            Case(name=case.id, inputs=case, metadata={"category": case.category}) for case in cases
        ],
        evaluators=[TenderEvaluator()],
    )
    try:
        evaluated = await dataset.evaluate(
            observed_task,
            name="Quantix synthetic Tender benchmarks",
            repeat=3,
            max_concurrency=1,
            progress=False,
        )
        write_record(
            folder / "pydantic-evals.json",
            {
                "name": evaluated.name,
                "cases": [
                    {
                        "name": item.name,
                        "source_case_name": item.source_case_name,
                        "scores": {key: value.value for key, value in item.scores.items()},
                        "assertions": {key: value.value for key, value in item.assertions.items()},
                        "task_duration": item.task_duration,
                        "evaluator_failures": [
                            error.error_type for error in item.evaluator_failures
                        ],
                    }
                    for item in evaluated.cases
                ],
                "failures": [
                    {"name": item.name, "error_type": type(item).__name__}
                    for item in evaluated.failures
                ],
            },
        )
        report.overall_completed = (
            len(report.cases) == 72
            and all(item.scores.task_completed for item in report.cases)
            and all(item.observed.usage_complete for item in report.cases)
            and not evaluated.failures
            and not evaluated.report_evaluator_failures
            and all(not item.evaluator_failures for item in evaluated.cases)
        )
        report.detail = (
            "Every repetition is retained, including failures. Synthetic executor results do not qualify a live model."
            if mode != "live"
            else "Observed approved synthetic jobs only. Estimated costs are not invoices; missing usage remains unknown."
        )
    finally:
        write_report(home, report)
        if mode == "live":
            executor.adoption.record_live_report(executor, report)
    return report


def compare_reports(
    baseline: BenchmarkReport, candidate: BenchmarkReport, review: RegressionReview | None = None
):
    if (
        baseline.dataset_hash != candidate.dataset_hash
        or baseline.mode != candidate.mode
        or baseline.evaluator_version != candidate.evaluator_version
    ):
        raise ValueError("Compare reports using the same dataset and execution mode.")
    before = {(item.case_id, item.repetition): item for item in baseline.cases}
    after = {(item.case_id, item.repetition): item for item in candidate.cases}
    if len(before) != 72:
        raise ValueError("A comparison baseline must retain all 24 cases and three repetitions.")
    if set(before) != {(case.id, repetition) for case in CASES for repetition in range(1, 4)}:
        raise ValueError("The baseline has missing, duplicate or unknown case repetitions.")
    coverage_complete = len(after) == 72 and set(before) == set(after)
    regressions = sorted(
        {
            key[0]
            for key, old in before.items()
            if old.critical
            and (
                key not in after
                or (old.scores.task_completed and not after[key].scores.task_completed)
                or after[key].case_hash != old.case_hash
                or (old.observed.usage_complete and not after[key].observed.usage_complete)
                or any(
                    getattr(after[key].scores, metric) < getattr(old.scores, metric)
                    for metric in (
                        "numerical_accuracy",
                        "evidence_precision",
                        "evidence_recall",
                        "findings_recall",
                    )
                )
            )
        }
    )
    reviewed = set()
    if review:
        if review.baseline_report_id != baseline.id or review.candidate_report_id != candidate.id:
            raise ValueError("The regression review is for different reports.")
        reviewed = set(review.case_ids)
        if not reviewed <= set(regressions):
            raise ValueError("Review only the actual critical regressions.")
    return {
        "passed": coverage_complete and not regressions,
        "critical_regressions": regressions,
        "coverage_complete": coverage_complete,
        "candidate_completed": candidate.overall_completed,
        "unreviewed_regressions": sorted(set(regressions) - reviewed),
        "reviewed_reason": review.rationale if review else None,
    }
