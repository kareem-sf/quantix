import { useCallback, useState } from "react";
import { Check, ChevronDown, RotateCcw, Square } from "lucide-react";
import {
  isActive,
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { Empty, ErrorNotice, Loading, Modal, Status } from "../components/ui";
import { Citations, type SourceSelection } from "./Sources";
import { Quotes } from "./Quotes";

export function PlanView({
  plan,
  tenderId,
  onChanges,
  onSource,
}: {
  plan: Schema<"WorkPlan">;
  tenderId: string;
  onChanges: () => void;
  onSource: (source: SourceSelection) => void;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const [approving, setApproving] = useState(false);
  const close = useCallback(() => setApproving(false), []);
  return (
    <section className="plan-section">
      <div className="section-heading">
        <h2>
          {plan.status === "proposed" ? "Proposed work plan" : "Work plan"}
        </h2>
        <Status value={plan.status} />
      </div>
      <p className="muted">
        {plan.title}
        {plan.version > 1 ? ` · Version ${plan.version}` : ""}
      </p>
      <ol className="plan-list">
        {plan.tasks.map((task, index) => (
          <li key={task.id}>
            <span className="plan-number">{index + 1}</span>
            <div>
              <strong>{task.title}</strong>
              <p>{task.description}</p>
              <Citations
                ids={task.source_ids}
                tenderId={tenderId}
                onOpen={onSource}
              />
            </div>
            {plan.status !== "proposed" ? <Status value={task.status} /> : null}
          </li>
        ))}
      </ol>
      {plan.status === "proposed" ? (
        <div className="inline-actions">
          <button className="button primary" onClick={() => setApproving(true)}>
            <Check size={18} />
            Approve plan
          </button>
          <button className="text-button" onClick={onChanges}>
            Request changes
          </button>
        </div>
      ) : null}
      {approving ? (
        <DecisionForm
          title="Approve work plan"
          action="Approve plan"
          description="The office will start the routine tasks in this plan. Record your approval and any limits on the work."
          onClose={close}
          onSubmit={async (rationale) => {
            await api.post(`${tenderPath(tenderId)}/plans/${plan.id}/approve`, {
              rationale,
            } satisfies Schema<"ApprovalRequest">);
            await refresh();
            close();
          }}
        />
      ) : null}
    </section>
  );
}

export function DecisionForm({
  title,
  action,
  description,
  onClose,
  onSubmit,
}: {
  title: string;
  action: string;
  description: string;
  onClose: () => void;
  onSubmit: (rationale: string) => Promise<void>;
}) {
  const [rationale, setRationale] = useState(""),
    [pending, setPending] = useState(false),
    [error, setError] = useState<unknown>(null);
  return (
    <Modal title={title} onClose={onClose}>
      <form
        onSubmit={async (event) => {
          event.preventDefault();
          setPending(true);
          setError(null);
          try {
            await onSubmit(rationale.trim());
          } catch (failure) {
            setError(failure);
          } finally {
            setPending(false);
          }
        }}
      >
        <p className="muted">{description}</p>
        <label>
          Decision note
          <textarea
            required
            rows={4}
            maxLength={4000}
            value={rationale}
            onChange={(event) => setRationale(event.target.value)}
          />
        </label>
        <ErrorNotice error={error} />
        <div className="form-actions">
          <button className="button" type="button" onClick={onClose}>
            Cancel
          </button>
          <button
            className="button primary"
            disabled={pending || !rationale.trim()}
          >
            {pending ? "Saving…" : action}
          </button>
        </div>
      </form>
    </Modal>
  );
}

export function Work({
  tenderId,
  onChanges,
  onSource,
}: {
  tenderId: string;
  onChanges: () => void;
  onSource: (source: SourceSelection) => void;
}) {
  const plans = useResource<Schema<"WorkPlan">[]>(
    `${tenderPath(tenderId)}/plans`,
    true,
  );
  const runs = useResource<Schema<"Run">[]>(
    `${tenderPath(tenderId)}/runs`,
    true,
  );
  const tasks = useResource<Schema<"Task">[]>(
    `${tenderPath(tenderId)}/tasks`,
    true,
  );
  const health = useResource<Schema<"Health">>("/health");
  const api = useApi(), refresh = useRefresh();
  const [stopping, setStopping] = useState(false),
    [stopError, setStopError] = useState<unknown>(null),
    [stopNotice, setStopNotice] = useState("");
  const hasActiveRuns = runs.data?.some(run => isActive(run.status)) ?? false;
  return (
    <div className="feature-page">
      <div className="section-heading">
        <h2>Work</h2>
        {hasActiveRuns ? <button type="button" className="button" disabled={stopping} onClick={async () => {
          setStopping(true); setStopError(null); setStopNotice("");
          try {
            await api.post<Schema<"MutationReceipt">>(`${tenderPath(tenderId)}/work/stop`);
            await refresh();
            setStopNotice("Stop requested for this tender. Check the run statuses below.");
          } catch (failure) { setStopError(failure); }
          finally { setStopping(false); }
        }}><Square size={15} />{stopping ? "Requesting stop…" : "Stop all work"}</button> : null}
      </div>
      <p className="page-description">
        Plans, specialist work, supplier requests and the record of each run.
      </p>
      <ErrorNotice error={plans.error || runs.error || tasks.error} />
      <ErrorNotice error={stopError} />
      {stopNotice ? <p role="status" className="field-help">{stopNotice}</p> : null}
      {plans.isPending ? <Loading>Loading work…</Loading> : null}
      {plans.data?.length === 0 ? (
        <Empty title="No work has been planned yet">
          Ask the Tender Manager to review the package and propose a plan.
        </Empty>
      ) : null}
      {plans.data
        ?.filter((plan) => plan.status !== "superseded")
        .map((plan) => (
          <PlanView
            key={plan.id}
            plan={plan}
            tenderId={tenderId}
            onChanges={onChanges}
            onSource={onSource}
          />
        ))}
      {tasks.data?.length ? (
        <section className="work-section">
          <h2>Task results</h2>
          {tasks.data.map((task) => (
            <TaskRow
              key={task.id}
              task={task}
              tenderId={tenderId}
              plans={plans.data ?? []}
              onSource={onSource}
            />
          ))}
        </section>
      ) : null}
      <section className="work-section">
        <h2>Run history</h2>
        {runs.data?.length === 0 ? (
          <p className="muted">No work has run for this tender.</p>
        ) : null}
        {runs.data?.map((run) => (
          <RunRow key={run.id} run={run} />
        ))}
      </section>
      {plans.data?.some((plan) => plan.status === "superseded") ? (
        <details className="work-section">
          <summary>Earlier plans</summary>
          {plans.data
            .filter((plan) => plan.status === "superseded")
            .map((plan) => (
              <PlanView
                key={plan.id}
                plan={plan}
                tenderId={tenderId}
                onChanges={onChanges}
                onSource={onSource}
              />
            ))}
        </details>
      ) : null}
      {health.data?.capabilities?.includes("quotations") ? (
        <Quotes tenderId={tenderId} onSource={onSource} />
      ) : null}
    </div>
  );
}
function TaskRow({
  task,
  tenderId,
  plans,
  onSource,
}: {
  task: Schema<"Task">;
  tenderId: string;
  plans: Schema<"WorkPlan">[];
  onSource: (source: SourceSelection) => void;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const [error, setError] = useState<unknown>(null),
    [pending, setPending] = useState(false);
  const canRun =
    plans.some(
      (plan) => plan.id === task.plan_id && plan.status === "approved",
    ) && ["ready", "failed", "interrupted", "cancelled"].includes(task.status);
  return (
    <details className="task-row">
      <summary>
        <strong>{task.title}</strong>
        <Status value={task.status} />
      </summary>
      <p>{task.description}</p>
      <p className="muted">{task.role.replaceAll("_", " ")}</p>
      {typeof task.result?.summary === "string" ? (
        <p className="result-text">{task.result.summary}</p>
      ) : null}
      <Citations ids={task.source_ids} tenderId={tenderId} onOpen={onSource} />
      <ErrorNotice error={error} />
      {canRun ? (
        <button
          className="button"
          disabled={pending}
          onClick={async () => {
            setPending(true);
            setError(null);
            try {
              await api.post(`${tenderPath(tenderId)}/tasks/${task.id}/run`);
              await refresh();
            } catch (failure) {
              setError(failure);
            } finally {
              setPending(false);
            }
          }}
        >
          {pending ? "Starting…" : "Run task"}
        </button>
      ) : null}
    </details>
  );
}
export function RunRow({
  run,
  compact = false,
}: {
  run: Schema<"Run">;
  compact?: boolean;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const [expanded, setExpanded] = useState(false),
    [pending, setPending] = useState(false),
    [error, setError] = useState<unknown>(null);
  async function act(action: "cancel" | "resume") {
    setPending(true);
    setError(null);
    try {
      await api.post(`/runs/${run.id}/${action}`);
      await refresh();
    } catch (failure) {
      setError(failure);
    } finally {
      setPending(false);
    }
  }
  return (
    <article className={`run-row ${compact ? "compact-run" : ""}`}>
      <div className="run-heading">
        <strong>
          {run.kind === "manager"
            ? "Tender Manager"
            : run.kind === "import"
              ? "Package import"
              : run.kind === "task"
                ? "Specialist work"
                : run.kind === "index"
                  ? "Prepare search"
                  : "Research"}
        </strong>
        <Status value={run.status} />
      </div>
      <p>
        {run.detail ||
          (isActive(run.status)
            ? "Waiting for work to start."
            : "No additional run detail.")}
      </p>
      {run.error ? <ErrorNotice error={new Error(run.error)} /> : null}
      {isActive(run.status) && run.progress > 0 ? (
        <progress
          value={run.progress}
          max={100}
          aria-label="Reported run progress"
        />
      ) : null}
      <div className="inline-actions">
        {isActive(run.status) ? (
          <button
            className="text-button"
            disabled={pending}
            onClick={() => void act("cancel")}
          >
            <Square size={13} />
            Cancel
          </button>
        ) : ["interrupted", "failed", "cancelled"].includes(run.status) &&
          run.kind !== "research" ? (
          <button
            className="text-button"
            disabled={pending}
            onClick={() => void act("resume")}
          >
            <RotateCcw size={15} />
            Resume
          </button>
        ) : null}
        {!compact ? (
          <button
            className="text-button muted"
            onClick={() => setExpanded((value) => !value)}
            aria-expanded={expanded}
          >
            <ChevronDown size={15} />
            Run details
          </button>
        ) : null}
      </div>
      <ErrorNotice error={error} />
      {expanded ? (
        <RunEvents runId={run.id} active={isActive(run.status)} />
      ) : null}
    </article>
  );
}
function RunEvents({ runId, active }: { runId: string; active: boolean }) {
  const events = useResource<Schema<"RunEvent">[]>(
    `/runs/${runId}/events`,
    active,
  );
  return (
    <div className="run-events">
      <ErrorNotice error={events.error} />
      {events.isPending ? <Loading>Loading run details…</Loading> : null}
      {events.data?.map((event) => (
        <div key={event.id}>
          <time>{new Date(event.created_at).toLocaleTimeString()}</time>
          <span>{event.message}</span>
        </div>
      ))}
    </div>
  );
}
