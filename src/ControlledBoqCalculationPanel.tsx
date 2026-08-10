import { FormEvent, useCallback, useEffect, useRef, useState } from "react";

import type { AgentTaskInputReference } from "./bindings/AgentTaskInputReference";
import type { CalculationInputState } from "./bindings/CalculationInputState";
import type { CalculationRoundingMode } from "./bindings/CalculationRoundingMode";
import type { CalculationWorkspaceInspection } from "./bindings/CalculationWorkspaceInspection";
import type { EvidenceSearchHit } from "./bindings/EvidenceSearchHit";
import type { ExchangeRateType } from "./bindings/ExchangeRateType";
import {
  approveCalculationRule,
  approveControlledBoqCalculationRun,
  createCalculationScenario,
  inspectCalculationWorkspace,
  proposeBoqCalculationRule,
  runCalculationRuleReview,
  runCostEstimatorCalculation,
  searchEvidence,
} from "./quantixHost";

interface ControlledBoqCalculationPanelProps {
  tenderId: string;
  runtimeReady: boolean;
  refreshToken: number;
  reportCommandFailure: () => void;
  onTenderStateChange: () => void;
}

type EvidenceTarget = "quantity" | "unit_rate" | "exchange_rate";
type RuleRoundingPolicy = "both" | CalculationRoundingMode;

const units = [
  "each",
  "mm",
  "m",
  "mm2",
  "m2",
  "mm3",
  "m3",
  "kg",
  "t",
  "min",
  "h",
];
function evidenceReference(hit: EvidenceSearchHit): AgentTaskInputReference {
  return {
    kind: "source_evidence",
    reference: `${hit.artifact_id}#${hit.location.ordinal}`,
    version: hit.version,
  };
}

function statusCopy(status: string) {
  return status.split("_").join(" ");
}

function ruleRoundingModes(
  policy: RuleRoundingPolicy,
): CalculationRoundingMode[] {
  return policy === "both"
    ? ["midpoint_away_from_zero", "midpoint_nearest_even"]
    : [policy];
}

export function ControlledBoqCalculationPanel({
  tenderId,
  runtimeReady,
  refreshToken,
  reportCommandFailure,
  onTenderStateChange,
}: ControlledBoqCalculationPanelProps) {
  const [workspace, setWorkspace] = useState<CalculationWorkspaceInspection>();
  const [busy, setBusy] = useState(false);
  const [scenarioOffset, setScenarioOffset] = useState(0);
  const [runOffset, setRunOffset] = useState(0);
  const [approvalRationale, setApprovalRationale] = useState("");
  const [ruleChangeRationale, setRuleChangeRationale] = useState("");
  const [ruleRoundingPolicy, setRuleRoundingPolicy] =
    useState<RuleRoundingPolicy>("both");
  const [description, setDescription] = useState("");
  const [scenarioName, setScenarioName] = useState("base");
  const [scenarioRationale, setScenarioRationale] = useState("");
  const [quantityUnit, setQuantityUnit] = useState("each");
  const [rateBasisUnit, setRateBasisUnit] = useState("each");
  const [rateCurrency, setRateCurrency] = useState("EGP");
  const [exchangeRateState, setExchangeRateState] =
    useState<CalculationInputState>("provided");
  const [exchangeRate, setExchangeRate] = useState("");
  const [exchangeRateEffectiveDate, setExchangeRateEffectiveDate] =
    useState("");
  const [pricingDate, setPricingDate] = useState("");
  const [exchangeRateType, setExchangeRateType] =
    useState<ExchangeRateType>("spot");
  const [outputCurrency, setOutputCurrency] = useState("EGP");
  const [precision, setPrecision] = useState(2);
  const [roundingMode, setRoundingMode] = useState<CalculationRoundingMode>(
    "midpoint_away_from_zero",
  );
  const [selectedScenarioId, setSelectedScenarioId] = useState("");
  const [evidenceTarget, setEvidenceTarget] =
    useState<EvidenceTarget>("quantity");
  const [evidenceQuery, setEvidenceQuery] = useState("");
  const [evidenceMatches, setEvidenceMatches] = useState<EvidenceSearchHit[]>(
    [],
  );
  const [quantityEvidence, setQuantityEvidence] = useState<EvidenceSearchHit>();
  const [unitRateEvidence, setUnitRateEvidence] = useState<EvidenceSearchHit>();
  const [exchangeRateEvidence, setExchangeRateEvidence] =
    useState<EvidenceSearchHit>();
  const [runApprovalDrafts, setRunApprovalDrafts] = useState<
    Record<string, string>
  >({});
  const requestGeneration = useRef(0);
  const currencies = workspace?.rule?.supported_currencies ?? [];

  const load = useCallback(
    async (
      requestedScenarioOffset = scenarioOffset,
      requestedRunOffset = runOffset,
    ) => {
      const generation = ++requestGeneration.current;
      setBusy(true);
      try {
        const next = await inspectCalculationWorkspace(
          tenderId,
          requestedScenarioOffset,
          requestedRunOffset,
        );
        if (generation !== requestGeneration.current) return;
        setWorkspace(next);
        setRoundingMode((current) =>
          next.rule?.supported_rounding.includes(current)
            ? current
            : (next.rule?.supported_rounding[0] ?? current),
        );
        setSelectedScenarioId((current) =>
          next.recent_scenarios.some(
            (scenario) => scenario.scenario_id === current,
          )
            ? current
            : (next.recent_scenarios[0]?.scenario_id ?? ""),
        );
      } catch {
        if (generation === requestGeneration.current) reportCommandFailure();
      } finally {
        if (generation === requestGeneration.current) setBusy(false);
      }
    },
    [reportCommandFailure, runOffset, scenarioOffset, tenderId],
  );

  useEffect(() => {
    void load();
    return () => {
      requestGeneration.current += 1;
    };
  }, [load, refreshToken]);

  const mutate = async (action: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await action();
      setScenarioOffset(0);
      setRunOffset(0);
      await load(0, 0);
      onTenderStateChange();
    } catch {
      reportCommandFailure();
    } finally {
      setBusy(false);
    }
  };

  const handleEvidenceSearch = async (event: FormEvent) => {
    event.preventDefault();
    if (!evidenceQuery.trim()) return;
    setBusy(true);
    try {
      const result = await searchEvidence(tenderId, evidenceQuery.trim());
      setEvidenceMatches(result.matches.slice(0, 8));
    } catch {
      reportCommandFailure();
    } finally {
      setBusy(false);
    }
  };

  const selectEvidence = (hit: EvidenceSearchHit) => {
    if (evidenceTarget === "quantity") setQuantityEvidence(hit);
    if (evidenceTarget === "unit_rate") setUnitRateEvidence(hit);
    if (evidenceTarget === "exchange_rate") setExchangeRateEvidence(hit);
  };

  const handleCreateScenario = async (event: FormEvent) => {
    event.preventDefault();
    const sameCurrency = rateCurrency === outputCurrency;
    await mutate(() =>
      createCalculationScenario({
        tender_id: tenderId,
        name: scenarioName.trim(),
        quantity_unit: quantityUnit,
        rate_basis_unit: rateBasisUnit,
        rate_currency: rateCurrency,
        exchange_rate: sameCurrency
          ? { state: "not_applicable", value: null, evidence: [] }
          : {
              state: exchangeRateState,
              value: exchangeRateState === "provided" ? exchangeRate : null,
              evidence:
                exchangeRateState !== "missing" && exchangeRateEvidence
                  ? [evidenceReference(exchangeRateEvidence)]
                  : [],
            },
        exchange_rate_effective_date:
          !sameCurrency && exchangeRateState === "provided"
            ? exchangeRateEffectiveDate
            : null,
        pricing_date: pricingDate,
        exchange_rate_type: sameCurrency ? null : exchangeRateType,
        output_currency: outputCurrency,
        precision,
        rounding_mode: roundingMode,
        rationale: scenarioRationale.trim(),
      }),
    );
  };

  const handleRunEstimator = async (event: FormEvent) => {
    event.preventDefault();
    const selected = workspace?.recent_scenarios.find(
      (scenario) => scenario.scenario_id === selectedScenarioId,
    );
    if (!selected || !quantityEvidence || !unitRateEvidence) return;
    await mutate(() =>
      runCostEstimatorCalculation({
        tender_id: tenderId,
        scenario_id: selected.scenario_id,
        scenario_version: selected.version,
        description: description.trim(),
        quantity_evidence: [evidenceReference(quantityEvidence)],
        unit_rate_evidence: [evidenceReference(unitRateEvidence)],
      }),
    );
  };

  const rule = workspace?.rule;
  const sameCurrency = rateCurrency === outputCurrency;
  const proposeRule = () =>
    proposeBoqCalculationRule({
      tender_id: tenderId,
      supported_rounding: ruleRoundingModes(ruleRoundingPolicy),
      change_rationale: ruleChangeRationale,
    });

  return (
    <section className="office-card">
      <div className="section-heading">
        <div>
          <p className="eyebrow">Deterministic commercial control</p>
          <h2>Controlled BOQ calculation</h2>
          <p>
            The Cost Estimator proposes evidence-backed inputs. Quantix alone
            performs the arithmetic and renders the immutable canonical run.
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

      {!rule ? (
        <div className="stacked-form">
          <label>
            Supported rounding policy
            <select
              value={ruleRoundingPolicy}
              onChange={(event) =>
                setRuleRoundingPolicy(event.target.value as RuleRoundingPolicy)
              }
            >
              <option value="both">Both exact policies</option>
              <option value="midpoint_away_from_zero">
                Midpoint away from zero only
              </option>
              <option value="midpoint_nearest_even">
                Midpoint nearest even only
              </option>
            </select>
          </label>
          <label>
            Rule proposal rationale
            <textarea
              value={ruleChangeRationale}
              onChange={(event) => setRuleChangeRationale(event.target.value)}
            />
          </label>
          <button
            disabled={busy || !runtimeReady || !ruleChangeRationale.trim()}
            onClick={() => void mutate(proposeRule)}
          >
            Propose controlled BOQ rule
          </button>
        </div>
      ) : (
        <div className="record-card">
          <p className="eyebrow">Calculation Rule v{rule.version}</p>
          <h3>{rule.name}</h3>
          <p>{rule.formula}</p>
          <p>
            Engine <code>{rule.engine_version}</code> · Manifest{" "}
            <code>{rule.manifest_sha256}</code>
          </p>
          <p>{rule.change_rationale}</p>
          <ul>
            {rule.deterministic_tests.map((test) => (
              <li key={test.case_name}>
                {test.case_name}: {test.actual_final_amount} (
                {test.passed ? "passed" : "failed"})
              </li>
            ))}
          </ul>
          {!rule.review ? (
            <button
              disabled={busy || !runtimeReady}
              onClick={() =>
                void mutate(() =>
                  runCalculationRuleReview({
                    tender_id: tenderId,
                    rule_id: rule.rule_id,
                    version: rule.version,
                  }),
                )
              }
            >
              Run independent cost review
            </button>
          ) : (
            <p>
              Independent review: <strong>{rule.review.outcome}</strong> ·{" "}
              {rule.review.findings.length} finding(s)
            </p>
          )}
          {rule.review?.outcome === "failed" ? (
            <div className="stacked-form">
              <label>
                Corrected rounding policy
                <select
                  value={ruleRoundingPolicy}
                  onChange={(event) =>
                    setRuleRoundingPolicy(
                      event.target.value as RuleRoundingPolicy,
                    )
                  }
                >
                  <option value="both">Both exact policies</option>
                  <option value="midpoint_away_from_zero">
                    Midpoint away from zero only
                  </option>
                  <option value="midpoint_nearest_even">
                    Midpoint nearest even only
                  </option>
                </select>
              </label>
              <label>
                Finding disposition and material change rationale
                <textarea
                  value={ruleChangeRationale}
                  onChange={(event) =>
                    setRuleChangeRationale(event.target.value)
                  }
                />
              </label>
              <button
                disabled={busy || !ruleChangeRationale.trim()}
                onClick={() => void mutate(proposeRule)}
              >
                Propose corrected next rule version
              </button>
            </div>
          ) : null}
          {rule.review?.outcome === "passed" && !rule.approval ? (
            <div className="stacked-form">
              <label>
                EITL activation rationale
                <textarea
                  value={approvalRationale}
                  onChange={(event) => setApprovalRationale(event.target.value)}
                />
              </label>
              <button
                disabled={busy || !approvalRationale.trim()}
                onClick={() =>
                  void mutate(() =>
                    approveCalculationRule({
                      tender_id: tenderId,
                      rule_id: rule.rule_id,
                      version: rule.version,
                      manifest_sha256: rule.manifest_sha256,
                      rationale: approvalRationale.trim(),
                    }),
                  )
                }
              >
                Activate exact reviewed rule
              </button>
            </div>
          ) : null}
          {rule.active ? (
            <p className="status-badge status-badge--ready">
              Active exact rule
            </p>
          ) : null}
        </div>
      )}

      {rule?.active ? (
        <>
          <form className="stacked-form" onSubmit={handleEvidenceSearch}>
            <div className="form-grid">
              <label>
                Evidence purpose
                <select
                  value={evidenceTarget}
                  onChange={(event) =>
                    setEvidenceTarget(event.target.value as EvidenceTarget)
                  }
                >
                  <option value="quantity">Quantity</option>
                  <option value="unit_rate">Unit rate</option>
                  <option value="exchange_rate">Exchange rate</option>
                </select>
              </label>
              <label>
                Find exact Evidence
                <input
                  value={evidenceQuery}
                  onChange={(event) => setEvidenceQuery(event.target.value)}
                  placeholder="Search parsed Tender evidence"
                />
              </label>
            </div>
            <button
              className="button-secondary"
              disabled={busy || !evidenceQuery.trim()}
            >
              Search Evidence
            </button>
          </form>
          <div className="record-list">
            {evidenceMatches.map((hit) => (
              <button
                type="button"
                className="button-secondary"
                key={`${hit.artifact_id}:${hit.version}:${hit.location.ordinal}`}
                onClick={() => selectEvidence(hit)}
              >
                Use for {statusCopy(evidenceTarget)} · {hit.package_path} ·{" "}
                {hit.location.structural_path} · {hit.location.original_text}
              </button>
            ))}
          </div>
          <p>
            Quantity Evidence:{" "}
            {quantityEvidence?.location.original_text ?? "not selected"}
          </p>
          <p>
            Unit-rate Evidence:{" "}
            {unitRateEvidence?.location.original_text ?? "not selected"}
          </p>
          <p>
            Exchange-rate Evidence:{" "}
            {exchangeRateEvidence?.location.original_text ??
              (sameCurrency ? "not applicable" : "not selected")}
          </p>

          <form className="stacked-form" onSubmit={handleCreateScenario}>
            <h3>Approve exact calculation scenario and policies</h3>
            <div className="form-grid">
              <label>
                Scenario name
                <input
                  value={scenarioName}
                  onChange={(event) => setScenarioName(event.target.value)}
                />
              </label>
              <label>
                Quantity unit
                <select
                  value={quantityUnit}
                  onChange={(event) => setQuantityUnit(event.target.value)}
                >
                  {units.map((unit) => (
                    <option key={unit}>{unit}</option>
                  ))}
                </select>
              </label>
              <label>
                Rate basis unit
                <select
                  value={rateBasisUnit}
                  onChange={(event) => setRateBasisUnit(event.target.value)}
                >
                  {units.map((unit) => (
                    <option key={unit}>{unit}</option>
                  ))}
                </select>
              </label>
              <label>
                Rate currency
                <select
                  value={rateCurrency}
                  onChange={(event) => setRateCurrency(event.target.value)}
                >
                  {currencies.map((currency) => (
                    <option key={currency}>{currency}</option>
                  ))}
                </select>
              </label>
              <label>
                Output currency
                <select
                  value={outputCurrency}
                  onChange={(event) => setOutputCurrency(event.target.value)}
                >
                  {currencies.map((currency) => (
                    <option key={currency}>{currency}</option>
                  ))}
                </select>
              </label>
              {!sameCurrency ? (
                <>
                  <label>
                    Exchange-rate input state
                    <select
                      value={exchangeRateState}
                      onChange={(event) =>
                        setExchangeRateState(
                          event.target.value as CalculationInputState,
                        )
                      }
                    >
                      <option value="provided">Provided</option>
                      <option value="missing">Missing</option>
                      <option value="unavailable">Unavailable</option>
                      <option value="ambiguous">Ambiguous</option>
                    </select>
                  </label>
                  {exchangeRateState === "provided" ? (
                    <>
                      <label>
                        {outputCurrency} per {rateCurrency}
                        <input
                          value={exchangeRate}
                          onChange={(event) =>
                            setExchangeRate(event.target.value)
                          }
                        />
                      </label>
                      <label>
                        Exchange-rate effective date
                        <input
                          type="date"
                          value={exchangeRateEffectiveDate}
                          onChange={(event) =>
                            setExchangeRateEffectiveDate(event.target.value)
                          }
                        />
                      </label>
                    </>
                  ) : null}
                  <label>
                    Exchange-rate type
                    <select
                      value={exchangeRateType}
                      onChange={(event) =>
                        setExchangeRateType(
                          event.target.value as ExchangeRateType,
                        )
                      }
                    >
                      <option value="spot">Spot</option>
                      <option value="contract">Contract</option>
                      <option value="budget">Budget</option>
                      <option value="central_bank">Central bank</option>
                    </select>
                  </label>
                </>
              ) : (
                <p>Exchange rate: not applicable.</p>
              )}
              <label>
                Pricing date
                <input
                  type="date"
                  value={pricingDate}
                  onChange={(event) => setPricingDate(event.target.value)}
                />
              </label>
              <label>
                Decimal places
                <input
                  type="number"
                  min={0}
                  max={12}
                  value={precision}
                  onChange={(event) => setPrecision(Number(event.target.value))}
                />
              </label>
              <label>
                Rounding policy
                <select
                  value={roundingMode}
                  onChange={(event) =>
                    setRoundingMode(
                      event.target.value as CalculationRoundingMode,
                    )
                  }
                >
                  {rule.supported_rounding.map((mode) => (
                    <option key={mode} value={mode}>
                      {statusCopy(mode)}
                    </option>
                  ))}
                </select>
              </label>
            </div>
            <label>
              EITL scenario rationale
              <textarea
                value={scenarioRationale}
                onChange={(event) => setScenarioRationale(event.target.value)}
              />
            </label>
            <button
              disabled={
                busy ||
                !scenarioName.trim() ||
                !scenarioRationale.trim() ||
                !pricingDate ||
                (!sameCurrency &&
                  exchangeRateState === "provided" &&
                  (!exchangeRate ||
                    !exchangeRateEvidence ||
                    !exchangeRateEffectiveDate))
              }
            >
              Approve immutable scenario
            </button>
          </form>

          <form className="stacked-form" onSubmit={handleRunEstimator}>
            <h3>Run approved Cost Estimator</h3>
            <label>
              Scenario
              <select
                value={selectedScenarioId}
                onChange={(event) => setSelectedScenarioId(event.target.value)}
              >
                {workspace?.recent_scenarios.map((scenario) => (
                  <option
                    key={scenario.scenario_id}
                    value={scenario.scenario_id}
                  >
                    {scenario.name} · {scenario.rate_currency}→
                    {scenario.output_currency} · {scenario.precision} dp
                  </option>
                ))}
              </select>
            </label>
            <div className="inline-actions">
              <button
                type="button"
                className="button-secondary"
                disabled={busy || scenarioOffset === 0}
                onClick={() =>
                  setScenarioOffset((current) => Math.max(0, current - 8))
                }
              >
                Newer scenarios
              </button>
              <button
                type="button"
                className="button-secondary"
                disabled={busy || !workspace?.has_older_scenarios}
                onClick={() => setScenarioOffset((current) => current + 8)}
              >
                Older scenarios
              </button>
            </div>
            <label>
              BOQ description
              <input
                value={description}
                onChange={(event) => setDescription(event.target.value)}
              />
            </label>
            <button
              disabled={
                busy ||
                !runtimeReady ||
                !selectedScenarioId ||
                !description.trim() ||
                !quantityEvidence ||
                !unitRateEvidence
              }
            >
              Extract inputs and calculate canonically
            </button>
          </form>
        </>
      ) : null}

      <div className="inline-actions">
        <button
          type="button"
          className="button-secondary"
          disabled={busy || runOffset === 0}
          onClick={() => setRunOffset((current) => Math.max(0, current - 8))}
        >
          Newer Calculation Runs
        </button>
        <button
          type="button"
          className="button-secondary"
          disabled={busy || !workspace?.has_older_runs}
          onClick={() => setRunOffset((current) => current + 8)}
        >
          Older Calculation Runs
        </button>
      </div>
      <div className="record-list">
        {workspace?.recent_runs.map((run) => (
          <article className="record-card" key={run.calculation_run_id}>
            <p className="eyebrow">{statusCopy(run.status)}</p>
            <h3>{run.description}</h3>
            <p>
              Cost Estimator run <code>{run.cost_estimator_run_id}</code> ·
              Scenario {run.scenario_name} v{run.scenario_version}
            </p>
            {run.final_amount ? (
              <p>
                Canonical value:{" "}
                <strong>
                  {run.final_amount} {run.output_currency}
                </strong>
              </p>
            ) : (
              <p>
                Not calculated: {statusCopy(run.diagnostic_code ?? run.status)}
              </p>
            )}
            <p>
              Quantity {run.quantity.value ?? statusCopy(run.quantity.state)}{" "}
              {run.quantity_unit} · Rate{" "}
              {run.unit_rate.value ?? statusCopy(run.unit_rate.state)}{" "}
              {run.rate_currency}/{run.rate_basis_unit}
            </p>
            <p>
              Unrounded source {run.unrounded_source_amount ?? "—"} · Unrounded
              output {run.unrounded_output_amount ?? "—"}
            </p>
            <p>
              Exchange rate {run.rate_currency}→{run.output_currency}:{" "}
              {run.exchange_rate.value ?? statusCopy(run.exchange_rate.state)} ·
              Rate <code>{run.exchange_rate_id}</code> v
              {run.exchange_rate_version}
            </p>
            <p>
              Pricing date {run.pricing_date} · Rate type{" "}
              {run.exchange_rate_type
                ? statusCopy(run.exchange_rate_type)
                : "not applicable"}
              {run.exchange_rate_effective_date
                ? ` · Effective ${run.exchange_rate_effective_date}`
                : ""}
            </p>
            <p>
              Rounding policy <code>{run.rounding_policy_id}</code> v
              {run.rounding_policy_version} · {statusCopy(run.rounding_mode)} ·{" "}
              {run.precision} decimal places
            </p>
            <p>
              Rule <code>{run.rule_id}</code> v{run.rule_version} · Engine{" "}
              <code>{run.engine_version}</code>
            </p>
            <details>
              <summary>Exact input provenance</summary>
              <p>
                Quantity: {run.quantity.evidence.length ? "" : "none"}
                {run.quantity.evidence.map((reference) => (
                  <code
                    key={`${reference.kind}:${reference.reference}:${reference.version}`}
                  >
                    {reference.kind}:{reference.reference}:v
                    {reference.version}{" "}
                  </code>
                ))}
              </p>
              <p>
                Unit rate: {run.unit_rate.evidence.length ? "" : "none"}
                {run.unit_rate.evidence.map((reference) => (
                  <code
                    key={`${reference.kind}:${reference.reference}:${reference.version}`}
                  >
                    {reference.kind}:{reference.reference}:v
                    {reference.version}{" "}
                  </code>
                ))}
              </p>
              <p>
                Exchange rate: {run.exchange_rate.evidence.length ? "" : "none"}
                {run.exchange_rate.evidence.map((reference) => (
                  <code
                    key={`${reference.kind}:${reference.reference}:${reference.version}`}
                  >
                    {reference.kind}:{reference.reference}:v
                    {reference.version}{" "}
                  </code>
                ))}
              </p>
            </details>
            <p>
              Scenario manifest <code>{run.scenario_manifest_sha256}</code> ·
              Run manifest <code>{run.manifest_sha256}</code>
            </p>
            {run.status === "completed" && !run.approval ? (
              <div className="stacked-form">
                <p>Host arithmetic complete — awaiting EITL approval.</p>
                <label>
                  Exact value approval rationale
                  <textarea
                    value={runApprovalDrafts[run.calculation_run_id] ?? ""}
                    onChange={(event) =>
                      setRunApprovalDrafts((current) => ({
                        ...current,
                        [run.calculation_run_id]: event.target.value,
                      }))
                    }
                  />
                </label>
                <button
                  disabled={
                    busy ||
                    !(runApprovalDrafts[run.calculation_run_id] ?? "").trim()
                  }
                  onClick={() =>
                    void mutate(() =>
                      approveControlledBoqCalculationRun({
                        tender_id: tenderId,
                        calculation_run_id: run.calculation_run_id,
                        manifest_sha256: run.manifest_sha256,
                        rationale: (
                          runApprovalDrafts[run.calculation_run_id] ?? ""
                        ).trim(),
                      }),
                    )
                  }
                >
                  Approve exact canonical value
                </button>
              </div>
            ) : null}
            {run.approval ? (
              <p className="status-badge status-badge--ready">
                EITL-approved exact value · {run.approval.rationale}
              </p>
            ) : null}
          </article>
        ))}
      </div>
    </section>
  );
}
