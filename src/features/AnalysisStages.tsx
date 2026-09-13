import {
  Circle,
  CircleAlert,
  CircleCheck,
  Clock,
  LoaderCircle,
} from "lucide-react";
import { isActive, useResource, type Schema } from "../api";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";

const STAGES = [
  ["register", "Registering documents"],
  ["recognise", "Recognising scanned pages"],
  ["index", "Indexing tender evidence"],
  ["structure", "Extracting BOQ, schedules and tables"],
  ["map", "Mapping the tender package"],
] as const;

type StageState = "pending" | "running" | "completed" | "waiting" | "failed";

/** Analyzing tender package: each stage in engineering terms, as it happens. */
export function AnalysisStages({ run }: { run: Schema<"Run"> }) {
  const events = useResource<Schema<"RunEvent">[]>(
    `/runs/${run.id}/events`,
    isActive(run.status),
  );
  const states: Record<string, { state: StageState; detail: string }> = {};
  if (run.kind === "import")
    states.register = { state: "running", detail: run.detail };
  for (const event of events.data ?? []) {
    if (event.kind !== "analysis_stage") continue;
    const data = event.data as {
      stage?: string;
      state?: StageState;
      detail?: string;
    };
    if (data.stage)
      states[data.stage] = {
        state: data.state ?? "running",
        detail: data.detail ?? "",
      };
  }
  return (
    <section
      aria-label="Analyzing tender package"
      className="flex flex-col gap-3 rounded-xl border bg-card p-3 text-sm shadow-xs"
    >
      <div className="flex items-center justify-between gap-2">
        <h3 className="font-medium">Analyzing tender package</h3>
        {isActive(run.status) ? (
          <span className="text-xs text-muted-foreground">{run.progress}%</span>
        ) : null}
      </div>
      {isActive(run.status) ? (
        <Progress value={run.progress} aria-label="Analysis progress" />
      ) : null}
      <ol className="flex flex-col gap-2">
        {STAGES.map(([key, label]) => {
          const stage = states[key] ?? { state: "pending", detail: "" };
          return (
            <li key={key} className="flex items-start gap-2.5">
              <StageIcon state={stage.state} />
              <div className="flex min-w-0 flex-col">
                <span
                  className={cn(
                    stage.state === "pending" && "text-muted-foreground",
                  )}
                >
                  {label}
                </span>
                {stage.detail && stage.state !== "running" ? (
                  <span className="text-xs text-muted-foreground">
                    {stage.detail}
                  </span>
                ) : null}
              </div>
            </li>
          );
        })}
      </ol>
    </section>
  );
}

function StageIcon({ state }: { state: StageState }) {
  const shared = "mt-0.5 size-4 shrink-0";
  if (state === "completed")
    return (
      <CircleCheck
        aria-label="Done"
        className={cn(shared, "text-emerald-600 dark:text-emerald-400")}
      />
    );
  if (state === "running")
    return (
      <LoaderCircle
        aria-label="In progress"
        className={cn(shared, "animate-spin text-foreground/70")}
      />
    );
  if (state === "waiting")
    return (
      <Clock
        aria-label="Waiting"
        className={cn(shared, "text-amber-600 dark:text-amber-400")}
      />
    );
  if (state === "failed")
    return (
      <CircleAlert
        aria-label="Needs attention"
        className={cn(shared, "text-destructive")}
      />
    );
  return (
    <Circle
      aria-label="Not started"
      className={cn(shared, "text-muted-foreground/50")}
    />
  );
}
