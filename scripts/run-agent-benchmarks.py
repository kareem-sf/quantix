"""Validate synthetic cases by default; live inference always needs explicit flags."""

from __future__ import annotations

import argparse
import asyncio
import json
import sys
from decimal import Decimal
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "backend"))

from quantix.benchmarks.cases import CASES
from quantix.benchmarks.models import BenchmarkReport, RegressionReview
from quantix.benchmarks.runner import (
    compare_reports,
    deterministic_checks,
    evaluate,
    new_report,
    write_report,
)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--live",
        action="store_true",
        help="Run the explicitly selected configured AI on synthetic approved Tenders.",
    )
    mode.add_argument(
        "--deterministic",
        action="store_true",
        help="Run real calculation probes only, without AI requests.",
    )
    mode.add_argument(
        "--compare",
        action="store_true",
        help="Check two retained reports for critical regressions.",
    )
    parser.add_argument("--home", type=Path, default=Path.home() / ".quantix")
    parser.add_argument("--connection")
    parser.add_argument("--model")
    parser.add_argument(
        "--budget",
        type=Decimal,
        help="Positive total USD ceiling, divided across all selected attempts.",
    )
    parser.add_argument("--max-requests", type=int, default=12)
    parser.add_argument(
        "--timeout", type=int, default=600, help="Maximum seconds per synthetic job."
    )
    parser.add_argument(
        "--case",
        action="append",
        choices=[case.id for case in CASES],
        help="Run a three-repetition pilot; it cannot qualify full-suite completion.",
    )
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--candidate", type=Path)
    parser.add_argument("--regression-review", type=Path)
    args = parser.parse_args(argv)
    if args.compare:
        if not args.baseline or not args.candidate:
            parser.error(
                "--compare requires --baseline and --candidate report JSON paths."
            )
        before = BenchmarkReport.model_validate_json(
            args.baseline.read_text(encoding="utf-8")
        )
        after = BenchmarkReport.model_validate_json(
            args.candidate.read_text(encoding="utf-8")
        )
        review = (
            RegressionReview.model_validate_json(
                args.regression_review.read_text(encoding="utf-8")
            )
            if args.regression_review
            else None
        )
        result = compare_reports(before, after, review)
        print(json.dumps(result, indent=2))
        return 0 if result["passed"] and result["candidate_completed"] else 1
    if args.live:
        if (
            not args.connection
            or not args.model
            or args.budget is None
            or not args.budget.is_finite()
            or args.budget <= 0
        ):
            parser.error(
                "--live requires an existing --connection, --model and explicit positive --budget."
            )
        if not 1 <= args.max_requests <= 100 or not 10 <= args.timeout <= 3600:
            parser.error(
                "Choose 1–100 requests and a timeout between 10 and 3600 seconds."
            )
        from quantix.benchmarks.live import LiveJobExecutor

        cases = [case for case in CASES if not args.case or case.id in args.case]
        executor = LiveJobExecutor(
            args.home,
            connection_id=args.connection,
            model_id=args.model,
            total_budget=args.budget,
            attempt_count=len(cases) * 3,
            max_requests=args.max_requests,
            timeout_seconds=args.timeout,
        )
        report = asyncio.run(evaluate(args.home, executor, mode="live", cases=cases))
    elif args.deterministic:
        if args.connection or args.model or args.budget is not None:
            parser.error(
                "AI route/budget flags require --live; deterministic checks make no model requests."
            )
        report = deterministic_checks(args.home)
    else:
        if args.connection or args.model or args.budget is not None:
            parser.error(
                "AI route/budget flags require --live. The default validates data only."
            )
        report = new_report("dataset_validation")
        write_report(args.home, report)
    print(
        json.dumps(
            {
                "report": str(args.home / "benchmarks" / report.id / "report.json"),
                "mode": report.mode,
                "validated_cases": report.validated_cases,
                "attempts": len(report.cases),
                "completed_attempts": sum(
                    case.scores.task_completed for case in report.cases
                ),
                "overall_completed": report.overall_completed,
                "detail": report.detail,
            },
            indent=2,
        )
    )
    if report.mode == "live":
        return 0 if report.overall_completed else 1
    if report.mode == "deterministic_checks":
        return 0 if all(item["passed"] for item in report.deterministic_checks) else 1
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, OSError) as error:
        print(f"Benchmark could not start: {error}", file=sys.stderr)
        raise SystemExit(2)
