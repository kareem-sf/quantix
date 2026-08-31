import { LoaderCircle } from "lucide-react";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FormEvent,
} from "react";

import type { BasisOfEstimateVersion } from "./bindings/BasisOfEstimateVersion";
import {
  approveBasisOfEstimate,
  inspectEstimateWorkspace,
  runBasisOfEstimateReview,
} from "./quantixHost";

interface TenderEstimateReviewProps {
  tenderId: string;
  runtimeReady: boolean;
  reportCommandFailure: () => void;
  onRefresh: () => Promise<void>;
  onOpenCalculations: () => void;
  onClose: () => void;
}

function humanize(value: string): string {
  return value.split("_").join(" ");
}

function lines(values: string[]): string {
  return values.join(" · ");
}

export function TenderEstimateReview({
  tenderId,
  runtimeReady,
  reportCommandFailure,
  onRefresh,
  onOpenCalculations,
  onClose,
}: TenderEstimateReviewProps) {
  const [basis, setBasis] = useState<BasisOfEstimateVersion | null>(null);
  const [loading, setLoading] = useState(true);
  const [pending, setPending] = useState(false);
  const [approvalRationale, setApprovalRationale] = useState("");
  const requestGeneration = useRef(0);

  const loadBasis = useCallback(async () => {
    const generation = ++requestGeneration.current;
    setLoading(true);
    try {
      const next = await inspectEstimateWorkspace(tenderId);
      if (generation !== requestGeneration.current) return;
      setBasis(next.basis);
    } catch {
      if (generation === requestGeneration.current) reportCommandFailure();
    } finally {
      if (generation === requestGeneration.current) setLoading(false);
    }
  }, [reportCommandFailure, tenderId]);

  useEffect(() => {
    void loadBasis();
    return () => {
      requestGeneration.current += 1;
    };
  }, [loadBasis]);

  const mutate = useCallback(
    async (work: () => Promise<unknown>): Promise<boolean> => {
      setPending(true);
      try {
        await work();
        await loadBasis();
        await onRefresh();
        return true;
      } catch {
        reportCommandFailure();
        return false;
      } finally {
        setPending(false);
      }
    },
    [loadBasis, onRefresh, reportCommandFailure],
  );

  const runReview = (target: BasisOfEstimateVersion) => {
    void mutate(() =>
      runBasisOfEstimateReview({
        tender_id: tenderId,
        basis_id: target.basis_id,
        version: target.version,
      }),
    );
  };

  const approveBasis = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!basis || basis.review?.outcome !== "passed" || basis.approval) return;
    const rationale = approvalRationale.trim();
    if (!rationale) return;
    void mutate(async () => {
      await approveBasisOfEstimate({
        tender_id: tenderId,
        basis_id: basis.basis_id,
        version: basis.version,
        manifest_sha256: basis.manifest_sha256,
        rationale,
      });
      setApprovalRationale("");
    }).then((succeeded) => {
      if (succeeded) onClose();
    });
  };

  const reviewPending =
    basis !== null &&
    basis.current &&
    basis.complete &&
    basis.reconciled &&
    !basis.review &&
    !basis.approval;
  const approvalPending =
    basis !== null &&
    basis.current &&
    basis.review?.outcome === "passed" &&
    !basis.approval;

  return (
    <section
      className="tender-estimate"
      data-testid="tender-estimate-review"
      aria-labelledby="tender-estimate-title"
    >
      <header className="tender-estimate__header">
        <div>
          <p className="section-label">Tender decision workspace</p>
          <h2 id="tender-estimate-title">Basis of Estimate review</h2>
          <p>
            The Cost Estimator's estimate is recorded as a controlled Basis of
            Estimate. It is independently reviewed before you approve it for
            reliance, and material findings come to you here.
          </p>
        </div>
        <button
          type="button"
          className="manager-workspace__secondary"
          onClick={onClose}
        >
          Back to Manager conversation
        </button>
      </header>
      {loading && !basis ? (
        <p className="tender-estimate__loading" role="status">
          <LoaderCircle size={16} aria-hidden="true" /> Loading the Basis of
          Estimate…
        </p>
      ) : !basis ? (
        <p className="tender-estimate__empty">
          No Basis of Estimate has been published for this Tender yet.
        </p>
      ) : (
        <>
          <section
            className="tender-estimate__card"
            aria-label="Basis of Estimate content"
            data-testid="estimate-basis-card"
          >
            <h3>
              Basis of Estimate{" "}
              <code>
                {basis.basis_id} · v{basis.version}
              </code>
            </h3>
            <p className="tender-estimate__status">
              {basis.approval
                ? "Approved for reliance"
                : basis.review
                  ? basis.review.outcome === "passed"
                    ? "Review passed — awaiting approval"
                    : "Review found problems"
                  : basis.complete && basis.reconciled
                    ? "Waiting for independent review"
                    : "Not complete yet"}
              {basis.current ? "" : " · superseded by a newer version"}
            </p>
            <p>
              <strong>Reconciled total:</strong>{" "}
              <span>
                {basis.total_amount} {basis.total_currency}
              </span>
            </p>
            <dl className="tender-estimate__facts">
              <div>
                <dt>Scope</dt>
                <dd>{basis.scope}</dd>
              </div>
              <div>
                <dt>Pricing date</dt>
                <dd>{basis.pricing_date}</dd>
              </div>
              <div>
                <dt>Currencies</dt>
                <dd>{lines(basis.currencies)}</dd>
              </div>
              {basis.taxes.length > 0 ? (
                <div>
                  <dt>Taxes</dt>
                  <dd>{lines(basis.taxes)}</dd>
                </div>
              ) : null}
              {basis.exclusions.length > 0 ? (
                <div>
                  <dt>Exclusions</dt>
                  <dd>{lines(basis.exclusions)}</dd>
                </div>
              ) : null}
              {basis.gaps.length > 0 ? (
                <div>
                  <dt>Disclosed gaps</dt>
                  <dd>{lines(basis.gaps)}</dd>
                </div>
              ) : null}
            </dl>
            {basis.boq_rows.length > 0 ? (
              <table className="tender-estimate__table">
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
            <div className="tender-estimate__links">
              <h4>Evidence and assumption links</h4>
              <ul>
                {basis.quotations.map((quotation) => (
                  <li key={quotation.quotation_id}>
                    Quotation from {quotation.counterparty} (
                    {quotation.currency}) · evidence{" "}
                    <code>
                      {quotation.evidence.artifact_id} · v
                      {quotation.evidence.version} ·{" "}
                      {quotation.evidence.ordinal}
                    </code>
                  </li>
                ))}
                {basis.allowances.map((allowance) => (
                  <li key={allowance.allowance_id}>
                    Allowance {allowance.description} · query{" "}
                    {allowance.query_id} v{allowance.query_version}
                  </li>
                ))}
                {basis.material_assumptions.map((assumption) => (
                  <li key={assumption.decision_id}>
                    Approved assumption for query {assumption.query_id} v
                    {assumption.query_version}: {assumption.rationale} ·{" "}
                    {humanize(assumption.treatment)} ·{" "}
                    {assumption.treatment_details}
                  </li>
                ))}
              </ul>
            </div>
            <button
              type="button"
              className="manager-workspace__secondary"
              disabled={pending || loading}
              onClick={onOpenCalculations}
            >
              Open controlled calculations
            </button>
          </section>
          {basis.review ? (
            <section
              className="tender-estimate__card"
              aria-label="Independent review findings"
              data-testid="estimate-review-findings"
            >
              <h3>Independent review</h3>
              <p>
                Outcome: <strong>{basis.review.outcome}</strong> ·{" "}
                {basis.review.findings.length} finding
                {basis.review.findings.length === 1 ? "" : "s"}
              </p>
              {basis.review.findings.length === 0 ? (
                <p>No findings were reported.</p>
              ) : (
                <ul className="tender-estimate__findings">
                  {basis.review.findings.map((finding) => (
                    <li key={finding.code}>
                      <strong>{finding.code}</strong> ·{" "}
                      <span>{finding.summary}</span>
                      {finding.affected_boq_row_keys.length > 0 ? (
                        <span>
                          {" "}
                          · Affected rows:{" "}
                          {finding.affected_boq_row_keys.join(", ")}
                        </span>
                      ) : null}
                    </li>
                  ))}
                </ul>
              )}
            </section>
          ) : null}
          {reviewPending ? (
            <section
              className="tender-estimate__card"
              aria-label="Run independent review"
            >
              <h3>Independent review</h3>
              <p>
                The basis is complete and reconciled. An independent reviewer
                must check it before you can approve it for reliance.
              </p>
              <button
                type="button"
                className="manager-workspace__primary"
                disabled={pending || loading || !runtimeReady}
                onClick={() => runReview(basis)}
              >
                Run independent review
              </button>
            </section>
          ) : null}
          {approvalPending ? (
            <form
              className="tender-estimate__card tender-estimate__form"
              aria-label="Approve the Basis of Estimate"
              onSubmit={approveBasis}
              data-testid="estimate-approval-form"
            >
              <h3>Approve for reliance</h3>
              <p>
                The independent review passed. Your approval binds exactly this
                basis version and manifest before any pricing may rely on it.
              </p>
              <label>
                Approval rationale
                <textarea
                  value={approvalRationale}
                  disabled={pending || loading}
                  onChange={(event) => setApprovalRationale(event.target.value)}
                />
              </label>
              <button
                type="submit"
                className="manager-workspace__primary"
                disabled={
                  pending || loading || approvalRationale.trim().length === 0
                }
              >
                Approve basis for reliance
              </button>
            </form>
          ) : null}
          {basis.approval ? (
            <section
              className="tender-estimate__card"
              aria-label="Recorded approval"
            >
              <h3>Recorded approval</h3>
              <p>
                Approved by you on {basis.approval.created_at}:{" "}
                {basis.approval.rationale}
              </p>
              <p>
                The aggregate calculation is approved for reliance, and pricing
                may use this basis.
              </p>
            </section>
          ) : null}
        </>
      )}
    </section>
  );
}
