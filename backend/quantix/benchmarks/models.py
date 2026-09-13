from decimal import Decimal
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, field_validator

Category = Literal[
    "boq",
    "specification_conflict",
    "drawing_revision",
    "market_observation",
    "supplier_comparison",
    "integrated_work",
]


class Model(BaseModel):
    model_config = ConfigDict(extra="forbid")


class Source(Model):
    id: str
    title: str
    text: str


class ExpectedNumber(Model):
    value: str
    unit: str
    tolerance: str = "0.01"
    evidence: list[str]

    @field_validator("value", "tolerance")
    @classmethod
    def finite_decimal(cls, value):
        if not Decimal(value).is_finite():
            raise ValueError("Benchmark goldens must be finite literal decimals.")
        return value


class ExpectedFinding(Model):
    code: str
    evidence: list[str]


class CalculationProbe(Model):
    method: str
    inputs: dict
    units: dict
    expected: dict


class BenchmarkCase(Model):
    id: str
    category: Category
    title: str
    instruction: str
    sources: list[Source]
    numbers: dict[str, ExpectedNumber]
    findings: list[ExpectedFinding]
    forbidden_findings: list[str] = Field(default_factory=list)
    critical: bool = True
    required_assignments: int = Field(default=0, ge=0)
    required_events: list[str] = Field(
        default_factory=lambda: ["calculation_completed", "saved_work_product"]
    )
    probe: CalculationProbe


class NumberAnswer(Model):
    value: str
    unit: str
    evidence: list[str] = Field(default_factory=list)
    calculation_id: str | None = None


class IndependentCheck(Model):
    creator_actor_id: str
    reviewer_actor_id: str
    reviewer_assignment_id: str
    reviewer_result_id: str
    evidence: list[str]


class FindingAnswer(Model):
    code: str
    description: str
    evidence: list[str] = Field(default_factory=list)


class ObservedRun(Model):
    """Constructed by the driver from saved records, never by provider output."""

    status: Literal["completed", "failed", "cancelled", "blocked"]
    numbers: dict[str, NumberAnswer] = Field(default_factory=dict)
    findings: list[FindingAnswer] = Field(default_factory=list)
    read_evidence: list[str] = Field(default_factory=list)
    events: list[str] = Field(default_factory=list)
    artifact_ids: list[str] = Field(default_factory=list)
    calculation_ids: list[str] = Field(default_factory=list)
    checked_calculation_ids: list[str] = Field(default_factory=list)
    calculation_values: list[str] = Field(default_factory=list)
    calculation_outputs: dict[str, list[str]] = Field(default_factory=dict)
    calculation_evidence: dict[str, list[str]] = Field(default_factory=dict)
    independent_checks: dict[str, IndependentCheck] = Field(default_factory=dict)
    calculations_verified: bool = False
    completed_assignments: int = 0
    tender_id: str | None = None
    root_run_id: str | None = None
    connection_id: str | None = None
    model_id: str | None = None
    engine: str | None = None
    manager_profile_version: int | None = None
    configuration_hash: str | None = None
    latency_seconds: float = 0
    requests: int | None = None
    input_tokens: int | None = None
    output_tokens: int | None = None
    estimated_cost_usd: str | None = None
    provider_reported_cost_usd: str | None = None
    provider_cost_is_partial: bool | None = None
    usage_complete: bool = False
    detail: str = ""


class Scores(Model):
    task_completed: bool
    evidence_precision: float
    evidence_recall: float
    numerical_accuracy: float
    findings_recall: float
    violations: list[str]


class CaseResult(Model):
    case_id: str
    category: Category
    repetition: int
    critical: bool
    case_hash: str
    observed: ObservedRun
    scores: Scores


class BenchmarkReport(Model):
    id: str
    dataset_version: str
    dataset_hash: str
    evaluator_version: str = "1"
    mode: Literal[
        "dataset_validation",
        "deterministic_checks",
        "synthetic_executor",
        "synthetic_model",
        "live",
    ]
    created_at: str
    repetitions: int
    cases: list[CaseResult] = Field(default_factory=list)
    validated_cases: int
    deterministic_checks: list[dict] = Field(default_factory=list)
    overall_completed: bool = False
    detail: str = ""


class RegressionReview(Model):
    baseline_report_id: str
    candidate_report_id: str
    case_ids: list[str]
    engineer_confirmed: Literal[True]
    rationale: str = Field(min_length=10, max_length=4000)
