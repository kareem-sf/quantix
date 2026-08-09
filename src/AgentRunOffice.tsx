import { useCallback, useEffect, useRef, useState } from "react";

import type { AgentRunActivity } from "./bindings/AgentRunActivity";
import type { AgentRunHistoryPage } from "./bindings/AgentRunHistoryPage";
import type { AgentRunInspection } from "./bindings/AgentRunInspection";
import type { AgentRunRecoveryDisposition } from "./bindings/AgentRunRecoveryDisposition";
import {
  inspectAgentRunActivity,
  inspectAgentRunHistory,
  inspectAgentRun,
  interruptAgentRun,
  resolveIndeterminateAgentRun,
  runBootstrapAgent,
} from "./quantixHost";

interface AgentRunOfficeProps {
  tenderId: string;
  runtimeReady: boolean;
  reportCommandFailure: () => void;
  refreshToken: number;
  productionScheduling: boolean;
}

type RunsState =
  | { kind: "loading" }
  | { kind: "ready"; page: AgentRunHistoryPage }
  | { kind: "error" };

const readable = (value: string) => value.replace(/_/g, " ");

export function AgentRunOffice({
  tenderId,
  runtimeReady,
  reportCommandFailure,
  refreshToken,
  productionScheduling,
}: AgentRunOfficeProps) {
  const [runsState, setRunsState] = useState<RunsState>({ kind: "loading" });
  const [runningCommand, setRunningCommand] = useState(false);
  const [interruptingRunId, setInterruptingRunId] = useState<string>();
  const [resolvingRunId, setResolvingRunId] = useState<string>();
  const [recoveryRationale, setRecoveryRationale] = useState("");
  const [beforeSequence, setBeforeSequence] = useState<bigint | null>(null);
  const [cursorStack, setCursorStack] = useState<(bigint | null)[]>([]);
  const [pageLoading, setPageLoading] = useState(false);
  const [activity, setActivity] = useState<AgentRunActivity>();
  const [runDetails, setRunDetails] = useState<
    Record<string, AgentRunInspection>
  >({});
  const [loadingDetailRunIds, setLoadingDetailRunIds] = useState<Set<string>>(
    new Set(),
  );
  const activityRef = useRef<AgentRunActivity | undefined>(undefined);
  const activityRequestGeneration = useRef(0);
  const pageRequestGeneration = useRef(0);
  const beforeSequenceRef = useRef<bigint | null>(null);
  const runDetailsRef = useRef<Record<string, AgentRunInspection>>({});
  const detailRequestGenerations = useRef<Record<string, number>>({});
  const expandedRunIds = useRef<Set<string>>(new Set());
  const visibleRunStates = useRef<Map<string, string>>(new Map());
  const observedTenderId = useRef<string | undefined>(undefined);
  const observedRefreshToken = useRef<number | undefined>(undefined);
  const runs =
    runsState.kind === "ready"
      ? runsState.page.items.map((item) => item.run)
      : [];
  const hasRunningRun =
    (activity?.running_count ?? 0) > 0 ||
    runs.some((run) => run.state === "running");
  const unresolvedIndeterminateRuns = runs.filter(
    (run) => run.state === "indeterminate" && !run.recovery_decision,
  );
  const pendingRecoveryRetries = runs.filter(
    (run) =>
      run.state === "indeterminate" &&
      run.linked_retry_supported &&
      run.recovery_decision?.disposition === "retry_task" &&
      !run.has_linked_retry,
  );
  const recoveryBlocksNewRun =
    unresolvedIndeterminateRuns.length > 0 || pendingRecoveryRetries.length > 0;

  const loadPage = useCallback(
    async (cursor: bigint | null, nextCursorStack: (bigint | null)[]) => {
      const generation = ++pageRequestGeneration.current;
      setPageLoading(true);
      try {
        const page = await inspectAgentRunHistory(tenderId, cursor, 4);
        if (generation !== pageRequestGeneration.current) return;
        visibleRunStates.current = new Map(
          page.items.map((item) => [item.run.run_id, item.run.state]),
        );
        for (const runId of expandedRunIds.current) {
          if (visibleRunStates.current.has(runId)) continue;
          expandedRunIds.current.delete(runId);
          detailRequestGenerations.current[runId] =
            (detailRequestGenerations.current[runId] ?? 0) + 1;
        }
        runDetailsRef.current = Object.fromEntries(
          Object.entries(runDetailsRef.current).filter(([runId]) =>
            visibleRunStates.current.has(runId),
          ),
        );
        setRunDetails(runDetailsRef.current);
        beforeSequenceRef.current = cursor;
        setBeforeSequence(cursor);
        setCursorStack(nextCursorStack);
        setRunsState({ kind: "ready", page });
      } catch {
        if (generation !== pageRequestGeneration.current) return;
        setRunsState({ kind: "error" });
        reportCommandFailure();
      } finally {
        if (generation === pageRequestGeneration.current) {
          setPageLoading(false);
        }
      }
    },
    [reportCommandFailure, tenderId],
  );

  const loadRunDetail = useCallback(
    async (runId: string, force = false) => {
      if (!force && runDetailsRef.current[runId]) return;
      const generation = (detailRequestGenerations.current[runId] ?? 0) + 1;
      detailRequestGenerations.current[runId] = generation;
      setLoadingDetailRunIds((current) => new Set(current).add(runId));
      try {
        const detail = await inspectAgentRun(tenderId, runId);
        if (detailRequestGenerations.current[runId] !== generation) return;
        runDetailsRef.current = {
          ...runDetailsRef.current,
          [runId]: detail,
        };
        setRunDetails(runDetailsRef.current);
      } catch {
        if (detailRequestGenerations.current[runId] === generation) {
          reportCommandFailure();
        }
      } finally {
        if (detailRequestGenerations.current[runId] === generation) {
          setLoadingDetailRunIds((current) => {
            const next = new Set(current);
            next.delete(runId);
            return next;
          });
        }
      }
    },
    [reportCommandFailure, tenderId],
  );

  const refreshActivity = useCallback(async () => {
    const generation = ++activityRequestGeneration.current;
    try {
      const nextActivity = await inspectAgentRunActivity(tenderId);
      if (generation !== activityRequestGeneration.current) return false;
      const previous = activityRef.current;
      activityRef.current = nextActivity;
      setActivity(nextActivity);
      return (
        !previous ||
        previous.run_count !== nextActivity.run_count ||
        previous.event_count !== nextActivity.event_count ||
        previous.running_count !== nextActivity.running_count
      );
    } catch {
      if (generation === activityRequestGeneration.current) {
        reportCommandFailure();
      }
      return false;
    }
  }, [reportCommandFailure, tenderId]);

  const refreshLatest = useCallback(async () => {
    await Promise.all([
      loadPage(null, []),
      refreshActivity(),
      ...[...expandedRunIds.current].map((runId) => loadRunDetail(runId, true)),
    ]);
  }, [loadPage, loadRunDetail, refreshActivity]);

  useEffect(() => {
    const tenderChanged = observedTenderId.current !== tenderId;
    const refreshRequested = observedRefreshToken.current !== refreshToken;
    if (!tenderChanged && !refreshRequested) return;
    observedTenderId.current = tenderId;
    observedRefreshToken.current = refreshToken;
    if (tenderChanged) {
      activityRef.current = undefined;
      runDetailsRef.current = {};
      detailRequestGenerations.current = {};
      expandedRunIds.current.clear();
      visibleRunStates.current.clear();
      setRunDetails({});
      setLoadingDetailRunIds(new Set());
      setRunsState({ kind: "loading" });
    }
    void refreshLatest();
  }, [refreshLatest, refreshToken, tenderId]);

  useEffect(() => {
    if (!productionScheduling && !runningCommand && !hasRunningRun) return;
    let polling = false;
    const interval = window.setInterval(() => {
      if (polling) return;
      polling = true;
      void refreshActivity()
        .then((changed) => {
          if (changed && beforeSequenceRef.current === null) {
            return loadPage(null, []);
          }
        })
        .finally(() => {
          polling = false;
        });
    }, 500);
    return () => window.clearInterval(interval);
  }, [
    hasRunningRun,
    loadPage,
    productionScheduling,
    refreshActivity,
    runningCommand,
  ]);

  useEffect(() => {
    for (const runId of expandedRunIds.current) {
      if (visibleRunStates.current.get(runId) === "running") {
        void loadRunDetail(runId, true);
      }
    }
  }, [
    activity?.event_count,
    activity?.run_count,
    activity?.running_count,
    loadRunDetail,
  ]);

  const loadOlder = async () => {
    if (runsState.kind !== "ready" || pageLoading) return;
    const next = runsState.page.next_before_sequence;
    if (next === null) return;
    await loadPage(next, [...cursorStack, beforeSequence]);
  };

  const loadNewer = async () => {
    if (pageLoading) return;
    const previous = cursorStack[cursorStack.length - 1];
    if (previous === undefined) return;
    await loadPage(previous, cursorStack.slice(0, -1));
  };

  const executeAgentRun = async (retryOfRunId: string | null = null) => {
    setRunningCommand(true);
    try {
      await runBootstrapAgent(tenderId, retryOfRunId);
    } catch {
      reportCommandFailure();
    } finally {
      await refreshLatest();
      setRunningCommand(false);
    }
  };

  const interrupt = async (runId: string) => {
    setInterruptingRunId(runId);
    try {
      if (!(await interruptAgentRun(tenderId, runId))) {
        await refreshLatest();
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
      await refreshLatest();
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
        <span>
          {runsState.kind === "ready" ? runsState.page.total_count : 0} runs
        </span>
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
          onClick={() => void refreshLatest()}
          disabled={runsState.kind === "loading" || pageLoading}
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
        <>
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
                    <h5>{run.profile_identity}</h5>
                    <p>
                      {run.profile_profession} / profile v{run.profile_version}{" "}
                      / task {run.task_id.slice(0, 8)}
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
                    {run.failure?.retry_safe && run.linked_retry_supported ? (
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
                    run.linked_retry_supported &&
                    !run.has_linked_retry ? (
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
                      {run.provider_thread_ref?.slice(0, 16) ??
                        "Not established"}
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
                      {run.linked_retry_supported ? (
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
                      ) : (
                        <span>
                          This task type must be rerun from its exact record
                          workflow after closure.
                        </span>
                      )}
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
                <details
                  className="agent-run__details"
                  onToggle={(event) => {
                    if (event.currentTarget.open) {
                      expandedRunIds.current.add(run.run_id);
                      void loadRunDetail(run.run_id, true);
                    } else {
                      expandedRunIds.current.delete(run.run_id);
                    }
                  }}
                >
                  <summary>Inspect exact task, permissions, and events</summary>
                  {loadingDetailRunIds.has(run.run_id) ? (
                    <p>Loading exact Agent Run...</p>
                  ) : null}
                  {runDetails[run.run_id]?.proposed_result ? (
                    <div className="agent-run__result">
                      <div>
                        <strong>Proposed result</strong>
                        <span>Independent review required</span>
                      </div>
                      <pre>
                        {runDetails[run.run_id].proposed_result?.payload_json}
                      </pre>
                    </div>
                  ) : null}
                  {runDetails[run.run_id] ? (
                    <dl>
                      <div>
                        <dt>Objective</dt>
                        <dd>{runDetails[run.run_id].task.objective}</dd>
                      </div>
                      <div>
                        <dt>Review policy</dt>
                        <dd>{runDetails[run.run_id].task.review_policy}</dd>
                      </div>
                      <div>
                        <dt>Deadline</dt>
                        <dd>{runDetails[run.run_id].task.deadline}</dd>
                      </div>
                      <div>
                        <dt>Permissions</dt>
                        <dd>
                          Network denied /{" "}
                          {
                            runDetails[run.run_id].task.permissions
                              .allowed_tools.length
                          }{" "}
                          tools /{" "}
                          {runDetails[
                            run.run_id
                          ].task.permissions.data_scopes.join(", ")}
                        </dd>
                      </div>
                    </dl>
                  ) : null}
                  {runDetails[run.run_id] ? (
                    <ol className="agent-run__events">
                      {runDetails[run.run_id].events.map((event) => (
                        <li key={event.sequence}>
                          <span>{event.sequence}</span>
                          <strong>{readable(event.kind)}</strong>
                          <p>{event.summary}</p>
                        </li>
                      ))}
                    </ol>
                  ) : null}
                </details>
              </li>
            ))}
          </ol>
          <div className="agent-office__actions" aria-label="Agent Run pages">
            <button
              type="button"
              className="button-secondary"
              onClick={() => void loadNewer()}
              disabled={pageLoading || cursorStack.length === 0}
            >
              Newer runs
            </button>
            <button
              type="button"
              className="button-secondary"
              onClick={() => void loadOlder()}
              disabled={
                pageLoading ||
                runsState.kind !== "ready" ||
                runsState.page.next_before_sequence === null
              }
            >
              Older runs
            </button>
          </div>
        </>
      ) : null}
    </section>
  );
}
