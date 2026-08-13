import { useCallback, useEffect, useRef, useState } from "react";

import type { FinalReviewInspection } from "./bindings/FinalReviewInspection";
import type { ManualVerificationResult } from "./bindings/ManualVerificationResult";
import type { SubmissionPackageVersion } from "./bindings/SubmissionPackageVersion";
import {
  approvePackageFindingException,
  inspectCurrentSubmissionPackage,
  inspectFinalReview,
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
  const [busy, setBusy] = useState(false);
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
      if (request !== requestGeneration.current) return;
      setSubmissionPackage(assembled);
      setReview(inspected);
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
        </>
      )}
    </section>
  );
}
