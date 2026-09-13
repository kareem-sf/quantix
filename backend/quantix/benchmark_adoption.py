"""Known benchmark failures block adoption; unseen configurations make no quality claim.

Only the live job driver records results. Uploaded JSON, synthetic executors and
model-authored success statements cannot register or clear adoption decisions.
"""

from __future__ import annotations

import hashlib
import json
import re
from decimal import Decimal

from .ai_connections import AIConnectionService
from .ai_models import AIRoute
from .ai_readiness import require_ready
from .benchmark_adoption_models import BenchmarkAdoptionDecision, BenchmarkAdoptionReview
from .db import dump, now
from .manager_runtime import ManagerRunProfiles
from .office_checkpoints import tool_implementation_fingerprint


def _hash(value):
    return hashlib.sha256(
        json.dumps(
            value, sort_keys=True, ensure_ascii=False, separators=(",", ":"), allow_nan=False
        ).encode()
    ).hexdigest()


def configuration_basis(repo, route, manager_version):
    """Keep the full approved route, including native tools and generation settings."""
    route = AIRoute.model_validate(route).model_dump(mode="json")
    service = AIConnectionService(repo)
    connection = service.get(route["connection_id"])
    model = next(
        (
            item
            for item in service.models(connection["id"])
            if item["model_id"] == route["model_id"]
        ),
        None,
    )
    if model is None:
        raise ValueError("The benchmark's configured model is no longer available.")
    return {
        "route": route,
        "connection_revision": connection["revision"],
        "checked_component_version": require_ready(repo, connection, route["model_id"]),
        "model_basis": {
            key: value
            for key, value in model.items()
            if key not in {"updated_at", "fetched_at", "checked_at"}
        },
        "manager_profile_version": manager_version,
        "implementation": tool_implementation_fingerprint(),
    }


def configuration_fingerprint(repo, route, manager_version):
    return _hash(configuration_basis(repo, route, manager_version))


class BenchmarkAdoptionService:
    def __init__(self, repo):
        self.repo = repo
        with repo.atomic() as conn:
            conn.execute(
                "CREATE TABLE IF NOT EXISTS benchmark_adoption (configuration_hash TEXT PRIMARY KEY, data_json TEXT NOT NULL)"
            )
            conn.execute(
                "CREATE TABLE IF NOT EXISTS benchmark_live_admissions (root_run_id TEXT PRIMARY KEY REFERENCES runs(id), data_json TEXT NOT NULL)"
            )
            conn.execute(
                "CREATE TABLE IF NOT EXISTS benchmark_verified_reports (id TEXT PRIMARY KEY, data_json TEXT NOT NULL, report_hash TEXT NOT NULL, created_at TEXT NOT NULL)"
            )
            conn.execute(
                "CREATE TRIGGER IF NOT EXISTS benchmark_report_no_update BEFORE UPDATE ON benchmark_verified_reports BEGIN SELECT RAISE(ABORT, 'Verified benchmark reports are immutable'); END"
            )
            conn.execute(
                "CREATE TRIGGER IF NOT EXISTS benchmark_report_no_delete BEFORE DELETE ON benchmark_verified_reports BEGIN SELECT RAISE(ABORT, 'Verified benchmark reports are immutable'); END"
            )
            conn.execute(
                "CREATE TRIGGER IF NOT EXISTS benchmark_report_no_replace BEFORE INSERT ON benchmark_verified_reports WHEN EXISTS (SELECT 1 FROM benchmark_verified_reports WHERE id=NEW.id) BEGIN SELECT RAISE(ABORT, 'Verified benchmark reports are immutable'); END"
            )

    def list(self):
        with self.repo.db.connect() as conn:
            values = [
                BenchmarkAdoptionDecision.model_validate_json(row[0])
                for row in conn.execute("SELECT data_json FROM benchmark_adoption")
            ]
        return sorted(values, key=lambda item: item.updated_at, reverse=True)

    def _sources(self, tender_id):
        sources = []
        for item in self.repo.list_artifacts(tender_id):
            path = self.repo.object_path(tender_id, item["id"])
            if (
                path.is_symlink()
                or hashlib.sha256(path.read_bytes()).hexdigest() != item["content_hash"]
            ):
                raise ValueError("A saved synthetic benchmark source changed.")
            sources.append((item["id"], item["content_hash"], item["version"]))
        return sorted(sources)

    def admit_live(
        self,
        *,
        tender_id,
        root_run_id,
        route,
        manager_version,
        budget,
        case_id,
        repetition,
        source_map,
        group_id,
    ):
        """Internal CLI-only hook, inside the normal plan approval transaction."""
        from .benchmarks.cases import CASES
        from .benchmarks.fixtures import validate_case_fixture

        case = next((case for case in CASES if case.id == case_id), None)
        if (
            case is None
            or repetition not in {1, 2, 3}
            or Decimal(budget) <= 0
            or not Decimal(budget).is_finite()
        ):
            raise ValueError(
                "A live benchmark needs a known synthetic case and positive explicit budget."
            )
        validate_case_fixture(case)
        run = self.repo.get_run(root_run_id)
        if run["tender_id"] != tender_id or run["status"] != "queued":
            raise ValueError("Benchmark admission must belong to the newly queued approved root.")
        # Exact imported originals, not a name/flag on arbitrary customer material.
        fixture_hashes = sorted(
            hashlib.sha256(path.read_bytes()).hexdigest()
            for path in validate_case_fixture(case).glob("*.docx")
        )
        sources = self._sources(tender_id)
        if sorted(item[1] for item in sources) != fixture_hashes or set(source_map.values()) != {
            source.id for source in case.sources
        }:
            raise ValueError(
                "Only unchanged synthetic benchmark originals can receive this admission."
            )
        basis = configuration_basis(self.repo, route, manager_version)
        value = dict(
            tender_id=tender_id,
            root_run_id=root_run_id,
            configuration_hash=_hash(basis),
            basis=basis,
            budget=str(budget),
            case_id=case_id,
            repetition=repetition,
            source_map=source_map,
            sources=sources,
            group_id=group_id,
        )
        with self.repo.atomic() as conn:
            conn.execute(
                "INSERT INTO benchmark_live_admissions VALUES (?,?)", (root_run_id, dump(value))
            )

    def _admission(self, root_run_id):
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT data_json FROM benchmark_live_admissions WHERE root_run_id=?",
                (root_run_id,),
            ).fetchone()
        return json.loads(row[0]) if row else None

    def assert_allowed(self, tender_id, root_run_id, route):
        with self.repo.atomic():
            return self._assert_allowed(tender_id, root_run_id, route)

    def _assert_allowed(self, tender_id, root_run_id, route):
        version = ManagerRunProfiles(self.repo).get(tender_id, root_run_id).version
        fingerprint = configuration_fingerprint(self.repo, route, version)
        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT data_json FROM benchmark_adoption WHERE configuration_hash=?",
                (fingerprint,),
            ).fetchone()
        if not row:
            return fingerprint  # Unseen, without claiming acceptance.
        decision = BenchmarkAdoptionDecision.model_validate_json(row[0])
        if decision.state == "accepted":
            self._require_decision_reports(decision)
            return fingerprint
        admission = self._admission(root_run_id)
        if (
            admission
            and admission["tender_id"] == tender_id
            and admission["configuration_hash"] == fingerprint
        ):
            from .ai_policy import AIPolicyService

            policy = AIPolicyService(self.repo).get(tender_id)
            if (
                self.repo.get_run(root_run_id)["status"] in {"queued", "running"}
                and _hash(self._sources(tender_id)) == _hash(admission["sources"])
                and policy["run_budget_usd"] is not None
                and policy["tender_budget_usd"] is not None
                and 0 < Decimal(str(policy["run_budget_usd"])) <= Decimal(admission["budget"])
                and 0 < Decimal(str(policy["tender_budget_usd"])) <= Decimal(admission["budget"])
            ):
                return fingerprint
        raise ValueError(
            "This AI configuration has a recorded benchmark "
            + (
                "critical error. Run a corrected, explicitly budgeted synthetic benchmark before using it for Tender work."
                if decision.state == "critical_block"
                else "regression. Open AI benchmark reviews to inspect and review the exact comparison before using it for Tender work."
            )
        )

    def guard(self, tender_id, root_run_id, route, before_request):
        # Guard once before SDK/client entry and again at every actual request.
        pinned = dict(route)
        self.assert_allowed(tender_id, root_run_id, pinned)

        async def checked(*args, **kwargs):
            self.assert_allowed(tender_id, root_run_id, pinned)
            return await before_request(*args, **kwargs)

        return checked

    def record_live_report(self, executor, report):
        from .benchmarks.cases import CASES
        from .benchmarks.live import LiveJobExecutor
        from .benchmarks.models import CaseResult
        from .benchmarks.runner import validate_dataset
        from .benchmarks.scoring import TenderEvaluator, score

        if (
            type(executor) is not LiveJobExecutor
            or report.mode != "live"
            or executor.repo.home != self.repo.home
        ):
            raise ValueError(
                "Only real admitted Quantix jobs can register live benchmark decisions."
            )
        if (
            report.dataset_hash != validate_dataset()["dataset_hash"]
            or report.evaluator_version != TenderEvaluator().get_evaluator_version()
        ):
            raise ValueError("The benchmark dataset or evaluator changed.")
        actual = []
        basis = None
        for item in report.cases:
            admission = self._admission(item.observed.root_run_id)
            if not admission:  # Failed admission is not a model quality conclusion.
                continue
            if (
                admission["group_id"] != executor.run_group
                or admission["case_id"] != item.case_id
                or admission["repetition"] != item.repetition
            ):
                raise ValueError("The evaluation does not match its recorded synthetic admission.")
            if (
                configuration_fingerprint(
                    self.repo,
                    admission["basis"]["route"],
                    admission["basis"]["manager_profile_version"],
                )
                != admission["configuration_hash"]
            ):
                raise ValueError(
                    "The evaluated configuration changed; these results cannot authorize adoption."
                )
            if basis is not None and basis != admission["basis"]:
                raise ValueError("A live evaluation cannot mix configurations.")
            basis = admission["basis"]
            case = next(case for case in CASES if case.id == item.case_id)
            observed = executor.collect(
                case, admission["tender_id"], admission["root_run_id"], admission["source_map"]
            )
            observed.latency_seconds = item.observed.latency_seconds
            actual.append(
                CaseResult(
                    case_id=case.id,
                    category=case.category,
                    repetition=item.repetition,
                    critical=case.critical,
                    case_hash=_hash(case.model_dump(mode="json")),
                    observed=observed,
                    scores=score(case, observed),
                )
            )
        if basis is None:
            return None
        verified = report.model_copy(deep=True)
        verified.cases = actual
        verified.overall_completed = (
            len(actual) == 72
            and len({(v.case_id, v.repetition) for v in actual}) == 72
            and all(v.scores.task_completed and v.observed.usage_complete for v in actual)
        )
        return self._save_verified(verified, basis)

    def _save_verified(self, report, basis):
        """Private persistence for recollected live evidence, never an HTTP import."""
        with self.repo.atomic():
            return self._save_verified_locked(report, basis)

    def _save_verified_locked(self, report, basis):
        fingerprint = _hash(basis)
        report_hash = _hash(report.model_dump(mode="json"))
        if not re.fullmatch(r"[a-f0-9]{32}", report.id) or len(report.cases) > 72:
            raise ValueError("A verified report needs its bounded server-generated identity.")
        payload = dump(report.model_dump(mode="json"))
        if len(payload.encode()) > 8 * 1024 * 1024:
            raise ValueError("The verified report exceeds the bounded record size.")
        previous = next(
            (item for item in self.list() if item.configuration_hash == fingerprint), None
        )
        if previous:
            self._require_decision_reports(previous)
        with self.repo.db.connect() as conn:
            existing = conn.execute(
                "SELECT report_hash FROM benchmark_verified_reports WHERE id=?", (report.id,)
            ).fetchone()
        if existing:
            self.read_report(report.id, existing[0])
            if existing[0] != report_hash:
                raise ValueError(
                    "A verified report is immutable; changed evidence needs a new report ID."
                )
            return previous  # Identical replay never rewinds a later adoption decision.
        critical = sorted(
            {
                item.case_id
                for item in report.cases
                if item.critical
                and item.observed.status == "completed"
                and item.observed.requests
                and not item.scores.task_completed
            }
        )
        if not critical and not report.overall_completed:
            return previous  # Infrastructure/incomplete telemetry cannot establish acceptance.
        baseline = None
        reasons = ["Critical engineering result failed: " + case for case in critical]
        if not critical:
            with self.repo.db.connect() as conn:
                rows = conn.execute(
                    "SELECT id,report_hash FROM benchmark_verified_reports WHERE id<>? ORDER BY created_at DESC",
                    (report.id,),
                ).fetchall()
            for row in rows:
                old = self.read_report(row[0], row[1])
                if (
                    old.overall_completed
                    and old.dataset_hash == report.dataset_hash
                    and old.evaluator_version == report.evaluator_version
                ):
                    baseline = (old, row[1])
                    break
            if baseline:
                # A >20% increase in total latency, requests, tokens or attributable
                # cost requires explicit review, never a critical-error override.
                for field in (
                    "latency_seconds",
                    "requests",
                    "input_tokens",
                    "output_tokens",
                    "estimated_cost_usd",
                ):
                    old_values = [getattr(v.observed, field) for v in baseline[0].cases]
                    new_values = [getattr(v.observed, field) for v in report.cases]
                    label = field.replace("_", " ")
                    if any(value is None for value in [*old_values, *new_values]):
                        reasons.append(
                            f"{label} comparison unavailable: at least one attempt has unknown telemetry."
                        )
                        continue
                    before = sum(Decimal(str(value)) for value in old_values)
                    after = sum(Decimal(str(value)) for value in new_values)
                    if after > before * Decimal("1.2"):
                        unit = (
                            "USD estimated"
                            if field == "estimated_cost_usd"
                            else "seconds"
                            if field == "latency_seconds"
                            else "requests"
                            if field == "requests"
                            else "tokens"
                        )
                        change = "more than 20% increase" if before else "increased from zero"
                        reasons.append(f"{label}: {before} → {after} {unit}; {change}.")
        decision = BenchmarkAdoptionDecision(
            configuration_hash=fingerprint,
            report_id=report.id,
            report_hash=report_hash,
            baseline_report_id=baseline[0].id if baseline else None,
            baseline_report_hash=baseline[1] if baseline else None,
            state="critical_block" if critical else "review_required" if reasons else "accepted",
            connection_id=basis["route"]["connection_id"],
            model_id=basis["route"]["model_id"],
            manager_profile_version=basis["manager_profile_version"],
            reasons=reasons,
            updated_at=now(),
        )
        from .sandbox_protocol import write_record

        with self.repo.atomic() as conn:
            existing = conn.execute(
                "SELECT report_hash FROM benchmark_verified_reports WHERE id=?", (report.id,)
            ).fetchone()
            if existing:
                if existing[0] != report_hash:
                    raise ValueError(
                        "A verified report is immutable; changed evidence needs a new report ID."
                    )
                return previous
            write_record(
                self.repo.home / "benchmarks" / report.id / "verified-report.json",
                report.model_dump(mode="json"),
            )
            conn.execute(
                "INSERT INTO benchmark_verified_reports VALUES (?,?,?,?)",
                (report.id, payload, report_hash, now()),
            )
            conn.execute(
                "INSERT OR REPLACE INTO benchmark_adoption VALUES (?,?)",
                (fingerprint, dump(decision.model_dump(mode="json"))),
            )
        return decision

    def read_report(self, report_id, expected_hash):
        from .benchmarks.models import BenchmarkReport

        with self.repo.db.connect() as conn:
            row = conn.execute(
                "SELECT data_json,report_hash FROM benchmark_verified_reports WHERE id=?",
                (report_id,),
            ).fetchone()
        if not row:
            raise KeyError("Verified benchmark report not found.")
        if row[1] != expected_hash or len(row[0].encode()) > 8 * 1024 * 1024:
            raise ValueError("The report hash or bounded record size does not match.")
        try:
            report = BenchmarkReport.model_validate_json(row[0])
        except ValueError as error:
            raise ValueError(
                "The saved benchmark report is damaged. Restore its verified evidence before continuing."
            ) from error
        if (
            report.mode != "live"
            or len(report.cases) > 72
            or _hash(report.model_dump(mode="json")) != expected_hash
        ):
            raise ValueError("The saved verified report hash does not match its contents.")
        return report

    def _require_decision_reports(self, decision):
        """Caller holds the admission/review/save transaction; nested reads reuse it."""
        try:
            candidate = self.read_report(decision.report_id, decision.report_hash)
            if bool(decision.baseline_report_id) != bool(decision.baseline_report_hash):
                raise ValueError(
                    "The saved benchmark comparison is incomplete. Restore its verified evidence before continuing."
                )
            if decision.baseline_report_id:
                baseline = self.read_report(
                    decision.baseline_report_id, decision.baseline_report_hash
                )
                if (
                    not baseline.overall_completed
                    or baseline.dataset_hash != candidate.dataset_hash
                    or baseline.evaluator_version != candidate.evaluator_version
                ):
                    raise ValueError(
                        "The saved benchmark comparison has no comparable passing baseline. Restore its verified evidence before continuing."
                    )
            if (
                decision.state in {"accepted", "review_required"}
                and not candidate.overall_completed
            ):
                raise ValueError(
                    "The saved benchmark acceptance has no complete passing report. Restore its verified evidence before continuing."
                )
        except KeyError as error:
            raise ValueError(
                "Verified benchmark evidence is missing. Restore its exact report records before continuing; review cannot repair missing evidence."
            ) from error

    def review(self, command: BenchmarkAdoptionReview):
        with self.repo.atomic() as conn:
            row = conn.execute(
                "SELECT data_json FROM benchmark_adoption WHERE configuration_hash=?",
                (command.configuration_hash,),
            ).fetchone()
            if not row:
                raise KeyError("Benchmark decision not found.")
            current = BenchmarkAdoptionDecision.model_validate_json(row[0])
            if current.state != "review_required":
                raise ValueError(
                    "Critical errors cannot be cleared by review. A complete passing live evaluation is required."
                )
            for field in ("report_id", "report_hash", "baseline_report_id", "baseline_report_hash"):
                if getattr(command, field) != getattr(current, field):
                    raise ValueError(
                        "This comparison changed. Read the current benchmark review again."
                    )
            self._require_decision_reports(current)
            current.state, current.review_rationale, current.updated_at = (
                "accepted",
                command.rationale,
                now(),
            )
            conn.execute(
                "UPDATE benchmark_adoption SET data_json=? WHERE configuration_hash=?",
                (dump(current.model_dump(mode="json")), current.configuration_hash),
            )
        return current
