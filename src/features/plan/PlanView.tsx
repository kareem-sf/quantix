import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { tenderPath, useApi, type Schema } from "../../api";
import { ErrorNotice, Loading } from "../../components/common";
import { StatusMark, type TaskStatus } from "@/components/beautiful/task-rows";
import { plainText, RichText } from "@/components/rich-text";
import { rtlDir } from "@/lib/text-direction";
import { cn } from "@/lib/utils";

type WorkBrief = {
  outcome: string;
  status: string;
  next_step?: string;
  steps?: { title: string; state: string; note?: string; owner?: string }[];
  created_at: string;
};

type Step = NonNullable<WorkBrief["steps"]>[number];

const WORDS: Record<TaskStatus, string> = {
  todo: "not started",
  running: "in progress",
  waiting: "waiting",
  blocked: "blocked",
  done: "done",
  failed: "stopped",
};

function stepStatus(state: string, working: boolean): TaskStatus {
  if (state === "done") return "done";
  if (state === "blocked") return "blocked";
  if (state === "in_progress") return working ? "running" : "todo";
  return "todo";
}

function updated(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? ""
    : date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

/**
 * The Tender Manager's live plan for the current job, for a quick look. Nothing
 * is decided here: the Manager asks in the chat and approvals sit under its
 * replies.
 */
export function PlanView({ overview }: { overview: Schema<"Overview"> }) {
  const api = useApi();
  const tenderId = overview.tender.id;
  const working = overview.active_runs.some((run) =>
    ["manager", "conversation"].includes(run.kind),
  );
  const liveKey = overview.active_runs
    .map((run) => `${run.id}:${run.status}`)
    .join("|");
  const brief = useQuery<{ brief?: WorkBrief | null }>({
    queryKey: ["work-brief", tenderId, liveKey],
    queryFn: ({ signal }) =>
      api.get(`${tenderPath(tenderId)}/work-brief`, signal),
    refetchInterval: working ? 4000 : false,
  });
  const waiting = useQuery<Schema<"WaitingState">>({
    queryKey: ["waiting", tenderId, liveKey],
    queryFn: ({ signal }) => api.get(`${tenderPath(tenderId)}/waiting`, signal),
    refetchInterval: working ? 4000 : false,
  });

  if (brief.isPending) return <Loading>Loading the plan…</Loading>;
  const plan = brief.data?.brief;
  const steps = plan?.steps ?? [];
  const decisions = waiting.data?.total ?? 0;

  return (
    <section aria-label="Plan" className="flex flex-col gap-4">
      <div className="flex items-baseline justify-between gap-3">
        <h2 className="text-sm font-medium">Plan</h2>
        {plan ? (
          <span className="text-xs text-muted-foreground">
            Updated {updated(plan.created_at)}
          </span>
        ) : null}
      </div>
      <ErrorNotice error={brief.error} />
      {plan ? (
        <>
          <RichText text={plan.outcome} className="text-sm" />
          {steps.length ? (
            <ol aria-label="Plan steps" className="flex flex-col">
              {steps.map((step, index) => (
                <PlanStep
                  key={`${index}:${step.title}`}
                  step={step}
                  status={stepStatus(step.state, working)}
                />
              ))}
            </ol>
          ) : null}
          {plan.next_step ? (
            <p
              dir={rtlDir(plan.next_step)}
              className="text-sm text-muted-foreground wrap-anywhere"
            >
              Next: {plainText(plan.next_step)}
            </p>
          ) : null}
        </>
      ) : (
        <p className="text-sm text-muted-foreground">
          No plan yet. The Tender Manager writes one when it starts a real job.
        </p>
      )}
      {decisions ? (
        <p className="rounded-lg bg-(--warning-tint) px-3 py-2 text-sm text-(--warning-ink)">
          {decisions === 1
            ? "1 thing is waiting for you in the chat."
            : `${decisions} things are waiting for you in the chat.`}
        </p>
      ) : null}
    </section>
  );
}

function PlanStep({ step, status }: { step: Step; status: TaskStatus }) {
  const [open, setOpen] = useState(false);
  const note = step.note ? plainText(step.note) : "";
  const long = note.length > 140;
  return (
    <li className="flex gap-3 border-t py-3 first:border-t-0 first:pt-1">
      <span className="mt-0.5 flex size-5 shrink-0 items-center justify-center">
        <StatusMark status={status} />
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <span
          dir={rtlDir(step.title)}
          className={cn(
            "text-sm leading-snug font-medium wrap-break-word",
            status === "todo" && "text-foreground/75",
          )}
        >
          {plainText(step.title)}
        </span>
        <span className="text-xs text-muted-foreground">
          {[step.owner, WORDS[status]].filter(Boolean).join(" · ")}
        </span>
        {note ? (
          <span
            dir={rtlDir(note)}
            className={cn(
              "text-xs leading-relaxed text-muted-foreground wrap-break-word",
              !open && "line-clamp-2",
            )}
          >
            {note}
          </span>
        ) : null}
        {long ? (
          <button
            type="button"
            className="w-fit text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
            aria-expanded={open}
            onClick={() => setOpen(!open)}
          >
            {open ? "less" : "more"}
          </button>
        ) : null}
      </div>
    </li>
  );
}
