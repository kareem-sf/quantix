import { useState, type ReactNode } from "react";
import { cn } from "@/lib/utils";

export type StatusOption<S extends string> = {
  value: S;
  label: string;
  /** Tailwind classes for the status pill, e.g. text and background colour. */
  tone: string;
};

export type StatusColumn<R> = {
  id: string;
  header: string;
  cell: (row: R) => ReactNode;
  className?: string;
};

/**
 * A table whose status chips filter its rows, with counts per status.
 * Adapted from Beautiful UI's Filter Table.
 */
export function StatusTable<R, S extends string>({
  rows,
  columns,
  statuses,
  statusOf,
  rowKey,
  onRowClick,
  empty,
  className,
  "aria-label": ariaLabel,
}: {
  rows: R[];
  columns: StatusColumn<R>[];
  statuses: StatusOption<S>[];
  statusOf: (row: R) => S;
  rowKey: (row: R) => string;
  onRowClick?: (row: R) => void;
  empty?: ReactNode;
  className?: string;
  "aria-label": string;
}) {
  const [filter, setFilter] = useState<S | "all">("all");
  const visible =
    filter === "all" ? rows : rows.filter((row) => statusOf(row) === filter);
  const count = (value: S) =>
    rows.filter((row) => statusOf(row) === value).length;
  const chips: {
    value: S | "all";
    label: string;
    count: number;
    tone?: string;
  }[] = [
    { value: "all", label: "All", count: rows.length },
    ...statuses
      .map((status) => ({ ...status, count: count(status.value) }))
      .filter((status) => status.count > 0),
  ];

  return (
    <div className={cn("flex w-full flex-col gap-1", className)}>
      <div
        role="group"
        aria-label={`Filter ${ariaLabel.toLowerCase()} by status`}
        className="-mx-1 flex items-center gap-1 overflow-x-auto px-1 py-1 [scrollbar-width:none]"
      >
        {chips.map((chip) => {
          const active = filter === chip.value;
          return (
            <button
              key={chip.value}
              type="button"
              aria-pressed={active}
              onClick={() => setFilter(chip.value)}
              className={cn(
                "flex h-6.5 shrink-0 items-center gap-1.5 rounded-full px-2.5 text-xs font-medium transition-[background-color,box-shadow,color] duration-200",
                active
                  ? "bg-card text-foreground shadow-xs ring-1 ring-border"
                  : "text-muted-foreground hover:bg-muted",
              )}
            >
              {chip.label}
              <span
                className={cn(
                  "rounded-[4px] px-1 text-[10.5px] tabular-nums",
                  active
                    ? "bg-muted text-foreground/80"
                    : "text-muted-foreground",
                )}
              >
                {chip.count}
              </span>
            </button>
          );
        })}
      </div>
      <div
        role="region"
        aria-label={ariaLabel}
        tabIndex={0}
        className="overflow-x-auto rounded-lg bg-card ring-1 ring-border"
      >
        <table className="w-full min-w-[26rem] border-collapse text-sm">
          <thead>
            <tr className="border-b text-xs font-medium text-muted-foreground">
              {columns.map((column) => (
                <th
                  key={column.id}
                  className={cn(
                    "border-e px-3 py-2 text-start font-medium last:border-e-0",
                    column.className,
                  )}
                >
                  {column.header}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {visible.map((row) => (
              <tr
                key={rowKey(row)}
                onClick={onRowClick ? () => onRowClick(row) : undefined}
                className={cn(
                  "bui-fade-in border-b transition-colors duration-100 last:border-0",
                  onRowClick && "cursor-pointer hover:bg-muted/60",
                )}
              >
                {columns.map((column) => (
                  <td
                    key={column.id}
                    className={cn(
                      "border-e px-3 py-2 align-top last:border-e-0",
                      column.className,
                    )}
                  >
                    {column.cell(row)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
        {!visible.length ? (
          <div className="px-3 py-6 text-center text-sm text-muted-foreground">
            {empty ?? "Nothing here yet."}
          </div>
        ) : null}
      </div>
    </div>
  );
}

export function StatusPill({ label, tone }: { label: string; tone: string }) {
  return (
    <span
      className={cn(
        "inline-flex h-5.5 shrink-0 items-center rounded-md px-1.5 text-xs font-medium whitespace-nowrap",
        tone,
      )}
    >
      {label}
    </span>
  );
}
