import { useCallback, useEffect, useMemo, useState } from "react";

import type { BidDecisionPackageInspection } from "./bindings/BidDecisionPackageInspection";
import type { BidDecisionPackageRecordCategory } from "./bindings/BidDecisionPackageRecordCategory";
import type { BidDecisionPackageRecordPage } from "./bindings/BidDecisionPackageRecordPage";
import type { ComplianceDisposition } from "./bindings/ComplianceDisposition";
import type { ComplianceDispositionUpdate } from "./bindings/ComplianceDispositionUpdate";
import type { ComplianceMatrixPage } from "./bindings/ComplianceMatrixPage";
import type { ComplianceMatrixRow } from "./bindings/ComplianceMatrixRow";
import type { ManagerCapabilityDemandInput } from "./bindings/ManagerCapabilityDemandInput";
import {
  createBidDecisionPackage,
  inspectBidDecisionPackageRecords,
  inspectComplianceMatrix,
  inspectCurrentBidDecisionPackage,
  runBidDecisionPackageReview,
} from "./quantixHost";

interface BidDecisionPanelProps {
  tenderId: string;
  runtimeReady: boolean;
  reportCommandFailure: () => void;
}

const PAGE_SIZE = 4;
const humanize = (value: string) => value.replace(/_/g, " ");
const recordCategoryCopy: Record<BidDecisionPackageRecordCategory, string> = {
  project_fingerprint: "Project Fingerprint",
  risk: "Risks",
  opportunity: "Opportunities",
  assumption: "Assumptions",
  unresolved_query: "Unresolved Tender Queries",
};

function managerDemands(
  packageInspection: BidDecisionPackageInspection,
): ManagerCapabilityDemandInput[] {
  return packageInspection.capability_demands
    .filter((demand) => demand.classification === "manager_added")
    .map((demand) => ({
      capability: demand.capability,
      rationale: demand.rationale,
      triggering_record: demand.triggering_record,
    }));
}

function MatrixRowEditor({
  row,
  disabled,
  onSave,
}: {
  row: ComplianceMatrixRow;
  disabled: boolean;
  onSave: (update: ComplianceDispositionUpdate) => Promise<void>;
}) {
  const [disposition, setDisposition] = useState<ComplianceDisposition>(
    row.disposition,
  );
  const [responsibility, setResponsibility] = useState(row.responsibility);
  const [plannedTreatment, setPlannedTreatment] = useState(
    row.planned_treatment,
  );

  useEffect(() => {
    setDisposition(row.disposition);
    setResponsibility(row.responsibility);
    setPlannedTreatment(row.planned_treatment);
  }, [row]);

  const exactEvidence = row.record.fields.flatMap((field) => field.evidence);

  return (
    <article className="tender-record-card" data-trust={row.record.trust_class}>
      <header>
        <div>
          <p className="section-label">
            Matrix row {row.ordinal} · {humanize(row.record.kind)}
          </p>
          <h5>{row.record.title}</h5>
        </div>
        <span className="record-trust-badge">
          {humanize(row.record.verification_status)}
        </span>
      </header>
      <p className="record-identity">
        Exact record {row.record.record_id.slice(0, 12)} · v{row.record.version}
      </p>
      {exactEvidence.length > 0 ? (
        <ul className="bid-evidence-list" aria-label="Exact row Evidence">
          {exactEvidence.slice(0, 4).map((evidence) => (
            <li
              key={`${evidence.reference.artifact_id}:${evidence.reference.version}:${evidence.reference.ordinal}`}
            >
              {evidence.package_path} · {evidence.location.structural_path}
            </li>
          ))}
        </ul>
      ) : (
        <p className="catalogue-error">
          No exact authoritative Evidence is bound.
        </p>
      )}
      {row.blocker_codes.length > 0 ? (
        <p className="catalogue-error" role="status">
          Blocked: {humanize(row.blocker_codes.join(", "))}
        </p>
      ) : null}
      <div className="bid-matrix-editor">
        <label>
          Complete disposition
          <select
            value={disposition}
            onChange={(event) =>
              setDisposition(event.target.value as ComplianceDisposition)
            }
            disabled={disabled}
          >
            <option value="unresolved">Unresolved</option>
            <option value="comply">Comply</option>
            <option value="comply_with_qualification">
              Comply with qualification
            </option>
            <option value="deviation">Deviation</option>
            <option value="not_applicable">Not applicable</option>
          </select>
        </label>
        <label>
          Responsibility
          <input
            value={responsibility}
            onChange={(event) => setResponsibility(event.target.value)}
            maxLength={200}
            disabled={disabled}
          />
        </label>
        <label>
          Planned treatment
          <textarea
            value={plannedTreatment}
            onChange={(event) => setPlannedTreatment(event.target.value)}
            maxLength={2000}
            rows={3}
            disabled={disabled}
          />
        </label>
      </div>
      <button
        type="button"
        className="button-secondary"
        disabled={
          disabled || !responsibility.trim() || !plannedTreatment.trim()
        }
        onClick={() =>
          void onSave({
            record: {
              record_id: row.record.record_id,
              version: row.record.version,
            },
            disposition,
            responsibility: responsibility.trim(),
            planned_treatment: plannedTreatment.trim(),
            affected_work: row.affected_work,
            uncertainty: row.uncertainty,
            related_records: row.related_records,
          })
        }
      >
        Save as new package version
      </button>
    </article>
  );
}

export function BidDecisionPanel({
  tenderId,
  runtimeReady,
  reportCommandFailure,
}: BidDecisionPanelProps) {
  const [packageInspection, setPackageInspection] =
    useState<BidDecisionPackageInspection | null>(null);
  const [matrixPage, setMatrixPage] = useState<ComplianceMatrixPage>({
    rows: [],
    next_ordinal: null,
  });
  const [matrixAfter, setMatrixAfter] = useState<number | null>(null);
  const [matrixHistory, setMatrixHistory] = useState<(number | null)[]>([]);
  const [recordCategory, setRecordCategory] =
    useState<BidDecisionPackageRecordCategory>("project_fingerprint");
  const [recordPage, setRecordPage] = useState<BidDecisionPackageRecordPage>({
    records: [],
    next_ordinal: null,
  });
  const [recordAfter, setRecordAfter] = useState<number | null>(null);
  const [recordHistory, setRecordHistory] = useState<(number | null)[]>([]);
  const [busy, setBusy] = useState(false);
  const [capability, setCapability] = useState("");
  const [capabilityRationale, setCapabilityRationale] = useState("");

  const loadMatrix = useCallback(
    async (
      packageValue: BidDecisionPackageInspection,
      after: number | null,
    ) => {
      setMatrixPage(
        await inspectComplianceMatrix(
          tenderId,
          packageValue.package_id,
          packageValue.version,
          after,
          PAGE_SIZE,
        ),
      );
      setMatrixAfter(after);
    },
    [tenderId],
  );

  const loadRecords = useCallback(
    async (
      packageValue: BidDecisionPackageInspection,
      category: BidDecisionPackageRecordCategory,
      after: number | null,
    ) => {
      setRecordPage(
        await inspectBidDecisionPackageRecords(
          tenderId,
          packageValue.package_id,
          packageValue.version,
          category,
          after,
          PAGE_SIZE,
        ),
      );
      setRecordAfter(after);
    },
    [tenderId],
  );

  const refresh = useCallback(async () => {
    try {
      const current = await inspectCurrentBidDecisionPackage(tenderId);
      setPackageInspection(current);
      setMatrixHistory([]);
      setRecordHistory([]);
      if (current) {
        await Promise.all([
          loadMatrix(current, null),
          loadRecords(current, recordCategory, null),
        ]);
      } else {
        setMatrixPage({ rows: [], next_ordinal: null });
        setRecordPage({ records: [], next_ordinal: null });
      }
    } catch {
      reportCommandFailure();
    }
  }, [loadMatrix, loadRecords, recordCategory, reportCommandFailure, tenderId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const mutate = useCallback(
    async (operation: () => Promise<unknown>) => {
      setBusy(true);
      try {
        await operation();
        await refresh();
      } catch {
        reportCommandFailure();
      } finally {
        setBusy(false);
      }
    },
    [refresh, reportCommandFailure],
  );

  const savedManagerDemands = useMemo(
    () => (packageInspection ? managerDemands(packageInspection) : []),
    [packageInspection],
  );

  const saveRow = async (update: ComplianceDispositionUpdate) => {
    if (!packageInspection) return;
    await mutate(() =>
      createBidDecisionPackage(
        tenderId,
        packageInspection.version,
        [update],
        savedManagerDemands,
      ),
    );
  };

  return (
    <section
      className="tender-records bid-decision"
      aria-labelledby="bid-decision-title"
    >
      <div className="tender-records__heading">
        <div>
          <p className="section-label">Pre-bid decision control</p>
          <h4 id="bid-decision-title">
            Compliance Matrix &amp; Bid Decision Package
          </h4>
        </div>
        {packageInspection ? (
          <span
            className={
              packageInspection.decision_gate_ready
                ? "status-badge status-badge--ready"
                : "status-badge"
            }
          >
            {packageInspection.decision_gate_ready
              ? "Decision gate ready"
              : `${packageInspection.blocker_count} blockers`}
          </span>
        ) : null}
      </div>
      {!packageInspection ? (
        <div className="catalogue-panel">
          <p>
            Build the first exact package from every current obligation. Missing
            dispositions and unverified Evidence remain explicit blockers.
          </p>
          <button
            type="button"
            disabled={busy}
            onClick={() =>
              void mutate(() =>
                createBidDecisionPackage(tenderId, null, [], []),
              )
            }
          >
            Build Bid Decision Package
          </button>
        </div>
      ) : (
        <>
          <div className="bid-decision-summary">
            <dl>
              <div>
                <dt>Exact version</dt>
                <dd>v{packageInspection.version}</dd>
              </div>
              <div>
                <dt>Manifest</dt>
                <dd>{packageInspection.manifest_sha256.slice(0, 12)}…</dd>
              </div>
              <div>
                <dt>Matrix</dt>
                <dd>{packageInspection.compliance_row_count} rows</dd>
              </div>
              <div>
                <dt>Fingerprint</dt>
                <dd>
                  {packageInspection.project_fingerprint_count} verified signals
                </dd>
              </div>
              <div>
                <dt>Recommendation</dt>
                <dd>{packageInspection.recommendation.outcome}</dd>
              </div>
              <div>
                <dt>Independent Review</dt>
                <dd>{packageInspection.review?.outcome ?? "not reviewed"}</dd>
              </div>
              <div>
                <dt>Resource implications</dt>
                <dd>{packageInspection.resource_implications.length}</dd>
              </div>
            </dl>
            <p>{packageInspection.recommendation.rationale}</p>
          </div>

          {packageInspection.blockers.length > 0 ? (
            <ul
              className="bid-blocker-list"
              aria-label="Decision gate blockers"
            >
              {packageInspection.blockers.map((blocker, index) => (
                <li
                  key={`${blocker.code}:${blocker.record?.record_id ?? index}`}
                >
                  <strong>{humanize(blocker.code)}</strong> · {blocker.summary}
                </li>
              ))}
            </ul>
          ) : null}

          <section
            className="bid-capabilities"
            aria-labelledby="capability-demand-title"
          >
            <h5 id="capability-demand-title">Capability Demands</h5>
            <ul>
              {packageInspection.capability_demands.map((demand, index) => (
                <li
                  key={`${demand.classification}:${demand.capability}:${index}`}
                >
                  <strong>{demand.capability}</strong> ·{" "}
                  {humanize(demand.classification)}
                  {demand.supported ? " · supported" : " · Capability Gap"}
                </li>
              ))}
            </ul>
            <form
              className="bid-capability-form"
              onSubmit={(event) => {
                event.preventDefault();
                if (!capability.trim() || !capabilityRationale.trim()) return;
                void mutate(() =>
                  createBidDecisionPackage(
                    tenderId,
                    packageInspection.version,
                    [],
                    [
                      ...savedManagerDemands,
                      {
                        capability: capability.trim(),
                        rationale: capabilityRationale.trim(),
                        triggering_record: null,
                      },
                    ],
                  ),
                ).then(() => {
                  setCapability("");
                  setCapabilityRationale("");
                });
              }}
            >
              <label>
                Manager-added capability key
                <input
                  value={capability}
                  onChange={(event) => setCapability(event.target.value)}
                  placeholder="facade_engineering"
                  pattern="[a-z0-9_-]+"
                  maxLength={100}
                  disabled={busy}
                />
              </label>
              <label>
                Rationale
                <input
                  value={capabilityRationale}
                  onChange={(event) =>
                    setCapabilityRationale(event.target.value)
                  }
                  maxLength={1000}
                  disabled={busy}
                />
              </label>
              <button
                type="submit"
                className="button-secondary"
                disabled={
                  busy || !capability.trim() || !capabilityRationale.trim()
                }
              >
                Add demand in new version
              </button>
            </form>
          </section>

          <section aria-labelledby="compliance-matrix-title">
            <h5 id="compliance-matrix-title">Complete Compliance Matrix</h5>
            <div className="tender-record-list">
              {matrixPage.rows.map((row) => (
                <MatrixRowEditor
                  key={`${packageInspection.version}:${row.record.record_id}:${row.record.version}`}
                  row={row}
                  disabled={busy}
                  onSave={saveRow}
                />
              ))}
            </div>
            <div className="record-pagination">
              <button
                type="button"
                className="button-secondary"
                disabled={busy || matrixHistory.length === 0}
                onClick={() => {
                  const previous =
                    matrixHistory[matrixHistory.length - 1] ?? null;
                  setMatrixHistory((history) => history.slice(0, -1));
                  void loadMatrix(packageInspection, previous);
                }}
              >
                Previous matrix page
              </button>
              <button
                type="button"
                className="button-secondary"
                disabled={busy || matrixPage.next_ordinal === null}
                onClick={() => {
                  setMatrixHistory((history) => [...history, matrixAfter]);
                  void loadMatrix(packageInspection, matrixPage.next_ordinal);
                }}
              >
                Next matrix page
              </button>
            </div>
          </section>

          <section aria-labelledby="resource-implications-title">
            <h5 id="resource-implications-title">Resource implications</h5>
            <ul className="bid-blocker-list">
              {packageInspection.resource_implications.map((implication) => (
                <li key={implication.triggering_record.record_id}>
                  <strong>{implication.responsibility}</strong> ·{" "}
                  {implication.affected_work.join(", ")} ·{" "}
                  {implication.planned_treatment}
                  {implication.uncertainty
                    ? ` · uncertainty: ${implication.uncertainty}`
                    : ""}
                </li>
              ))}
            </ul>
          </section>

          <section
            className="bid-record-bindings"
            aria-labelledby="package-basis-title"
          >
            <div className="record-extraction-controls">
              <label>
                <span id="package-basis-title">Exact package basis</span>
                <select
                  value={recordCategory}
                  onChange={(event) => {
                    const category = event.target
                      .value as BidDecisionPackageRecordCategory;
                    setRecordCategory(category);
                    setRecordHistory([]);
                    void loadRecords(packageInspection, category, null);
                  }}
                >
                  {Object.entries(recordCategoryCopy).map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>
              </label>
            </div>
            <ul>
              {recordPage.records.map((binding) => (
                <li key={`${binding.category}:${binding.ordinal}`}>
                  <strong>{binding.record.title}</strong> · exact v
                  {binding.record.version} ·{" "}
                  {humanize(binding.record.trust_class)}
                </li>
              ))}
            </ul>
            <div className="record-pagination">
              <button
                type="button"
                className="button-secondary"
                disabled={busy || recordHistory.length === 0}
                onClick={() => {
                  const previous =
                    recordHistory[recordHistory.length - 1] ?? null;
                  setRecordHistory((history) => history.slice(0, -1));
                  void loadRecords(packageInspection, recordCategory, previous);
                }}
              >
                Previous basis page
              </button>
              <button
                type="button"
                className="button-secondary"
                disabled={busy || recordPage.next_ordinal === null}
                onClick={() => {
                  setRecordHistory((history) => [...history, recordAfter]);
                  void loadRecords(
                    packageInspection,
                    recordCategory,
                    recordPage.next_ordinal,
                  );
                }}
              >
                Next basis page
              </button>
            </div>
          </section>

          {packageInspection.review ? (
            <section className="bid-review" aria-labelledby="bid-review-title">
              <h5 id="bid-review-title">Independent Review</h5>
              <p>
                {packageInspection.review.outcome} · exact package v
                {packageInspection.version}
              </p>
              <ul>
                {packageInspection.review.findings.map((finding) => (
                  <li key={finding.code}>
                    <strong>{finding.severity}</strong> · {finding.summary}
                  </li>
                ))}
              </ul>
            </section>
          ) : (
            <button
              type="button"
              disabled={busy || !runtimeReady || !packageInspection.current}
              onClick={() =>
                void mutate(() =>
                  runBidDecisionPackageReview(
                    tenderId,
                    packageInspection.package_id,
                    packageInspection.version,
                  ),
                )
              }
            >
              Run Independent Review of exact version
            </button>
          )}

          {packageInspection.decision_gate_ready ? (
            <p className="intake-result" role="status">
              This exact, independently reviewed package is ready to support the
              formal Bid Decision gate.
            </p>
          ) : null}
        </>
      )}
    </section>
  );
}
