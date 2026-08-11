import { FormEvent, useCallback, useEffect, useRef, useState } from "react";

import type { CalculationWorkspaceInspection } from "./bindings/CalculationWorkspaceInspection";
import type { EstimateWorkspaceInspection } from "./bindings/EstimateWorkspaceInspection";
import type { EvidenceSearchHit } from "./bindings/EvidenceSearchHit";
import type { TenderEvidenceReference } from "./bindings/TenderEvidenceReference";
import type { TenderQueryPage } from "./bindings/TenderQueryPage";
import {
  approveBasisOfEstimate,
  designateBoqTable,
  inspectCalculationWorkspace,
  inspectEstimateWorkspace,
  inspectTenderQueries,
  runBasisOfEstimateReview,
  runCostEstimatorBasis,
  searchEvidence,
} from "./quantixHost";

interface BasisOfEstimatePanelProps {
  tenderId: string;
  runtimeReady: boolean;
  refreshToken: number;
  reportCommandFailure: () => void;
  onTenderStateChange: () => void;
}

function evidenceKey(reference: TenderEvidenceReference) {
  return `${reference.artifact_id}:${reference.version}:${reference.ordinal}`;
}

function evidenceFromHit(hit: EvidenceSearchHit): TenderEvidenceReference {
  return {
    artifact_id: hit.artifact_id,
    version: hit.version,
    ordinal: hit.location.ordinal,
  };
}

function copy(value: string) {
  return value.split("_").join(" ");
}

interface EstimateNavigation {
  basisOffset: number;
  calculationOffset: number;
  boqCandidateCursor: string | null;
  boqCandidateCursorStack: (string | null)[];
  queryCursor: string | null;
  queryCursorStack: (string | null)[];
}

export function BasisOfEstimatePanel({
  tenderId,
  runtimeReady,
  refreshToken,
  reportCommandFailure,
  onTenderStateChange,
}: BasisOfEstimatePanelProps) {
  const [workspace, setWorkspace] = useState<EstimateWorkspaceInspection>();
  const [calculations, setCalculations] =
    useState<CalculationWorkspaceInspection>();
  const [queries, setQueries] = useState<TenderQueryPage>();
  const [basisOffset, setBasisOffset] = useState(0);
  const [calculationOffset, setCalculationOffset] = useState(0);
  const [boqCandidateCursor, setBoqCandidateCursor] = useState<string | null>(
    null,
  );
  const [boqCandidateCursorStack, setBoqCandidateCursorStack] = useState<
    (string | null)[]
  >([]);
  const [queryCursor, setQueryCursor] = useState<string | null>(null);
  const [queryCursorStack, setQueryCursorStack] = useState<(string | null)[]>(
    [],
  );
  const [busy, setBusy] = useState(false);
  const [evidenceQuery, setEvidenceQuery] = useState("");
  const [evidenceMatches, setEvidenceMatches] = useState<EvidenceSearchHit[]>(
    [],
  );
  const [quotationEvidence, setQuotationEvidence] = useState<
    TenderEvidenceReference[]
  >([]);
  const [calculationRunIds, setCalculationRunIds] = useState<string[]>([]);
  const [headerRows, setHeaderRows] = useState<Record<string, number>>({});
  const [approvalRationale, setApprovalRationale] = useState("");
  const requestGeneration = useRef(0);
  const searchGeneration = useRef(0);

  const load = useCallback(
    async (navigation: EstimateNavigation) => {
      const generation = ++requestGeneration.current;
      setBusy(true);
      try {
        const [nextWorkspace, nextCalculations, nextQueries] =
          await Promise.all([
            inspectEstimateWorkspace(
              tenderId,
              navigation.basisOffset,
              navigation.boqCandidateCursor,
            ),
            inspectCalculationWorkspace(
              tenderId,
              0,
              navigation.calculationOffset,
            ),
            inspectTenderQueries(tenderId, navigation.queryCursor, 8),
          ]);
        if (generation !== requestGeneration.current) return;
        setWorkspace(nextWorkspace);
        setCalculations(nextCalculations);
        setQueries(nextQueries);
        setBasisOffset(navigation.basisOffset);
        setCalculationOffset(navigation.calculationOffset);
        setBoqCandidateCursor(navigation.boqCandidateCursor);
        setBoqCandidateCursorStack(navigation.boqCandidateCursorStack);
        setQueryCursor(navigation.queryCursor);
        setQueryCursorStack(navigation.queryCursorStack);
      } catch {
        if (generation === requestGeneration.current) reportCommandFailure();
      } finally {
        if (generation === requestGeneration.current) setBusy(false);
      }
    },
    [reportCommandFailure, tenderId],
  );

  useEffect(() => {
    void load({
      basisOffset: 0,
      calculationOffset: 0,
      boqCandidateCursor: null,
      boqCandidateCursorStack: [],
      queryCursor: null,
      queryCursorStack: [],
    });
    return () => {
      requestGeneration.current += 1;
      searchGeneration.current += 1;
    };
  }, [load, refreshToken]);

  const resetAfterMutation = async () => {
    setQuotationEvidence([]);
    setCalculationRunIds([]);
    await load({
      basisOffset: 0,
      calculationOffset: 0,
      boqCandidateCursor: null,
      boqCandidateCursorStack: [],
      queryCursor: null,
      queryCursorStack: [],
    });
    onTenderStateChange();
  };

  const mutate = async (action: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await action();
      await resetAfterMutation();
    } catch {
      reportCommandFailure();
    } finally {
      setBusy(false);
    }
  };

  const handleEvidenceSearch = async (event: FormEvent) => {
    event.preventDefault();
    if (!evidenceQuery.trim()) return;
    const generation = ++searchGeneration.current;
    setBusy(true);
    try {
      const result = await searchEvidence(tenderId, evidenceQuery.trim());
      if (generation === searchGeneration.current) {
        setEvidenceMatches(result.matches.slice(0, 8));
      }
    } catch {
      if (generation === searchGeneration.current) reportCommandFailure();
    } finally {
      if (generation === searchGeneration.current) setBusy(false);
    }
  };

  const addQuotationEvidence = (hit: EvidenceSearchHit) => {
    const reference = evidenceFromHit(hit);
    const update = (current: TenderEvidenceReference[]) =>
      current.some((item) => evidenceKey(item) === evidenceKey(reference))
        ? current
        : [...current, reference];
    setQuotationEvidence(update);
  };

  const toggleCalculation = (runId: string) => {
    setCalculationRunIds((current) =>
      current.includes(runId)
        ? current.filter((candidate) => candidate !== runId)
        : [...current, runId],
    );
  };

  const basis = workspace?.basis;
  const reviewReady = basis?.current && !basis.review;
  const approvalReady =
    basis?.current &&
    basis.review?.outcome === "passed" &&
    basis.complete &&
    basis.reconciled &&
    !basis.approval;

  return (
    <section className="workspace-panel">
      <div className="section-heading">
        <div>
          <p className="eyebrow">Controlled commercial basis</p>
          <h2>BOQ account and Basis of Estimate</h2>
        </div>
        <button
          className="button-secondary"
          disabled={busy}
          onClick={() =>
            void load({
              basisOffset,
              calculationOffset,
              boqCandidateCursor,
              boqCandidateCursorStack,
              queryCursor,
              queryCursorStack,
            })
          }
        >
          Refresh
        </button>
      </div>
      <p>
        The Cost Estimator may structure evidence and approved Calculation Runs.
        Independent review and exact EITL approval remain separate authority
        boundaries.
      </p>

      <h3>Host BOQ table designation</h3>
      <p>
        Designate only actual BOQ tables and state their exact header depth. The
        Host derives every data row and all row cells; unrelated tables are not
        priced.
      </p>
      <div className="record-list">
        {workspace?.boq_table_candidates.map((candidate) => {
          const key = `${candidate.artifact_id}:${candidate.artifact_version}:${candidate.table_number}`;
          const selectedHeaderRows = headerRows[key] ?? 1;
          return (
            <div className="record-card" key={key}>
              <strong>
                {candidate.document_name} / table {candidate.table_number}
              </strong>
              <span>
                {candidate.row_count} parsed rows / {candidate.sample_text}
              </span>
              {candidate.designation ? (
                <span>
                  Designated with {candidate.designation.header_row_count}{" "}
                  header rows
                </span>
              ) : (
                <div className="button-row">
                  <label>
                    Header rows
                    <select
                      value={selectedHeaderRows}
                      onChange={(event) =>
                        setHeaderRows((current) => ({
                          ...current,
                          [key]: Number(event.target.value),
                        }))
                      }
                    >
                      {Array.from(
                        { length: Math.min(9, candidate.row_count) },
                        (_, value) => (
                          <option key={value} value={value}>
                            {value}
                          </option>
                        ),
                      )}
                    </select>
                  </label>
                  <button
                    className="button-secondary"
                    disabled={busy}
                    onClick={() =>
                      void mutate(() =>
                        designateBoqTable({
                          tender_id: tenderId,
                          artifact_id: candidate.artifact_id,
                          artifact_version: candidate.artifact_version,
                          table_number: candidate.table_number,
                          header_row_count: selectedHeaderRows,
                        }),
                      )
                    }
                  >
                    Designate BOQ table
                  </button>
                </div>
              )}
            </div>
          );
        })}
      </div>
      <div className="button-row">
        <button
          className="button-secondary"
          disabled={busy || boqCandidateCursorStack.length === 0}
          onClick={() => {
            const stack = [...boqCandidateCursorStack];
            const previous = stack.pop() ?? null;
            void load({
              basisOffset,
              calculationOffset,
              boqCandidateCursor: previous,
              boqCandidateCursorStack: stack,
              queryCursor,
              queryCursorStack,
            });
          }}
        >
          Previous BOQ tables
        </button>
        <button
          className="button-secondary"
          disabled={busy || !workspace?.boq_table_candidate_next_cursor}
          onClick={() => {
            const next = workspace?.boq_table_candidate_next_cursor ?? null;
            void load({
              basisOffset,
              calculationOffset,
              boqCandidateCursor: next,
              boqCandidateCursorStack: [
                ...boqCandidateCursorStack,
                boqCandidateCursor,
              ],
              queryCursor,
              queryCursorStack,
            });
          }}
        >
          Next BOQ tables
        </button>
      </div>

      <form className="stacked-form" onSubmit={handleEvidenceSearch}>
        <label>
          Find quotation Evidence
          <input
            value={evidenceQuery}
            onChange={(event) => setEvidenceQuery(event.target.value)}
            placeholder="Search supplier, subcontractor, and rate-source Evidence"
          />
        </label>
        <button
          className="button-secondary"
          disabled={busy || !evidenceQuery.trim()}
        >
          Search Evidence
        </button>
      </form>
      <div className="record-list">
        {evidenceMatches.map((hit) => (
          <article
            className="record-card"
            key={`${hit.artifact_id}:${hit.version}:${hit.location.ordinal}`}
          >
            <p className="eyebrow">
              {hit.package_path} / {hit.location.structural_path}
            </p>
            <p>{hit.location.original_text}</p>
            {hit.location.translated_text ? (
              <p>{hit.location.translated_text}</p>
            ) : null}
            <button
              type="button"
              className="button-secondary"
              onClick={() => addQuotationEvidence(hit)}
            >
              Add as quotation Evidence
            </button>
          </article>
        ))}
      </div>
      <div>
        <p>
          BOQ inventory is derived by the Host from every row of the exact
          designated current BOQ tables; the Cost Estimator cannot cherry-pick
          that account.
        </p>
        <h3>Quotation Evidence ({quotationEvidence.length})</h3>
        {quotationEvidence.map((reference) => (
          <button
            type="button"
            className="button-secondary"
            key={evidenceKey(reference)}
            onClick={() =>
              setQuotationEvidence((current) =>
                current.filter(
                  (candidate) =>
                    evidenceKey(candidate) !== evidenceKey(reference),
                ),
              )
            }
          >
            Remove {reference.artifact_id.slice(0, 8)} v{reference.version} #
            {reference.ordinal}
          </button>
        ))}
      </div>

      <div className="section-heading">
        <h3>Approved Calculation Runs</h3>
        <div className="button-row">
          <button
            className="button-secondary"
            disabled={busy || calculationOffset === 0}
            onClick={() => {
              const next = Math.max(0, calculationOffset - 8);
              void load({
                basisOffset,
                calculationOffset: next,
                boqCandidateCursor,
                boqCandidateCursorStack,
                queryCursor,
                queryCursorStack,
              });
            }}
          >
            Newer
          </button>
          <button
            className="button-secondary"
            disabled={busy || !calculations?.has_older_runs}
            onClick={() => {
              const next = calculationOffset + 8;
              void load({
                basisOffset,
                calculationOffset: next,
                boqCandidateCursor,
                boqCandidateCursorStack,
                queryCursor,
                queryCursorStack,
              });
            }}
          >
            Older
          </button>
        </div>
      </div>
      <div className="record-list">
        {calculations?.recent_runs.map((run) => {
          const eligible = run.status === "completed" && Boolean(run.approval);
          return (
            <label className="record-card" key={run.calculation_run_id}>
              <input
                type="checkbox"
                disabled={!eligible}
                checked={calculationRunIds.includes(run.calculation_run_id)}
                onChange={() => toggleCalculation(run.calculation_run_id)}
              />
              <strong>{run.description}</strong>
              <span>
                {run.final_amount ?? "no value"} {run.output_currency} /{" "}
                {copy(run.status)}
                {run.approval ? " / EITL approved" : " / not approved"}
              </span>
            </label>
          );
        })}
      </div>

      <div className="section-heading">
        <h3>Exact Tender Query versions</h3>
        <div className="button-row">
          <button
            className="button-secondary"
            disabled={busy || queryCursorStack.length === 0}
            onClick={() => {
              const stack = [...queryCursorStack];
              const previous = stack.pop() ?? null;
              void load({
                basisOffset,
                calculationOffset,
                boqCandidateCursor,
                boqCandidateCursorStack,
                queryCursor: previous,
                queryCursorStack: stack,
              });
            }}
          >
            Previous
          </button>
          <button
            className="button-secondary"
            disabled={busy || !queries?.next_cursor}
            onClick={() => {
              const next = queries?.next_cursor ?? null;
              void load({
                basisOffset,
                calculationOffset,
                boqCandidateCursor,
                boqCandidateCursorStack,
                queryCursor: next,
                queryCursorStack: [...queryCursorStack, queryCursor],
              });
            }}
          >
            Next
          </button>
        </div>
      </div>
      <div className="record-list">
        {queries?.items.map((query) => {
          return (
            <div
              className="record-card"
              key={`${query.query_id}:${query.version}`}
            >
              <strong>{query.question}</strong>
              <span>
                {copy(query.status)} /{" "}
                {query.material ? "material" : "nonmaterial"}
                {query.approved_treatment
                  ? ` / ${copy(query.approved_treatment.treatment)}`
                  : " / unresolved"}
              </span>
            </div>
          );
        })}
      </div>

      <button
        disabled={busy || !runtimeReady || calculationRunIds.length === 0}
        onClick={() =>
          void mutate(() =>
            runCostEstimatorBasis({
              tender_id: tenderId,
              quotation_evidence: quotationEvidence,
              calculation_run_ids: calculationRunIds,
            }),
          )
        }
      >
        Run Cost Estimator for exact Basis
      </button>

      <div className="section-heading">
        <h3>Versioned Basis history</h3>
        <div className="button-row">
          <button
            className="button-secondary"
            disabled={busy || !workspace?.has_newer_basis}
            onClick={() => {
              const next = Math.max(0, basisOffset - 1);
              void load({
                basisOffset: next,
                calculationOffset,
                boqCandidateCursor,
                boqCandidateCursorStack,
                queryCursor,
                queryCursorStack,
              });
            }}
          >
            Newer
          </button>
          <button
            className="button-secondary"
            disabled={busy || !workspace?.has_older_basis}
            onClick={() => {
              const next = basisOffset + 1;
              void load({
                basisOffset: next,
                calculationOffset,
                boqCandidateCursor,
                boqCandidateCursorStack,
                queryCursor,
                queryCursorStack,
              });
            }}
          >
            Older
          </button>
        </div>
      </div>

      {basis ? (
        <article className="record-card">
          <p className="eyebrow">
            Basis v{basis.version} / Tender revision {basis.tender_revision}
          </p>
          <h3>{basis.scope}</h3>
          <p>
            Pricing date {basis.pricing_date} / Total {basis.total_amount}{" "}
            {basis.total_currency}
          </p>
          <p>
            <strong>{basis.complete ? "Complete" : "Incomplete"}</strong> /{" "}
            <strong>{basis.reconciled ? "Reconciled" : "Unreconciled"}</strong>{" "}
            /{" "}
            {basis.relied_upon
              ? "Approved for reliance"
              : "Not approved for reliance"}
          </p>
          {basis.blockers.length ? (
            <p>Blockers: {basis.blockers.map(copy).join(", ")}</p>
          ) : null}
          <p>
            Currencies: {basis.currencies.join(", ")} / Design maturity:{" "}
            {basis.design_maturity}
          </p>
          <p>
            Host BOQ inventory <code>{basis.boq_inventory_sha256}</code> / Query
            inventory <code>{basis.query_inventory_sha256}</code>
          </p>
          {basis.supersedes_basis_manifest_sha256 ? (
            <p>
              Supersedes Basis manifest{" "}
              <code>{basis.supersedes_basis_manifest_sha256}</code>
              {basis.remediates_review_manifest_sha256
                ? ` / remediates failed review ${basis.remediates_review_manifest_sha256}`
                : ""}
            </p>
          ) : null}
          <details>
            <summary>Basis states and exclusions</summary>
            <p>Taxes: {basis.taxes.join("; ") || "none recorded"}</p>
            <p>Rate sources: {basis.rate_sources.join("; ")}</p>
            <p>Productivity: {basis.productivity.join("; ")}</p>
            <p>Gaps: {basis.gaps.join("; ") || "none"}</p>
            <p>Exclusions: {basis.exclusions.join("; ") || "none"}</p>
          </details>
          <details open>
            <summary>BOQ account ({basis.boq_rows.length})</summary>
            {basis.boq_rows.map((row) => (
              <div className="record-card" key={row.row_key}>
                <strong>
                  {row.row_key}: {row.description}
                </strong>
                <p>
                  {copy(row.disposition)} / Evidence{" "}
                  {row.evidence
                    .map(
                      (reference) =>
                        `${reference.artifact_id.slice(0, 8)} v${reference.version} #${reference.ordinal}`,
                    )
                    .join(", ")}
                </p>
                <p>
                  Calculation {row.calculation_run_id ?? "none"} / Queries{" "}
                  {row.affected_queries
                    .map(
                      (query) =>
                        `${query.query_id.slice(0, 8)} v${query.version}`,
                    )
                    .join(", ") || "none"}
                </p>
              </div>
            ))}
          </details>
          <details>
            <summary>
              Cost Breakdown Structure ({basis.cbs_components.length})
            </summary>
            {basis.cbs_components.map((component) => (
              <p key={component.component_id}>
                <strong>{component.cost_code}</strong> /{" "}
                {component.work_package} / {copy(component.category)} /{" "}
                {component.boq_row_keys.join(", ")} / resource build-ups{" "}
                {component.resource_build_up_ids.join(", ")}
              </p>
            ))}
          </details>
          <details>
            <summary>
              Immutable resource build-ups ({basis.resource_build_ups.length})
            </summary>
            {basis.resource_build_ups.map((buildUp) => (
              <p key={buildUp.build_up_id}>
                <strong>{buildUp.description}</strong> /{" "}
                {copy(buildUp.category)}
                {" / component "}
                {buildUp.cbs_component_id} / approved Calculation Run{" "}
                {buildUp.calculation_run_id}
              </p>
            ))}
          </details>
          <details open>
            <summary>Host aggregate Calculation Run</summary>
            <p>
              {basis.aggregate_calculation.final_amount}{" "}
              {basis.aggregate_calculation.currency} /{" "}
              {basis.aggregate_calculation.engine_version} /{" "}
              {basis.aggregate_calculation.approved_for_reliance
                ? "approved for reliance"
                : "awaiting exact Basis approval"}
            </p>
            <p>
              Aggregate run {basis.aggregate_calculation.aggregate_run_id} /
              manifest{" "}
              <code>{basis.aggregate_calculation.manifest_sha256}</code>
            </p>
            <p>
              Rule {basis.aggregate_calculation.rule_id} v
              {basis.aggregate_calculation.rule_version} / approval{" "}
              {basis.aggregate_calculation.rule_approval_id} / scenario{" "}
              {basis.aggregate_calculation.scenario_id} v
              {basis.aggregate_calculation.scenario_version} / precision{" "}
              {basis.aggregate_calculation.precision} / rounding{" "}
              {copy(basis.aggregate_calculation.rounding_mode)}
            </p>
            <p>
              Independent comparison run{" "}
              {basis.aggregate_calculation.comparison_total_calculation_run_id}{" "}
              / {basis.aggregate_calculation.comparison_total_amount}{" "}
              {basis.aggregate_calculation.currency} / manifest{" "}
              <code>
                {basis.aggregate_calculation.comparison_total_manifest_sha256}
              </code>
            </p>
            {basis.aggregate_calculation.inputs.map((input) => (
              <p key={input.build_up_id}>
                Build-up {input.build_up_id} / component{" "}
                {input.cbs_component_id} / {input.amount} {input.currency} / run{" "}
                {input.calculation_run_id} / manifest{" "}
                <code>{input.calculation_manifest_sha256}</code>
              </p>
            ))}
          </details>
          <details>
            <summary>
              Supplier and subcontract quotations ({basis.quotations.length})
            </summary>
            {basis.quotations.map((quotation) => (
              <p key={quotation.quotation_id}>
                <strong>{quotation.counterparty}</strong> /{" "}
                {copy(quotation.kind)} / {quotation.exact_scope} /{" "}
                {quotation.currency} / quoted {quotation.quotation_date} / valid
                until {quotation.valid_until} / rows{" "}
                {quotation.covered_boq_row_keys.join(", ")} / exclusions{" "}
                {quotation.exclusions.join("; ") || "none"} / assumptions{" "}
                {quotation.comparison_assumptions.join("; ")} / normalization
                run {quotation.normalization_calculation_run_id} / Evidence{" "}
                {quotation.evidence.artifact_id.slice(0, 8)} v
                {quotation.evidence.version} #{quotation.evidence.ordinal}
              </p>
            ))}
          </details>
          <details>
            <summary>
              Allowances and approved material assumptions (
              {basis.allowances.length + basis.material_assumptions.length})
            </summary>
            {basis.allowances.map((allowance) => (
              <p key={allowance.allowance_id}>
                {allowance.description} / component {allowance.cbs_component_id}{" "}
                / build-up {allowance.resource_build_up_id} / Query{" "}
                {allowance.query_id.slice(0, 8)} v{allowance.query_version} /
                decision {allowance.decision_id} / Evidence{" "}
                {allowance.evidence
                  .map(
                    (reference) =>
                      `${reference.artifact_id.slice(0, 8)} v${reference.version} #${reference.ordinal}`,
                  )
                  .join(", ")}{" "}
                / {allowance.rationale}
              </p>
            ))}
            {basis.material_assumptions.map((assumption) => (
              <p key={assumption.decision_id}>
                Query {assumption.query_id.slice(0, 8)} v
                {assumption.query_version} / {copy(assumption.treatment)} /{" "}
                {assumption.rationale} / {assumption.treatment_details}
              </p>
            ))}
          </details>
          <p>
            Manifest <code>{basis.manifest_sha256}</code>
          </p>

          {reviewReady ? (
            <button
              disabled={busy || !runtimeReady}
              onClick={() =>
                void mutate(() =>
                  runBasisOfEstimateReview({
                    tender_id: tenderId,
                    basis_id: basis.basis_id,
                    version: basis.version,
                  }),
                )
              }
            >
              Run independent exact Basis review
            </button>
          ) : null}
          {basis.review ? (
            <div>
              <p>
                Independent review:{" "}
                <strong>{copy(basis.review.outcome)}</strong> / reviewer{" "}
                {basis.review.reviewer_profile_id} v
                {basis.review.reviewer_profile_version}
              </p>
              {basis.review.findings.map((finding) => (
                <p key={finding.code}>
                  <strong>{copy(finding.code)}</strong>: {finding.summary} /
                  rows{" "}
                  {finding.affected_boq_row_keys.join(", ") || "basis-wide"}
                </p>
              ))}
            </div>
          ) : null}
          {approvalReady ? (
            <div className="stacked-form">
              <label>
                EITL approval rationale for this exact reviewed Basis
                <textarea
                  value={approvalRationale}
                  onChange={(event) => setApprovalRationale(event.target.value)}
                />
              </label>
              <button
                disabled={busy || !approvalRationale.trim()}
                onClick={() =>
                  void mutate(() =>
                    approveBasisOfEstimate({
                      tender_id: tenderId,
                      basis_id: basis.basis_id,
                      version: basis.version,
                      manifest_sha256: basis.manifest_sha256,
                      rationale: approvalRationale.trim(),
                    }),
                  )
                }
              >
                Approve exact Basis for reliance
              </button>
            </div>
          ) : null}
          {basis.approval ? (
            <p className="status-badge status-badge--ready">
              EITL approved by {basis.approval.approved_by} as{" "}
              {copy(basis.approval.acting_role)} / {basis.approval.rationale}
            </p>
          ) : null}
        </article>
      ) : (
        <p>No Basis of Estimate has been published.</p>
      )}
    </section>
  );
}
