import { useState, type ReactNode } from "react";
import {
  BrainCircuit,
  Check,
  ChevronDown,
  CircleAlert,
  Loader2,
} from "lucide-react";
import { cn } from "@/lib/utils";

export type PlanStepStatus = "pending" | "active" | "success" | "error";

export interface PlanStep {
  id: string;
  title: string;
  content?: ReactNode;
  status: PlanStepStatus;
  icon?: ReactNode;
  duration?: string;
  defaultExpanded?: boolean;
}

export interface AgentPlanningProps {
  title: string;
  steps: PlanStep[];
  /** Shown beside the title, for example an elapsed time. */
  meta?: ReactNode;
  /** Rendered above the step timeline, for example the agent's reasoning. */
  header?: ReactNode;
  /** Rendered below the step timeline, for example a link to full activity. */
  footer?: ReactNode;
  defaultExpanded?: boolean;
  className?: string;
}

const statusRing: Record<PlanStepStatus, string> = {
  success:
    "bg-emerald-100 text-emerald-700 dark:bg-emerald-500/20 dark:text-emerald-400",
  active: "bg-primary text-primary-foreground",
  error: "bg-destructive/10 text-destructive dark:bg-destructive/20",
  pending: "bg-muted text-muted-foreground",
};

const statusLabel: Record<PlanStepStatus, string> = {
  success: "Done",
  active: "In progress",
  error: "Needs attention",
  pending: "Not started",
};

/** A collapsible timeline of the steps an agent is taking, in order. */
export function AgentPlanning({
  title,
  steps,
  meta,
  header,
  footer,
  defaultExpanded = true,
  className,
}: AgentPlanningProps) {
  const [open, setOpen] = useState(defaultExpanded);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const hasActive = steps.some((step) => step.status === "active");
  const hasError = steps.some((step) => step.status === "error");
  const allDone =
    steps.length > 0 && steps.every((step) => step.status === "success");

  return (
    <section
      aria-label={title}
      className={cn(
        "w-full overflow-hidden rounded-xl border bg-card text-card-foreground shadow-xs",
        className,
      )}
    >
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className={cn(
          "flex w-full items-center gap-3 px-4 py-3 text-start transition-colors outline-none select-none hover:bg-muted/40 focus-visible:ring-3 focus-visible:ring-ring/50",
          open && "border-b bg-muted/30",
        )}
      >
        <span className="flex size-5 items-center justify-center" aria-hidden>
          {hasActive ? (
            <Loader2 className="size-4 animate-spin text-foreground/70" />
          ) : hasError ? (
            <CircleAlert className="size-4 text-destructive" />
          ) : allDone ? (
            <Check className="size-4 text-emerald-600 dark:text-emerald-400" />
          ) : (
            <BrainCircuit className="size-4 text-muted-foreground" />
          )}
        </span>
        <span className="min-w-0 flex-1 truncate text-sm font-medium">
          {title}
        </span>
        {meta ? (
          <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
            {meta}
          </span>
        ) : null}
        <ChevronDown
          aria-hidden
          className={cn(
            "size-4 shrink-0 text-muted-foreground transition-transform duration-200",
            !open && "-rotate-90",
          )}
        />
      </button>

      <div
        className={cn(
          "grid transition-all duration-300 ease-out motion-reduce:transition-none",
          open ? "grid-rows-[1fr] opacity-100" : "grid-rows-[0fr] opacity-0",
        )}
        inert={!open}
      >
        <div className="overflow-hidden">
          <div className="flex flex-col px-4 pt-3 pb-1">
            {header ? <div className="mb-3">{header}</div> : null}
            <ol aria-live="polite" className="flex flex-col">
              {steps.map((step, index) => {
                const isLast = index === steps.length - 1;
                const stepOpen = expanded[step.id] ?? !!step.defaultExpanded;
                return (
                  <li
                    key={step.id}
                    className={cn(
                      "relative flex gap-3 animate-in fade-in slide-in-from-top-1 duration-300 fill-mode-both motion-reduce:animate-none",
                      step.status === "pending" && "opacity-60",
                    )}
                    style={{ animationDelay: `${Math.min(index, 8) * 50}ms` }}
                  >
                    {!isLast ? (
                      <span
                        aria-hidden
                        className="absolute start-[11px] top-7 -bottom-1 w-0.5 rounded-full bg-border"
                      />
                    ) : null}
                    <span
                      className={cn(
                        "relative z-10 mt-0.5 flex size-6 flex-none items-center justify-center rounded-full ring-4 ring-card transition-colors",
                        statusRing[step.status],
                      )}
                      aria-label={statusLabel[step.status]}
                      role="img"
                    >
                      {step.status === "success" ? (
                        <Check className="size-3.5" />
                      ) : step.status === "active" ? (
                        <Loader2 className="size-3.5 animate-spin" />
                      ) : step.status === "error" ? (
                        <CircleAlert className="size-3.5" />
                      ) : (
                        (step.icon ?? (
                          <span className="size-1.5 rounded-full bg-current" />
                        ))
                      )}
                    </span>
                    <div className="min-w-0 flex-1 pb-4">
                      {step.content ? (
                        <button
                          type="button"
                          aria-expanded={stepOpen}
                          onClick={() =>
                            setExpanded((current) => ({
                              ...current,
                              [step.id]: !stepOpen,
                            }))
                          }
                          className="group -mx-2 flex w-[calc(100%+1rem)] items-center gap-3 rounded-md px-2 py-1 text-start outline-none hover:bg-muted/50 focus-visible:ring-3 focus-visible:ring-ring/50"
                        >
                          <StepTitle step={step} />
                          <ChevronDown
                            aria-hidden
                            className={cn(
                              "size-4 shrink-0 text-muted-foreground/60 transition-transform group-hover:text-muted-foreground",
                              !stepOpen && "-rotate-90",
                            )}
                          />
                        </button>
                      ) : (
                        <div className="-mx-2 flex items-center gap-3 px-2 py-1">
                          <StepTitle step={step} />
                        </div>
                      )}
                      {step.content ? (
                        <div
                          className={cn(
                            "grid transition-all duration-300 ease-out motion-reduce:transition-none",
                            stepOpen
                              ? "mt-1 grid-rows-[1fr] opacity-100"
                              : "grid-rows-[0fr] opacity-0",
                          )}
                          inert={!stepOpen}
                        >
                          <div className="overflow-hidden">
                            <div className="pt-1 pb-1 text-xs leading-relaxed text-muted-foreground">
                              {step.content}
                            </div>
                          </div>
                        </div>
                      ) : null}
                    </div>
                  </li>
                );
              })}
            </ol>
            {footer ? <div className="pb-3">{footer}</div> : null}
          </div>
        </div>
      </div>
    </section>
  );
}

function StepTitle({ step }: { step: PlanStep }) {
  return (
    <>
      <span
        className={cn(
          "min-w-0 flex-1 text-sm wrap-anywhere",
          step.status === "active" && "font-medium text-foreground",
          step.status === "error" && "font-medium text-destructive",
          step.status === "success" && "text-foreground/80",
          step.status === "pending" && "text-muted-foreground",
        )}
      >
        {step.title}
      </span>
      {step.duration ? (
        <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
          {step.duration}
        </span>
      ) : null}
    </>
  );
}

export default AgentPlanning;
