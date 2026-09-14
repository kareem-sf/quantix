import { useEffect, useMemo, useState } from "react";
import { Check, CircleAlert, ListTree } from "lucide-react";
import { isActive } from "../api";
import { AgentPlanning, type PlanStep } from "@/components/ui/ai-planning";
import { Button } from "@/components/ui/button";
import ChatReasoning from "@/components/ui/chat-reasoning";
import { LiveRunStream } from "./LiveRunStream";
import { emptyFilters } from "./activity/types";
import {
  formatDuration,
  runPlan,
  type RunReasoningPart,
} from "./activity/run-plan";
import { useRunActivity } from "./activity/useRunActivity";
import type { SourceSelection } from "./Sources";

/**
 * How the Tender Manager is tackling a request: its reasoning, then each step
 * it takes (searching, reading, checking, writing) as a live timeline. The full
 * captured activity stays one click away for anyone who wants every detail.
 */
export function ManagerPlanning({
  tenderId,
  runId,
  status,
  startedAt,
  onSource,
  onRunDetails,
}: {
  tenderId: string;
  runId: string;
  status?: string;
  startedAt?: string;
  onSource?: (source: SourceSelection) => void;
  onRunDetails?: () => void;
}) {
  const [showActivity, setShowActivity] = useState(false);
  const activity = useRunActivity(tenderId, runId, emptyFilters, true, true);
  const page = activity.data;
  const active = isActive(page?.run_status ?? status ?? "");
  const { steps, reasoning } = useMemo(
    () => runPlan(page?.items ?? [], active),
    [page?.items, active],
  );
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (!active) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [active]);

  const failed = ["failed", "cancelled", "interrupted"].includes(
    page?.run_status ?? status ?? "",
  );
  const began = startedAt ?? page?.items?.[0]?.created_at;
  const ended = active ? now : Date.parse(page?.run_updated_at ?? began ?? "");
  const elapsed = began ? formatDuration(ended - Date.parse(began)) : undefined;

  const planSteps: PlanStep[] = steps.map((step) => ({
    id: step.id,
    title: step.count > 1 ? `${step.title} (${step.count})` : step.title,
    status: step.status,
    duration: formatDuration(step.durationMs),
    content: step.detail ? (
      <p className="whitespace-pre-wrap wrap-anywhere">{step.detail}</p>
    ) : undefined,
  }));
  if (active && !planSteps.some((step) => step.status === "active"))
    planSteps.push({
      id: "next",
      title: planSteps.length
        ? "Deciding the next step"
        : "Working out what this request needs",
      status: "active",
    });

  const title = active
    ? "Tender Manager is planning"
    : failed
      ? "Tender Manager stopped"
      : "How the Tender Manager worked";

  return (
    <div className="flex flex-col gap-2">
      <AgentPlanning
        title={title}
        meta={elapsed}
        steps={planSteps}
        defaultExpanded={active}
        header={
          reasoning.some((part) => part.kind === "thought") ? (
            <ChatReasoning<RunReasoningPart>
              parts={reasoning}
              reasoning={active}
              renderPart={renderReasoning}
            />
          ) : undefined
        }
        footer={
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="-ms-2 text-muted-foreground"
            aria-expanded={showActivity}
            onClick={() => setShowActivity((value) => !value)}
          >
            <ListTree data-icon="inline-start" />
            {showActivity ? "Hide full activity" : "Show full activity"}
          </Button>
        }
      />
      {showActivity ? (
        <LiveRunStream
          tenderId={tenderId}
          runId={runId}
          status={status}
          startedAt={startedAt}
          onSource={onSource}
          onRunDetails={onRunDetails}
        />
      ) : null}
    </div>
  );
}

function renderReasoning(part: RunReasoningPart) {
  if (part.kind === "thought")
    return (
      <p className="py-1 text-sm leading-relaxed whitespace-pre-wrap text-muted-foreground wrap-anywhere">
        {part.text}
      </p>
    );
  return (
    <div className="flex items-center gap-1.5 py-1 text-sm text-muted-foreground">
      {part.status === "error" ? (
        <CircleAlert className="size-4 text-destructive" aria-hidden />
      ) : (
        <Check
          className="size-4 text-emerald-600 dark:text-emerald-400"
          aria-hidden
        />
      )}
      <span>{part.title}</span>
    </div>
  );
}
