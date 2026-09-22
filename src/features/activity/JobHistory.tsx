import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ChevronDown, RotateCcw } from "lucide-react";
import {
  isActive,
  tenderPath,
  useApi,
  useRefresh,
  type Schema,
} from "../../api";
import { ErrorNotice } from "../../components/common";
import { Button } from "@/components/ui/button";
import { formatElapsed } from "@/components/beautiful/live-status";
import { cn } from "@/lib/utils";
import { WorkLog } from "../work-log/WorkLog";
import { stopReason } from "../work-log/model";
import type { SourceSelection } from "../Sources";
import { rtlDir } from "@/lib/text-direction";
import { TechnicalLog } from "./TechnicalLog";

type Run = Schema<"Run">;
type JobSummary = Schema<"JobSummary">;

export type Job = {
  id: string;
  kind: string;
  request: string;
  attempts: Run[];
  latest: Run;
  startedAt: string;
  endedAt: string;
};

const KIND_LABELS: Record<string, string> = {
  import: "Importing the tender package",
  index: "Preparing search",
  identify: "Identifying the project",
  analysis: "Analysing the tender package",
  research: "Researching",
};

const STOPPED = new Set(["failed", "cancelled", "interrupted"]);

/**
 * Runs grouped into jobs: a Resume or Continue starts a new run with the same
 * request right after a stopped one, so consecutive runs of the same kind and
 * request form one job, newest first.
 */
export function groupJobs(runs: Run[]): Job[] {
  const ordered = [...runs].sort((a, b) =>
    a.created_at.localeCompare(b.created_at),
  );
  const jobs: Job[] = [];
  for (const run of ordered) {
    const request = (run.instruction ?? "").trim();
    const last = jobs.at(-1);
    if (
      last &&
      last.kind === run.kind &&
      last.request === request &&
      STOPPED.has(last.latest.status)
    ) {
      last.attempts.push(run);
      last.latest = run;
      last.endedAt = run.updated_at;
      continue;
    }
    jobs.push({
      id: run.id,
      kind: run.kind,
      request,
      attempts: [run],
      latest: run,
      startedAt: run.created_at,
      endedAt: run.updated_at,
    });
  }
  return jobs.reverse();
}

function counted(count: number, one: string, many: string) {
  if (!count) return null;
  return count === 1 ? one : many.replace("{n}", String(count));
}

/** "Read 23 documents · hired 7 staff" across every attempt of one job. */
export function jobSummaryLine(summaries: JobSummary[]) {
  const documents = new Set(summaries.flatMap((item) => item.documents ?? []));
  const colleagues = new Set(
    summaries.flatMap((item) => item.colleagues_asked ?? []),
  );
  const total = (key: keyof JobSummary) =>
    summaries.reduce((sum, item) => sum + Number(item[key] ?? 0), 0);
  const parts = [
    counted(documents.size, "read 1 document", "read {n} documents"),
    counted(total("searches"), "searched once", "searched {n} times"),
    counted(total("page_views"), "looked at 1 page", "looked at {n} pages"),
    counted(total("staff_hired"), "hired 1 staff member", "hired {n} staff"),
    counted(colleagues.size, "asked 1 colleague", "asked {n} colleagues"),
    counted(total("drafts_saved"), "saved 1 draft", "saved {n} drafts"),
    counted(
      total("proposals"),
      "prepared 1 proposal",
      "prepared {n} proposals",
    ),
  ].filter(Boolean) as string[];
  if (!parts.length) return "";
  const line = parts.join(" · ");
  return line.charAt(0).toUpperCase() + line.slice(1);
}

function jobTitle(job: Job) {
  if (["manager", "conversation"].includes(job.kind) && job.request)
    return job.request;
  return KIND_LABELS[job.kind] ?? (job.request || "Tender work");
}

function dayLabel(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Earlier";
  const today = new Date();
  const yesterday = new Date();
  yesterday.setDate(today.getDate() - 1);
  if (date.toDateString() === today.toDateString()) return "Today";
  if (date.toDateString() === yesterday.toDateString()) return "Yesterday";
  return date.toLocaleDateString([], { dateStyle: "medium" });
}

function statusWord(job: Job) {
  const status = job.latest.status;
  if (isActive(status)) return { word: "working", tone: "text-foreground" };
  if (STOPPED.has(status)) return { word: "stopped", tone: "text-destructive" };
  return { word: "done", tone: "text-(--success)" };
}

function JobRow({
  job,
  newest,
  summary,
  tenderId,
  onSource,
  onRaiseLimit,
}: {
  job: Job;
  newest: boolean;
  summary: string;
  tenderId: string;
  onSource?: (source: SourceSelection) => void;
  onRaiseLimit?: () => void;
}) {
  const api = useApi();
  const refresh = useRefresh();
  const [open, setOpen] = useState(false);
  const [showAttempts, setShowAttempts] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const status = statusWord(job);
  const stopped = STOPPED.has(job.latest.status);
  const reason = stopped
    ? stopReason(job.latest.status, job.latest.error).reason
    : null;
  const total = Date.parse(job.endedAt) - Date.parse(job.startedAt);
  const conversation = ["manager", "conversation"].includes(job.kind);

  async function resume() {
    setPending(true);
    setError(null);
    try {
      await api.post(`/runs/${encodeURIComponent(job.latest.id)}/resume`);
      await refresh();
    } catch (failure) {
      setError(failure);
    } finally {
      setPending(false);
    }
  }

  return (
    <li className="border-b py-2.5 last:border-0">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="flex w-full min-w-0 items-start gap-2 text-start"
      >
        <ChevronDown
          aria-hidden
          className={cn(
            "mt-0.5 size-4 shrink-0 text-muted-foreground transition-transform",
            !open && "-rotate-90",
          )}
        />
        <span className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span
            dir={rtlDir(jobTitle(job))}
            className="text-sm font-medium wrap-anywhere"
          >
            {jobTitle(job)}
          </span>
          {reason ? (
            <span className="text-xs text-muted-foreground wrap-anywhere">
              Stopped: {reason}.
            </span>
          ) : null}
          {summary ? (
            <span className="text-xs text-muted-foreground wrap-anywhere">
              {summary}
            </span>
          ) : null}
        </span>
        <span className="mt-0.5 shrink-0 text-xs text-muted-foreground tabular-nums">
          <span className={status.tone}>{status.word}</span>
          {Number.isFinite(total) ? ` · ${formatElapsed(total)}` : ""}
        </span>
      </button>
      <div className="mt-1.5 flex flex-wrap items-center gap-2 ps-6">
        {stopped && newest && job.kind !== "research" ? (
          <Button
            type="button"
            size="xs"
            disabled={pending}
            onClick={() => void resume()}
          >
            <RotateCcw data-icon="inline-start" />
            Continue
          </Button>
        ) : null}
        {job.attempts.length > 1 ? (
          <button
            type="button"
            aria-expanded={showAttempts}
            onClick={() => setShowAttempts(!showAttempts)}
            className="text-xs text-muted-foreground hover:text-foreground"
          >
            {job.attempts.length} attempts
          </button>
        ) : null}
      </div>
      <ErrorNotice error={error} />
      {showAttempts ? (
        <ol
          aria-label="Attempts"
          className="mt-1.5 flex flex-col gap-1 ps-6 text-xs text-muted-foreground"
        >
          {job.attempts.map((attempt, index) => (
            <li key={attempt.id}>
              Attempt {index + 1} ·{" "}
              {new Date(attempt.created_at).toLocaleTimeString([], {
                hour: "numeric",
                minute: "2-digit",
              })}
              {" · "}
              {STOPPED.has(attempt.status)
                ? `stopped: ${stopReason(attempt.status, attempt.error).reason}`
                : isActive(attempt.status)
                  ? "working"
                  : "done"}
            </li>
          ))}
        </ol>
      ) : null}
      {open ? (
        <div className="mt-2 ps-6">
          {conversation ? (
            <div className="flex flex-col gap-2">
              {job.attempts.map((attempt, index) => (
                <WorkLog
                  key={attempt.id}
                  tenderId={tenderId}
                  runId={attempt.id}
                  run={attempt}
                  status={attempt.status}
                  startedAt={attempt.created_at}
                  onSource={onSource}
                  onRaiseLimit={onRaiseLimit}
                  current={newest && index === job.attempts.length - 1}
                />
              ))}
            </div>
          ) : (
            <p className="text-sm text-muted-foreground wrap-anywhere">
              {job.latest.error ||
                job.latest.detail ||
                "No further detail was recorded."}
            </p>
          )}
          <div className="mt-2 flex flex-col gap-1">
            {job.attempts.map((attempt, index) => (
              <TechnicalLog
                key={attempt.id}
                tenderId={tenderId}
                runId={attempt.id}
                label={
                  job.attempts.length > 1
                    ? `Technical log · attempt ${index + 1}`
                    : "Technical log"
                }
                onSource={onSource}
              />
            ))}
          </div>
        </div>
      ) : null}
    </li>
  );
}

/** Every job on this tender, newest first, grouped by day. */
export function JobHistory({
  tenderId,
  runs,
  onSource,
  onRaiseLimit,
}: {
  tenderId: string;
  runs: Run[];
  onSource?: (source: SourceSelection) => void;
  onRaiseLimit?: () => void;
}) {
  const api = useApi();
  const jobs = groupJobs(runs);
  // Refetched whenever a job starts, finishes or changes state.
  const revision = runs.map((run) => `${run.id}:${run.status}`).join("|");
  const summaries = useQuery({
    queryKey: ["job-summaries", tenderId, revision],
    enabled: jobs.length > 0,
    queryFn: ({ signal }) =>
      api.get<Schema<"JobSummaryList">>(
        `${tenderPath(tenderId)}/job-summaries`,
        signal,
      ),
  });
  const byRun = new Map(
    (summaries.data?.jobs ?? []).map((item) => [item.run_id, item]),
  );
  if (!jobs.length)
    return (
      <p className="text-sm text-muted-foreground">
        No work has run on this tender yet.
      </p>
    );
  const days: { label: string; jobs: Job[] }[] = [];
  for (const job of jobs) {
    const label = dayLabel(job.startedAt);
    const day = days.at(-1);
    if (day?.label === label) day.jobs.push(job);
    else days.push({ label, jobs: [job] });
  }
  return (
    <section aria-label="Activity" className="flex flex-col gap-4">
      <div className="flex flex-col gap-0.5">
        <h2 className="text-sm font-medium">Activity</h2>
        <p className="text-xs text-muted-foreground">
          Every job on this tender, newest first.
        </p>
      </div>
      {days.map((day) => (
        <div key={day.label} className="flex flex-col">
          <h3 className="text-xs font-medium text-muted-foreground">
            {day.label}
          </h3>
          <ul aria-label={`Jobs ${day.label}`}>
            {day.jobs.map((job) => (
              <JobRow
                key={job.id}
                job={job}
                newest={job === jobs[0]}
                summary={jobSummaryLine(
                  job.attempts.flatMap((attempt) => {
                    const item = byRun.get(attempt.id);
                    return item ? [item] : [];
                  }),
                )}
                tenderId={tenderId}
                onSource={onSource}
                onRaiseLimit={onRaiseLimit}
              />
            ))}
          </ul>
        </div>
      ))}
    </section>
  );
}
