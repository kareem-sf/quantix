import { useCallback, useEffect, useState } from "react";

import type { AgentRunInspection } from "./bindings/AgentRunInspection";
import type { AgentRunRecoveryDisposition } from "./bindings/AgentRunRecoveryDisposition";
import {
  inspectAgentRuns,
  interruptAgentRun,
  resolveIndeterminateAgentRun,
  runBootstrapAgent,
} from "./quantixHost";

interface AgentRunOfficeProps {
  tenderId: string;
  runtimeReady: boolean;
  reportCommandFailure: () => void;
}

type RunsState =
  | { kind: "loading" }
  | { kind: "ready"; runs: AgentRunInspection[] }
  | { kind: "error" };

const readable = (value: string) => value.replace(/_/g, " ");

export function AgentRunOffice({
  tenderId,
  runtimeReady,
  reportCommandFailure,
}: AgentRunOfficeProps) {
  const [runsState, setRunsState] = useState<RunsState>({ kind: "loading" });
  const [runningCommand, setRunningCommand] = useState(false);
  const [interruptingRunId, setInterruptingRunId] = useState<string>();
  const [resolvingRunId, setResolvingRunId] = useState<string>();
  const [recoveryRationale, setRecoveryRationale] = useState("");
  const runs = runsState.kind === "ready" ? runsState.runs : [];
  const hasRunningRun = runs.some((run) => run.state === "running");
  const unresolvedIndeterminateRuns = runs.filter(
    (run) => run.state === "indeterminate" && !run.recovery_decision,
  );
  const pendingRecoveryRetries = runs.filter(
    (run) =>
      run.state === "indeterminate" &&
      run.recovery_decision?.disposition === "retry_task" &&
      !runs.some((candidate) => candidate.retry_of_run_id === run.run_id),
  );
  const recoveryBlocksNewRun =
    unresolvedIndeterminateRuns.length > 0 || pendingRecoveryRetries.length > 0;

  const refresh = useCallback(async () => {
    try {
      setRunsState({
        kind: "ready",
        runs: (await inspectAgentRuns(tenderId)).reverse(),
      });
    } catch {
      setRunsState({ kind: "error" });
      reportCommandFailure();
    }
  }, [reportCommandFailure, tenderId]);

  useEffect(() => {
    setRunsState({ kind: "loading" });
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!runningCommand && !hasRunningRun) return;
    const interval = window.setInterval(() => void refresh(), 500);
    return () => window.clearInterval(interval);
  }, [hasRunningRun, refresh, runningCommand]);

  const executeAgentRun = async (retryOfRunId: string | null = null) => {
    setRunningCommand(true);
    try {
      await runBootstrapAgent(tenderId, retryOfRunId);
    } catch {
      reportCommandFailure();
    } finally {
      await refresh();
      setRunningCommand(false);
    }
  };

  const interrupt = async (runId: string) => {
    setInterruptingRunId(runId);
    try {
      if (!(await interruptAgentRun(tenderId, runId))) {
        await refresh();
      }
    } catch {
      reportCommandFailure();
    } finally {
      setInterruptingRunId(undefined);
    }
  };

  const resolveRecovery = async (
    runId: string,
    disposition: AgentRunRecoveryDisposition,
  ) => {
    const rationale = recoveryRationale.trim();
    if (!rationale) return;
    setResolvingRunId(runId);
    try {
      await resolveIndeterminateAgentRun(
        tenderId,
        runId,
        disposition,
        rationale,
      );
      setRecoveryRationale("");
      await refresh();
    } catch {
      reportCommandFailure();
    } finally {
      setResolvingRunId(undefined);
    }
  };

  return (
    <section className="agent-office" aria-labelledby="agent-office-title">
      <div className="agent-office__heading">
        <div>
          <p className="section-label">Controlled delegation</p>
          <h4 id="agent-office-title">Agent Profile turns</h4>
        </div>
        <span>{runs.length} runs</span>
      </div>
      <p className="agent-office__introduction">
        Run the immutable Bootstrap Tender Analyst profile for this Tender.
        Every result remains Proposed until an independent Engineer review.
      </p>
      <div className="agent-office__actions">
        <button
          type="button"
          onClick={() => void executeAgentRun()}
          disabled={
            !runtimeReady ||
            runningCommand ||
            hasRunningRun ||
            recoveryBlocksNewRun
          }
        >
          {runningCommand || hasRunningRun
            ? "Agent Turn running"
            : "Run Bootstrap Agent"}
        </button>
        <button
          type="button"
          className="button-secondary"
          onClick={() => void refresh()}
          disabled={runsState.kind === "loading"}
        >
          Refresh runs
        </button>
      </div>

      {unresolvedIndeterminateRuns.length > 0 ? (
        <p className="catalogue-error" role="status">
          An accepted Provider Turn has an unknown outcome. Inspect its facts
          and record an Engineer disposition before any further task runs.
        </p>
      ) : null}

      {runsState.kind === "loading" ? (
        <p className="agent-office__message" aria-live="polite">
          Loading Agent Runs...
        </p>
      ) : null}
      {runsState.kind === "error" ? (
        <p className="catalogue-error" role="alert">
          Agent Run inspection is unavailable. The Tender was not changed.
        </p>
      ) : null}
      {runsState.kind === "ready" && runs.length === 0 ? (
        <p className="agent-office__message">
          No Agent Runs yet. The first run creates one exact Tender Task and one
          Provider Turn.
        </p>
      ) : null}
      {runs.length > 0 ? (
        <ol className="agent-run-list" aria-label="Agent Run history">
          {runs.map((run) => (
            <li className="agent-run" key={run.run_id}>
              <div className="agent-run__summary">
                <div>
                  <span
                    className={`agent-run__state agent-run__state--${run.state}`}
                  >
                    {readable(run.state)}
                  </span>
                  <h5>{run.profile.identity}</h5>
                  <p>
                    {run.profile.profession} / profile v{run.profile.version} /
                    task {run.task.task_id.slice(0, 8)}
                  </p>
                </div>
                <div className="agent-run__controls">
                  {run.state === "running" ? (
                    <button
                      type="button"
                      className="agent-run__interrupt"
                      onClick={() => void interrupt(run.run_id)}
                      disabled={interruptingRunId === run.run_id}
                    >
                      {interruptingRunId === run.run_id
                        ? "Interrupting..."
                        : "Interrupt"}
                    </button>
                  ) : null}
                  {run.failure?.retry_safe ? (
                    <button
                      type="button"
                      onClick={() => void executeAgentRun(run.run_id)}
                      disabled={
                        !runtimeReady ||
                        runningCommand ||
                        hasRunningRun ||
                        recoveryBlocksNewRun
                      }
                    >
                      Create linked retry
                    </button>
                  ) : null}
                  {run.recovery_decision?.disposition === "retry_task" &&
                  !runs.some(
                    (candidate) => candidate.retry_of_run_id === run.run_id,
                  ) ? (
                    <button
                      type="button"
                      onClick={() => void executeAgentRun(run.run_id)}
                      disabled={
                        !runtimeReady ||
                        runningCommand ||
                        hasRunningRun ||
                        unresolvedIndeterminateRuns.length > 0
                      }
                    >
                      Create attributable retry
                    </button>
                  ) : null}
                </div>
              </div>

              <dl className="agent-run__facts">
                <div>
                  <dt>Run</dt>
                  <dd title={run.run_id}>{run.run_id.slice(0, 12)}</dd>
                </div>
                <div>
                  <dt>Provider Thread</dt>
                  <dd title={run.provider_thread_ref ?? undefined}>
                    {run.provider_thread_ref?.slice(0, 16) ?? "Not established"}
                  </dd>
                </div>
                <div>
                  <dt>Provider Turn</dt>
                  <dd title={run.provider_turn_ref ?? undefined}>
                    {run.provider_turn_ref?.slice(0, 16) ?? "Not accepted"}
                  </dd>
                </div>
                <div>
                  <dt>Usage</dt>
                  <dd>
                    {run.usage.total_tokens === null
                      ? "Not reported"
                      : `${run.usage.total_tokens} tokens`}
                  </dd>
                </div>
              </dl>

              {run.failure ? (
                <div className="agent-run__failure" role="status">
                  <strong>{readable(run.failure.category)}</strong>
                  <span>{run.failure.required_user_action}</span>
                </div>
              ) : null}
              {run.state === "indeterminate" && !run.recovery_decision ? (
                <div className="agent-run__recovery" role="group">
                  <strong>Engineer disposition required</strong>
                  <label>
                    Evidence-based rationale
                    <textarea
                      value={recoveryRationale}
                      onChange={(event) =>
                        setRecoveryRationale(event.target.value)
                      }
                      maxLength={500}
                      disabled={resolvingRunId === run.run_id}
                    />
                  </label>
                  <div className="agent-run__controls">
                    <button
                      type="button"
                      onClick={() =>
                        void resolveRecovery(run.run_id, "retry_task")
                      }
                      disabled={
                        resolvingRunId === run.run_id ||
                        !recoveryRationale.trim()
                      }
                    >
                      Authorize one linked retry
                    </button>
                    <button
                      type="button"
                      className="button-secondary"
                      onClick={() =>
                        void resolveRecovery(run.run_id, "close_task")
                      }
                      disabled={
                        resolvingRunId === run.run_id ||
                        !recoveryRationale.trim()
                      }
                    >
                      Close uncertain task
                    </button>
                  </div>
                </div>
              ) : null}
              {run.recovery_decision ? (
                <div className="agent-run__recovery" role="status">
                  <strong>
                    Engineer disposition:{" "}
                    {readable(run.recovery_decision.disposition)}
                  </strong>
                  <span>{run.recovery_decision.rationale}</span>
                  <small>
                    {run.recovery_decision.decided_by} ·{" "}
                    {run.recovery_decision.decided_at}
                  </small>
                </div>
              ) : null}
              {run.proposed_result ? (
                <div className="agent-run__result">
                  <div>
                    <strong>Proposed result</strong>
                    <span>Independent review required</span>
                  </div>
                  <pre>{run.proposed_result.payload_json}</pre>
                </div>
              ) : null}

              <details className="agent-run__details">
                <summary>Inspect exact task, permissions, and events</summary>
                <dl>
                  <div>
                    <dt>Objective</dt>
                    <dd>{run.task.objective}</dd>
                  </div>
                  <div>
                    <dt>Review policy</dt>
                    <dd>{run.task.review_policy}</dd>
                  </div>
                  <div>
                    <dt>Deadline</dt>
                    <dd>{run.task.deadline}</dd>
                  </div>
                  <div>
                    <dt>Permissions</dt>
                    <dd>
                      Network denied /{" "}
                      {run.task.permissions.allowed_tools.length} tools /{" "}
                      {run.task.permissions.data_scopes.join(", ")}
                    </dd>
                  </div>
                </dl>
                <ol className="agent-run__events">
                  {run.events.map((event) => (
                    <li key={event.sequence}>
                      <span>{event.sequence}</span>
                      <strong>{readable(event.kind)}</strong>
                      <p>{event.summary}</p>
                    </li>
                  ))}
                </ol>
              </details>
            </li>
          ))}
        </ol>
      ) : null}
    </section>
  );
}
