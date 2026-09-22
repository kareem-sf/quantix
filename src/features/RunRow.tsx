import { useState } from "react";
import { AiOrb, orbStateFor } from "@/components/ui/thinking-orb";
import { CircleAlert, RotateCcw, Square } from "lucide-react";
import { isActive, useApi, useRefresh, type Schema } from "../api";
import { ErrorNotice, Status } from "../components/common";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";

/** One piece of Tender work: its state, reported progress and recovery. */
export function RunRow({
  run,
  compact = false,
}: {
  run: Schema<"Run">;
  compact?: boolean;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const [pending, setPending] = useState(false),
    [error, setError] = useState<unknown>(null);
  const active = isActive(run.status);

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
    <article
      className={cn(
        "flex flex-col gap-2 rounded-xl border bg-card text-sm shadow-xs",
        compact ? "p-2.5" : "p-3",
      )}
      id={`run-${run.id}`}
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          {active ? (
            <AiOrb state={orbStateFor(run.detail || runLabel(run.kind))} />
          ) : null}
          <strong className="truncate font-medium">{runLabel(run.kind)}</strong>
        </div>
        <Status value={run.status} />
      </div>
      {/* The reason replaces the generic detail instead of nesting a second
          bordered alert inside this card. "Work needs attention." above a boxed
          error said the same thing twice and doubled the card's height. */}
      {run.error ? (
        <p role="alert" className="flex items-start gap-2 text-destructive/90">
          <CircleAlert aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
          <span className="wrap-anywhere">{run.error}</span>
        </p>
      ) : (
        <p className="text-muted-foreground">
          {run.detail ||
            (active
              ? "Waiting for work to start."
              : "No additional run detail.")}
        </p>
      )}
      {active && run.progress > 0 ? (
        <Progress value={run.progress} aria-label="Reported run progress" />
      ) : null}
      <div className="flex flex-wrap items-center gap-1">
        {active ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={pending}
            onClick={() => void act("cancel")}
          >
            <Square data-icon="inline-start" />
            Cancel
          </Button>
        ) : ["interrupted", "failed", "cancelled"].includes(run.status) &&
          run.kind !== "research" ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={pending}
            onClick={() => void act("resume")}
          >
            <RotateCcw data-icon="inline-start" />
            Resume
          </Button>
        ) : null}
      </div>
      <ErrorNotice error={error} />
    </article>
  );
}

const runLabels: Record<string, string> = {
  manager: "Tender Manager",
  conversation: "Tender Manager",
  import: "Registering documents",
  analysis: "Analyzing tender package",
  index: "Indexing tender evidence",
  identify: "Identifying the project",
};

function runLabel(kind: string) {
  return runLabels[kind] ?? "Research";
}
