import { useEffect, useRef, useState, type ReactNode } from "react";
import { Check, ChevronDown, RotateCcw, Square } from "lucide-react";
import {
  isActive,
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import {
  Empty,
  ErrorNotice,
  Loading,
  Modal,
  Status,
} from "../components/common";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { Citations, type SourceSelection } from "./Sources";
import { AIUsage } from "./AIUsage";
import { TenderAI } from "./TenderAI";
import { PlanReview } from "./PlanReview";
import { ReviewRoom } from "./office/ReviewRoom";
import { WorkDecisions } from "./WorkDecisions";
import { ProjectMap } from "./ProjectMap";
import { TenderBrief } from "./TenderBrief";
import { Watchers } from "./Watchers";
import { draftStorageKey, useFormDraft, type DraftScope } from "./useFormDraft";

export function PlanView({
  plan,
  tenderId,
  onChanges,
  onSource,
  onRecord,
  focusReview = false,
}: {
  plan: Schema<"WorkPlan">;
  tenderId: string;
  onChanges: () => void;
  onSource: (source: SourceSelection) => void;
  onRecord?: (view: string, recordId: string) => void;
  focusReview?: boolean;
}) {
  const [reviewing, setReviewing] = useState(focusReview);
  function openReview() {
    if (onRecord) onRecord("plan-review", plan.id);
    else setReviewing(true);
  }
  if (reviewing)
    return (
      <div className="legacy-screen">
        <PlanReview
          tenderId={tenderId}
          planId={plan.id}
          onBack={() => setReviewing(false)}
          onSource={onSource}
        />
      </div>
    );
  return (
    <section
      className="flex flex-col gap-3 rounded-lg border bg-card p-4"
      id={`plan-${plan.id}`}
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-0.5">
          <h3 className="text-sm font-semibold tracking-tight" dir="auto">
            {plan.title}
          </h3>
          <p className="text-xs text-muted-foreground">
            {plan.tasks.length} tasks · Version {plan.version}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Status value={plan.status} />
          {plan.status !== "superseded" ? (
            <Button type="button" size="sm" onClick={openReview}>
              <Check data-icon="inline-start" />
              Review plan
            </Button>
          ) : null}
        </div>
      </div>
      <ol className="flex flex-col gap-1.5">
        {plan.tasks.map((task, index) => (
          <li key={task.id} className="flex min-w-0 items-start gap-2.5">
            <span className="mt-1.5 flex size-5 shrink-0 items-center justify-center rounded-full bg-muted text-xs tabular-nums text-muted-foreground">
              {index + 1}
            </span>
            <details className="min-w-0 flex-1 rounded-md px-2 py-1.5 hover:bg-accent/40">
              <summary className="flex cursor-pointer flex-wrap items-center justify-between gap-2">
                <strong className="min-w-0 text-sm font-medium" dir="auto">
                  {task.title}
                </strong>
                <Status value={task.status} />
              </summary>
              <div className="mt-2 flex flex-col gap-1.5">
                <p className="text-sm text-muted-foreground" dir="auto">
                  {task.description}
                </p>
                <p className="text-xs text-muted-foreground">
                  {task.role.replaceAll("_", " ")}
                </p>
                <Citations
                  ids={task.source_ids}
                  tenderId={tenderId}
                  onOpen={onSource}
                />
              </div>
            </details>
          </li>
        ))}
      </ol>
      {plan.status === "proposed" ? (
        <Button
          type="button"
          variant="link"
          size="sm"
          className="h-auto w-fit p-0"
          onClick={onChanges}
        >
          Request changes in Manager
        </Button>
      ) : null}
    </section>
  );
}

type DecisionFormProps = {
  title: string;
  action: string;
  description: string;
  onClose: () => void;
  onSubmit: (rationale: string) => Promise<void>;
  children?: ReactNode;
  submitDisabled?: boolean;
  draftScope?: DraftScope;
};
export function DecisionForm(props: DecisionFormProps) {
  return props.draftScope ? (
    <SavedDecisionForm
      key={draftStorageKey(props.draftScope)}
      {...props}
      draftScope={props.draftScope}
    />
  ) : (
    <DecisionFormContent {...props} />
  );
}
function SavedDecisionForm(
  props: DecisionFormProps & { draftScope: DraftScope },
) {
  const draft = useFormDraft(props.draftScope, { rationale: "" }, [
    "rationale",
  ]);
  return <DecisionFormContent {...props} savedDraft={draft} />;
}
function DecisionFormContent({
  title,
  action,
  description,
  onClose,
  onSubmit,
  children,
  submitDisabled = false,
  savedDraft,
}: DecisionFormProps & {
  savedDraft?: ReturnType<typeof useFormDraft<{ rationale: string }>>;
}) {
  const [localRationale, setLocalRationale] = useState("");
  const rationale = savedDraft?.value.rationale ?? localRationale;
  const setRationale = (value: string) =>
    savedDraft
      ? savedDraft.setField("rationale", value)
      : setLocalRationale(value);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>(null);
  return (
    <Modal title={title} onClose={onClose}>
      <form
        onSubmit={async (event) => {
          event.preventDefault();
          if (submitDisabled || pending || !rationale.trim()) return;
          setPending(true);
          setError(null);
          const revision = savedDraft?.revision;
          try {
            await onSubmit(rationale.trim());
            savedDraft?.markAccepted(revision);
          } catch (failure) {
            setError(failure);
          } finally {
            setPending(false);
          }
        }}
      >
        <p className="muted">{description}</p>
        {children}
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
        <ErrorNotice error={error || savedDraft?.error} />
        <div className="form-actions">
          <button type="button" className="button" onClick={onClose}>
            Cancel
          </button>
          <button
            className="button primary"
            disabled={pending || submitDisabled || !rationale.trim()}
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
  onRecord,
  recordId,
  recordView,
  onView,
}: {
  tenderId: string;
  onChanges: () => void;
  onSource: (source: SourceSelection) => void;
  onRecord?: (view: string, recordId: string) => void;
  recordId?: string;
  recordView?: string;
  onView?: (view: string) => void;
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
  const overview = useResource<Schema<"Overview">>(tenderPath(tenderId));
  const artifacts = useResource<Schema<"Artifact">[]>(
    `${tenderPath(tenderId)}/artifacts`,
  );
  const messages = useResource<Schema<"Message">[]>(
    `${tenderPath(tenderId)}/messages`,
  );
  const api = useApi(),
    refresh = useRefresh();
  const [section, setSection] = useState<
    "plan" | "decisions" | "reviews" | "scope" | "activity" | "ai"
  >(
    recordView === "ai"
      ? "ai"
      : recordView === "finding" || recordView === "decisions"
        ? "decisions"
        : recordView === "reviews"
          ? "reviews"
          : recordView === "scope"
            ? "scope"
            : recordView === "run" || recordView === "activity"
              ? "activity"
              : "plan",
  );
  // Reviews live with decisions, and scope with the plan, so the engineer
  // reads four areas instead of six. Older links to either still open here.
  const active =
    section === "reviews"
      ? "decisions"
      : section === "scope"
        ? "plan"
        : section;
  const [aiUsageOpen, setAiUsageOpen] = useState(false);
  const [stopping, setStopping] = useState(false),
    [stopError, setStopError] = useState<unknown>(null),
    [stopNotice, setStopNotice] = useState("");
  useEffect(() => {
    if (recordView === "ai") setSection("ai");
    else if (recordView === "finding" || recordView === "decisions")
      setSection("decisions");
    else if (recordView === "reviews") setSection("reviews");
    else if (recordView === "scope") setSection("scope");
    else if (recordView === "run" || recordView === "activity")
      setSection("activity");
    else if (
      recordView === "plan" ||
      recordView === "plan-review" ||
      recordView === "task"
    )
      setSection("plan");
  }, [recordView]);
  const hasActiveRuns = runs.data?.some((run) => isActive(run.status)) ?? false;
  async function stopAllWork() {
    if (stopping) return;
    setStopping(true);
    setStopError(null);
    setStopNotice("");
    try {
      await api.post<Schema<"MutationReceipt">>(
        `${tenderPath(tenderId)}/work/stop`,
      );
      await refresh();
      setStopNotice(
        "Stop requested for this Tender. Check each run’s final status in Activity.",
      );
    } catch (failure) {
      setStopError(failure);
    } finally {
      setStopping(false);
    }
  }
  const currentPlans =
    plans.data?.filter((plan) => plan.status !== "superseded") ?? [];
  const earlierPlans =
    plans.data?.filter((plan) => plan.status === "superseded") ?? [];
  return (
    <div className="flex flex-col gap-5">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex max-w-2xl flex-col gap-1">
          <h2 className="text-lg font-semibold tracking-tight">Work</h2>
          <p className="text-sm text-muted-foreground">
            Review the approved plan, resolve findings and follow every run.
          </p>
        </div>
        {hasActiveRuns ? (
          <Button
            type="button"
            variant="destructive"
            size="sm"
            disabled={stopping}
            onClick={() => void stopAllWork()}
          >
            <Square data-icon="inline-start" />
            {stopping ? "Stopping…" : "Stop all work"}
          </Button>
        ) : null}
      </header>
      <nav
        aria-label="Work sections"
        className="flex w-fit max-w-full flex-wrap gap-0.5 rounded-lg bg-muted p-0.5"
      >
        {(
          [
            ["plan", "Plan and tasks"],
            ["decisions", "Decisions and findings"],
            ["activity", "Activity"],
            ["ai", "AI and spending"],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            type="button"
            aria-current={active === id ? "page" : undefined}
            className={cn(
              "rounded-md px-3 py-1.5 text-sm transition-colors",
              active === id
                ? "bg-background text-foreground shadow-xs"
                : "text-muted-foreground hover:text-foreground",
            )}
            onClick={() => {
              setSection(id);
              onView?.(id);
            }}
          >
            {label}
          </button>
        ))}
      </nav>
      <ErrorNotice
        error={
          plans.error ||
          runs.error ||
          tasks.error ||
          overview.error ||
          artifacts.error ||
          messages.error ||
          stopError
        }
      />
      {stopNotice ? (
        <p className="text-sm text-muted-foreground" role="status">
          {stopNotice}
        </p>
      ) : null}
      {plans.isPending ? <Loading>Loading work…</Loading> : null}
      {active === "plan" ? (
        <PlanSection
          plans={currentPlans}
          earlierPlans={earlierPlans}
          tasks={tasks.data ?? []}
          tenderId={tenderId}
          onChanges={onChanges}
          onSource={onSource}
          onRecord={onRecord}
          recordId={recordId}
          recordView={recordView}
        />
      ) : null}
      {active === "decisions" ? (
        <WorkDecisions
          tenderId={tenderId}
          onSource={onSource}
          focusedId={
            recordView === "finding" || recordView === "decisions"
              ? recordId
              : undefined
          }
        />
      ) : null}
      {active === "decisions" ? (
        <div className="legacy-screen">
          <ReviewRoom
            tenderId={tenderId}
            onDecide={() => {
              setSection("decisions");
              onView?.("decisions");
            }}
          />
        </div>
      ) : null}
      {active === "ai" ? (
        <section
          className="legacy-screen flex flex-col gap-4 rounded-xl border bg-card p-4"
          aria-label="AI and spending"
        >
          <TenderAI tenderId={tenderId} />
          <AIUsage tenderId={tenderId} />
        </section>
      ) : null}
      {active === "activity" ? (
        <ActivitySection
          runs={runs.data ?? []}
          messages={messages.data ?? []}
          focusedId={recordView === "run" ? recordId : undefined}
        />
      ) : null}
      <details
        className="rounded-xl border bg-card p-4"
        onToggle={(event) => setAiUsageOpen(event.currentTarget.open)}
      >
        <summary className="flex cursor-pointer flex-col">
          <strong className="text-sm font-medium">More options</strong>
          <span className="text-xs text-muted-foreground">
            Tender brief, scope map and watchers
          </span>
        </summary>
        {aiUsageOpen ? (
          <div className="legacy-screen mt-4 flex flex-col gap-4">
            <TenderBrief tenderId={tenderId} />
            <ScopeSection
              tenderId={tenderId}
              overview={overview.data}
              artifactCount={overview.data?.artifact_count ?? 0}
              artifacts={artifacts.data ?? []}
              onSource={onSource}
            />
            <Watchers tenderId={tenderId} />
          </div>
        ) : null}
      </details>
    </div>
  );
}

function PlanSection({
  plans,
  earlierPlans,
  tasks,
  tenderId,
  onChanges,
  onSource,
  onRecord,
  recordId,
  recordView,
}: {
  plans: Schema<"WorkPlan">[];
  earlierPlans: Schema<"WorkPlan">[];
  tasks: Schema<"Task">[];
  tenderId: string;
  onChanges: () => void;
  onSource: (source: SourceSelection) => void;
  onRecord?: (view: string, recordId: string) => void;
  recordId?: string;
  recordView?: string;
}) {
  if (!plans.length && !earlierPlans.length)
    return (
      <Empty title="No work has been planned yet">
        Ask the Tender Manager to review the package and propose a plan.
      </Empty>
    );
  return (
    <div className="flex flex-col gap-4">
      <section className="flex flex-col gap-3 rounded-xl border bg-card p-4">
        <div className="flex flex-col gap-0.5">
          <h2 className="text-sm font-semibold tracking-tight">
            Plan and tasks
          </h2>
          <p className="text-xs text-muted-foreground">
            Open each task for its detail and source basis.
          </p>
        </div>
        {plans.map((plan) => (
          <PlanView
            key={plan.id}
            plan={plan}
            tenderId={tenderId}
            onChanges={onChanges}
            onSource={onSource}
            onRecord={onRecord}
            focusReview={recordView === "plan-review" && recordId === plan.id}
          />
        ))}
      </section>
      {tasks.length ? (
        <section className="flex flex-col gap-3 rounded-xl border bg-card p-4">
          <h2 className="text-sm font-semibold tracking-tight">Task results</h2>
          {tasks.map((task) => (
            <TaskRow
              key={task.id}
              task={task}
              tenderId={tenderId}
              plans={[...plans, ...earlierPlans]}
              onSource={onSource}
              focused={recordView === "task" && recordId === task.id}
            />
          ))}
        </section>
      ) : null}
      {earlierPlans.length ? (
        <details className="rounded-xl border bg-card p-4">
          <summary className="flex cursor-pointer flex-col">
            <strong className="text-sm font-medium">Earlier plans</strong>
            <span className="text-xs text-muted-foreground">
              {earlierPlans.length} superseded plan
              {earlierPlans.length === 1 ? "" : "s"}
            </span>
          </summary>
          <div className="mt-3 flex flex-col gap-3">
            {earlierPlans.map((plan) => (
              <PlanView
                key={plan.id}
                plan={plan}
                tenderId={tenderId}
                onChanges={onChanges}
                onSource={onSource}
                onRecord={onRecord}
              />
            ))}
          </div>
        </details>
      ) : null}
    </div>
  );
}

function ScopeSection({
  tenderId,
  overview,
  artifactCount,
  artifacts,
  onSource,
}: {
  tenderId: string;
  overview?: Schema<"Overview">;
  artifactCount: number;
  artifacts: Schema<"Artifact">[];
  onSource: (source: SourceSelection) => void;
}) {
  return (
    <div className="flex flex-col gap-4">
      <section className="flex flex-col gap-3 rounded-xl border bg-card p-4">
        <h2 className="text-sm font-semibold tracking-tight">
          Scope and project map
        </h2>
        <p className="text-xs text-muted-foreground">
          The current Tender register gives the working scope. Use the map to
          connect areas and work items to their sources.
        </p>
        <dl className="review-scope-facts">
          <div>
            <dt>Documents</dt>
            <dd>{artifactCount}</dd>
          </div>
          <div>
            <dt>Areas</dt>
            <dd>
              {overview?.areas?.length
                ? overview.areas.join(", ")
                : "Not identified"}
            </dd>
          </div>
          <div>
            <dt>Findings</dt>
            <dd>{overview?.findings?.length ?? 0}</dd>
          </div>
          <div>
            <dt>BOQ items</dt>
            <dd>{overview?.boq_count ?? 0}</dd>
          </div>
        </dl>
      </section>
      <ProjectMap
        tenderId={tenderId}
        artifacts={artifacts}
        onSource={onSource}
      />
    </div>
  );
}

export function ActivitySection({
  runs,
  messages,
  focusedId,
}: {
  runs: Schema<"Run">[];
  messages: Schema<"Message">[];
  focusedId?: string;
}) {
  const systemMessages = messages.filter(
    (message) => message.role === "system",
  );
  useEffect(() => {
    if (!focusedId) return;
    document
      .getElementById(`run-${focusedId}`)
      ?.scrollIntoView?.({ block: "center" });
  }, [focusedId, runs]);
  return (
    <section className="flex flex-col gap-3 rounded-xl border bg-card p-4">
      <div className="flex flex-col gap-0.5">
        <h2 className="text-sm font-semibold tracking-tight">Activity</h2>
        <p className="text-xs text-muted-foreground">
          System messages and run history stay here, outside the Manager
          conversation.
        </p>
      </div>
      {runs.length || systemMessages.length ? (
        <div className="flex flex-col gap-3">
          {runs.map((run) => (
            <div className="flex flex-col gap-1" key={run.id}>
              <time className="text-xs text-muted-foreground">
                {formatTime(run.updated_at)}
              </time>
              <RunRow run={run} focused={focusedId === run.id} />
            </div>
          ))}
          {systemMessages.map((message) => (
            <div className="flex flex-col gap-1" key={message.id}>
              <time className="text-xs text-muted-foreground">
                {formatTime(message.created_at)}
              </time>
              <span className="text-sm text-muted-foreground" dir="auto">
                {message.content}
              </span>
            </div>
          ))}
        </div>
      ) : (
        <p className="text-sm text-muted-foreground">
          No Activity has been recorded yet.
        </p>
      )}
    </section>
  );
}

function TaskRow({
  task,
  tenderId,
  plans,
  onSource,
  focused,
}: {
  task: Schema<"Task">;
  tenderId: string;
  plans: Schema<"WorkPlan">[];
  onSource: (source: SourceSelection) => void;
  focused?: boolean;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const detailsRef = useRef<HTMLDetailsElement>(null);
  const [error, setError] = useState<unknown>(null),
    [pending, setPending] = useState(false);
  useEffect(() => {
    if (focused && detailsRef.current) {
      detailsRef.current.open = true;
      detailsRef.current.scrollIntoView?.({ block: "center" });
      detailsRef.current.querySelector("summary")?.focus();
    }
  }, [focused]);
  const canRun =
    plans.some(
      (plan) => plan.id === task.plan_id && plan.status === "approved",
    ) && ["ready", "failed", "interrupted", "cancelled"].includes(task.status);
  return (
    <details
      ref={detailsRef}
      className="flex flex-col gap-2 rounded-lg border p-3"
      id={`task-${task.id}`}
    >
      <summary className="flex cursor-pointer flex-wrap items-center justify-between gap-2">
        <strong className="min-w-0 text-sm font-medium" dir="auto">
          {task.title}
        </strong>
        <Status value={task.status} />
      </summary>
      <p className="mt-2 text-sm text-muted-foreground" dir="auto">
        {task.description}
      </p>
      <p className="mt-1 text-xs text-muted-foreground">
        {task.role.replaceAll("_", " ")}
      </p>
      {typeof task.result?.summary === "string" ? (
        <p className="mt-2 text-sm" dir="auto">
          {task.result.summary}
        </p>
      ) : null}
      <Citations ids={task.source_ids} tenderId={tenderId} onOpen={onSource} />
      <ErrorNotice error={error} />
      {canRun ? (
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="mt-2 w-fit"
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
        </Button>
      ) : null}
    </details>
  );
}

import { RunRow } from "./RunRow";
export { RunRow };

function formatTime(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? ""
    : date.toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
}
