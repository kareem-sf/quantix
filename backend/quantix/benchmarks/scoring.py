"""Literal expected outcomes and server receipts, without model-as-judge scores."""

from dataclasses import dataclass
from decimal import Decimal, InvalidOperation

from pydantic_evals.evaluators import Evaluator, EvaluatorContext

from .models import BenchmarkCase, ObservedRun, Scores


def _unit(value):
    return value.replace("m³", "m3").replace("m²", "m2").strip()


def _near(left, right, tolerance):
    try:
        left, right, tolerance = Decimal(left), Decimal(right), Decimal(tolerance)
        return (
            left.is_finite()
            and right.is_finite()
            and tolerance >= 0
            and abs(left - right) <= tolerance
        )
    except (InvalidOperation, TypeError, ValueError):
        return False


def score(case: BenchmarkCase, observed: ObservedRun) -> Scores:
    violations = []
    if observed.status != "completed":
        violations.append("run_not_completed")
    if not observed.artifact_ids:
        violations.append("saved_artifact_missing")
    if not observed.calculation_ids or not observed.calculations_verified:
        violations.append("verified_calculation_missing")
    for event in case.required_events:
        if event not in observed.events:
            violations.append("required_event_missing:" + event)
    if "calculation_checked" in case.required_events and not (
        set(observed.checked_calculation_ids) & set(observed.calculation_ids)
    ):
        violations.append("independent_calculation_check_missing")
    if observed.completed_assignments < case.required_assignments:
        violations.append("completed_colleague_result_missing")
    correct_numbers = 0
    found_findings = 0
    claimed, expected = set(), set()
    actual_reads = set(observed.read_evidence)
    for key, golden in case.numbers.items():
        expected.update(("number:" + key, ref) for ref in golden.evidence)
        answer = observed.numbers.get(key)
        if answer:
            claimed.update(("number:" + key, ref) for ref in answer.evidence)
            if _unit(answer.unit) == _unit(golden.unit) and _near(
                answer.value, golden.value, golden.tolerance
            ):
                correct_numbers += 1
            else:
                violations.append("number_wrong:" + key)
            if not any(
                _near(answer.value, value, golden.tolerance)
                for value in observed.calculation_outputs.get(answer.calculation_id or "", [])
            ):
                violations.append("calculation_does_not_support_number:" + key)
            if not set(golden.evidence) <= set(
                observed.calculation_evidence.get(answer.calculation_id or "", [])
            ):
                violations.append("calculation_evidence_missing:" + key)
            if case.required_assignments:
                proof = observed.independent_checks.get(answer.calculation_id or "")
                if (
                    proof is None
                    or not proof.creator_actor_id
                    or not proof.reviewer_actor_id
                    or proof.creator_actor_id == proof.reviewer_actor_id
                    or not proof.reviewer_assignment_id
                    or not proof.reviewer_result_id
                    or not set(golden.evidence) <= set(proof.evidence)
                ):
                    violations.append("independent_review_missing:" + key)
        else:
            violations.append("number_missing:" + key)
    for key, answer in observed.numbers.items():
        if key not in case.numbers:
            claimed.update(("unexpected_number:" + key, ref) for ref in answer.evidence)
            violations.append("unexpected_number:" + key)
    finding_map = {}
    for finding in observed.findings:
        if finding.code in finding_map:
            violations.append("duplicate_finding:" + finding.code)
        finding_map[finding.code] = finding
        claimed.update(("finding:" + finding.code, ref) for ref in finding.evidence)
    for golden in case.findings:
        expected.update(("finding:" + golden.code, ref) for ref in golden.evidence)
        answer = finding_map.get(golden.code)
        if answer and len(answer.description.strip()) >= 8:
            found_findings += 1
        else:
            violations.append("finding_missing:" + golden.code)
    for code in case.forbidden_findings:
        if code in finding_map:
            violations.append("forbidden_finding:" + code)
    supported = {(claim, ref) for claim, ref in claimed & expected if ref in actual_reads}
    precision = len(supported) / len(claimed) if claimed else 0.0
    recall = len(supported) / len(expected) if expected else 1.0
    if precision < 1:
        violations.append("unsupported_or_unread_evidence")
    if recall < 1:
        violations.append("required_evidence_missing")
    if not observed.requests or observed.requests < 1:
        violations.append("provider_request_missing")
    return Scores(
        task_completed=not violations,
        evidence_precision=precision,
        evidence_recall=recall,
        numerical_accuracy=correct_numbers / len(case.numbers),
        findings_recall=found_findings / len(case.findings),
        violations=violations,
    )


@dataclass
class TenderEvaluator(Evaluator[BenchmarkCase, ObservedRun, dict]):
    def evaluate(self, ctx: EvaluatorContext[BenchmarkCase, ObservedRun, dict]):
        result = score(ctx.inputs, ctx.output)
        return {key: value for key, value in result.model_dump().items() if key != "violations"}

    def get_evaluator_version(self):
        return "2"
