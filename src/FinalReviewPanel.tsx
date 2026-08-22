import { useCallback, useEffect, useRef, useState } from "react";

import type { FinalReviewInspection } from "./bindings/FinalReviewInspection";
import type { ManualVerificationResult } from "./bindings/ManualVerificationResult";
import type { SubmissionPackageVersion } from "./bindings/SubmissionPackageVersion";
import type { SubmissionReleaseInspection } from "./bindings/SubmissionReleaseInspection";
import { evidenceTextAttributes } from "./evidenceTypography";
import {
  approvePackageFindingException,
  approveSubmissionRelease,
  exportReleaseCopy,
  inspectCurrentSubmissionPackage,
  inspectFinalReview,
  inspectSubmissionRelease,
  recordPackageManualVerification,
  runPackageValidation,
  runSubmissionSectionReview,
} from "./quantixHost";

interface FinalReviewPanelProps {
  tenderId: string;
  runtimeReady: boolean;
  refreshToken: number;
  reportCommandFailure: () => void;
  onTenderStateChange: () => void;
}

function humanize(value: string) {
  return value.replace(/_/g, " ");
}

export function FinalReviewPanel({
  tenderId,
  runtimeReady,
  refreshToken,
  reportCommandFailure,
  onTenderStateChange,
}: FinalReviewPanelProps) {
  const [submissionPackage, setSubmissionPackage] =
    useState<SubmissionPackageVersion | null>(null);
  const [review, setReview] = useState<FinalReviewInspection | null>(null);
  const [release, setRelease] = useState<SubmissionReleaseInspection | null>(
    null,
  );
  const [busy, setBusy] = useState(false);
  const [releaseRationale, setReleaseRationale] = useState("");
  const [releaseConditions, setReleaseConditions] = useState("");
  const [releaseExceptions, setReleaseExceptions] = useState("");
  const [manualOutcomes, setManualOutcomes] = useState<
    Record<string, ManualVerificationResult | "">
  >({});
  const [manualLimitations, setManualLimitations] = useState<
    Record<string, string>
  >({});
  const [exceptionRationales, setExceptionRationales] = useState<
    Record<string, string>
  >({});
  const requestGeneration = useRef(0);

  const load = useCallback(async () => {
    const request = ++requestGeneration.current;
    try {
      const [assembled, inspected] = await Promise.all([
        inspectCurrentSubmissionPackage(tenderId),
        inspectFinalReview(tenderId),
      ]);
      const releaseInspection = inspected
        ? await inspectSubmissionRelease(tenderId)
        : null;
      if (request !== requestGeneration.current) return;
      setSubmissionPackage(assembled);
      setReview(inspected);
      setRelease(releaseInspection);
    } catch {
      if (request === requestGeneration.current) reportCommandFailure();
    }
  }, [reportCommandFailure, tenderId]);

  useEffect(() => {
    void load();
    return () => {
      requestGeneration.current += 1;
    };
  }, [load, refreshToken]);

  async function act(action: () => Promise<FinalReviewInspection>) {
    if (busy) return;
    setBusy(true);
    try {
      const inspected = await action();
      setReview(inspected);
      onTenderStateChange();
    } catch {
      reportCommandFailure();
    } finally {
      setBusy(false);
    }
  }

  async function actOnRelease(
    action: () => Promise<SubmissionReleaseInspection>,
  ) {
    if (busy) return;
    setBusy(true);
    try {
      const inspected = await action();
      setRelease(inspected);
      setReview(inspected.final_review);
      onTenderStateChange();
    } catch {
      reportCommandFailure();
    } finally {
      setBusy(false);
    }
  }

  const pendingManual = review?.validation_run.results.filter(
    (result) =>
      result.outcome === "manual_verification_required" &&
      !review.manual_verifications.some(
        (verification) =>
          verification.validation_result_id === result.result_id,
      ),
  );
  const exactCurrent = Boolean(
    submissionPackage?.current &&
    review?.current &&
    submissionPackage.package_id === review.package.package_id &&
    submissionPackage.version === review.package.version &&
    submissionPackage.manifest_sha256 === review.package.manifest_sha256,
  );

  return (
    <section className="workspace-section" aria-labelledby="final-review-title">
      <div className="section-heading">
        <div>
          <p className="eyebrow">Submission assurance</p>
          <h2 id="final-review-title">Final Review and Release Readiness</h2>
        </div>
        {submissionPackage && !review ? (
          <button
            type="button"
            disabled={
              busy ||
              !submissionPackage.current ||
              submissionPackage.assessment !== "complete"
            }
            onClick={() =>
              void act(() =>
                runPackageValidation({
                  tender_id: tenderId,
                  package_id: submissionPackage.package_id,
                  package_version: submissionPackage.version,
                  package_manifest_sha256: submissionPackage.manifest_sha256,
                }),
              )
            }
          >
            Validate exact package
          </button>
        ) : null}
      </div>

      {!submissionPackage ? (
        <p>Assemble a complete Submission Package before Final Review.</p>
      ) : !review ? (
        <p>The exact package has not been validated for Final Review.</p>
      ) : (
        <>
          <p>
            Package v{review.package.version} ·{" "}
            <code>{review.package.manifest_sha256}</code>
          </p>
          <p role="status">
            {review.ready
              ? "Ready for Final Approval"
              : `${review.live_blockers.length} release blocker${review.live_blockers.length === 1 ? "" : "s"}`}
          </p>
          {!exactCurrent ? (
            <div role="alert">
              <strong>This Final Review is historical and read-only.</strong>
              <ul>
                {review.live_changes
                  .filter((change) => !change.current)
                  .map((change) => (
                    <li key={change.code + ":" + change.reference_id}>
                      {humanize(change.code)}: expected {change.expected_value},
                      actual {change.actual_value ?? "missing"}
                    </li>
                  ))}
              </ul>
            </div>
          ) : null}

          <h3>Validation policy and exact results</h3>
          <p>
            Policy v{review.policy.version} ·{" "}
            <code>{review.policy.manifest_sha256}</code> · Run{" "}
            {review.validation_run.run_id}
          </p>
          <ul>
            {review.validation_run.results.map((result) => (
              <li key={result.result_id}>
                <strong>{humanize(result.category)}</strong>:{" "}
                {humanize(result.outcome)} — {result.detail}
                {result.reused_from_result_id
                  ? ` (reused from ${result.reused_from_result_id})`
                  : ""}
                <ul aria-label={result.check_id + " evidence"}>
                  {result.evidence_references.map((reference) => (
                    <li key={reference}>
                      <code>{reference}</code>
                    </li>
                  ))}
                </ul>
              </li>
            ))}
          </ul>

          {pendingManual?.length ? (
            <h3>Exact-hash Manual Verification</h3>
          ) : null}
          {pendingManual?.map((result) => {
            const item = review.package.items.find(
              (candidate) => candidate.item_id === result.item_id,
            );
            const section = review.package.sections.find((candidate) =>
              candidate.item_ids.includes(item?.item_id ?? ""),
            );
            const rule = [
              ...review.policy.fixed_rules,
              ...review.policy.tender_rules,
            ].find((candidate) => candidate.rule_id === result.check_id);
            if (!item || !section || !rule) return null;
            const outcome = manualOutcomes[result.result_id] ?? "";
            const limitation = manualLimitations[result.result_id] ?? "";
            return (
              <fieldset key={result.result_id}>
                <legend>Verify {item.package_path}</legend>
                <p>
                  Exact SHA-256: <code>{item.content_sha256}</code>
                </p>
                <ul aria-label={item.package_path + " manual checklist"}>
                  {rule.manual_checklist.map((check) => (
                    <li key={check}>{check}</li>
                  ))}
                </ul>
                <p>
                  Evidence:
                  {item.evidence.map((evidence) => (
                    <code
                      key={
                        evidence.reference.artifact_id +
                        ":" +
                        evidence.reference.ordinal
                      }
                    >
                      {evidence.reference.artifact_id}:
                      {evidence.reference.version}:{evidence.reference.ordinal}
                    </code>
                  ))}
                </p>
                <label>
                  Engineer outcome
                  <select
                    value={outcome}
                    disabled={busy || !exactCurrent}
                    onChange={(event) =>
                      setManualOutcomes((current) => ({
                        ...current,
                        [result.result_id]: event.target.value as
                          ManualVerificationResult | "",
                      }))
                    }
                  >
                    <option value="">Select outcome</option>
                    <option value="passed">Passed</option>
                    <option value="failed">Failed</option>
                  </select>
                </label>
                <label>
                  Limitations
                  <textarea
                    value={limitation}
                    disabled={busy || !exactCurrent}
                    onChange={(event) =>
                      setManualLimitations((current) => ({
                        ...current,
                        [result.result_id]: event.target.value,
                      }))
                    }
                  />
                </label>
                <button
                  type="button"
                  disabled={
                    busy ||
                    !exactCurrent ||
                    !outcome ||
                    !section.required_capabilities[0] ||
                    !item.evidence.length
                  }
                  onClick={() =>
                    void act(() =>
                      recordPackageManualVerification({
                        tender_id: tenderId,
                        package_id: review.package.package_id,
                        package_version: review.package.version,
                        package_manifest_sha256: review.package.manifest_sha256,
                        validation_run_id: review.validation_run.run_id,
                        validation_result_id: result.result_id,
                        item_id: item.item_id,
                        content_sha256: item.content_sha256,
                        capability: section.required_capabilities[0],
                        checks: rule.manual_checklist,
                        evidence_references: item.evidence.map(
                          (evidence) =>
                            `source:${evidence.reference.artifact_id}:${evidence.reference.version}:${evidence.reference.ordinal}`,
                        ),
                        result: outcome as ManualVerificationResult,
                        limitations: limitation.trim()
                          ? [limitation.trim()]
                          : [],
                      }),
                    )
                  }
                >
                  Record Manual Verification
                </button>
              </fieldset>
            );
          })}

          {review.manual_verifications.length ? (
            <ul aria-label="Completed Manual Verifications">
              {review.manual_verifications.map((verification) => (
                <li key={verification.verification_id}>
                  {humanize(verification.result)} by{" "}
                  {verification.verifier_identity} ·{" "}
                  <code>{verification.content_sha256}</code>
                  {verification.limitations.length
                    ? ` — ${verification.limitations.join("; ")}`
                    : ""}
                </li>
              ))}
            </ul>
          ) : null}

          <h3>Independent section reviews</h3>
          <ul>
            {review.review_plan.assignments.map((assignment) => {
              const completed = review.reviews.find(
                (item) => item.assignment_id === assignment.assignment_id,
              );
              return (
                <li key={assignment.assignment_id}>
                  {assignment.section_key} / {assignment.envelope_key} ·{" "}
                  {assignment.required_capability} ·{" "}
                  {assignment.reviewer?.identity ?? "No qualified reviewer"}
                  <ul>
                    {assignment.criteria.map((criterion) => (
                      <li key={criterion}>{criterion}</li>
                    ))}
                    {assignment.risk_references.map((risk) => (
                      <li key={risk.reference_id + ":" + risk.version}>
                        Risk <code>{risk.reference_id}</code> ·{" "}
                        <code>{risk.manifest_sha256}</code>
                      </li>
                    ))}
                    {assignment.author_profile_versions.map((author) => (
                      <li
                        key={author.profile_id + ":" + author.profile_version}
                      >
                        Excluded author {author.profile_id} v
                        {author.profile_version}
                      </li>
                    ))}
                  </ul>
                  {completed ? (
                    ` — ${humanize(completed.result)}`
                  ) : assignment.reviewer ? (
                    <button
                      type="button"
                      disabled={busy || !runtimeReady || !exactCurrent}
                      onClick={async () => {
                        if (busy) return;
                        setBusy(true);
                        try {
                          const result = await runSubmissionSectionReview({
                            tender_id: tenderId,
                            package_id: review.package.package_id,
                            package_version: review.package.version,
                            package_manifest_sha256:
                              review.package.manifest_sha256,
                            plan_id: review.review_plan.plan_id,
                            plan_manifest_sha256:
                              review.review_plan.manifest_sha256,
                            assignment_id: assignment.assignment_id,
                          });
                          setReview(result.final_review);
                          onTenderStateChange();
                        } catch {
                          reportCommandFailure();
                        } finally {
                          setBusy(false);
                        }
                      }}
                    >
                      Run independent review
                    </button>
                  ) : null}
                </li>
              );
            })}
          </ul>

          <h3>Findings and exceptions</h3>
          {review.reviews.flatMap((item) => item.findings).length ? (
            <ul>
              {review.reviews
                .flatMap((item) => item.findings)
                .map((finding) => {
                  const owningReview = review.reviews.find((item) =>
                    item.findings.some(
                      (candidate) =>
                        candidate.finding_id === finding.finding_id,
                    ),
                  );
                  const excepted = review.exceptions.some(
                    (exception) => exception.finding_id === finding.finding_id,
                  );
                  const exception = review.exceptions.find(
                    (candidate) => candidate.finding_id === finding.finding_id,
                  );
                  const policyRule = [
                    ...review.policy.fixed_rules,
                    ...review.policy.tender_rules,
                  ].find((rule) => rule.rule_id === finding.policy_rule_id);
                  const rationale =
                    exceptionRationales[finding.finding_id] ?? "";
                  return (
                    <li key={finding.finding_id}>
                      <strong>{humanize(finding.severity)}</strong>:{" "}
                      {finding.summary}
                      <ul aria-label={finding.finding_id + " evidence"}>
                        {finding.evidence_references.map((reference) => (
                          <li key={reference}>
                            <code>{reference}</code>
                          </li>
                        ))}
                      </ul>
                      {exception ? (
                        <p>
                          Engineer exception: {exception.rationale} ·{" "}
                          <code>{exception.manifest_sha256}</code>
                        </p>
                      ) : null}
                      {finding.severity === "major" &&
                      !excepted &&
                      policyRule?.major_exception_allowed &&
                      owningReview ? (
                        <>
                          <label>
                            Exception rationale
                            <textarea
                              value={rationale}
                              disabled={busy || !exactCurrent}
                              onChange={(event) =>
                                setExceptionRationales((current) => ({
                                  ...current,
                                  [finding.finding_id]: event.target.value,
                                }))
                              }
                            />
                          </label>
                          <button
                            type="button"
                            disabled={
                              busy || !exactCurrent || !rationale.trim()
                            }
                            onClick={() =>
                              void act(() =>
                                approvePackageFindingException({
                                  tender_id: tenderId,
                                  package_id: review.package.package_id,
                                  package_version: review.package.version,
                                  package_manifest_sha256:
                                    review.package.manifest_sha256,
                                  review_id: owningReview.review_id,
                                  finding_id: finding.finding_id,
                                  rationale: rationale.trim(),
                                }),
                              )
                            }
                          >
                            Approve permitted exception
                          </button>
                        </>
                      ) : null}
                    </li>
                  );
                })}
            </ul>
          ) : (
            <p>No Final Review findings recorded.</p>
          )}

          <h3>Release Readiness Report</h3>
          <p>
            Report v{review.report.version} ·{" "}
            <code>{review.report.manifest_sha256}</code>
          </p>
          <ul>
            {review.report.summaries.map((summary) => (
              <li key={summary.category}>
                {humanize(summary.category)}: {summary.references.length}
                {summary.references.length ? (
                  <ul>
                    {summary.references.map((reference) => (
                      <li key={reference}>
                        <code>{reference}</code>
                      </li>
                    ))}
                  </ul>
                ) : null}
              </li>
            ))}
          </ul>
          <h4>Underlying decision evidence</h4>
          <ul aria-label="Release readiness underlying evidence">
            {review.package.coverage.map((row) => (
              <li key={row.requirement.requirement_id}>
                <strong>{humanize(row.requirement.kind)}</strong>:{" "}
                {row.requirement.record.title}
                {row.requirement.evidence.length ? (
                  <ul>
                    {row.requirement.evidence.map((evidence) => (
                      <li
                        key={`${evidence.reference.artifact_id}:${evidence.reference.version}:${evidence.reference.ordinal}`}
                      >
                        {evidence.package_path} ·{" "}
                        {evidence.location.structural_path} ·{" "}
                        <span {...evidenceTextAttributes(evidence.location)}>
                          {evidence.location.original_text}
                        </span>
                      </li>
                    ))}
                  </ul>
                ) : null}
                {row.blockers.length ? (
                  <ul>
                    {row.blockers.map((blocker) => (
                      <li key={blocker.code + ":" + blocker.detail}>
                        {humanize(blocker.code)}: {blocker.detail}
                      </li>
                    ))}
                  </ul>
                ) : null}
              </li>
            ))}
          </ul>
          <h4>Qualifications, exclusions, departures, and open queries</h4>
          <ul aria-label="Release readiness decision records">
            {review.decision_evidence.map((decision) => (
              <li
                key={`${decision.binding.kind}:${decision.binding.reference_id}:${decision.binding.version}`}
              >
                <strong>{humanize(decision.category)}</strong>:{" "}
                {decision.binding.summary}
                {decision.question ? <p>Query: {decision.question}</p> : null}
                {decision.ambiguity_or_gap ? (
                  <p>Gap or ambiguity: {decision.ambiguity_or_gap}</p>
                ) : null}
                {decision.treatment ? (
                  <p>Treatment: {humanize(decision.treatment)}</p>
                ) : null}
                {decision.rationale ? (
                  <p>Rationale: {decision.rationale}</p>
                ) : null}
                {decision.treatment_details ? (
                  <p>Decision details: {decision.treatment_details}</p>
                ) : null}
                {decision.closed !== null ? (
                  <p>Status: {decision.closed ? "Closed" : "Open"}</p>
                ) : null}
                <p>
                  {humanize(decision.binding.kind)}{" "}
                  {decision.binding.reference_id} v{decision.binding.version} ·
                  source {humanize(decision.binding.source)} ·{" "}
                  <code>{decision.binding.manifest_sha256}</code>
                </p>
                {decision.binding.approval_id ? (
                  <p>Approval record: {decision.binding.approval_id}</p>
                ) : null}
                {decision.binding.supporting_review_id ? (
                  <p>
                    Independent review: {decision.binding.supporting_review_id}
                  </p>
                ) : null}
              </li>
            ))}
            {review.decision_evidence.length === 0 ? (
              <li>
                No assumptions, qualifications, exclusions, departures, or open
                queries.
              </li>
            ) : null}
          </ul>
          <h4>Exact package evidence and provenance</h4>
          <ul aria-label="Release readiness package items">
            {review.package.items.map((item) => (
              <li key={item.item_id}>
                <strong>{item.package_path}</strong> ·{" "}
                <code>{item.content_sha256}</code>
                <ul>
                  {item.provenance.map((reference) => (
                    <li key={reference}>{reference}</li>
                  ))}
                  {item.evidence.map((evidence) => (
                    <li
                      key={`${evidence.reference.artifact_id}:${evidence.reference.version}:${evidence.reference.ordinal}`}
                    >
                      {evidence.package_path} ·{" "}
                      {evidence.location.structural_path} ·{" "}
                      <span {...evidenceTextAttributes(evidence.location)}>
                        {evidence.location.original_text}
                      </span>
                    </li>
                  ))}
                </ul>
              </li>
            ))}
          </ul>
          {review.live_blockers.length ? (
            <ul aria-label="Release blockers">
              {review.live_blockers.map((blocker) => (
                <li key={`${blocker.code}:${blocker.reference_id}`}>
                  {humanize(blocker.code)}: {blocker.detail}
                </li>
              ))}
            </ul>
          ) : null}
          <p>
            Deadline:{" "}
            {review.report.deadline?.reference_id ?? "No bound deadline"} ·
            Changes:{" "}
            {review.live_changes.filter((change) => !change.current).length}
          </p>

          <h3>Final Approval and Release Copy</h3>
          {release?.approval ? (
            <>
              <p role="status">
                {release.state === "ready_for_submission"
                  ? "Ready for Submission"
                  : "Final Approval revoked by a changed release condition"}
              </p>
              <p>
                Approved by {release.approval.engineer_identity} as{" "}
                {humanize(release.approval.acting_role)} ·{" "}
                <code>{release.approval.canonical_manifest_root}</code>
              </p>
              {release.approval.conditions.length ? (
                <ul aria-label="Final Approval conditions">
                  {release.approval.conditions.map((condition) => (
                    <li key={condition}>{condition}</li>
                  ))}
                </ul>
              ) : null}
              {release.approval.exceptions.length ? (
                <ul aria-label="Final Approval exceptions">
                  {release.approval.exceptions.map((exception) => (
                    <li key={exception}>{exception}</li>
                  ))}
                </ul>
              ) : null}
              <button
                type="button"
                disabled={busy || release.state !== "ready_for_submission"}
                onClick={() =>
                  void actOnRelease(() =>
                    exportReleaseCopy({
                      tender_id: tenderId,
                      approval_id: release.approval!.approval_id,
                      approval_manifest_sha256:
                        release.approval!.manifest_sha256,
                    }),
                  )
                }
              >
                Export and verify Release Copy
              </button>
              {release.exports.map((exportRecord) => (
                <p key={exportRecord.export_id}>
                  Verified Release Copy: {exportRecord.relative_path} ·{" "}
                  {exportRecord.items.length} exact file
                  {exportRecord.items.length === 1 ? "" : "s"} · no submission
                  claimed
                </p>
              ))}
            </>
          ) : (
            <fieldset>
              <legend>Atomic Tendering Manager decision</legend>
              <label>
                Approval rationale
                <textarea
                  value={releaseRationale}
                  disabled={busy || !exactCurrent || !review.ready}
                  onChange={(event) => setReleaseRationale(event.target.value)}
                />
              </label>
              <label>
                Conditions (one per line)
                <textarea
                  value={releaseConditions}
                  disabled={busy || !exactCurrent || !review.ready}
                  onChange={(event) => setReleaseConditions(event.target.value)}
                />
              </label>
              <label>
                Approved exceptions (one per line)
                <textarea
                  value={releaseExceptions}
                  disabled={busy || !exactCurrent || !review.ready}
                  onChange={(event) => setReleaseExceptions(event.target.value)}
                />
              </label>
              <button
                type="button"
                disabled={
                  busy ||
                  !exactCurrent ||
                  !review.ready ||
                  !releaseRationale.trim()
                }
                onClick={() =>
                  void actOnRelease(() =>
                    approveSubmissionRelease({
                      tender_id: tenderId,
                      package_id: review.package.package_id,
                      package_version: review.package.version,
                      package_manifest_sha256: review.package.manifest_sha256,
                      readiness_report_id: review.report.report_id,
                      readiness_report_version: review.report.version,
                      readiness_report_manifest_sha256:
                        review.report.manifest_sha256,
                      rationale: releaseRationale.trim(),
                      conditions: releaseConditions
                        .split("\n")
                        .map((value) => value.trim())
                        .filter(Boolean),
                      exceptions: releaseExceptions
                        .split("\n")
                        .map((value) => value.trim())
                        .filter(Boolean),
                    }),
                  )
                }
              >
                Approve exact package as Ready for Submission
              </button>
              <p>
                This decision freezes the exact package and permits a verified
                copy. It does not email, upload, deliver, or submit anything.
              </p>
            </fieldset>
          )}
        </>
      )}
    </section>
  );
}
