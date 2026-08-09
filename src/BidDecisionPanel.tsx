import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type { BidDecisionPackageInspection } from "./bindings/BidDecisionPackageInspection";
import type { BidDecisionApprovalDecision } from "./bindings/BidDecisionApprovalDecision";
import type { BidDecisionApprovalHistoryPage } from "./bindings/BidDecisionApprovalHistoryPage";
import type { BidDecisionPackageRecordCategory } from "./bindings/BidDecisionPackageRecordCategory";
import type { BidDecisionPackageRecordPage } from "./bindings/BidDecisionPackageRecordPage";
import type { ComplianceDisposition } from "./bindings/ComplianceDisposition";
import type { ComplianceDispositionUpdate } from "./bindings/ComplianceDispositionUpdate";
import type { ComplianceMatrixPage } from "./bindings/ComplianceMatrixPage";
import type { ComplianceMatrixRow } from "./bindings/ComplianceMatrixRow";
import type { ManagerCapabilityDemandInput } from "./bindings/ManagerCapabilityDemandInput";
import {
  createBidDecisionPackage,
  decideBidDecisionPackage,
  inspectBidDecisionApprovalHistory,
  inspectBidDecisionPackageRecords,
  inspectComplianceMatrix,
  inspectCurrentBidDecisionPackage,
  invalidateBidDecisionApproval,
  resolveBidDecisionReturnRework,
  runBidDecisionPackageReview,
} from "./quantixHost";

interface BidDecisionPanelProps {
  tenderId: string;
  runtimeReady: boolean;
  reportCommandFailure: () => void;
  onTenderStateChange: () => void;
}

const PAGE_SIZE = 4;
const APPROVAL_PAGE_SIZE = 5;
const humanize = (value: string) => value.replace(/_/g, " ");
const lines = (value: string) =>
  value
    .split("\n")
    .map((item) => item.trim())
    .filter(Boolean);
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
  onTenderStateChange,
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
  const [decisionRationale, setDecisionRationale] = useState("");
  const [conditions, setConditions] = useState("");
  const [exceptions, setExceptions] = useState("");
  const [requiredRework, setRequiredRework] = useState("");
  const [approvalHistory, setApprovalHistory] =
    useState<BidDecisionApprovalHistoryPage>({
      approvals: [],
      next_sequence: null,
    });
  const [approvalBefore, setApprovalBefore] = useState<number | null>(null);
  const [approvalCursors, setApprovalCursors] = useState<(number | null)[]>([]);
  const [historyBusy, setHistoryBusy] = useState(false);
  const approvalRequest = useRef(0);
  const [reworkResolutions, setReworkResolutions] = useState("");
  const [materialChangeSummary, setMaterialChangeSummary] = useState("");
  const [affectedAreas, setAffectedAreas] = useState("");

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

  const loadApprovalHistory = useCallback(
    async (before: number | null) => {
      const request = ++approvalRequest.current;
      setHistoryBusy(true);
      try {
        const page = await inspectBidDecisionApprovalHistory(
          tenderId,
          before,
          APPROVAL_PAGE_SIZE,
        );
        if (request === approvalRequest.current) {
          setApprovalHistory(page);
          setApprovalBefore(before);
          return true;
        }
        return false;
      } finally {
        if (request === approvalRequest.current) setHistoryBusy(false);
      }
    },
    [tenderId],
  );

  const refresh = useCallback(async () => {
    try {
      const current = await inspectCurrentBidDecisionPackage(tenderId);
      setPackageInspection(current);
      setMatrixHistory([]);
      setRecordHistory([]);
      setApprovalCursors([]);
      if (current) {
        await Promise.all([
          loadMatrix(current, null),
          loadRecords(current, recordCategory, null),
          loadApprovalHistory(null),
        ]);
      } else {
        setMatrixPage({ rows: [], next_ordinal: null });
        setRecordPage({ records: [], next_ordinal: null });
        await loadApprovalHistory(null);
      }
    } catch {
      reportCommandFailure();
    }
  }, [
    loadApprovalHistory,
    loadMatrix,
    loadRecords,
    recordCategory,
    reportCommandFailure,
    tenderId,
  ]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const mutate = useCallback(
    async (operation: () => Promise<unknown>) => {
      setBusy(true);
      try {
        await operation();
        onTenderStateChange();
        await refresh();
      } catch {
        reportCommandFailure();
      } finally {
        setBusy(false);
      }
    },
    [onTenderStateChange, refresh, reportCommandFailure],
  );

  const savedManagerDemands = useMemo(
    () => (packageInspection ? managerDemands(packageInspection) : []),
    [packageInspection],
  );

  const packageEditable =
    packageInspection?.current === true &&
    packageInspection.approval === null &&
    packageInspection.lifecycle_phase === "bid_decision";

  const submitDecision = async (decision: BidDecisionApprovalDecision) => {
    if (!packageInspection || !decisionRationale.trim()) return;
    await mutate(() =>
      decideBidDecisionPackage(
        tenderId,
        packageInspection.package_id,
        packageInspection.version,
        packageInspection.manifest_sha256,
        decision,
        decisionRationale.trim(),
        lines(conditions),
        lines(exceptions),
        decision === "return" ? lines(requiredRework) : [],
      ),
    );
  };

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
                <dt>Lifecycle</dt>
                <dd>{humanize(packageInspection.lifecycle_phase)}</dd>
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
              <div>
                <dt>Prior decisions</dt>
                <dd>{packageInspection.prior_approval_count}</dd>
              </div>
            </dl>
            <p>{packageInspection.recommendation.rationale}</p>
          </div>

          <section aria-labelledby="package-change-title">
            <h5 id="package-change-title">Changes in this exact version</h5>
            <p>
              {packageInspection.change_summary.prior_version === null
                ? "Initial package snapshot"
                : `Compared with v${packageInspection.change_summary.prior_version}`}
              {" · "}
              {packageInspection.change_summary.added_record_count} records
              added
              {" · "}
              {packageInspection.change_summary.removed_record_count} removed
              {" · "}
              {
                packageInspection.change_summary.changed_compliance_row_count
              }{" "}
              matrix rows changed
              {packageInspection.change_summary.capability_demands_changed
                ? " · capability demands changed"
                : ""}
              {packageInspection.change_summary.resource_implications_changed
                ? " · resource implications changed"
                : ""}
            </p>
            {packageInspection.return_rework_basis ? (
              <div className="bid-review">
                <p>
                  This version carries the exact disposition of Return Approval{" "}
                  {packageInspection.return_rework_basis.approval_id.slice(
                    0,
                    12,
                  )}
                  ….
                </p>
                <ul aria-label="Carried Return rework dispositions">
                  {packageInspection.return_rework_basis.items.map((item) => (
                    <li key={item.required_rework}>
                      <strong>{item.required_rework}</strong> ·{" "}
                      {item.resolution}
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}
            {packageInspection.material_change_basis ? (
              <div className="bid-review">
                <p>
                  This version carries the exact material-change assessment that
                  invalidated Approval{" "}
                  {packageInspection.material_change_basis.approval_id.slice(
                    0,
                    12,
                  )}
                  ….
                </p>
                <p>
                  {
                    packageInspection.material_change_basis
                      .material_change_summary
                  }
                </p>
                <ul aria-label="Affected material-change areas">
                  {packageInspection.material_change_basis.affected_areas.map(
                    (area) => (
                      <li key={area}>{area}</li>
                    ),
                  )}
                </ul>
                <p className="record-identity">
                  Exact changed records:{" "}
                  {packageInspection.material_change_basis.changed_records
                    .map(
                      (record) =>
                        `${record.record_id.slice(0, 12)}… v${record.version}`,
                    )
                    .join(", ")}
                </p>
              </div>
            ) : null}
          </section>

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
                  disabled={busy || !packageEditable}
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
                  disabled={busy || !packageEditable}
                />
              </label>
              <button
                type="submit"
                className="button-secondary"
                disabled={
                  busy ||
                  !packageEditable ||
                  !capability.trim() ||
                  !capabilityRationale.trim()
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
                  disabled={busy || !packageEditable}
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
              disabled={
                busy ||
                !runtimeReady ||
                !packageEditable ||
                !packageInspection.current
              }
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
              This exact, independently reviewed package is ready for the
              Tendering Manager's formal decision.
            </p>
          ) : null}

          {packageInspection.approval ? (
            <>
              <section className="bid-review" aria-labelledby="approval-title">
                <h5 id="approval-title">Immutable Approval Record</h5>
                <p>
                  {humanize(packageInspection.approval.decision)} · exact
                  package v{packageInspection.approval.package_version} ·{" "}
                  {packageInspection.approval.decided_by} acting as{" "}
                  {humanize(packageInspection.approval.acting_role)}
                </p>
                <p>{packageInspection.approval.rationale}</p>
                <p>{packageInspection.approval.consequence}</p>
                <p className="record-identity">
                  Approval{" "}
                  {packageInspection.approval.approval_sha256.slice(0, 12)}… ·{" "}
                  {packageInspection.approval.evidence_count} exact Evidence
                  references
                </p>
                {packageInspection.approval.conditions.length > 0 ? (
                  <ul aria-label="Decision conditions">
                    {packageInspection.approval.conditions.map((condition) => (
                      <li key={condition}>{condition}</li>
                    ))}
                  </ul>
                ) : null}
                {packageInspection.approval.exceptions.length > 0 ? (
                  <ul aria-label="Decision exceptions">
                    {packageInspection.approval.exceptions.map((exception) => (
                      <li key={exception}>{exception}</li>
                    ))}
                  </ul>
                ) : null}
                {packageInspection.approval.required_rework.length > 0 ? (
                  <ul aria-label="Required rework">
                    {packageInspection.approval.required_rework.map((item) => (
                      <li key={item}>{item}</li>
                    ))}
                  </ul>
                ) : null}
                {packageInspection.approval.invalidation ? (
                  <div className="bid-review">
                    <p>
                      This approval was explicitly invalidated by the Engineer
                      User acting as Tendering Manager on{" "}
                      {packageInspection.approval.invalidation.created_at}.
                    </p>
                    <p>
                      {
                        packageInspection.approval.invalidation
                          .material_change_summary
                      }
                    </p>
                    <ul aria-label="Approval invalidation affected areas">
                      {packageInspection.approval.invalidation.affected_areas.map(
                        (area) => (
                          <li key={area}>{area}</li>
                        ),
                      )}
                    </ul>
                    <p className="record-identity">
                      Exact changed records:{" "}
                      {packageInspection.approval.invalidation.changed_records
                        .map(
                          (record) =>
                            `${record.record_id.slice(0, 12)}… v${record.version}`,
                        )
                        .join(", ")}
                    </p>
                  </div>
                ) : null}
              </section>
              {packageInspection.approval.decision === "accept" &&
              packageInspection.approval.invalidation === null ? (
                <section
                  className="bid-review"
                  aria-labelledby="material-change-title"
                >
                  <h5 id="material-change-title">
                    Reopen for a material change
                  </h5>
                  <p>
                    This attributable Change Assessment invalidates the exact
                    Proceed approval and returns the Tender to Bid Decision.
                    Declined Tenders cannot be reopened here.
                  </p>
                  <label>
                    Material-change summary
                    <textarea
                      value={materialChangeSummary}
                      onChange={(event) =>
                        setMaterialChangeSummary(event.target.value)
                      }
                      maxLength={4000}
                      rows={4}
                      disabled={busy}
                    />
                  </label>
                  <label>
                    Affected areas · one per line
                    <textarea
                      value={affectedAreas}
                      onChange={(event) => setAffectedAreas(event.target.value)}
                      maxLength={32000}
                      rows={3}
                      disabled={busy}
                    />
                  </label>
                  <button
                    type="button"
                    className="button-secondary"
                    disabled={
                      busy ||
                      !materialChangeSummary.trim() ||
                      lines(affectedAreas).length === 0
                    }
                    onClick={() =>
                      void mutate(() =>
                        invalidateBidDecisionApproval(
                          tenderId,
                          packageInspection.approval!.approval_id,
                          packageInspection.approval!.approval_sha256,
                          materialChangeSummary.trim(),
                          lines(affectedAreas),
                        ),
                      )
                    }
                  >
                    Invalidate Proceed and reopen Bid Decision
                  </button>
                </section>
              ) : null}
              {packageInspection.approval.decision === "accept" &&
              packageInspection.approval.invalidation &&
              !packageInspection.current ? (
                <section className="bid-review">
                  <p>
                    The registered material change has made this approved
                    package stale. Publish a successor that carries the exact
                    Change Assessment into independent review.
                  </p>
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() =>
                      void mutate(() =>
                        createBidDecisionPackage(
                          tenderId,
                          packageInspection.version,
                          [],
                          savedManagerDemands,
                        ),
                      )
                    }
                  >
                    Publish material-change successor package
                  </button>
                </section>
              ) : null}
              {packageInspection.approval.decision === "return" &&
              packageInspection.return_rework === null ? (
                <section className="bid-review" aria-labelledby="rework-title">
                  <h5 id="rework-title">Required rework disposition</h5>
                  <p>
                    Record one attributable resolution per required item. The
                    pending gate cannot receive a successor package until every
                    item is disposed.
                  </p>
                  <textarea
                    aria-label="Return rework resolutions in required-item order"
                    value={reworkResolutions}
                    onChange={(event) =>
                      setReworkResolutions(event.target.value)
                    }
                    maxLength={32000}
                    rows={4}
                    disabled={busy}
                  />
                  <button
                    type="button"
                    disabled={
                      busy ||
                      lines(reworkResolutions).length !==
                        packageInspection.approval.required_rework.length
                    }
                    onClick={() =>
                      void mutate(() =>
                        resolveBidDecisionReturnRework(
                          tenderId,
                          packageInspection.approval!.approval_id,
                          lines(reworkResolutions),
                        ),
                      )
                    }
                  >
                    Record exact rework disposition
                  </button>
                </section>
              ) : null}
              {packageInspection.approval.decision === "return" &&
              packageInspection.return_rework ? (
                <section
                  className="bid-review"
                  aria-labelledby="rework-resolved-title"
                >
                  <h5 id="rework-resolved-title">Return rework resolved</h5>
                  <ul>
                    {packageInspection.return_rework.items.map((item) => (
                      <li key={item.required_rework}>
                        <strong>{item.required_rework}</strong> ·{" "}
                        {item.resolution}
                      </li>
                    ))}
                  </ul>
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() =>
                      void mutate(() =>
                        createBidDecisionPackage(
                          tenderId,
                          packageInspection.version,
                          [],
                          savedManagerDemands,
                        ),
                      )
                    }
                  >
                    Publish reworked successor package
                  </button>
                </section>
              ) : null}
            </>
          ) : (
            <section className="bid-review" aria-labelledby="formal-gate-title">
              <h5 id="formal-gate-title">Formal EITL Bid Decision</h5>
              <p>
                Only the authenticated Engineer User acting as Tendering Manager
                can decide. Agents, provider output, silence, navigation, and
                chat cannot create this Approval Record.
              </p>
              <label>
                Decision rationale
                <textarea
                  value={decisionRationale}
                  onChange={(event) => setDecisionRationale(event.target.value)}
                  maxLength={4000}
                  rows={4}
                  disabled={busy || !packageEditable}
                />
              </label>
              <label>
                Conditions · one per line
                <textarea
                  value={conditions}
                  onChange={(event) => setConditions(event.target.value)}
                  maxLength={32000}
                  rows={3}
                  disabled={busy || !packageEditable}
                />
              </label>
              <label>
                Exceptions · one per line
                <textarea
                  value={exceptions}
                  onChange={(event) => setExceptions(event.target.value)}
                  maxLength={32000}
                  rows={3}
                  disabled={busy || !packageEditable}
                />
              </label>
              <label>
                Required rework for Return · one per line
                <textarea
                  value={requiredRework}
                  onChange={(event) => setRequiredRework(event.target.value)}
                  maxLength={32000}
                  rows={3}
                  disabled={busy || !packageEditable}
                />
              </label>
              <div className="record-pagination">
                <button
                  type="button"
                  disabled={
                    busy ||
                    !packageInspection.decision_gate_ready ||
                    !decisionRationale.trim()
                  }
                  onClick={() => void submitDecision("accept")}
                >
                  Accept · Proceed to Tender Planning
                </button>
                <button
                  type="button"
                  className="button-secondary"
                  disabled={
                    busy ||
                    !packageEditable ||
                    !decisionRationale.trim() ||
                    lines(requiredRework).length === 0
                  }
                  onClick={() => void submitDecision("return")}
                >
                  Return for required rework
                </button>
                <button
                  type="button"
                  className="button-secondary"
                  disabled={
                    busy ||
                    !packageInspection.decision_gate_ready ||
                    !decisionRationale.trim()
                  }
                  onClick={() => void submitDecision("reject")}
                >
                  Reject · Decline Tender
                </button>
              </div>
              <p className="record-identity">
                Proceed enters Tender Planning but does not authorize
                production. Decline is terminal and preserves the complete
                Tender history.
              </p>
            </section>
          )}

          <section aria-labelledby="approval-history-title">
            <h5 id="approval-history-title">Approval history</h5>
            {approvalHistory.approvals.length === 0 ? (
              <p>No formal Tendering Manager decision has been recorded.</p>
            ) : (
              <ul className="bid-blocker-list">
                {approvalHistory.approvals.map((approval) => (
                  <li key={approval.approval_id}>
                    <details>
                      <summary>
                        <strong>{humanize(approval.decision)}</strong> · package
                        v{approval.package_version} · {approval.created_at} ·{" "}
                        {approval.approval_sha256.slice(0, 12)}…
                      </summary>
                      <p>{approval.rationale}</p>
                      <p>{approval.consequence}</p>
                      <p>
                        {approval.evidence_count} exact Evidence references ·{" "}
                        {approval.decided_by} acting as{" "}
                        {humanize(approval.acting_role)}
                      </p>
                      {approval.conditions.length > 0 ? (
                        <ul aria-label="Historical decision conditions">
                          {approval.conditions.map((condition) => (
                            <li key={condition}>{condition}</li>
                          ))}
                        </ul>
                      ) : null}
                      {approval.exceptions.length > 0 ? (
                        <ul aria-label="Historical decision exceptions">
                          {approval.exceptions.map((exception) => (
                            <li key={exception}>{exception}</li>
                          ))}
                        </ul>
                      ) : null}
                      {approval.required_rework.length > 0 ? (
                        <ul aria-label="Historical required rework">
                          {approval.required_rework.map((item) => (
                            <li key={item}>{item}</li>
                          ))}
                        </ul>
                      ) : null}
                      {approval.invalidation ? (
                        <div>
                          <p>
                            Invalidated for material change:{" "}
                            {approval.invalidation.material_change_summary}
                          </p>
                          <ul aria-label="Historical invalidation affected areas">
                            {approval.invalidation.affected_areas.map(
                              (area) => (
                                <li key={area}>{area}</li>
                              ),
                            )}
                          </ul>
                        </div>
                      ) : null}
                    </details>
                  </li>
                ))}
              </ul>
            )}
            <div className="record-pagination">
              <button
                type="button"
                className="button-secondary"
                disabled={busy || historyBusy || approvalCursors.length === 0}
                onClick={() => {
                  const previous =
                    approvalCursors[approvalCursors.length - 1] ?? null;
                  void loadApprovalHistory(previous).then((applied) => {
                    if (applied) {
                      setApprovalCursors((cursors) => cursors.slice(0, -1));
                    }
                  });
                }}
              >
                Newer decisions
              </button>
              <button
                type="button"
                className="button-secondary"
                disabled={
                  busy || historyBusy || approvalHistory.next_sequence === null
                }
                onClick={() => {
                  const next = approvalHistory.next_sequence;
                  if (next === null) return;
                  void loadApprovalHistory(next).then((applied) => {
                    if (applied) {
                      setApprovalCursors((cursors) => [
                        ...cursors,
                        approvalBefore,
                      ]);
                    }
                  });
                }}
              >
                Older decisions
              </button>
            </div>
          </section>
        </>
      )}
    </section>
  );
}
