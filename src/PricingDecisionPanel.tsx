import { FormEvent, useCallback, useEffect, useRef, useState } from "react";

import type { CalculationWorkspaceInspection } from "./bindings/CalculationWorkspaceInspection";
import type { EstimateWorkspaceInspection } from "./bindings/EstimateWorkspaceInspection";
import type { PricingAdjustmentDirection } from "./bindings/PricingAdjustmentDirection";
import type { PricingAdjustmentKind } from "./bindings/PricingAdjustmentKind";
import type { PricingWorkspaceInspection } from "./bindings/PricingWorkspaceInspection";
import {
  approveCommercialStrategy,
  approvePricedCostBaseline,
  approvePricingAdjustment,
  approveTenderPrice,
  createCommercialStrategy,
  createPricedCostBaseline,
  createPricingAdjustment,
  createPricingScenario,
  inspectCalculationWorkspace,
  inspectEstimateWorkspace,
  inspectPricingWorkspace,
  runPricedCostBaselineReview,
  runPricingAdjustmentReview,
  selectPricingScenario,
} from "./quantixHost";

interface PricingDecisionPanelProps {
  tenderId: string;
  runtimeReady: boolean;
  refreshToken: number;
  reportCommandFailure: () => void;
  onTenderStateChange: () => void;
}

function copy(value: string) {
  return value.split("_").join(" ");
}

function statements(value: string) {
  return value
    .split("\n")
    .map((item) => item.trim())
    .filter(Boolean);
}

const CALCULATION_PAGE_SIZE = 8;

export function PricingDecisionPanel({
  tenderId,
  runtimeReady,
  refreshToken,
  reportCommandFailure,
  onTenderStateChange,
}: PricingDecisionPanelProps) {
  const [workspace, setWorkspace] = useState<PricingWorkspaceInspection>();
  const [estimate, setEstimate] = useState<EstimateWorkspaceInspection>();
  const [calculations, setCalculations] =
    useState<CalculationWorkspaceInspection>();
  const [runOffset, setRunOffset] = useState(0);
  const [loading, setLoading] = useState(false);
  const [mutating, setMutating] = useState(false);
  const [baselineRationale, setBaselineRationale] = useState("");
  const [decisionDrafts, setDecisionDrafts] = useState<Record<string, string>>(
    {},
  );
  const [adjustmentRunId, setAdjustmentRunId] = useState("");
  const [adjustmentKind, setAdjustmentKind] =
    useState<PricingAdjustmentKind>("contingency");
  const [adjustmentDirection, setAdjustmentDirection] =
    useState<PricingAdjustmentDirection>("add");
  const [adjustmentScope, setAdjustmentScope] = useState("");
  const [adjustmentRationale, setAdjustmentRationale] = useState("");
  const [remediationTargetId, setRemediationTargetId] = useState("");
  const [commercialAppetite, setCommercialAppetite] = useState("");
  const [exclusions, setExclusions] = useState("");
  const [qualifications, setQualifications] = useState("");
  const [scenarioName, setScenarioName] = useState("");
  const [strategyId, setStrategyId] = useState("");
  const [strategyAdjustmentId, setStrategyAdjustmentId] = useState("");
  const [selectedAdjustmentIds, setSelectedAdjustmentIds] = useState<string[]>(
    [],
  );
  const requestGeneration = useRef(0);
  const actionActive = useRef(false);

  const load = useCallback(
    async (requestedRunOffset = runOffset) => {
      const generation = ++requestGeneration.current;
      setLoading(true);
      try {
        const [nextWorkspace, nextEstimate, nextCalculations] =
          await Promise.all([
            inspectPricingWorkspace(tenderId),
            inspectEstimateWorkspace(tenderId),
            inspectCalculationWorkspace(tenderId, 0, requestedRunOffset),
          ]);
        if (generation !== requestGeneration.current) return;
        setWorkspace(nextWorkspace);
        setEstimate(nextEstimate);
        setCalculations(nextCalculations);
        setStrategyId((current) => {
          const approved = nextWorkspace.strategies.filter(
            (strategy) => strategy.current && strategy.approval,
          );
          return approved.some((strategy) => strategy.strategy_id === current)
            ? current
            : (approved[0]?.strategy_id ?? "");
        });
        setAdjustmentRunId((current) =>
          nextCalculations.recent_runs.some(
            (run) => run.calculation_run_id === current && run.approval,
          )
            ? current
            : "",
        );
        setStrategyAdjustmentId((current) => {
          const eligible = nextWorkspace.adjustments.filter(
            (adjustment) =>
              adjustment.current &&
              adjustment.approval &&
              adjustment.kind === "commercial_strategy",
          );
          return eligible.some(
            (adjustment) => adjustment.adjustment_id === current,
          )
            ? current
            : (eligible[0]?.adjustment_id ?? "");
        });
        setRemediationTargetId((current) =>
          nextWorkspace.adjustments.some(
            (adjustment) =>
              adjustment.adjustment_id === current &&
              adjustment.current &&
              adjustment.review?.outcome === "failed",
          )
            ? current
            : "",
        );
      } catch {
        if (generation === requestGeneration.current) reportCommandFailure();
      } finally {
        if (generation === requestGeneration.current) setLoading(false);
      }
    },
    [reportCommandFailure, runOffset, tenderId],
  );

  useEffect(() => {
    void load();
    return () => {
      requestGeneration.current += 1;
    };
  }, [load, refreshToken]);

  async function execute(action: () => Promise<unknown>) {
    if (actionActive.current) return;
    actionActive.current = true;
    requestGeneration.current += 1;
    setMutating(true);
    try {
      await action();
      onTenderStateChange();
      await load();
    } catch {
      reportCommandFailure();
    } finally {
      actionActive.current = false;
      setMutating(false);
    }
  }

  const decisionDraft = (key: string) => decisionDrafts[key] ?? "";
  const setDecisionDraft = (key: string, value: string) =>
    setDecisionDrafts((current) => ({ ...current, [key]: value }));

  const baseline = workspace?.baseline;
  const busy = loading || mutating;
  const basis = estimate?.basis;
  const approvedRuns =
    calculations?.recent_runs.filter(
      (run) => run.status === "completed" && run.approval,
    ) ?? [];
  const approvedAdjustments =
    workspace?.adjustments.filter(
      (adjustment) => adjustment.current && adjustment.approval,
    ) ?? [];
  const approvedStrategies =
    workspace?.strategies.filter(
      (strategy) => strategy.current && strategy.approval,
    ) ?? [];
  const failedAdjustments =
    workspace?.adjustments.filter(
      (adjustment) =>
        adjustment.current && adjustment.review?.outcome === "failed",
    ) ?? [];
  const reviewedStrategyInputs = approvedAdjustments.filter(
    (adjustment) => adjustment.kind === "commercial_strategy",
  );

  function submitBaseline(event: FormEvent) {
    event.preventDefault();
    if (!basis) return;
    void execute(() =>
      createPricedCostBaseline({
        tender_id: tenderId,
        basis_id: basis.basis_id,
        basis_version: basis.version,
        basis_manifest_sha256: basis.manifest_sha256,
        rationale: baselineRationale,
      }),
    );
  }

  function submitAdjustment(event: FormEvent) {
    event.preventDefault();
    const run = approvedRuns.find(
      (candidate) => candidate.calculation_run_id === adjustmentRunId,
    );
    if (!baseline || !run) return;
    void execute(() =>
      createPricingAdjustment({
        tender_id: tenderId,
        baseline_id: baseline.baseline_id,
        baseline_version: baseline.version,
        baseline_manifest_sha256: baseline.manifest_sha256,
        calculation_run_id: run.calculation_run_id,
        calculation_manifest_sha256: run.manifest_sha256,
        kind: adjustmentKind,
        direction: adjustmentDirection,
        scope: adjustmentScope,
        rationale: adjustmentRationale,
        commercial_appetite:
          adjustmentKind === "commercial_strategy" ? commercialAppetite : null,
        exclusions:
          adjustmentKind === "commercial_strategy"
            ? statements(exclusions)
            : [],
        qualifications:
          adjustmentKind === "commercial_strategy"
            ? statements(qualifications)
            : [],
        remediates: failedAdjustments
          .filter(
            (adjustment) => adjustment.adjustment_id === remediationTargetId,
          )
          .map((adjustment) => ({
            adjustment_id: adjustment.adjustment_id,
            version: adjustment.version,
            manifest_sha256: adjustment.manifest_sha256,
          })),
      }),
    );
  }

  function submitStrategy(event: FormEvent) {
    event.preventDefault();
    const reviewedInput = reviewedStrategyInputs.find(
      (adjustment) => adjustment.adjustment_id === strategyAdjustmentId,
    );
    if (!baseline || !reviewedInput) return;
    void execute(() =>
      createCommercialStrategy({
        tender_id: tenderId,
        baseline_id: baseline.baseline_id,
        baseline_version: baseline.version,
        baseline_manifest_sha256: baseline.manifest_sha256,
        reviewed_inputs: [
          {
            adjustment_id: reviewedInput.adjustment_id,
            version: reviewedInput.version,
            manifest_sha256: reviewedInput.manifest_sha256,
          },
        ],
      }),
    );
  }

  function submitScenario(event: FormEvent) {
    event.preventDefault();
    if (!baseline) return;
    const strategy = approvedStrategies.find(
      (candidate) => candidate.strategy_id === strategyId,
    );
    if (!strategy) return;
    void execute(() =>
      createPricingScenario({
        tender_id: tenderId,
        name: scenarioName,
        baseline_id: baseline.baseline_id,
        baseline_version: baseline.version,
        baseline_manifest_sha256: baseline.manifest_sha256,
        strategy_id: strategy.strategy_id,
        strategy_manifest_sha256: strategy.manifest_sha256,
        adjustments: approvedAdjustments
          .filter((adjustment) =>
            selectedAdjustmentIds.includes(adjustment.adjustment_id),
          )
          .map((adjustment) => ({
            adjustment_id: adjustment.adjustment_id,
            version: adjustment.version,
            manifest_sha256: adjustment.manifest_sha256,
          })),
      }),
    );
  }

  return (
    <section className="workspace-section">
      <div className="section-heading">
        <div>
          <p className="eyebrow">Commercial pricing</p>
          <h2>Pricing scenarios and Approved Tender Price</h2>
          <p>
            Cost remains separate from sell price. All amounts come from exact
            approved Calculation Runs; agents review and recommend but cannot
            select commercial appetite, scenario, or Final Price.
          </p>
        </div>
        <button
          className="button-secondary"
          disabled={busy}
          onClick={() => void load()}
        >
          Refresh
        </button>
      </div>

      {baseline ? (
        <article className="record-card">
          <p className="eyebrow">
            Priced Cost Baseline v{baseline.version} / Tender revision{" "}
            {baseline.tender_revision}
          </p>
          <h3>
            {baseline.amount} {baseline.currency}
          </h3>
          <p>
            {baseline.current ? "Current" : "Stale"} /{" "}
            {baseline.review?.outcome ?? "Awaiting independent review"} /{" "}
            {baseline.approved_for_commercial_pricing
              ? "EITL approved"
              : "Not approved for commercial pricing"}
          </p>
          <p>
            Basis {baseline.basis_id} v{baseline.basis_version} / aggregate run{" "}
            {baseline.aggregate_calculation_run_id}
          </p>
          <p>
            Baseline manifest <code>{baseline.manifest_sha256}</code> /
            aggregate manifest{" "}
            <code>{baseline.aggregate_calculation_manifest_sha256}</code>
          </p>
          {baseline.approval ? (
            <p>
              Approval {baseline.approval.approval_id} by{" "}
              {baseline.approval.approved_by} as{" "}
              {copy(baseline.approval.acting_role)} at{" "}
              {baseline.approval.created_at}: {baseline.approval.rationale} /{" "}
              <code>{baseline.approval.manifest_sha256}</code>
            </p>
          ) : null}
          {baseline.review?.findings.map((finding) => (
            <p key={finding.code}>
              <strong>{copy(finding.code)}:</strong> {finding.summary}
            </p>
          ))}
          {!baseline.review && baseline.current ? (
            <button
              disabled={busy || !runtimeReady}
              onClick={() =>
                void execute(() =>
                  runPricedCostBaselineReview({
                    tender_id: tenderId,
                    baseline_id: baseline.baseline_id,
                    version: baseline.version,
                  }),
                )
              }
            >
              Run independent baseline review
            </button>
          ) : null}
          {baseline.review?.outcome === "passed" && !baseline.approval ? (
            <div className="decision-grid">
              <label>
                Exact EITL rationale
                <textarea
                  value={decisionDraft(
                    `baseline:${baseline.baseline_id}:${baseline.version}`,
                  )}
                  onChange={(event) =>
                    setDecisionDraft(
                      `baseline:${baseline.baseline_id}:${baseline.version}`,
                      event.target.value,
                    )
                  }
                />
              </label>
              <button
                disabled={
                  busy ||
                  !decisionDraft(
                    `baseline:${baseline.baseline_id}:${baseline.version}`,
                  ).trim()
                }
                onClick={() =>
                  void execute(() =>
                    approvePricedCostBaseline({
                      tender_id: tenderId,
                      baseline_id: baseline.baseline_id,
                      version: baseline.version,
                      manifest_sha256: baseline.manifest_sha256,
                      rationale: decisionDraft(
                        `baseline:${baseline.baseline_id}:${baseline.version}`,
                      ),
                    }),
                  )
                }
              >
                Approve Priced Cost Baseline
              </button>
            </div>
          ) : null}
        </article>
      ) : (
        <form className="decision-grid" onSubmit={submitBaseline}>
          <h3>Establish Priced Cost Baseline</h3>
          <p>
            {basis?.relied_upon
              ? `Approved Basis v${basis.version}: ${basis.total_amount} ${basis.total_currency}`
              : "An exact approved Basis of Estimate is required."}
          </p>
          <label>
            Rationale
            <textarea
              value={baselineRationale}
              onChange={(event) => setBaselineRationale(event.target.value)}
            />
          </label>
          <button
            disabled={
              busy ||
              !runtimeReady ||
              !basis?.relied_upon ||
              !baselineRationale.trim()
            }
          >
            Create immutable cost baseline
          </button>
        </form>
      )}

      {baseline && !baseline.current && basis?.relied_upon ? (
        <form className="decision-grid" onSubmit={submitBaseline}>
          <h3>Supersede stale cost baseline</h3>
          <p>
            Bind a new immutable version to current approved Basis v
            {basis.version}.
          </p>
          <label>
            Successor rationale
            <textarea
              value={baselineRationale}
              onChange={(event) => setBaselineRationale(event.target.value)}
            />
          </label>
          <button disabled={busy || !baselineRationale.trim()}>
            Create successor baseline
          </button>
        </form>
      ) : null}

      {baseline?.approved_for_commercial_pricing ? (
        <>
          <form className="decision-grid" onSubmit={submitAdjustment}>
            <h3>Separate calculation adjustment</h3>
            <label>
              Approved Calculation Run
              <select
                value={adjustmentRunId}
                onChange={(event) => setAdjustmentRunId(event.target.value)}
              >
                <option value="">Choose exact run</option>
                {approvedRuns.map((run) => (
                  <option
                    key={run.calculation_run_id}
                    value={run.calculation_run_id}
                  >
                    {run.description} / {run.final_amount} {run.output_currency}
                  </option>
                ))}
              </select>
            </label>
            <div className="inline-actions">
              <button
                type="button"
                className="button-secondary"
                disabled={busy || runOffset === 0}
                onClick={() =>
                  setRunOffset((current) =>
                    Math.max(0, current - CALCULATION_PAGE_SIZE),
                  )
                }
              >
                Newer Calculation Runs
              </button>
              <button
                type="button"
                className="button-secondary"
                disabled={busy || !calculations?.has_older_runs}
                onClick={() =>
                  setRunOffset((current) => current + CALCULATION_PAGE_SIZE)
                }
              >
                Older Calculation Runs
              </button>
              <span>
                {calculations?.total_run_count ?? 0} total / offset {runOffset}
              </span>
            </div>
            <label>
              Kind
              <select
                value={adjustmentKind}
                onChange={(event) =>
                  setAdjustmentKind(event.target.value as PricingAdjustmentKind)
                }
              >
                {(
                  [
                    "contingency",
                    "markup",
                    "exclusion",
                    "qualification",
                    "commercial_strategy",
                    "other",
                  ] as PricingAdjustmentKind[]
                ).map((kind) => (
                  <option key={kind} value={kind}>
                    {copy(kind)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Direction
              <select
                value={adjustmentDirection}
                onChange={(event) =>
                  setAdjustmentDirection(
                    event.target.value as PricingAdjustmentDirection,
                  )
                }
              >
                <option value="add">add</option>
                <option value="deduct">deduct</option>
              </select>
            </label>
            <label>
              Scope
              <input
                value={adjustmentScope}
                onChange={(event) => setAdjustmentScope(event.target.value)}
              />
            </label>
            <label>
              Rationale
              <textarea
                value={adjustmentRationale}
                onChange={(event) => setAdjustmentRationale(event.target.value)}
              />
            </label>
            <label>
              Failed review remediated (optional)
              <select
                value={remediationTargetId}
                onChange={(event) => setRemediationTargetId(event.target.value)}
              >
                <option value="">New independent adjustment</option>
                {failedAdjustments.map((adjustment) => (
                  <option
                    key={adjustment.adjustment_id}
                    value={adjustment.adjustment_id}
                  >
                    {copy(adjustment.kind)} / {adjustment.scope}
                  </option>
                ))}
              </select>
            </label>
            {adjustmentKind === "commercial_strategy" ? (
              <>
                <label>
                  Commercial appetite reviewed with this exact input
                  <textarea
                    value={commercialAppetite}
                    onChange={(event) =>
                      setCommercialAppetite(event.target.value)
                    }
                  />
                </label>
                <label>
                  Exclusions reviewed with this exact input (one per line)
                  <textarea
                    value={exclusions}
                    onChange={(event) => setExclusions(event.target.value)}
                  />
                </label>
                <label>
                  Qualifications reviewed with this exact input (one per line)
                  <textarea
                    value={qualifications}
                    onChange={(event) => setQualifications(event.target.value)}
                  />
                </label>
              </>
            ) : null}
            <button
              disabled={
                busy ||
                !adjustmentRunId ||
                !adjustmentScope.trim() ||
                !adjustmentRationale.trim() ||
                (adjustmentKind === "commercial_strategy" &&
                  !commercialAppetite.trim())
              }
            >
              Create reviewed input
            </button>
          </form>

          <form className="decision-grid" onSubmit={submitStrategy}>
            <h3>Commercial strategy</h3>
            <label>
              Independently reviewed commercial input
              <select
                value={strategyAdjustmentId}
                onChange={(event) =>
                  setStrategyAdjustmentId(event.target.value)
                }
              >
                <option value="">Choose exact reviewed input</option>
                {reviewedStrategyInputs.map((adjustment) => (
                  <option
                    key={adjustment.adjustment_id}
                    value={adjustment.adjustment_id}
                  >
                    {adjustment.amount} {adjustment.currency} /{" "}
                    {adjustment.scope}
                  </option>
                ))}
              </select>
            </label>
            <button disabled={busy || !strategyAdjustmentId}>
              Create immutable strategy
            </button>
          </form>
        </>
      ) : null}

      {workspace?.adjustments.map((adjustment) => (
        <article className="record-card" key={adjustment.adjustment_id}>
          <h3>
            {copy(adjustment.kind)} / {adjustment.direction} {adjustment.amount}{" "}
            {adjustment.currency}
          </h3>
          <p>{adjustment.scope}</p>
          {adjustment.kind === "commercial_strategy" ? (
            <>
              <p>Commercial appetite: {adjustment.commercial_appetite}</p>
              <p>Exclusions: {adjustment.exclusions.join("; ") || "none"}</p>
              <p>
                Qualifications: {adjustment.qualifications.join("; ") || "none"}
              </p>
            </>
          ) : null}
          <p>
            Calculation Run {adjustment.calculation_run_id} /{" "}
            <code>{adjustment.calculation_manifest_sha256}</code>
          </p>
          <p>
            Adjustment manifest <code>{adjustment.manifest_sha256}</code>
          </p>
          <p>
            {adjustment.review?.outcome ?? "Awaiting independent review"} /{" "}
            {adjustment.current
              ? adjustment.approval
                ? "Current EITL approval"
                : "Not approved"
              : adjustment.approval
                ? "Revoked historical approval"
                : adjustment.review?.outcome === "failed"
                  ? "Historical failed review"
                  : "Historical unapproved input"}
          </p>
          {adjustment.approval ? (
            <p>
              Approval {adjustment.approval.approval_id} by{" "}
              {adjustment.approval.approved_by} as{" "}
              {copy(adjustment.approval.acting_role)} at{" "}
              {adjustment.approval.created_at}: {adjustment.approval.rationale}{" "}
              / <code>{adjustment.approval.manifest_sha256}</code>
            </p>
          ) : null}
          {adjustment.review?.findings.map((finding) => (
            <p key={finding.code}>
              <strong>{copy(finding.code)}:</strong> {finding.summary}
            </p>
          ))}
          {!adjustment.review && adjustment.current ? (
            <button
              disabled={busy || !runtimeReady}
              onClick={() =>
                void execute(() =>
                  runPricingAdjustmentReview({
                    tender_id: tenderId,
                    adjustment_id: adjustment.adjustment_id,
                    version: adjustment.version,
                  }),
                )
              }
            >
              Run independent adjustment review
            </button>
          ) : null}
          {adjustment.review?.outcome === "passed" && !adjustment.approval ? (
            <div className="decision-grid">
              <label>
                Exact EITL rationale
                <textarea
                  value={decisionDraft(
                    `adjustment:${adjustment.adjustment_id}:${adjustment.version}`,
                  )}
                  onChange={(event) =>
                    setDecisionDraft(
                      `adjustment:${adjustment.adjustment_id}:${adjustment.version}`,
                      event.target.value,
                    )
                  }
                />
              </label>
              <button
                disabled={
                  busy ||
                  !decisionDraft(
                    `adjustment:${adjustment.adjustment_id}:${adjustment.version}`,
                  ).trim()
                }
                onClick={() =>
                  void execute(() =>
                    approvePricingAdjustment({
                      tender_id: tenderId,
                      adjustment_id: adjustment.adjustment_id,
                      version: adjustment.version,
                      manifest_sha256: adjustment.manifest_sha256,
                      rationale: decisionDraft(
                        `adjustment:${adjustment.adjustment_id}:${adjustment.version}`,
                      ),
                    }),
                  )
                }
              >
                Approve exact adjustment
              </button>
            </div>
          ) : null}
        </article>
      ))}

      {workspace?.strategies.map((strategy) => (
        <article className="record-card" key={strategy.strategy_id}>
          <h3>Commercial appetite</h3>
          <p>{strategy.commercial_appetite}</p>
          <p>
            Reviewed input {strategy.reviewed_input.adjustment_id} / review{" "}
            {strategy.input_review_id} / approval {strategy.input_approval_id}
          </p>
          <p>Exclusions: {strategy.exclusions.join("; ") || "none"}</p>
          <p>Qualifications: {strategy.qualifications.join("; ") || "none"}</p>
          <p>
            Manifest <code>{strategy.manifest_sha256}</code>
          </p>
          {strategy.approval ? (
            <p>
              Approval {strategy.approval.approval_id} by{" "}
              {strategy.approval.approved_by} as{" "}
              {copy(strategy.approval.acting_role)} at{" "}
              {strategy.approval.created_at}: {strategy.approval.rationale} /{" "}
              <code>{strategy.approval.manifest_sha256}</code>
            </p>
          ) : null}
          {!strategy.approval && strategy.current ? (
            <div className="decision-grid">
              <label>
                EITL rationale
                <textarea
                  value={decisionDraft(`strategy:${strategy.strategy_id}`)}
                  onChange={(event) =>
                    setDecisionDraft(
                      `strategy:${strategy.strategy_id}`,
                      event.target.value,
                    )
                  }
                />
              </label>
              <button
                disabled={
                  busy ||
                  !decisionDraft(`strategy:${strategy.strategy_id}`).trim()
                }
                onClick={() =>
                  void execute(() =>
                    approveCommercialStrategy({
                      tender_id: tenderId,
                      strategy_id: strategy.strategy_id,
                      manifest_sha256: strategy.manifest_sha256,
                      rationale: decisionDraft(
                        `strategy:${strategy.strategy_id}`,
                      ),
                    }),
                  )
                }
              >
                Approve commercial strategy
              </button>
            </div>
          ) : (
            <p>
              {strategy.current
                ? strategy.approval
                  ? "Current EITL approval"
                  : "Not approved"
                : strategy.approval
                  ? "Revoked historical approval"
                  : "Historical unapproved strategy"}
            </p>
          )}
        </article>
      ))}

      {baseline?.approved_for_commercial_pricing &&
      approvedStrategies.length ? (
        <form className="decision-grid" onSubmit={submitScenario}>
          <h3>Create immutable pricing scenario</h3>
          <label>
            Name
            <input
              value={scenarioName}
              onChange={(event) => setScenarioName(event.target.value)}
            />
          </label>
          <label>
            Approved strategy
            <select
              value={strategyId}
              onChange={(event) => setStrategyId(event.target.value)}
            >
              {approvedStrategies.map((strategy) => (
                <option key={strategy.strategy_id} value={strategy.strategy_id}>
                  {strategy.commercial_appetite}
                </option>
              ))}
            </select>
          </label>
          <fieldset>
            <legend>Reviewed adjustments</legend>
            {approvedAdjustments.map((adjustment) => (
              <label key={adjustment.adjustment_id}>
                <input
                  type="checkbox"
                  checked={selectedAdjustmentIds.includes(
                    adjustment.adjustment_id,
                  )}
                  onChange={(event) =>
                    setSelectedAdjustmentIds((current) =>
                      event.target.checked
                        ? [...current, adjustment.adjustment_id]
                        : current.filter(
                            (id) => id !== adjustment.adjustment_id,
                          ),
                    )
                  }
                />
                {copy(adjustment.kind)} / {adjustment.direction}{" "}
                {adjustment.amount} {adjustment.currency}
              </label>
            ))}
          </fieldset>
          <button disabled={busy || !scenarioName.trim() || !strategyId}>
            Calculate scenario
          </button>
        </form>
      ) : null}

      {workspace?.scenarios.map((scenario) => (
        <article className="record-card" key={scenario.pricing_scenario_id}>
          <p className="eyebrow">
            Scenario v{scenario.version} /{" "}
            {scenario.current ? "Current" : "Revoked"}
          </p>
          <h3>
            {scenario.name}: {scenario.calculation.final_amount}{" "}
            {scenario.calculation.currency}
          </h3>
          <p>
            Baseline {scenario.calculation.baseline_amount}{" "}
            {scenario.calculation.currency} / {scenario.adjustments.length}{" "}
            adjustment(s)
          </p>
          <p>
            Rule {scenario.calculation.rule_id} v
            {scenario.calculation.rule_version} / calculation scenario{" "}
            {scenario.calculation.scenario_id} v
            {scenario.calculation.scenario_version} / precision{" "}
            {scenario.calculation.precision} / rounding{" "}
            {copy(scenario.calculation.rounding_mode)} / engine{" "}
            {scenario.calculation.engine_version}
          </p>
          <p>
            Scenario manifest <code>{scenario.manifest_sha256}</code> /
            Calculation Manifest{" "}
            <code>{scenario.calculation.manifest_sha256}</code>
          </p>
          <p>
            {scenario.selection?.current
              ? "Current EITL selection"
              : scenario.selection
                ? "Historical selection"
                : "Comparison only"}{" "}
            /{" "}
            {scenario.approved_tender_price
              ? `${scenario.approved_tender_price.current ? "Approved" : "Revoked"} Tender Price`
              : "No Final Price approval"}
          </p>
          {!scenario.selection?.current && scenario.current ? (
            <div className="decision-grid">
              <label>
                Exact scenario-selection rationale
                <textarea
                  value={decisionDraft(
                    `selection:${scenario.pricing_scenario_id}:${scenario.version}`,
                  )}
                  onChange={(event) =>
                    setDecisionDraft(
                      `selection:${scenario.pricing_scenario_id}:${scenario.version}`,
                      event.target.value,
                    )
                  }
                />
              </label>
              <button
                disabled={
                  busy ||
                  !decisionDraft(
                    `selection:${scenario.pricing_scenario_id}:${scenario.version}`,
                  ).trim()
                }
                onClick={() =>
                  void execute(() =>
                    selectPricingScenario({
                      tender_id: tenderId,
                      pricing_scenario_id: scenario.pricing_scenario_id,
                      version: scenario.version,
                      manifest_sha256: scenario.manifest_sha256,
                      rationale: decisionDraft(
                        `selection:${scenario.pricing_scenario_id}:${scenario.version}`,
                      ),
                    }),
                  )
                }
              >
                Select exact scenario
              </button>
            </div>
          ) : null}
          {scenario.selection?.current &&
          !scenario.approved_tender_price &&
          scenario.current ? (
            <div className="decision-grid">
              <label>
                Exact Final Price rationale
                <textarea
                  value={decisionDraft(
                    `price:${scenario.pricing_scenario_id}:${scenario.version}`,
                  )}
                  onChange={(event) =>
                    setDecisionDraft(
                      `price:${scenario.pricing_scenario_id}:${scenario.version}`,
                      event.target.value,
                    )
                  }
                />
              </label>
              <button
                disabled={
                  busy ||
                  !decisionDraft(
                    `price:${scenario.pricing_scenario_id}:${scenario.version}`,
                  ).trim()
                }
                onClick={() =>
                  void execute(() =>
                    approveTenderPrice({
                      tender_id: tenderId,
                      pricing_scenario_id: scenario.pricing_scenario_id,
                      version: scenario.version,
                      manifest_sha256: scenario.manifest_sha256,
                      calculation_manifest_sha256:
                        scenario.calculation.manifest_sha256,
                      rationale: decisionDraft(
                        `price:${scenario.pricing_scenario_id}:${scenario.version}`,
                      ),
                    }),
                  )
                }
              >
                Approve exact Tender Price
              </button>
            </div>
          ) : null}
        </article>
      ))}

      {workspace?.decision_history.length ? (
        <div className="record-card">
          <p className="eyebrow">Immutable decision history</p>
          <h3>Scenario selections and Final Prices</h3>
          {workspace.decision_history.map((entry) => (
            <div className="decision-grid" key={entry.selection.selection_id}>
              <p>
                <strong>{entry.scenario_name}</strong> v
                {entry.pricing_scenario_version} /{" "}
                {entry.selection.current ? "Current selection" : "Superseded"}
              </p>
              <p>{entry.selection.rationale}</p>
              <p>
                Selected by {entry.selection.selected_by} as{" "}
                {copy(entry.selection.acting_role)} at{" "}
                {entry.selection.created_at} /{" "}
                <code>{entry.selection.manifest_sha256}</code>
              </p>
              {entry.approved_tender_price ? (
                <>
                  <p>
                    {entry.approved_tender_price.current
                      ? "Current"
                      : "Revoked"}{" "}
                    Final Price: {entry.approved_tender_price.amount}{" "}
                    {entry.approved_tender_price.currency}
                  </p>
                  <p>{entry.approved_tender_price.rationale}</p>
                  <p>
                    Approved by {entry.approved_tender_price.approved_by} as{" "}
                    {copy(entry.approved_tender_price.acting_role)} at{" "}
                    {entry.approved_tender_price.created_at} / decision{" "}
                    <code>{entry.approved_tender_price.manifest_sha256}</code> /
                    Calculation Manifest{" "}
                    <code>
                      {entry.approved_tender_price.calculation_manifest_sha256}
                    </code>
                  </p>
                </>
              ) : (
                <p>No Final Price was approved for this selection.</p>
              )}
            </div>
          ))}
        </div>
      ) : null}
    </section>
  );
}
