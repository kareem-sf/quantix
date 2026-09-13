import { useState } from "react";
import { useApi, useRefresh, useResource, type Schema } from "../api";
import { ErrorNotice, Loading } from "../components/common";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Textarea } from "@/components/ui/textarea";

type Decision = Schema<"BenchmarkAdoptionDecision">;

export function BenchmarkAdoptionReviewPanel() {
  const decisions = useResource<Decision[]>("/benchmark-adoption");
  return (
    <details>
      <summary>AI benchmark reviews</summary>
      <p>
        Recorded critical engineering errors block that exact AI configuration.
        A complete passing live benchmark is required to clear them. New
        configurations remain available without a quality claim.
      </p>
      {decisions.isPending ? (
        <Loading>Loading benchmark reviews…</Loading>
      ) : decisions.error ? (
        <ErrorNotice error={decisions.error} />
      ) : (
        <>
          {!decisions.data?.length && (
            <p>
              No live benchmark decisions have been recorded. Dataset checks and
              synthetic model tests do not qualify an AI configuration.
            </p>
          )}
          {decisions.data?.map((decision) => (
            <DecisionCard
              key={decision.configuration_hash + decision.report_hash}
              decision={decision}
            />
          ))}
        </>
      )}
    </details>
  );
}

function DecisionCard({ decision }: { decision: Decision }) {
  const api = useApi(),
    refresh = useRefresh();
  const [rationale, setRationale] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  async function review() {
    if (busy || !confirmed || rationale.trim().length < 10) return;
    setBusy(true);
    setError(null);
    try {
      await api.post("/benchmark-adoption/review", {
        configuration_hash: decision.configuration_hash,
        report_id: decision.report_id,
        report_hash: decision.report_hash,
        baseline_report_id: decision.baseline_report_id,
        baseline_report_hash: decision.baseline_report_hash,
        engineer_confirmed: true,
        rationale,
      });
      await refresh();
    } catch (cause) {
      setError(cause);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Card>
      <CardHeader>
        <CardTitle>
          {decision.model_id}:{" "}
          {decision.state === "critical_block"
            ? "Critical error — work paused"
            : decision.state === "review_required"
              ? "Comparison needs review"
              : "Accepted for this recorded configuration"}
        </CardTitle>
      </CardHeader>
      <CardContent className="stack">
        <p>
          Manager version {decision.manager_profile_version}.{" "}
          {new Date(decision.updated_at).toLocaleString()}
        </p>
        {decision.reasons.length > 0 && (
          <ul>
            {decision.reasons.map((reason) => (
              <li key={reason}>{reason}</li>
            ))}
          </ul>
        )}
        <details>
          <summary>Exact evaluation records</summary>
          <p>
            Read the exact saved attempts and evidence used for this comparison.
          </p>
          <VerifiedReport
            label="candidate"
            id={decision.report_id}
            hash={decision.report_hash}
          />
          {decision.baseline_report_id && decision.baseline_report_hash && (
            <VerifiedReport
              label="baseline"
              id={decision.baseline_report_id}
              hash={decision.baseline_report_hash}
            />
          )}
          <dl>
            <dt>Configuration</dt>
            <dd className="break-all">{decision.configuration_hash}</dd>
            <dt>Candidate report</dt>
            <dd>{decision.report_id}</dd>
            <dt>Candidate hash</dt>
            <dd className="break-all">{decision.report_hash}</dd>
            <dt>Baseline report</dt>
            <dd>
              {decision.baseline_report_id ?? "No previous passing evaluation"}
            </dd>
            <dt>Baseline hash</dt>
            <dd className="break-all">
              {decision.baseline_report_hash ?? "—"}
            </dd>
          </dl>
        </details>
        {decision.state === "critical_block" && (
          <p>
            Run a corrected synthetic benchmark with an explicit account, model
            and spending limit. Review cannot waive a critical engineering
            error.
          </p>
        )}
        {decision.state === "review_required" && (
          <form
            className="stack"
            onSubmit={(event) => {
              event.preventDefault();
              void review();
            }}
          >
            {error != null && <ErrorNotice error={error} />}
            <label>
              Why is this measured regression acceptable?
              <Textarea
                value={rationale}
                onChange={(event) => {
                  setRationale(event.target.value);
                  setConfirmed(false);
                }}
                minLength={10}
                maxLength={4000}
                required
              />
            </label>
            <label>
              <input
                type="checkbox"
                checked={confirmed}
                onChange={(event) => setConfirmed(event.target.checked)}
              />
              I reviewed these exact reports and accept the listed performance
              or cost changes.
            </label>
            <Button
              type="submit"
              disabled={busy || !confirmed || rationale.trim().length < 10}
            >
              Accept this comparison
            </Button>
          </form>
        )}
        {decision.review_rationale && (
          <p>Engineer review: {decision.review_rationale}</p>
        )}
      </CardContent>
    </Card>
  );
}

function VerifiedReport({
  label,
  id,
  hash,
}: {
  label: string;
  id: string;
  hash: string;
}) {
  const api = useApi();
  const [report, setReport] = useState<Schema<"BenchmarkReport"> | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  async function read() {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      setReport(
        await api.get<Schema<"BenchmarkReport">>(
          `/benchmark-adoption/reports/${encodeURIComponent(id)}?expected_hash=${hash}`,
        ),
      );
    } catch (cause) {
      setError(cause);
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="stack">
      <Button
        type="button"
        variant="outline"
        disabled={busy}
        onClick={() => void read()}
      >
        Show {label} records
      </Button>
      {error != null && <ErrorNotice error={error} />}
      {report && (
        <div>
          <p>
            {report.dataset_version}; evaluator {report.evaluator_version}.{" "}
            {report.cases?.length ?? 0} retained attempts.{" "}
            {report.overall_completed
              ? "All engineering assertions and usage checks passed."
              : "This evaluation did not complete every assertion and usage check."}
          </p>
          {report.cases?.map((attempt) => (
            <details key={`${attempt.case_id}-${attempt.repetition}`}>
              <summary>
                {attempt.case_id}, attempt {attempt.repetition}:{" "}
                {attempt.scores.task_completed ? "completed" : "did not pass"}
              </summary>
              <p>
                Latency {attempt.observed.latency_seconds ?? "unknown"} seconds;
                requests {attempt.observed.requests ?? "unknown"}; input/output
                tokens {attempt.observed.input_tokens ?? "unknown"} /{" "}
                {attempt.observed.output_tokens ?? "unknown"}; estimated USD{" "}
                {attempt.observed.estimated_cost_usd ?? "unknown"}.
              </p>
              <pre className="whitespace-pre-wrap break-all">
                {JSON.stringify(attempt, null, 2)}
              </pre>
            </details>
          ))}
        </div>
      )}
    </div>
  );
}
