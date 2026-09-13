"""Synthetic work admitted through the real policy, review and job boundaries."""

from __future__ import annotations

import asyncio
import json
import time
from decimal import ROUND_DOWN, Decimal
from pathlib import Path

from ..ai_connections import AIConnectionService
from ..ai_models import AIRoute, TenderAIInput
from ..ai_policy import AIPolicyService
from ..calculation_models import CalculationCheckRequest
from ..calculations import CalculationService
from ..execution_context import engineer_identity
from ..jobs import JobManager, safe_error
from ..manager_profile import ManagerProfileService
from ..manager_runtime import ManagerRunProfiles
from ..plan_review import PlanReviewService
from ..repository import Repository
from ..sandbox_protocol import write_record
from ..settings import SettingsService
from .cases import CASES
from .fixtures import validate_case_fixture
from .models import FindingAnswer, IndependentCheck, NumberAnswer, ObservedRun
from .runner import digest

_BASE_TOOLS = [
    "read_source",
    "search_sources",
    "list_documents",
    "calculate_engineering",
    "check_engineering_calculation",
    "save_work_product",
    "read_work_product",
]
_DELEGATION_TOOLS = ["create_staff", "execute_staff", "read_staff_result", "read_office_messages"]


def instruction_for(case, sources):
    # Goldens and expected finding membership never enter this prompt. The
    # output field names/units and a shared finding vocabulary are a schema.
    vocabulary = sorted(
        {finding.code for item in CASES for finding in item.findings}
        | {code for item in CASES for code in item.forbidden_findings}
    )
    task = {
        "synthetic_benchmark": True,
        "task": case.instruction,
        "sources": sources,
        "required_numeric_fields": {
            key: {"unit": value.unit} for key, value in case.numbers.items()
        },
        "finding_code_vocabulary": vocabulary,
        "output": {
            "tool": "save_work_product",
            "kind": "table",
            "title": "Benchmark result " + case.id,
            "numeric_row": {
                "kind": "number",
                "key": "required field name",
                "value": "decimal string",
                "unit": "requested engineering unit",
                "source_ids": ["actual inspected source IDs"],
                "calculation_id": "exact saved calculation ID producing this value",
            },
            "finding_row": {
                "kind": "finding",
                "key": "applicable finding code",
                "value": "explanation",
                "source_ids": ["actual inspected source IDs"],
            },
        },
        "requirements": [
            "Inspect the supplied source passages before using their facts.",
            "Use calculate_engineering for every reported number, even a copied source value (product by 1).",
            "For engineering labels outside the calculator's unit catalog, compute decimal values with dimensionless units and retain the original engineering unit in the table. Do not invent unit conversions.",
            "Save one final table. Cite actual source IDs in each row and in the work product's source_refs.",
            "Do not send commercial messages, change source files, retrieve live market prices or create unsupported facts.",
        ],
    }
    if case.required_assignments:
        task["requirements"] += [
            "After saving each numeric calculation, hand its exact ID and source IDs to a different colleague in a real assignment. That reviewer must inspect those sources, call check_engineering_calculation on each handed-off ID, and include the checked IDs in their completed result.",
            "Inspect the completed review and the source passages yourself before the final table. Every numeric row must identify its exact independently checked calculation_id. Checking your own calculation does not count as independent review.",
        ]
    return json.dumps(task, ensure_ascii=False)


class LiveJobExecutor:
    def __init__(
        self,
        home: Path,
        *,
        connection_id: str,
        model_id: str,
        total_budget: Decimal,
        attempt_count=72,
        max_requests=12,
        timeout_seconds=600,
    ):
        if not total_budget.is_finite() or total_budget <= 0:
            raise ValueError("Live evaluation requires an explicit positive total budget.")
        self.repo = Repository(Path(home))
        self.connections = AIConnectionService(self.repo)
        self.connection = self.connections.get(connection_id)
        self.model = next(
            (
                item
                for item in self.connections.models(connection_id)
                if item["model_id"] == model_id
            ),
            None,
        )
        if self.model is None:
            raise ValueError("Select an existing configured model for the benchmark.")
        self.model_basis = self._model_basis(self.model)
        self.manager_version = ManagerProfileService(self.repo).get().version
        self.route = AIRoute(
            connection_id=connection_id,
            model_id=model_id,
            max_output_tokens=4000,
            web_search=False,
            max_search_calls=1,
        )
        self.per_attempt = (total_budget / Decimal(attempt_count)).quantize(
            Decimal("0.000001"), rounding=ROUND_DOWN
        )
        if self.per_attempt <= 0:
            raise ValueError(
                "The total budget is too small to divide across the requested repetitions."
            )
        self.max_requests, self.timeout_seconds = max_requests, timeout_seconds
        self.blocked = None
        self.run_group = str(time.time_ns())
        self.policy = AIPolicyService(self.repo)
        # This validates actual existing readiness/capabilities and prices. It
        # never creates credentials, installs a client or changes account extras.
        self.policy.validate_route(self.route, [connection_id])
        from ..benchmark_adoption import BenchmarkAdoptionService

        self.adoption = BenchmarkAdoptionService(self.repo)

    @staticmethod
    def _model_basis(model):
        return digest(
            {
                key: value
                for key, value in model.items()
                if key not in {"updated_at", "fetched_at", "checked_at"}
            }
        )

    async def _wait(self, jobs, tender_id, runs):
        tasks = [jobs.tasks[run["id"]] for run in runs if run["id"] in jobs.tasks]
        if tasks:
            done, pending = await asyncio.wait(tasks, timeout=self.timeout_seconds)
            if pending:
                jobs.stop_tender_work(tender_id)
                await asyncio.gather(*tasks, return_exceptions=True)
                raise TimeoutError("The synthetic benchmark job exceeded its time allowance.")
            for task in done:
                if not task.cancelled():
                    task.result()

    async def __call__(self, case, repetition):
        if self.blocked:
            return ObservedRun(status="blocked", detail=self.blocked)
        connection = self.connections.get(self.connection["id"])
        model = next(
            (
                item
                for item in self.connections.models(connection["id"])
                if item["model_id"] == self.route.model_id
            ),
            None,
        )
        if (
            connection["revision"] != self.connection["revision"]
            or model is None
            or self._model_basis(model) != self.model_basis
            or ManagerProfileService(self.repo).get().version != self.manager_version
        ):
            self.blocked = "The selected account or model configuration changed. Start a freshly reviewed benchmark."
            return ObservedRun(status="blocked", detail=self.blocked)
        tender = self.repo.create_tender(f"SYNTHETIC BENCHMARK {case.id} repetition {repetition}")
        tender_id = tender["id"]
        root = validate_case_fixture(case)
        admission_root = self.repo.home / "benchmarks" / "admissions" / self.run_group
        admission_root.mkdir(parents=True, exist_ok=True)
        jobs = JobManager(self.repo, SettingsService(self.repo))
        jobs.auto_analyze = False  # measured benchmark work stays isolated
        run_id = None
        mapping = {}
        try:
            imported = jobs.start_import(tender_id, str(root))
            await self._wait(jobs, tender_id, [imported])
            if self.repo.get_run(imported["id"])["status"] != "completed":
                raise ValueError("Synthetic source import did not complete.")
            sources = []
            for artifact in self.repo.list_artifacts(tender_id):
                logical = Path(artifact["relative_path"]).stem
                if logical not in {source.id for source in case.sources}:
                    raise ValueError("An unexpected source entered the synthetic Tender.")
                for evidence in self.repo.artifact_evidence(tender_id, artifact["id"]):
                    mapping[evidence["id"]] = logical
                    sources.append(
                        {
                            "source_id": evidence["id"],
                            "document": logical,
                            "locator": evidence["locator"],
                        }
                    )
            if set(mapping.values()) != {source.id for source in case.sources}:
                raise ValueError("Some synthetic evidence could not be extracted.")
            self.policy.update(
                tender_id,
                TenderAIInput(
                    allowed_connection_ids=[connection["id"]],
                    manager=self.route,
                    specialist=self.route,
                    run_budget_usd=float(self.per_attempt),
                    tender_budget_usd=float(self.per_attempt),
                    max_requests=self.max_requests,
                    engineer_confirmed=True,
                    rationale=f"Explicit CLI synthetic benchmark budget: at most USD {self.per_attempt} for this attempt; no account extras or fallback routes.",
                ).model_dump(),
            )
            instruction = instruction_for(case, sources)
            plan = self.repo.create_plan(
                tender_id,
                "Synthetic benchmark " + case.id,
                [
                    {
                        "title": case.title,
                        "description": instruction,
                        "role": "Tender benchmark reviewer",
                        "source_ids": list(mapping),
                    }
                ],
            )

            def queue_benchmark(tender_id, plan_id, plan, review):
                queued = jobs.queue_approved_plan_runs(tender_id, plan_id, plan, review)
                for item in queued:
                    self.adoption.admit_live(
                        tender_id=tender_id,
                        root_run_id=item["run"]["id"],
                        route=self.route.model_dump(mode="json"),
                        manager_version=self.manager_version,
                        budget=self.per_attempt,
                        case_id=case.id,
                        repetition=repetition,
                        source_map=mapping,
                        group_id=self.run_group,
                    )
                return queued

            review = PlanReviewService(
                self.repo,
                save_runs_in_transaction=queue_benchmark,
                schedule_after_commit=jobs.schedule_approved_plan_runs,
            )
            displayed = review.review(tender_id, plan["id"])
            envelope = displayed.get("delegation")
            if not envelope:
                raise ValueError("The selected route has no ready reviewed delegation scope.")
            available = {tool["id"] for tool in displayed["delegation_options"]["tools"]}
            tools = list(_BASE_TOOLS)
            # Manager coordination tools can be intrinsic rather than staff
            # catalogue entries. Include only ones exposed as review choices.
            if case.required_assignments:
                tools += [name for name in _DELEGATION_TOOLS if name in available]
            if set(_BASE_TOOLS) - available:
                raise ValueError(
                    "Required calculation/work-product tools are not available for review."
                )
            review.update_delegation(
                tender_id,
                plan["id"],
                {
                    "expected_version": displayed["delegation_proposal_version"],
                    "source_scope": "selected_sources",
                    "artifact_ids": [item["artifact_id"] for item in envelope["artifacts"]],
                    "tool_ids": tools,
                    "native_tools": {},
                    "allowed_draft_outputs": ["summary", "findings"],
                    "route_option_ids": [option["id"] for option in envelope["route_options"]],
                    "max_staff": 2 if case.required_assignments else 1,
                    "max_assignments": 3 if case.required_assignments else 1,
                    "max_depth": 1,
                    "max_concurrency": 2 if case.required_assignments else 1,
                    "max_requests": self.max_requests,
                    "max_search_calls": 0,
                },
            )
            displayed = review.review(tender_id, plan["id"])
            approved = review.approve_and_start(
                tender_id,
                plan["id"],
                {
                    "fingerprint": displayed["fingerprint"],
                    "engineer_confirmed": True,
                    "rationale": "The explicit benchmark command approves this synthetic-only case and bounded chosen AI route.",
                },
            )
            intents = approved["work_intents"]
            if len(intents) != 1:
                raise ValueError("A synthetic case must admit exactly one root budget owner.")
            run_id = intents[0]["run_id"]
            write_record(
                admission_root / f"{case.id}-{repetition}-admission.json",
                {
                    "tender_id": tender_id,
                    "root_run_id": run_id,
                    "review_fingerprint": displayed["fingerprint"],
                    "connection_id": connection["id"],
                    "connection_revision": connection["revision"],
                    "model_id": self.route.model_id,
                    "budget_usd": str(self.per_attempt),
                    "source_map": mapping,
                },
            )
            await self._wait(jobs, tender_id, [self.repo.get_run(run_id)])
            observed = self.collect(case, tender_id, run_id, mapping)
            if observed.manager_profile_version != self.manager_version:
                observed.status = "failed"
                observed.detail = "The Manager configuration changed during benchmark admission. This attempt is not comparable."
                self.blocked = observed.detail
            if not observed.usage_complete:
                self.blocked = "The preceding attempt has missing or uncertain usage. Reconcile it before another paid benchmark request."
            return observed
        except asyncio.CancelledError:
            jobs.stop_tender_work(tender_id)
            raise
        except Exception as error:
            self.blocked = "The synthetic benchmark could not continue: " + safe_error(error)
            if run_id:
                observed = self.collect(case, tender_id, run_id, mapping)
                observed.status, observed.detail = "failed", self.blocked
                return observed
            return ObservedRun(status="blocked", tender_id=tender_id, detail=self.blocked)
        finally:
            await jobs.close()

    def collect(self, case, tender_id, run_id, mapping):
        run = self.repo.get_run(run_id)
        events = self.repo.run_events(run_id)
        observed = ObservedRun(
            status="completed" if run["status"] == "completed" else "failed",
            tender_id=tender_id,
            root_run_id=run_id,
            connection_id=self.connection["id"],
            model_id=self.route.model_id,
            engine=self.connection["protocol"],
            detail=run.get("error") or "",
        )
        observed.manager_profile_version = (
            ManagerRunProfiles(self.repo).get(tender_id, run_id).version
        )
        from ..benchmark_adoption import configuration_fingerprint

        observed.configuration_hash = configuration_fingerprint(
            self.repo, self.route.model_dump(mode="json"), observed.manager_profile_version
        )
        observed.read_evidence = sorted(
            {
                mapping[identifier]
                for identifier in run.get("result", {}).get("source_ids_read", [])
                if identifier in mapping
            }
        )
        observed.events = [event["kind"] for event in events]
        calculation_ids = {
            event["data"]["calculation_id"]
            for event in events
            if event["kind"] == "calculation_completed" and event["data"].get("calculation_id")
        }
        checked = {
            event["data"]["calculation_id"]
            for event in events
            if event["kind"] == "calculation_checked" and event["data"].get("reproducible") is True
        }
        service = CalculationService(self.repo)
        from ..work_products import WorkProductService

        WorkProductService(self.repo)
        verified = True
        with self.repo.db.connect() as conn:
            calculations = [
                dict(row)
                for row in conn.execute(
                    "SELECT * FROM office_calculations WHERE tender_id=?", (tender_id,)
                )
                if row["id"] in calculation_ids
            ]
            work_receipts = (
                [
                    dict(row)
                    for row in conn.execute(
                        "SELECT actor_id,result_json FROM engineering_work_receipts WHERE root_id=? AND capability='calculate_engineering'",
                        (run_id,),
                    )
                ]
                if conn.execute(
                    "SELECT 1 FROM sqlite_master WHERE name='engineering_work_receipts'"
                ).fetchone()
                else []
            )
        creator_records = {
            json.loads(row["result_json"])["id"]: (row["actor_id"], json.loads(row["result_json"]))
            for row in work_receipts
        }
        for row in calculations:
            check = service.check(
                engineer_identity(tender_id),
                CalculationCheckRequest(
                    calculation_id=row["id"], method_id="recompute", method_version="1"
                ),
            )
            verified = verified and check["reproducible"] and row["status"] == "calculated"
            observed.calculation_ids.append(row["id"])
            observed.calculation_outputs[row["id"]] = [
                str(value)
                for key, value in json.loads(row["outputs_json"]).items()
                if key != "unit"
            ]
            creator = creator_records.get(row["id"])
            observed.calculation_evidence[row["id"]] = (
                [
                    mapping.get(source, "UNKNOWN:" + source)
                    for source in creator[1].get("source_refs", [])
                ]
                if creator
                else []
            )
            for key, value in json.loads(row["outputs_json"]).items():
                if key != "unit":
                    observed.calculation_values.append(str(value))
        observed.calculations_verified = bool(calculations) and verified
        observed.checked_calculation_ids = sorted(checked & calculation_ids)
        version_ids = [
            event["data"]["version_id"]
            for event in events
            if event["kind"] == "saved_work_product" and event["data"].get("version_id")
        ]
        with self.repo.db.connect() as conn:
            products = [
                dict(row)
                for row in conn.execute(
                    "SELECT * FROM work_product_versions WHERE tender_id=? ORDER BY created_at,id",
                    (tender_id,),
                )
                if row["id"] in version_ids
            ]
            reviewers = [
                dict(row)
                for row in conn.execute(
                    """SELECT a.id,a.staff_id,a.result_id,r.payload_json FROM office_assignments a JOIN office_staff_results r
                ON a.result_id=r.id AND a.tender_id=r.tender_id AND r.assignment_id=a.id WHERE a.tender_id=? AND a.root_run_id=? AND a.status='completed'""",
                    (tender_id, run_id),
                )
            ]
            observed.completed_assignments = len(reviewers)
            source_receipts = (
                [
                    dict(row)
                    for row in conn.execute(
                        "SELECT actor_id,assignment_id,source_id FROM staff_source_receipts WHERE tender_id=? AND root_run_id=? AND text_length>0",
                        (tender_id, run_id),
                    )
                ]
                if conn.execute(
                    "SELECT 1 FROM sqlite_master WHERE name='staff_source_receipts'"
                ).fetchone()
                else []
            )
        for event in events:
            data = event["data"]
            if event["kind"] != "calculation_checked" or data.get("reproducible") is not True:
                continue
            calculation_id = data.get("calculation_id")
            creator = creator_records.get(calculation_id)
            reviewer = next(
                (
                    item
                    for item in reviewers
                    if item["id"] == data.get("assignment_id")
                    and item["staff_id"] == data.get("actor_id")
                ),
                None,
            )
            if (
                not creator
                or not reviewer
                or creator[0] == reviewer["staff_id"]
                or calculation_id not in reviewer["payload_json"]
            ):
                continue
            inspected = {
                item["source_id"]
                for item in source_receipts
                if item["actor_id"] == reviewer["staff_id"]
                and item["assignment_id"] == reviewer["id"]
            }
            if not set(creator[1].get("source_refs", [])) <= inspected:
                continue
            observed.independent_checks[calculation_id] = IndependentCheck(
                creator_actor_id=creator[0],
                reviewer_actor_id=reviewer["staff_id"],
                reviewer_assignment_id=reviewer["id"],
                reviewer_result_id=reviewer["result_id"],
                evidence=[mapping.get(source, "UNKNOWN:" + source) for source in inspected],
            )
        # The final named artifact must have been saved by an actual tool in
        # this root, not merely described by the model or found in another run.
        final = next(
            (item for item in reversed(products) if item["title"] == "Benchmark result " + case.id),
            None,
        )
        if final:
            observed.artifact_ids = [final["id"]]
            for row in json.loads(final["rows_json"]):
                evidence = [
                    mapping.get(identifier, "UNKNOWN:" + str(identifier))
                    for identifier in row.get("source_ids", [])
                ]
                if row.get("kind") == "number" and row.get("key"):
                    if str(row["key"]) in observed.numbers:
                        observed.status = "failed"
                        observed.detail = (
                            "The saved table contains duplicate numeric output fields."
                        )
                    observed.numbers[str(row["key"])] = NumberAnswer(
                        value=str(row.get("value", "")),
                        unit=str(row.get("unit", "")),
                        evidence=evidence,
                        calculation_id=row.get("calculation_id")
                        if isinstance(row.get("calculation_id"), str)
                        else None,
                    )
                elif row.get("kind") == "finding" and row.get("key"):
                    observed.findings.append(
                        FindingAnswer(
                            code=str(row["key"]),
                            description=str(row.get("value", "")),
                            evidence=evidence,
                        )
                    )
                else:
                    observed.status = "failed"
                    observed.detail = "The saved table contains an unsupported result row."
        usage = [item for item in self.policy.usage(tender_id) if item["run_id"] == run_id]
        if usage:
            observed.requests = sum(int(item.get("requests", 0)) for item in usage)
            observed.input_tokens = (
                sum(int(item.get("input_tokens", 0)) for item in usage)
                if all(item.get("input_tokens") is not None for item in usage)
                else None
            )
            observed.output_tokens = (
                sum(int(item.get("output_tokens", 0)) for item in usage)
                if all(item.get("output_tokens") is not None for item in usage)
                else None
            )
            observed.usage_complete = all(
                item.get("status") not in {"reserved", "uncertain"}
                and item.get("usage_complete", True) is not False
                for item in usage
            )
            if all(item.get("estimated_cost_usd") is not None for item in usage):
                observed.estimated_cost_usd = str(
                    sum((Decimal(str(item["estimated_cost_usd"])) for item in usage), Decimal(0))
                )
            if all(item.get("provider_reported_cost_usd") is not None for item in usage):
                observed.provider_reported_cost_usd = str(
                    sum(
                        (Decimal(str(item["provider_reported_cost_usd"])) for item in usage),
                        Decimal(0),
                    )
                )
                observed.provider_cost_is_partial = any(
                    item.get("provider_cost_is_partial") is True
                    or item.get("provider_usage_is_incomplete") is True
                    for item in usage
                )
        return observed
