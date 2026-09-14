import { useState } from "react";
import { ArrowLeft, ChevronDown, Play } from "lucide-react";
import {
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { ErrorNotice, Loading, Status } from "../components/common";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Citations, type SourceSelection } from "./Sources";

type PlanReviewProps = {
  tenderId: string;
  planId: string;
  onBack?: () => void;
  onSource?: (source: SourceSelection) => void;
  onApproved?: () => void;
};

/** The engineer approves a proposed plan; the Tender Manager then carries it out with its team. */
export function PlanReview({
  tenderId,
  planId,
  onBack,
  onSource,
  onApproved,
}: PlanReviewProps) {
  const api = useApi();
  const refresh = useRefresh();
  const plans = useResource<Schema<"WorkPlan">[]>(
    `${tenderPath(tenderId)}/plans`,
  );
  const [note, setNote] = useState("");
  const [approving, setApproving] = useState(false);
  const [error, setError] = useState<unknown>(null);

  if (plans.isPending) return <Loading>Loading the plan…</Loading>;
  const plan = plans.data?.find((item) => item.id === planId);
  if (!plan)
    return (
      <ErrorNotice
        error={plans.error ?? new Error("This plan is no longer available.")}
      />
    );

  async function approve() {
    setApproving(true);
    setError(null);
    try {
      await api.post<Schema<"WorkPlan">>(
        `${tenderPath(tenderId)}/plans/${encodeURIComponent(planId)}/approve`,
        {
          rationale: note.trim() || "Approved by the engineer.",
        } satisfies Schema<"ApprovalRequest">,
      );
      await refresh();
      onApproved?.();
    } catch (failure) {
      setError(failure);
    } finally {
      setApproving(false);
    }
  }

  const proposed = plan.status === "proposed";
  return (
    <section
      className="flex flex-col gap-4 @container"
      aria-labelledby="plan-review-title"
    >
      {onBack ? (
        <Button
          variant="ghost"
          size="sm"
          className="self-start"
          onClick={onBack}
        >
          <ArrowLeft data-icon="inline-start" />
          Back to conversation
        </Button>
      ) : null}
      <header className="flex flex-wrap items-start justify-between gap-2">
        <div className="flex min-w-0 flex-col gap-0.5">
          <h1
            id="plan-review-title"
            className="text-base font-semibold tracking-tight"
            dir="auto"
          >
            {plan.title}
          </h1>
          <p className="text-xs text-muted-foreground">
            {plan.tasks.length} {plan.tasks.length === 1 ? "task" : "tasks"} ·
            Version {plan.version}
          </p>
        </div>
        <Status value={plan.status} />
      </header>
      <ol className="flex flex-col gap-2" aria-label="Plan tasks">
        {plan.tasks.map((task, index) => (
          <li key={task.id}>
            <PlanTask
              task={task}
              number={index + 1}
              tenderId={tenderId}
              onSource={onSource}
            />
          </li>
        ))}
      </ol>
      <ErrorNotice error={error} />
      {proposed ? (
        <div className="flex flex-col gap-3 rounded-xl border bg-card p-4">
          <label className="flex flex-col gap-1.5 text-sm">
            Note for the Manager (optional)
            <Textarea
              dir="auto"
              rows={3}
              maxLength={4000}
              value={note}
              onChange={(event) => setNote(event.target.value)}
              placeholder="Anything the team should keep in mind…"
            />
          </label>
          <div className="flex flex-wrap items-center justify-between gap-2">
            <p className="text-xs text-muted-foreground">
              The Tender Manager starts the work and assigns staff as needed.
            </p>
            <div className="flex gap-2">
              {onBack ? (
                <Button variant="ghost" onClick={onBack}>
                  Request changes
                </Button>
              ) : null}
              <Button disabled={approving} onClick={() => void approve()}>
                <Play data-icon="inline-start" />
                {approving ? "Starting…" : "Approve and start"}
              </Button>
            </div>
          </div>
        </div>
      ) : null}
    </section>
  );
}

function PlanTask({
  task,
  number,
  tenderId,
  onSource,
}: {
  task: Schema<"Task">;
  number: number;
  tenderId: string;
  onSource?: (source: SourceSelection) => void;
}) {
  return (
    <details className="group rounded-xl border bg-card">
      <summary className="flex cursor-pointer list-none items-start gap-3 p-3">
        <span className="mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full bg-muted text-xs">
          {number}
        </span>
        <span className="min-w-0 flex-1">
          <span className="block text-sm font-medium" dir="auto">
            {task.title}
          </span>
          <span className="block text-xs text-muted-foreground" dir="auto">
            {task.role}
          </span>
        </span>
        <ChevronDown className="mt-0.5 size-4 shrink-0 text-muted-foreground transition-transform group-open:rotate-180" />
      </summary>
      <div className="flex flex-col gap-2 px-3 pb-3 ps-11 text-sm">
        <p className="whitespace-pre-wrap" dir="auto">
          {task.description}
        </p>
        {onSource && task.source_ids.length ? (
          <Citations ids={task.source_ids} tenderId={tenderId} onOpen={onSource} />
        ) : null}
      </div>
    </details>
  );
}
