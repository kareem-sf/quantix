import {
  FormEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import type { WorkPlanDecision } from "./bindings/WorkPlanDecision";
import type { WorkPlanProposalInspection } from "./bindings/WorkPlanProposalInspection";
import type { WorkPlanRevisionAction } from "./bindings/WorkPlanRevisionAction";
import type { TenderProductionInspection } from "./bindings/TenderProductionInspection";
import type { ProductionReview } from "./bindings/ProductionReview";
import type { ProductionReviewFinding } from "./bindings/ProductionReviewFinding";
import type { ProductionTaskReviewInspection } from "./bindings/ProductionTaskReviewInspection";
import {
  activateTenderProduction,
  approveProductionFindingException,
  composeTenderOffice,
  decideWorkPlanProposal,
  inspectCurrentWorkPlan,
  inspectProductionTaskReview,
  inspectTenderProduction,
  interruptAgentRun,
  reviseWorkPlanProposal,
  runProductionTask,
} from "./quantixHost";

interface TenderOfficePanelProps {
  tenderId: string;
  runtimeReady: boolean;
  reportCommandFailure: () => void;
  refreshToken: number;
  onTenderStateChange: () => void;
  onProductionSchedulingChange: (active: boolean) => void;
}

type RevisionKind =
  "rebase" | "add" | "remove" | "split" | "combine" | "rename" | "adjust";

const humanize = (value: string) => value.replace(/_/g, " ");
const splitLines = (value: string) =>
  value
    .split("\n")
    .map((item) => item.trim())
    .filter(Boolean);

export function TenderOfficePanel({
  tenderId,
  runtimeReady,
  reportCommandFailure,
  refreshToken,
  onTenderStateChange,
  onProductionSchedulingChange,
}: TenderOfficePanelProps) {
  const [plan, setPlan] = useState<WorkPlanProposalInspection | null>();
  const [production, setProduction] =
    useState<TenderProductionInspection | null>();
  const [busy, setBusy] = useState(false);
  const [revisionKind, setRevisionKind] = useState<RevisionKind>("rename");
  const [profileId, setProfileId] = useState("");
  const [combinedProfileIds, setCombinedProfileIds] = useState<string[]>([]);
  const [identity, setIdentity] = useState("");
  const [archetype, setArchetype] = useState("planning_engineer");
  const [splitIdentities, setSplitIdentities] = useState("");
  const [objective, setObjective] = useState("");
  const [behavior, setBehavior] = useState("");
  const [skepticism, setSkepticism] = useState("");
  const [riskTolerance, setRiskTolerance] = useState("");
  const [durationSeconds, setDurationSeconds] = useState(120);
  const [outputBytes, setOutputBytes] = useState(256 * 1024);
  const [decision, setDecision] = useState<WorkPlanDecision>("approve");
  const [rationale, setRationale] = useState("");
  const [reviewTaskId, setReviewTaskId] = useState<string | null>(null);
  const [reviewDetail, setReviewDetail] =
    useState<ProductionTaskReviewInspection | null>(null);
  const [reviewBusy, setReviewBusy] = useState(false);
  const [queryControlTaskId, setQueryControlTaskId] = useState<string | null>(
    null,
  );
  const [exceptionDrafts, setExceptionDrafts] = useState<
    Record<string, { rationale: string; consequence: string }>
  >({});
  const requestSequence = useRef(0);
  const reviewRequestSequence = useRef(0);

  const loadPlan = useCallback(async () => {
    const request = ++requestSequence.current;
    try {
      const [next, nextProduction] = await Promise.all([
        inspectCurrentWorkPlan(tenderId),
        inspectTenderProduction(tenderId),
      ]);
      if (request === requestSequence.current) {
        setPlan(next);
        setProduction(nextProduction);
      }
    } catch {
      if (request === requestSequence.current) reportCommandFailure();
    }
  }, [reportCommandFailure, tenderId]);

  useEffect(() => {
    void loadPlan();
    return () => {
      requestSequence.current += 1;
      reviewRequestSequence.current += 1;
    };
  }, [loadPlan, refreshToken]);

  const loadReviewDetail = useCallback(
    async (productionTaskId: string) => {
      const request = ++reviewRequestSequence.current;
      setReviewBusy(true);
      try {
        const next = await inspectProductionTaskReview(
          tenderId,
          productionTaskId,
        );
        if (request === reviewRequestSequence.current) {
          setReviewTaskId(productionTaskId);
          setReviewDetail(next);
        }
      } catch {
        if (request === reviewRequestSequence.current) reportCommandFailure();
      } finally {
        if (request === reviewRequestSequence.current) setReviewBusy(false);
      }
    },
    [reportCommandFailure, tenderId],
  );

  const selectedProductionTask = production?.tasks.find(
    (task) => task.production_task_id === reviewTaskId,
  );
  const selectedReviewObservation = selectedProductionTask
    ? `${selectedProductionTask.state}:${selectedProductionTask.artifact_version_count}:${selectedProductionTask.review_count}:${selectedProductionTask.finding_count}:${selectedProductionTask.open_blocking_finding_count}`
    : null;

  useEffect(() => {
    if (reviewTaskId && selectedReviewObservation) {
      void loadReviewDetail(reviewTaskId);
    }
  }, [loadReviewDetail, reviewTaskId, selectedReviewObservation]);

  useEffect(() => {
    if (
      !production?.active ||
      !production.tasks.some((task) =>
        [
          "ready",
          "running",
          "review_ready",
          "reviewing",
          "remediation_ready",
        ].includes(task.state),
      )
    ) {
      return;
    }
    const interval = window.setInterval(() => void loadPlan(), 500);
    return () => window.clearInterval(interval);
  }, [loadPlan, production]);

  const productionScheduling = Boolean(
    production?.active &&
    production.tasks.some((task) =>
      [
        "ready",
        "running",
        "review_ready",
        "reviewing",
        "remediation_ready",
      ].includes(task.state),
    ),
  );

  useEffect(() => {
    onProductionSchedulingChange(productionScheduling);
    return () => onProductionSchedulingChange(false);
  }, [onProductionSchedulingChange, productionScheduling]);

  const selectedProfile = useMemo(
    () =>
      plan?.profiles.find(
        (binding) => binding.profile.profile_id === profileId,
      ),
    [plan, profileId],
  );

  useEffect(() => {
    if (!selectedProfile) return;
    const profile = selectedProfile.profile;
    setIdentity(profile.identity);
    setObjective(profile.objective);
    setBehavior(profile.behavior);
    setSkepticism(profile.skepticism);
    setRiskTolerance(profile.risk_tolerance);
    setDurationSeconds(profile.resource_budget.duration_seconds);
    setOutputBytes(profile.resource_budget.output_bytes);
  }, [selectedProfile]);

  const execute = async (
    command: () => Promise<WorkPlanProposalInspection>,
  ) => {
    const request = ++requestSequence.current;
    setBusy(true);
    try {
      const next = await command();
      onTenderStateChange();
      if (request === requestSequence.current) setPlan(next);
      return true;
    } catch {
      if (request === requestSequence.current) reportCommandFailure();
      return false;
    } finally {
      if (request === requestSequence.current) setBusy(false);
    }
  };

  const compose = () => void execute(() => composeTenderOffice(tenderId));

  const revisionAction = (): WorkPlanRevisionAction | null => {
    if (revisionKind === "rebase") {
      return { action: "rebase_package_basis" };
    }
    if (revisionKind === "add") {
      return identity.trim()
        ? { action: "add_profile", archetype, identity: identity.trim() }
        : null;
    }
    if (revisionKind === "combine") {
      return combinedProfileIds.length >= 2 && identity.trim()
        ? {
            action: "combine_profiles",
            profile_ids: combinedProfileIds,
            identity: identity.trim(),
          }
        : null;
    }
    if (!selectedProfile) return null;
    if (revisionKind === "remove") {
      return { action: "remove_profile", profile_id: profileId };
    }
    if (revisionKind === "split") {
      const identities = splitLines(splitIdentities);
      return identities.length >= 2
        ? { action: "split_profile", profile_id: profileId, identities }
        : null;
    }
    if (revisionKind === "rename") {
      return identity.trim()
        ? {
            action: "rename_profile",
            profile_id: profileId,
            identity: identity.trim(),
          }
        : null;
    }
    return objective.trim() &&
      behavior.trim() &&
      skepticism.trim() &&
      riskTolerance.trim()
      ? {
          action: "adjust_profile",
          profile_id: profileId,
          objective: objective.trim(),
          behavior: behavior.trim(),
          skepticism: skepticism.trim(),
          risk_tolerance: riskTolerance.trim(),
          resource_budget: {
            provider_turns: 1,
            duration_seconds: durationSeconds,
            output_bytes: outputBytes,
          },
        }
      : null;
  };

  const revise = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!plan) return;
    const action = revisionAction();
    if (!action) return;
    void execute(() =>
      reviseWorkPlanProposal(tenderId, plan.plan_id, plan.version, [action]),
    ).then((updated) => {
      if (updated) {
        setCombinedProfileIds([]);
        setSplitIdentities("");
      }
    });
  };

  const decide = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!plan || !rationale.trim()) return;
    void execute(() =>
      decideWorkPlanProposal(
        tenderId,
        plan.plan_id,
        plan.version,
        decision,
        rationale.trim(),
      ),
    ).then((updated) => {
      if (updated) setRationale("");
    });
  };

  const activate = async () => {
    if (!plan) return;
    setBusy(true);
    try {
      const next = await activateTenderProduction(
        tenderId,
        plan.plan_id,
        plan.version,
        plan.manifest_sha256,
      );
      setProduction(next);
      onTenderStateChange();
      await loadPlan();
    } catch {
      reportCommandFailure();
    } finally {
      setBusy(false);
    }
  };

  const cancelTask = async (runId: string) => {
    try {
      await interruptAgentRun(tenderId, runId);
      onTenderStateChange();
    } catch {
      reportCommandFailure();
    }
  };

  const requestQueryOwnerUpdate = async (productionTaskId: string) => {
    setQueryControlTaskId(productionTaskId);
    try {
      await runProductionTask(tenderId, productionTaskId);
      onTenderStateChange();
      await loadPlan();
    } catch {
      reportCommandFailure();
    } finally {
      setQueryControlTaskId(null);
    }
  };

  const approveFindingException = async (
    review: ProductionReview,
    finding: ProductionReviewFinding,
  ) => {
    const draft = exceptionDrafts[finding.finding_id];
    if (
      !reviewTaskId ||
      !reviewDetail ||
      !draft?.rationale.trim() ||
      !draft.consequence.trim()
    ) {
      return;
    }
    const artifact = reviewDetail.artifact_versions.find(
      (candidate) =>
        candidate.artifact_id === review.target_artifact_id &&
        candidate.version === review.target_version,
    );
    if (!artifact) return;
    const request = ++reviewRequestSequence.current;
    setReviewBusy(true);
    try {
      const next = await approveProductionFindingException(
        tenderId,
        reviewTaskId,
        finding.finding_id,
        review.review_id,
        artifact.artifact_id,
        artifact.version,
        artifact.payload_sha256,
        draft.rationale.trim(),
        draft.consequence.trim(),
      );
      if (request === reviewRequestSequence.current) {
        setReviewDetail(next);
        setExceptionDrafts((current) => {
          const next = { ...current };
          delete next[finding.finding_id];
          return next;
        });
        await loadPlan();
        onTenderStateChange();
      }
    } catch {
      if (request === reviewRequestSequence.current) reportCommandFailure();
    } finally {
      if (request === reviewRequestSequence.current) setReviewBusy(false);
    }
  };

  const rebaseRequired = plan?.current === false;
  const productionMatchesPlan = Boolean(
    plan &&
    production?.active &&
    production.plan_id === plan.plan_id &&
    production.plan_version === plan.version &&
    production.plan_manifest_sha256 === plan.manifest_sha256,
  );
  const revisionClosed = rebaseRequired
    ? revisionKind !== "rebase"
    : !plan?.current ||
      (plan.approval?.decision === "approve" && !productionMatchesPlan);
  const approvalBlocked =
    !plan?.current ||
    plan.approval !== null ||
    plan.blocker_codes.length > 0 ||
    plan.capability_gaps.length > 0;

  useEffect(() => {
    if (rebaseRequired) {
      setRevisionKind("rebase");
    } else {
      setRevisionKind((current) => (current === "rebase" ? "rename" : current));
    }
  }, [rebaseRequired]);

  return (
    <section
      className="agent-office tender-office"
      aria-labelledby="tender-office-title"
    >
      <div className="agent-office__heading">
        <div>
          <p className="section-label">Production authorization</p>
          <h4 id="tender-office-title">Project Tender Office</h4>
        </div>
        <span>{plan ? `Work Plan v${plan.version}` : "Not composed"}</span>
      </div>

      {plan === undefined ? (
        <p className="catalogue-message">
          Inspecting the exact planning basis…
        </p>
      ) : plan === null ? (
        <div className="agent-office__introduction">
          <p>
            After Proceed, compose the deterministic core team, mandatory Cost
            Estimator, and evidence-triggered specialists as a proposal. No
            production work is authorized until the exact Work Plan is approved.
          </p>
          <div className="agent-office__actions">
            <button
              type="button"
              onClick={compose}
              disabled={busy || !runtimeReady}
            >
              Compose Tender Office proposal
            </button>
          </div>
        </div>
      ) : (
        <>
          <div className="bid-decision-summary">
            <dl>
              <div>
                <dt>Exact package</dt>
                <dd>
                  {plan.bid_package_id.slice(0, 12)} · v
                  {plan.bid_package_version}
                </dd>
              </div>
              <div>
                <dt>Profiles / tasks</dt>
                <dd>
                  {plan.profiles.length} / {plan.tasks.length}
                </dd>
              </div>
              <div>
                <dt>Policy versions</dt>
                <dd>
                  Capabilities v{plan.capability_catalogue_version} ·
                  permissions v{plan.permission_policy_version}
                </dd>
              </div>
              <div>
                <dt>Decision</dt>
                <dd>
                  {plan.approval
                    ? humanize(plan.approval.decision)
                    : "Pending Engineer approval"}
                </dd>
              </div>
            </dl>
            <p className="record-identity">Manifest {plan.manifest_sha256}</p>
          </div>

          {plan.capability_gaps.length > 0 ? (
            <div className="bid-review" role="status">
              <h5>Capability Gaps block production</h5>
              <ul>
                {plan.capability_gaps.map((gap) => (
                  <li key={gap.capability}>
                    <strong>{humanize(gap.capability)}</strong>: {gap.reason} (
                    {gap.affected_work.join(", ")})
                  </li>
                ))}
              </ul>
            </div>
          ) : null}

          <div className="tender-office__profiles">
            {plan.profiles.map((binding) => (
              <article
                className="tender-record-card"
                key={`${binding.profile.profile_id}:${binding.profile.version}`}
              >
                <header>
                  <div>
                    <p className="section-label">
                      {humanize(binding.archetype)} · {humanize(binding.status)}
                    </p>
                    <h5>{binding.profile.identity}</h5>
                  </div>
                  <span className="record-trust-badge">
                    v{binding.profile.version}
                  </span>
                </header>
                <p>
                  <strong>
                    {binding.profile.seniority} {binding.profile.profession}
                  </strong>
                </p>
                <p>{binding.profile.objective}</p>
                <dl className="tender-office__profile-details">
                  <div>
                    <dt>Capabilities</dt>
                    <dd>
                      {binding.profile.capabilities.map(humanize).join(", ")}
                    </dd>
                  </div>
                  <div>
                    <dt>Behavior</dt>
                    <dd>{binding.profile.behavior}</dd>
                  </div>
                  <div>
                    <dt>Skepticism</dt>
                    <dd>{binding.profile.skepticism}</dd>
                  </div>
                  <div>
                    <dt>Risk tolerance</dt>
                    <dd>{binding.profile.risk_tolerance}</dd>
                  </div>
                  <div>
                    <dt>Data scopes</dt>
                    <dd>
                      {binding.profile.permissions.data_scopes.join(", ")}
                    </dd>
                  </div>
                  <div>
                    <dt>Tools / network</dt>
                    <dd>
                      {binding.profile.permissions.allowed_tools.join(", ") ||
                        "No tools"}{" "}
                      · network{" "}
                      {binding.profile.permissions.network_allowed
                        ? "allowed"
                        : "denied"}
                    </dd>
                  </div>
                  <div>
                    <dt>Prohibited</dt>
                    <dd>
                      {binding.profile.prohibited_actions
                        .map(humanize)
                        .join(", ")}
                    </dd>
                  </div>
                  <div>
                    <dt>Review</dt>
                    <dd>{binding.profile.review_policy}</dd>
                  </div>
                  <div>
                    <dt>Budget</dt>
                    <dd>
                      {binding.profile.resource_budget.provider_turns} turn ·{" "}
                      {binding.profile.resource_budget.duration_seconds}s ·{" "}
                      {binding.profile.resource_budget.output_bytes.toLocaleString()}{" "}
                      bytes
                    </dd>
                  </div>
                </dl>
                <details>
                  <summary>Exact output contract</summary>
                  <pre>{binding.profile.output_contract_json}</pre>
                </details>
              </article>
            ))}
          </div>

          <div className="bid-record-bindings">
            <h5>Workstreams, dependencies, and milestones</h5>
            <ul>
              {plan.workstreams.map((workstream) => (
                <li key={workstream.workstream_key}>
                  <strong>{workstream.name}</strong> ·{" "}
                  {humanize(workstream.capability)} · owner{" "}
                  {workstream.accountable_profile_id?.slice(0, 12) ??
                    "Capability Gap"}{" "}
                  · dependencies {workstream.dependencies.join(", ") || "none"}{" "}
                  · deadline {workstream.deadlines.join(", ")} · milestones{" "}
                  {workstream.milestones.join(", ")}
                </li>
              ))}
            </ul>
          </div>

          {plan.approval?.decision === "approve" && !productionMatchesPlan ? (
            <div className="bid-review" role="status">
              <h5>Active Production boundary</h5>
              <p>
                Approval fixes authority, but no profile or task becomes Active
                until this exact manifest is activated.
              </p>
              <button
                type="button"
                onClick={() => void activate()}
                disabled={busy || !runtimeReady || !plan.current}
              >
                Activate exact Work Plan
              </button>
            </div>
          ) : null}

          {production ? (
            <div className="bid-record-bindings tender-office__production">
              <h5>
                {production.active
                  ? "Active Production"
                  : "Production suspended"}
              </h5>
              <p className="record-identity">
                Activation {production.activation_id} · Work Plan v
                {production.plan_version}
              </p>
              <ul>
                {production.tasks.map((productionTask) => {
                  const runId =
                    productionTask.run_ids[productionTask.run_ids.length - 1];
                  return (
                    <li key={productionTask.production_task_id}>
                      <strong>{humanize(productionTask.task.task_key)}</strong>{" "}
                      · {humanize(productionTask.state)} · profile{" "}
                      {productionTask.task.profile_id.slice(0, 12)} v
                      {productionTask.task.profile_version} · dependencies{" "}
                      {productionTask.task.dependencies.join(", ") || "none"} ·{" "}
                      {productionTask.run_ids.length} run(s) ·{" "}
                      {productionTask.artifact_version_count} Artifact
                      Version(s) · {productionTask.review_count} Review(s) ·{" "}
                      {productionTask.finding_count} finding(s) ·{" "}
                      {productionTask.open_blocking_finding_count} blocking{" "}
                      {productionTask.state === "ready" ||
                      productionTask.state === "review_ready" ||
                      productionTask.state === "remediation_ready" ? (
                        <span>Coordinator scheduling automatically</span>
                      ) : (productionTask.state === "running" ||
                          productionTask.state === "reviewing") &&
                        runId ? (
                        <button
                          type="button"
                          onClick={() => void cancelTask(runId)}
                        >
                          Cancel run
                        </button>
                      ) : null}
                      {productionTask.state === "query_blocked" &&
                      productionTask.query_control_available ? (
                        <button
                          type="button"
                          onClick={() =>
                            void requestQueryOwnerUpdate(
                              productionTask.production_task_id,
                            )
                          }
                          disabled={
                            queryControlTaskId !== null || !runtimeReady
                          }
                        >
                          {queryControlTaskId ===
                          productionTask.production_task_id
                            ? "Requesting specialist updateâ€¦"
                            : "Request specialist Evidence/treatment proposal"}
                        </button>
                      ) : null}
                      {productionTask.artifact_version_count > 0 ? (
                        <button
                          type="button"
                          aria-expanded={
                            reviewTaskId === productionTask.production_task_id
                          }
                          onClick={() => {
                            setReviewDetail(null);
                            setExceptionDrafts({});
                            setReviewTaskId(productionTask.production_task_id);
                          }}
                          disabled={reviewBusy}
                        >
                          Inspect target, Evidence, and Review
                        </button>
                      ) : null}
                    </li>
                  );
                })}
              </ul>
              {reviewTaskId ? (
                <section className="bid-review" aria-live="polite">
                  <div className="panel-heading">
                    <h5>Exact production review ledger</h5>
                    <button
                      type="button"
                      onClick={() => {
                        reviewRequestSequence.current += 1;
                        setExceptionDrafts({});
                        setReviewTaskId(null);
                        setReviewDetail(null);
                      }}
                    >
                      Close
                    </button>
                  </div>
                  {reviewBusy && !reviewDetail ? <p>Loading review…</p> : null}
                  {reviewDetail ? (
                    <>
                      <p>
                        Integration gate:{" "}
                        <strong>
                          {reviewDetail.readiness
                            ? `ready on Artifact v${reviewDetail.readiness.artifact_version}`
                            : "not ready"}
                        </strong>
                      </p>
                      <h6>Immutable Artifact Versions</h6>
                      {reviewDetail.artifact_versions.map((artifact) => (
                        <details
                          key={`${artifact.artifact_id}:${artifact.version}`}
                        >
                          <summary>
                            Artifact v{artifact.version} ·{" "}
                            {artifact.payload_sha256.slice(0, 16)} · author run{" "}
                            {artifact.author_run_id.slice(0, 12)}
                          </summary>
                          <p>{artifact.payload.summary}</p>
                          <p>
                            Output validation{" "}
                            {artifact.output_validation_passed
                              ? "passed"
                              : "failed"}{" "}
                            · Evidence verification{" "}
                            {artifact.evidence_verified ? "passed" : "failed"}
                          </p>
                          <p>
                            Exact Evidence:{" "}
                            {artifact.payload.evidence_references.join(", ")}
                          </p>
                          {artifact.payload.gaps.length > 0 ? (
                            <p>
                              Disclosed gaps: {artifact.payload.gaps.join(", ")}
                            </p>
                          ) : null}
                          {artifact.payload.remediations?.map((remediation) => (
                            <p key={remediation.finding_id}>
                              Remediates {remediation.finding_id}:{" "}
                              {remediation.treatment}
                            </p>
                          ))}
                        </details>
                      ))}
                      <h6>Independent Reviews and findings</h6>
                      {reviewDetail.reviews.map((review) => (
                        <details key={review.review_id} open>
                          <summary>
                            Artifact v{review.target_version} ·{" "}
                            {humanize(review.result)} · reviewer{" "}
                            {review.reviewer_profile_id.slice(0, 12)} v
                            {review.reviewer_profile_version}
                          </summary>
                          <p>
                            Capability {review.capability} · run{" "}
                            {review.reviewer_run_id.slice(0, 12)}
                          </p>
                          <p>Scope: {review.scope.join(", ")}</p>
                          <p>Criteria: {review.criteria.join(", ")}</p>
                          <p>
                            Exact inputs:{" "}
                            {review.inputs
                              .map(
                                (input) =>
                                  `${input.kind}:${input.reference}:v${input.version}`,
                              )
                              .join(", ")}
                          </p>
                          {review.findings.length === 0 ? (
                            <p>No findings.</p>
                          ) : (
                            <ul>
                              {review.findings.map((finding) => (
                                <li key={finding.finding_id}>
                                  <strong>{humanize(finding.severity)}</strong>{" "}
                                  · {finding.summary} · Evidence{" "}
                                  {finding.evidence_references.join(", ")}
                                  {finding.disposition ? (
                                    <div>
                                      Disposition{" "}
                                      {humanize(finding.disposition.kind)} by{" "}
                                      {humanize(finding.disposition.decided_by)}
                                      . Consequence:{" "}
                                      {finding.disposition.consequence}
                                    </div>
                                  ) : finding.severity === "minor" ? (
                                    <div>
                                      Disclosed; does not block integration.
                                    </div>
                                  ) : finding.severity === "critical" ? (
                                    <div>
                                      Open and nonwaivable; author remediation
                                      and a new exact Review are required.
                                    </div>
                                  ) : selectedProductionTask?.task
                                      .major_finding_policy ===
                                    "engineer_exception_allowed" ? (
                                    <div className="decision-form">
                                      <label>
                                        Exception rationale
                                        <textarea
                                          value={
                                            exceptionDrafts[finding.finding_id]
                                              ?.rationale ?? ""
                                          }
                                          onChange={(event) => {
                                            const rationale =
                                              event.currentTarget.value;
                                            setExceptionDrafts((current) => ({
                                              ...current,
                                              [finding.finding_id]: {
                                                rationale,
                                                consequence:
                                                  current[finding.finding_id]
                                                    ?.consequence ?? "",
                                              },
                                            }));
                                          }}
                                          maxLength={4000}
                                        />
                                      </label>
                                      <label>
                                        Exact consequence
                                        <textarea
                                          value={
                                            exceptionDrafts[finding.finding_id]
                                              ?.consequence ?? ""
                                          }
                                          onChange={(event) => {
                                            const consequence =
                                              event.currentTarget.value;
                                            setExceptionDrafts((current) => ({
                                              ...current,
                                              [finding.finding_id]: {
                                                rationale:
                                                  current[finding.finding_id]
                                                    ?.rationale ?? "",
                                                consequence,
                                              },
                                            }));
                                          }}
                                          maxLength={4000}
                                        />
                                      </label>
                                      <button
                                        type="button"
                                        onClick={() =>
                                          void approveFindingException(
                                            review,
                                            finding,
                                          )
                                        }
                                        disabled={
                                          reviewBusy ||
                                          !exceptionDrafts[
                                            finding.finding_id
                                          ]?.rationale.trim() ||
                                          !exceptionDrafts[
                                            finding.finding_id
                                          ]?.consequence.trim()
                                        }
                                      >
                                        Approve exact Major exception
                                      </button>
                                    </div>
                                  ) : (
                                    <div>
                                      Open; the approved Review Policy requires
                                      author remediation and a new exact Review.
                                    </div>
                                  )}
                                </li>
                              ))}
                            </ul>
                          )}
                        </details>
                      ))}
                    </>
                  ) : null}
                </section>
              ) : null}
            </div>
          ) : null}
          <div className="bid-record-bindings">
            <h5>Exact task and review bindings</h5>
            <ul>
              {plan.tasks.map((task) => (
                <li key={task.task_key}>
                  <strong>{humanize(task.task_key)}</strong> · author{" "}
                  {task.profile_id.slice(0, 12)} v{task.profile_version} ·
                  reviewer{" "}
                  {task.review_profile_id?.slice(0, 12) ?? "not applicable"}{" "}
                  {task.review_profile_version
                    ? `v${task.review_profile_version}`
                    : ""}{" "}
                  · Major findings {humanize(task.major_finding_policy)} ·{" "}
                  {task.deadline}
                </li>
              ))}
            </ul>
          </div>
          <div className="bid-record-bindings">
            <h5>Unresolved Tender Query bindings</h5>
            {plan.query_bindings.length > 0 ? (
              <ul>
                {plan.query_bindings.map((query) => (
                  <li key={`${query.record_id}:${query.version}`}>
                    Exact Tender Query {query.record_id.slice(0, 12)} · v
                    {query.version}
                  </li>
                ))}
              </ul>
            ) : (
              <p>No unresolved Tender Query record is bound to this package.</p>
            )}
          </div>

          <form
            className="bid-decision-gate tender-office__revision"
            onSubmit={revise}
          >
            <h5>Revise this proposal</h5>
            <p>
              Every manager action publishes a new immutable Work Plan version.
            </p>
            <label>
              Action
              <select
                value={revisionKind}
                onChange={(event) =>
                  setRevisionKind(event.target.value as RevisionKind)
                }
                disabled={busy}
              >
                {rebaseRequired ? (
                  <option value="rebase">
                    Bind accepted successor package
                  </option>
                ) : (
                  <>
                    <option value="add">Add specialist</option>
                    <option value="remove">Remove profile</option>
                    <option value="split">Split responsibilities</option>
                    <option value="combine">Combine compatible profiles</option>
                    <option value="rename">Rename profile</option>
                    <option value="adjust">Adjust profile and budget</option>
                  </>
                )}
              </select>
            </label>
            {revisionKind === "rebase" ? (
              <p>
                Publish a new Proposed Work Plan bound to the exact accepted
                successor Bid Decision Package. The suspended profiles remain
                inactive until this new version is approved.
              </p>
            ) : revisionKind === "add" ? (
              <label>
                Specialist archetype
                <select
                  value={archetype}
                  onChange={(event) => setArchetype(event.target.value)}
                  disabled={busy || revisionClosed}
                >
                  <option value="tender_office_coordinator">
                    Tender Office Coordinator
                  </option>
                  <option value="tender_coordinator">Tender Coordinator</option>
                  <option value="query_rfi_controller">
                    Query and RFI Controller
                  </option>
                  <option value="document_controller">
                    Document Controller
                  </option>
                  <option value="tender_analyst">Tender Analyst</option>
                  <option value="independent_reviewer">
                    Independent Reviewer
                  </option>
                  <option value="planning_engineer">Planning Engineer</option>
                  <option value="contracts_specialist">
                    Contracts Specialist
                  </option>
                  <option value="procurement_specialist">
                    Procurement Specialist
                  </option>
                  <option value="technical_specialist">
                    Technical Specialist
                  </option>
                  <option value="cost_estimator">Cost Estimator</option>
                  <option value="independent_cost_reviewer">
                    Independent Cost Reviewer
                  </option>
                  <option value="independent_planning_reviewer">
                    Independent Planning Reviewer
                  </option>
                  <option value="independent_contracts_reviewer">
                    Independent Contracts Reviewer
                  </option>
                  <option value="independent_procurement_reviewer">
                    Independent Procurement Reviewer
                  </option>
                  <option value="independent_technical_reviewer">
                    Independent Technical Reviewer
                  </option>
                </select>
              </label>
            ) : revisionKind === "combine" ? (
              <fieldset disabled={busy || revisionClosed}>
                <legend>Compatible profiles to combine</legend>
                {plan.profiles.map((binding) => (
                  <label key={`combine-${binding.profile.profile_id}`}>
                    <input
                      type="checkbox"
                      checked={combinedProfileIds.includes(
                        binding.profile.profile_id,
                      )}
                      onChange={(event) =>
                        setCombinedProfileIds((current) =>
                          event.target.checked
                            ? current.concat(binding.profile.profile_id)
                            : current.filter(
                                (id) => id !== binding.profile.profile_id,
                              ),
                        )
                      }
                    />
                    {binding.profile.identity}
                  </label>
                ))}
              </fieldset>
            ) : (
              <label>
                Exact profile
                <select
                  value={profileId}
                  onChange={(event) => setProfileId(event.target.value)}
                  disabled={busy || revisionClosed}
                >
                  <option value="">Select a profile</option>
                  {plan.profiles.map((binding) => (
                    <option
                      key={binding.profile.profile_id}
                      value={binding.profile.profile_id}
                    >
                      {binding.profile.identity} · v{binding.profile.version}
                    </option>
                  ))}
                </select>
              </label>
            )}
            {revisionKind === "split" ? (
              <label>
                New identities, one per responsibility
                <textarea
                  value={splitIdentities}
                  onChange={(event) => setSplitIdentities(event.target.value)}
                  disabled={busy || revisionClosed}
                />
              </label>
            ) : revisionKind !== "remove" && revisionKind !== "rebase" ? (
              <label>
                Profile identity
                <input
                  value={identity}
                  onChange={(event) => setIdentity(event.target.value)}
                  disabled={busy || revisionClosed}
                />
              </label>
            ) : null}
            {revisionKind === "adjust" ? (
              <>
                <label>
                  Objective
                  <textarea
                    value={objective}
                    onChange={(event) => setObjective(event.target.value)}
                    disabled={busy || revisionClosed}
                  />
                </label>
                <label>
                  Behavior
                  <textarea
                    value={behavior}
                    onChange={(event) => setBehavior(event.target.value)}
                    disabled={busy || revisionClosed}
                  />
                </label>
                <label>
                  Skepticism
                  <textarea
                    value={skepticism}
                    onChange={(event) => setSkepticism(event.target.value)}
                    disabled={busy || revisionClosed}
                  />
                </label>
                <label>
                  Risk tolerance
                  <textarea
                    value={riskTolerance}
                    onChange={(event) => setRiskTolerance(event.target.value)}
                    disabled={busy || revisionClosed}
                  />
                </label>
                <label>
                  Duration seconds
                  <input
                    type="number"
                    min={1}
                    max={600}
                    value={durationSeconds}
                    onChange={(event) =>
                      setDurationSeconds(Number(event.target.value))
                    }
                    disabled={busy || revisionClosed}
                  />
                </label>
                <label>
                  Output bytes
                  <input
                    type="number"
                    min={1}
                    max={1024 * 1024}
                    value={outputBytes}
                    onChange={(event) =>
                      setOutputBytes(Number(event.target.value))
                    }
                    disabled={busy || revisionClosed}
                  />
                </label>
              </>
            ) : null}
            <button
              type="submit"
              disabled={
                busy ||
                !runtimeReady ||
                revisionClosed ||
                revisionAction() === null
              }
            >
              Publish revised proposal
            </button>
          </form>

          <form className="bid-decision-gate" onSubmit={decide}>
            <h5>Engineer decision on exact Work Plan v{plan.version}</h5>
            <label>
              Decision
              <select
                value={decision}
                onChange={(event) =>
                  setDecision(event.target.value as WorkPlanDecision)
                }
                disabled={busy || plan.approval !== null}
              >
                <option value="approve">Approve for production</option>
                <option value="return">Return for revision</option>
                <option value="reject">Reject proposal</option>
              </select>
            </label>
            <label>
              Attributable rationale
              <textarea
                value={rationale}
                onChange={(event) => setRationale(event.target.value)}
                disabled={busy || plan.approval !== null}
              />
            </label>
            {approvalBlocked && decision === "approve" ? (
              <p className="catalogue-error" role="status">
                Approval is blocked until this exact current proposal has no
                Capability Gaps or blockers.
              </p>
            ) : null}
            <button
              type="submit"
              disabled={
                busy ||
                !runtimeReady ||
                plan.approval !== null ||
                !rationale.trim() ||
                (decision === "approve" && approvalBlocked)
              }
            >
              {decision === "approve"
                ? "Approve exact Work Plan"
                : decision === "return"
                  ? "Return exact Work Plan"
                  : "Reject exact Work Plan"}
            </button>
          </form>
        </>
      )}
    </section>
  );
}
