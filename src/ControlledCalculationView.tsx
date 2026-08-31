import { LoaderCircle } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import type { BasisOfEstimateVersion } from "./bindings/BasisOfEstimateVersion";
import type { CalculationScenarioVersion } from "./bindings/CalculationScenarioVersion";
import type { CalculationWorkspaceInspection } from "./bindings/CalculationWorkspaceInspection";
import type { ControlledBoqCalculationRun } from "./bindings/ControlledBoqCalculationRun";
import {
  inspectCalculationWorkspace,
  inspectEstimateWorkspace,
} from "./quantixHost";

interface ControlledCalculationViewProps {
  tenderId: string;
  reportCommandFailure: () => void;
  onClose: () => void;
}

function humanize(value: string): string {
  return value.split("_").join(" ");
}

function inputText(input: ControlledBoqCalculationRun["quantity"]): string {
  return input.value ?? humanize(input.state);
}

function decimalText(value: string | null, fallback: string): string {
  return value ?? fallback;
}

function scenarioKey(scenarioId: string, version: number): string {
  return `${scenarioId}:${version}`;
}

export function ControlledCalculationView({
  tenderId,
  reportCommandFailure,
  onClose,
}: ControlledCalculationViewProps) {
  const [workspace, setWorkspace] =
    useState<CalculationWorkspaceInspection | null>(null);
  const [basis, setBasis] = useState<BasisOfEstimateVersion | null>(null);
  const [loading, setLoading] = useState(true);
  const requestGeneration = useRef(0);

  const load = useCallback(async () => {
    const generation = ++requestGeneration.current;
    setLoading(true);
    try {
      const [nextWorkspace, nextEstimate] = await Promise.all([
        inspectCalculationWorkspace(tenderId, 0, 0),
        inspectEstimateWorkspace(tenderId),
      ]);
      if (generation !== requestGeneration.current) return;
      setWorkspace(nextWorkspace);
      setBasis(nextEstimate.basis);
    } catch {
      if (generation === requestGeneration.current) reportCommandFailure();
    } finally {
      if (generation === requestGeneration.current) setLoading(false);
    }
  }, [reportCommandFailure, tenderId]);

  useEffect(() => {
    void load();
    return () => {
      requestGeneration.current += 1;
    };
  }, [load]);

  const runsByScenario = new Map<
    string,
    {
      scenario: CalculationScenarioVersion | null;
      runs: ControlledBoqCalculationRun[];
    }
  >();
  for (const run of workspace?.recent_runs ?? []) {
    const key = scenarioKey(run.scenario_id, run.scenario_version);
    const group = runsByScenario.get(key) ?? {
      scenario:
        workspace?.recent_scenarios.find(
          (scenario) =>
            scenarioKey(scenario.scenario_id, scenario.version) === key,
        ) ?? null,
      runs: [],
    };
    group.runs.push(run);
    runsByScenario.set(key, group);
  }
  const scenarios = workspace?.recent_scenarios ?? [];

  const renderScenarioComparison = () => {
    if (scenarios.length < 2) return null;
    const rows: Array<{
      label: string;
      value: (scenario: CalculationScenarioVersion) => string;
    }> = [
      { label: "Quantity unit", value: (scenario) => scenario.quantity_unit },
      {
        label: "Rate basis unit",
        value: (scenario) => scenario.rate_basis_unit,
      },
      {
        label: "Rate currency to output currency",
        value: (scenario) =>
          `${scenario.rate_currency} to ${scenario.output_currency}`,
      },
      {
        label: "Exchange rate",
        value: (scenario) =>
          `${decimalText(scenario.exchange_rate.value, humanize(scenario.exchange_rate.state))}${
            scenario.exchange_rate_effective_date
              ? ` from ${scenario.exchange_rate_effective_date}`
              : ""
          }`,
      },
      { label: "Pricing date", value: (scenario) => scenario.pricing_date },
      {
        label: "Decimal places",
        value: (scenario) => String(scenario.precision),
      },
      {
        label: "Rounding mode",
        value: (scenario) => humanize(scenario.rounding_mode),
      },
      {
        label: "Rounding policy",
        value: (scenario) =>
          `${scenario.rounding_policy_id} v${scenario.rounding_policy_version}`,
      },
    ];
    return (
      <section
        className="controlled-calculations__section"
        aria-label="Scenario differences"
        data-testid="calculation-scenario-differences"
      >
        <h3>Scenario differences</h3>
        <p>
          Each scenario fixes its own units, currency, exchange rate, pricing
          date, and rounding. The exact values are shown side by side.
        </p>
        <table className="controlled-calculations__table">
          <thead>
            <tr>
              <th scope="col">Setting</th>
              {scenarios.map((scenario) => (
                <th
                  scope="col"
                  key={scenarioKey(scenario.scenario_id, scenario.version)}
                >
                  {scenario.name} · v{scenario.version}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.label}>
                <th scope="row">{row.label}</th>
                {scenarios.map((scenario) => (
                  <td key={scenarioKey(scenario.scenario_id, scenario.version)}>
                    {row.value(scenario)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </section>
    );
  };

  const runDetails = (run: ControlledBoqCalculationRun) => (
    <details className="controlled-calculations__run-details">
      <summary>Exact inputs and policy</summary>
      <dl>
        <div>
          <dt>Unrounded source amount</dt>
          <dd>{run.unrounded_source_amount ?? "not calculated"}</dd>
        </div>
        <div>
          <dt>Unrounded output amount</dt>
          <dd>{run.unrounded_output_amount ?? "not calculated"}</dd>
        </div>
        <div>
          <dt>Exchange rate</dt>
          <dd>
            {decimalText(
              run.exchange_rate.value,
              humanize(run.exchange_rate.state),
            )}{" "}
            {run.rate_currency} to {run.output_currency}
            {run.exchange_rate_effective_date
              ? ` from ${run.exchange_rate_effective_date}`
              : ""}
          </dd>
        </div>
        <div>
          <dt>Pricing date</dt>
          <dd>{run.pricing_date}</dd>
        </div>
        <div>
          <dt>Rounding</dt>
          <dd>
            {humanize(run.rounding_mode)} · {run.precision} decimal places ·
            policy {run.rounding_policy_id} v{run.rounding_policy_version}
          </dd>
        </div>
        <div>
          <dt>Rule</dt>
          <dd>
            <code>{run.rule_id}</code> v{run.rule_version} · engine{" "}
            {run.engine_version}
          </dd>
        </div>
        <div>
          <dt>Manifest</dt>
          <dd>
            <code>{run.manifest_sha256}</code>
          </dd>
        </div>
        <div>
          <dt>Quantity evidence</dt>
          <dd>
            {run.quantity.evidence.length === 0
              ? "none"
              : run.quantity.evidence.map((reference) => (
                  <code
                    key={`${reference.kind}:${reference.reference}:${reference.version}`}
                  >
                    {reference.kind}:{reference.reference}:v
                    {reference.version}{" "}
                  </code>
                ))}
          </dd>
        </div>
        <div>
          <dt>Unit-rate evidence</dt>
          <dd>
            {run.unit_rate.evidence.length === 0
              ? "none"
              : run.unit_rate.evidence.map((reference) => (
                  <code
                    key={`${reference.kind}:${reference.reference}:${reference.version}`}
                  >
                    {reference.kind}:{reference.reference}:v
                    {reference.version}{" "}
                  </code>
                ))}
          </dd>
        </div>
      </dl>
    </details>
  );

  const renderRuns = () => (
    <section
      className="controlled-calculations__section"
      aria-label="Calculation runs"
    >
      <h3>Calculation runs</h3>
      {runsByScenario.size === 0 ? (
        <p>No controlled calculation has been recorded yet.</p>
      ) : (
        [...runsByScenario.entries()].map(([key, group]) => (
          <div key={key} className="controlled-calculations__group">
            <h4>
              {group.scenario
                ? `${group.scenario.name} · v${group.scenario.version}`
                : key}
            </h4>
            <table className="controlled-calculations__table">
              <thead>
                <tr>
                  <th scope="col">Description</th>
                  <th scope="col">Quantity</th>
                  <th scope="col">Unit rate</th>
                  <th scope="col">Result</th>
                  <th scope="col">Status</th>
                  <th scope="col">Reliance</th>
                </tr>
              </thead>
              <tbody>
                {group.runs.map((run) => (
                  <tr key={run.calculation_run_id}>
                    <td>
                      {run.description}
                      {run.approval ? (
                        <span className="controlled-calculations__approved">
                          Approved for reliance · {run.approval.rationale}
                        </span>
                      ) : run.final_amount ? (
                        <span>
                          Host arithmetic complete; approval of the exact value
                          is next.
                        </span>
                      ) : null}
                      {runDetails(run)}
                    </td>
                    <td>
                      {inputText(run.quantity)} {run.quantity_unit}
                    </td>
                    <td>
                      {inputText(run.unit_rate)} {run.rate_currency}/
                      {run.rate_basis_unit}
                    </td>
                    <td>
                      {run.final_amount
                        ? `${run.final_amount} ${run.output_currency}`
                        : `not calculated · ${humanize(run.diagnostic_code ?? run.status)}`}
                    </td>
                    <td>{humanize(run.status)}</td>
                    <td>{run.approval ? "approved" : "not yet approved"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ))
      )}
    </section>
  );

  const renderBasis = () => {
    if (!basis) return null;
    return (
      <section
        className="controlled-calculations__section"
        aria-label="Basis of Estimate records"
      >
        <h3>Basis of Estimate</h3>
        <p>
          <code>
            {basis.basis_id} · v{basis.version}
          </code>{" "}
          totals{" "}
          <strong>
            {basis.total_amount} {basis.total_currency}
          </strong>{" "}
          {basis.relied_upon
            ? "and is approved for reliance."
            : "and is not yet approved for reliance."}
        </p>
        {basis.boq_rows.length > 0 ? (
          <table className="controlled-calculations__table">
            <thead>
              <tr>
                <th scope="col">BOQ row</th>
                <th scope="col">Description</th>
                <th scope="col">Disposition</th>
                <th scope="col">Calculation run</th>
              </tr>
            </thead>
            <tbody>
              {basis.boq_rows.map((row) => (
                <tr key={row.row_key}>
                  <td>
                    <code>{row.row_key}</code>
                  </td>
                  <td>{row.description}</td>
                  <td>{humanize(row.disposition)}</td>
                  <td>
                    {row.calculation_run_id ? (
                      <code>{row.calculation_run_id}</code>
                    ) : (
                      "none"
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : null}
        <details>
          <summary>Build-ups, quotations, and assumptions</summary>
          <ul>
            {basis.resource_build_ups.map((buildUp) => (
              <li key={buildUp.build_up_id}>
                {buildUp.description} · run{" "}
                <code>{buildUp.calculation_run_id}</code>
              </li>
            ))}
            {basis.quotations.map((quotation) => (
              <li key={quotation.quotation_id}>
                Quotation from {quotation.counterparty} in {quotation.currency}{" "}
                · normalized by run{" "}
                <code>{quotation.normalization_calculation_run_id}</code>
              </li>
            ))}
            {basis.allowances.map((allowance) => (
              <li key={allowance.allowance_id}>
                Allowance: {allowance.description} · query {allowance.query_id}{" "}
                v{allowance.query_version}
              </li>
            ))}
            {basis.material_assumptions.map((assumption) => (
              <li key={assumption.decision_id}>
                Assumption for query {assumption.query_id} v
                {assumption.query_version}: {assumption.rationale}
              </li>
            ))}
          </ul>
        </details>
      </section>
    );
  };

  return (
    <section
      className="controlled-calculations"
      data-testid="controlled-calculations"
      aria-labelledby="controlled-calculations-title"
    >
      <header className="controlled-calculations__header">
        <div>
          <p className="section-label">Controlled records</p>
          <h2 id="controlled-calculations-title">Controlled calculations</h2>
          <p>
            The exact quantities, rates, currencies, rounding, and results come
            from the controlled calculation records. These tables present them;
            the records stay the authority.
          </p>
        </div>
        <button
          type="button"
          className="manager-workspace__secondary"
          onClick={onClose}
        >
          Back
        </button>
      </header>
      {loading && !workspace ? (
        <p className="controlled-calculations__loading" role="status">
          <LoaderCircle size={16} aria-hidden="true" /> Loading the controlled
          calculations…
        </p>
      ) : null}
      {renderScenarioComparison()}
      {renderRuns()}
      {renderBasis()}
    </section>
  );
}
