import { useId, useState, type ReactNode } from "react";
import {
  BookOpen,
  Calculator,
  ChevronDown,
  CircleAlert,
  ClipboardCheck,
  Eye,
  FilePenLine,
  FileText,
  ListChecks,
  Search,
  Send,
  UserPlus,
  type LucideIcon,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { plainText } from "../rich-text";
import { rtlDir } from "@/lib/text-direction";
import { formatElapsed } from "./live-status";

export type FactKind =
  | "read"
  | "search"
  | "view"
  | "check"
  | "save"
  | "hire"
  | "assign"
  | "calculate"
  | "propose"
  | "other";

export type FactState = "running" | "done" | "failed";

/** One real action and its result, as formatted by the backend. */
export type Fact = {
  id: string;
  kind: FactKind;
  /** Plain sentence, e.g. "Read" or "Searched the documents". */
  line: string;
  /** The thing acted on, shown as a chip, e.g. a file name. */
  subject?: string;
  /** Short result shown after the line, e.g. "4 matches in 2 files". */
  result?: string;
  state: FactState;
  durationMs?: number | null;
  /** What the action actually found, trimmed to the relevant lines. */
  found?: string[];
  /** Exact search terms, locators and similar, behind a small toggle. */
  details?: string[];
};

const ICONS: Record<FactKind, LucideIcon> = {
  read: FileText,
  search: Search,
  view: Eye,
  check: ListChecks,
  save: FilePenLine,
  hire: UserPlus,
  assign: Send,
  calculate: Calculator,
  propose: ClipboardCheck,
  other: BookOpen,
};

function FactRow({ fact, action }: { fact: Fact; action?: ReactNode }) {
  const [open, setOpen] = useState(false);
  const [showDetails, setShowDetails] = useState(false);
  const bodyId = useId();
  const Icon = fact.state === "failed" ? CircleAlert : ICONS[fact.kind];
  const expandable = Boolean(
    fact.found?.length || fact.details?.length || action,
  );
  return (
    <div className="bui-fade-up">
      <button
        type="button"
        aria-expanded={expandable ? open : undefined}
        aria-controls={expandable ? bodyId : undefined}
        disabled={!expandable}
        onClick={() => setOpen(!open)}
        className="group/row -mx-1 flex min-h-7 w-[calc(100%+0.5rem)] min-w-0 items-center gap-2 rounded-md px-1 text-start transition-colors duration-100 enabled:hover:bg-muted/70"
      >
        <span className="relative flex size-4 shrink-0 items-center justify-center text-muted-foreground">
          <Icon
            aria-hidden
            className={cn(
              "size-3.5 transition-opacity duration-100",
              expandable && "group-hover/row:opacity-0",
              open && "opacity-0",
              fact.state === "failed" && "text-destructive",
            )}
          />
          {expandable ? (
            <ChevronDown
              aria-hidden
              className={cn(
                "absolute size-3.5 opacity-0 transition-[opacity,transform] duration-150 group-hover/row:opacity-100",
                open ? "opacity-100" : "-rotate-90",
              )}
            />
          ) : null}
        </span>
        <span
          className={cn(
            "shrink-0 text-xs font-medium",
            fact.state === "running" ? "bui-shimmer" : "text-foreground",
            fact.state === "failed" && "text-destructive",
          )}
        >
          {fact.line}
        </span>
        {fact.subject ? (
          <span className="inline-flex h-5.5 min-w-0 items-center truncate rounded-sm bg-muted px-1.5 text-xs text-foreground/80 ring-1 ring-border/60">
            {plainText(fact.subject)}
          </span>
        ) : null}
        {fact.result ? (
          <span className="min-w-0 truncate text-xs text-muted-foreground">
            {plainText(fact.result)}
          </span>
        ) : null}
        <span className="ms-auto shrink-0 font-mono text-[11px] text-muted-foreground tabular-nums">
          {fact.state === "running"
            ? null
            : fact.durationMs != null
              ? fact.durationMs < 1000
                ? `${(fact.durationMs / 1000).toFixed(1)}s`
                : formatElapsed(fact.durationMs)
              : null}
        </span>
      </button>
      {expandable ? (
        <div id={bodyId} className="bui-collapse" data-open={open}>
          <div>
            <div className="ms-2 mt-0.5 mb-1.5 flex flex-col gap-1 border-s py-0.5 ps-3.5">
              {fact.found?.map((line, index) => (
                <p
                  key={index}
                  dir={rtlDir(line)}
                  className="bidi-text text-xs leading-relaxed text-foreground/80 wrap-anywhere"
                >
                  {plainText(line)}
                </p>
              ))}
              {action || fact.details?.length ? (
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                  {action}
                  {fact.details?.length ? (
                    <button
                      type="button"
                      aria-expanded={showDetails}
                      onClick={() => setShowDetails(!showDetails)}
                      className="text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
                    >
                      {showDetails ? "Hide details" : "Details"}
                    </button>
                  ) : null}
                </div>
              ) : null}
              {showDetails
                ? fact.details?.map((line, index) => (
                    <p
                      key={index}
                      className="bui-fade-in text-xs text-muted-foreground wrap-anywhere"
                    >
                      {line}
                    </p>
                  ))
                : null}
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

/**
 * The real actions taken in one turn, one compact row each. Rows expand to
 * show what the action found. Adapted from Beautiful UI's Tool Chips.
 */
export function FactList({
  facts,
  renderAction,
  className,
}: {
  facts: Fact[];
  /** Per-fact action, typically "Open page" for a document read. */
  renderAction?: (fact: Fact) => ReactNode;
  className?: string;
}) {
  if (!facts.length) return null;
  return (
    <div className={cn("flex flex-col gap-0.5 py-0.5", className)}>
      {facts.map((fact) => (
        <FactRow key={fact.id} fact={fact} action={renderAction?.(fact)} />
      ))}
    </div>
  );
}
