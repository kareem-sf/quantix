import { Clock } from "lucide-react";
import { isActive, useResource, type Schema } from "../api";
import {
  AgentPlanning,
  type PlanStep,
  type PlanStepStatus,
} from "@/components/ui/ai-planning";
import { Progress } from "@/components/ui/progress";

const STAGES = [
  ["register", "Registering documents"],
  ["recognise", "Recognising scanned pages"],
  ["index", "Indexing tender evidence"],
  ["structure", "Extracting BOQ, schedules and tables"],
  ["map", "Mapping the tender package"],
] as const;

type StageState = "pending" | "running" | "completed" | "waiting" | "failed";

const planStatus: Record<StageState, PlanStepStatus> = {
  pending: "pending",
  running: "active",
  completed: "success",
  waiting: "pending",
  failed: "error",
};

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
  const steps: PlanStep[] = STAGES.map(([key, label]) => {
    const stage = states[key] ?? { state: "pending" as const, detail: "" };
    return {
      id: key,
      title: label,
      status: planStatus[stage.state],
      icon:
        stage.state === "waiting" ? (
          <Clock className="size-3.5 text-amber-600 dark:text-amber-400" />
        ) : undefined,
      content:
        stage.detail && stage.state !== "running" ? (
          <p className="wrap-anywhere">{stage.detail}</p>
        ) : undefined,
      defaultExpanded: stage.state === "failed" || stage.state === "waiting",
    };
  });
  const active = isActive(run.status);
  return (
    <AgentPlanning
      title="Analyzing tender package"
      meta={active ? `${run.progress}%` : undefined}
      steps={steps}
      header={
        active ? (
          <Progress value={run.progress} aria-label="Analysis progress" />
        ) : undefined
      }
    />
  );
}
