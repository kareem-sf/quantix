import { useId, useState, type ReactNode } from "react";
import { Check, ChevronDown, Pause, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { plainText } from "../rich-text";
import { rtlDir } from "@/lib/text-direction";

export type TaskStatus =
  "todo" | "running" | "waiting" | "blocked" | "done" | "failed";

export type TaskDetail = { label: string; meta?: string };

export type TaskRow = {
  id: string;
  label: string;
  /** One real line under the label, e.g. what was found so far. */
  note?: string;
  /** Right-aligned context, e.g. the owner's name or "12 of 20". */
  meta?: string;
  status: TaskStatus;
  details?: TaskDetail[];
};

const PILLS: Partial<Record<TaskStatus, { label: string; className: string }>> =
  {
    done: { label: "Done", className: "bg-(--success-tint) text-(--success)" },
    failed: {
      label: "Stopped",
      className: "bg-(--destructive-tint) text-destructive",
    },
    waiting: {
      label: "Waiting for you",
      className: "bg-(--warning-tint) text-(--warning-ink)",
    },
    blocked: {
      label: "Blocked",
      className: "bg-(--warning-tint) text-(--warning-ink)",
    },
  };

export function StatusMark({ status }: { status: TaskStatus }) {
  if (status === "done")
    return (
      <span className="bui-pop-in flex size-5 items-center justify-center rounded-full bg-(--success) text-white">
        <Check aria-hidden className="size-3" strokeWidth={3.5} />
      </span>
    );
  if (status === "failed")
    return (
      <span className="bui-pop-in flex size-5 items-center justify-center rounded-full bg-destructive text-white">
        <X aria-hidden className="size-3" strokeWidth={3.5} />
      </span>
    );
  if (status === "waiting" || status === "blocked")
    return (
      <span className="flex size-5 items-center justify-center rounded-full bg-(--warning-tint) text-(--warning-ink)">
        <Pause aria-hidden className="size-3" strokeWidth={3} />
      </span>
    );
  const size = 20;
  const radius = (size - 2) / 2;
  const circumference = 2 * Math.PI * radius;
  return (
    <svg
      aria-hidden
      width={size}
      height={size}
      className={cn(status === "running" && "motion-safe:animate-spin")}
      style={status === "running" ? { animationDuration: "1.1s" } : undefined}
    >
      <circle
        cx={size / 2}
        cy={size / 2}
        r={radius}
        fill="none"
        stroke="var(--border)"
        strokeWidth={2}
      />
      {status === "running" ? (
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          stroke="var(--muted-foreground)"
          strokeWidth={2}
          strokeLinecap="round"
          strokeDasharray={`${circumference * 0.28} ${circumference * 0.72}`}
        />
      ) : null}
    </svg>
  );
}

const STATUS_WORDS: Record<TaskStatus, string> = {
  todo: "Not started",
  running: "In progress",
  waiting: "Waiting for you",
  blocked: "Blocked",
  done: "Done",
  failed: "Stopped",
};

function Row({
  row,
  trailing,
  onSelect,
  showPill,
}: {
  row: TaskRow;
  trailing?: ReactNode;
  onSelect?: (row: TaskRow) => void;
  showPill: boolean;
}) {
  const [open, setOpen] = useState(false);
  const bodyId = useId();
  const expandable = Boolean(row.details?.length);
  const pill = PILLS[row.status];
  const interactive = expandable || Boolean(onSelect);
  return (
    <div className="bui-fade-up border-b last:border-0">
      <div className="flex min-h-11 w-full items-start gap-2.5 px-2.5 py-2.5">
        <span className="mt-px flex size-5 shrink-0 items-center justify-center">
          <StatusMark status={row.status} />
          <span className="sr-only">{STATUS_WORDS[row.status]}</span>
        </span>
        <button
          type="button"
          disabled={!interactive}
          aria-expanded={expandable ? open : undefined}
          aria-controls={expandable ? bodyId : undefined}
          onClick={() => (expandable ? setOpen(!open) : onSelect?.(row))}
          className="flex min-w-0 flex-1 flex-col items-start gap-0.5 text-start"
        >
          <span
            dir={rtlDir(row.label)}
            className={cn(
              "w-full text-sm font-medium wrap-anywhere",
              row.status === "todo" && "text-foreground/75",
            )}
          >
            {plainText(row.label)}
          </span>
          {row.note ? (
            <span
              dir={rtlDir(row.note)}
              className="w-full text-xs leading-relaxed text-muted-foreground wrap-anywhere"
            >
              {plainText(row.note)}
            </span>
          ) : null}
        </button>
        {row.meta ? (
          // Long owner names wrap in their own narrow column instead of
          // squeezing the step title to one letter per line.
          <span className="mt-0.5 max-w-[35%] min-w-0 text-end text-xs text-muted-foreground tabular-nums wrap-break-word">
            {row.meta}
          </span>
        ) : null}
        {showPill && pill && row.status !== "done" ? (
          <span
            className={cn(
              "bui-fade-in inline-flex h-5.5 shrink-0 items-center rounded-full px-2 text-xs font-medium",
              pill.className,
            )}
          >
            {pill.label}
          </span>
        ) : null}
        {trailing}
        {expandable ? (
          <ChevronDown
            aria-hidden
            className={cn(
              "mt-0.5 size-4 shrink-0 text-muted-foreground transition-transform duration-300",
              open && "rotate-180",
            )}
          />
        ) : null}
      </div>
      {expandable ? (
        <div id={bodyId} className="bui-collapse" data-open={open}>
          <div>
            <div className="mb-2.5 grid grid-cols-[20px_1fr] gap-2.5 px-2.5">
              <span aria-hidden className="mx-auto h-full w-px bg-border" />
              <div className="flex flex-col gap-1.5">
                {row.details?.map((detail, index) => (
                  <div
                    key={index}
                    className="flex items-baseline justify-between gap-3"
                  >
                    <span className="text-xs text-foreground/80 wrap-anywhere">
                      {detail.label}
                    </span>
                    {detail.meta ? (
                      <span className="shrink-0 font-mono text-[11px] text-muted-foreground tabular-nums">
                        {detail.meta}
                      </span>
                    ) : null}
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

/**
 * A list of real tasks with their live status: plan items, staff work.
 * Adapted from Beautiful UI's Task Rows (List variant), without its scripted
 * status sequence.
 */
export function TaskRows({
  rows,
  renderTrailing,
  onSelect,
  showStatusPill = true,
  className,
  "aria-label": ariaLabel,
}: {
  rows: TaskRow[];
  renderTrailing?: (row: TaskRow) => ReactNode;
  onSelect?: (row: TaskRow) => void;
  showStatusPill?: boolean;
  className?: string;
  "aria-label"?: string;
}) {
  if (!rows.length) return null;
  return (
    <div
      role="list"
      aria-label={ariaLabel}
      className={cn(
        "flex w-full flex-col overflow-hidden rounded-lg bg-card ring-1 ring-border",
        className,
      )}
    >
      {rows.map((row) => (
        <div role="listitem" key={row.id}>
          <Row
            row={row}
            trailing={renderTrailing?.(row)}
            onSelect={onSelect}
            showPill={showStatusPill}
          />
        </div>
      ))}
    </div>
  );
}
