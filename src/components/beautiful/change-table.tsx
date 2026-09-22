import { useMemo, useState, type ReactNode } from "react";
import { Check } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export type ChangeKind = "add" | "remove" | "change" | "same";

export type ChangeRow = {
  id: string;
  kind: ChangeKind;
  cells: ReactNode[];
  /** Earlier values for a "change" row, shown struck through. */
  before?: ReactNode[];
};

function Mark({
  included,
  tone,
}: {
  included: boolean;
  tone: "add" | "remove";
}) {
  return (
    <span
      aria-hidden
      className={cn(
        "flex size-4.5 shrink-0 items-center justify-center rounded-[5px] transition-[background-color,color,transform] duration-150",
        included
          ? tone === "remove"
            ? "bg-destructive text-white"
            : "bg-(--success) text-white"
          : "scale-90 bg-muted text-muted-foreground ring-1 ring-border",
      )}
    >
      {included ? <Check className="size-3" strokeWidth={3} /> : null}
    </span>
  );
}

/**
 * Proposed changes to a table (plan items, quantities, rates), each row a
 * toggle, applied together. Adapted from Beautiful UI's Diff Table; the diff
 * is shown as soon as it exists, with no staged reveal.
 */
export function ChangeTable({
  title,
  columns,
  rows,
  onApply,
  applying = false,
  applied = false,
  className,
}: {
  title: string;
  columns: string[];
  rows: ChangeRow[];
  onApply: (rowIds: string[]) => void;
  applying?: boolean;
  applied?: boolean;
  className?: string;
}) {
  const changeable = useMemo(
    () => rows.filter((row) => row.kind !== "same"),
    [rows],
  );
  const [excluded, setExcluded] = useState<Set<string>>(new Set());
  const included = changeable.filter((row) => !excluded.has(row.id));
  const counts = {
    add: included.filter((row) => row.kind === "add").length,
    remove: included.filter((row) => row.kind === "remove").length,
    change: included.filter((row) => row.kind === "change").length,
  };
  const toggle = (id: string) =>
    setExcluded((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const summary = [
    counts.add &&
      `${counts.add} ${counts.add === 1 ? "addition" : "additions"}`,
    counts.change &&
      `${counts.change} ${counts.change === 1 ? "change" : "changes"}`,
    counts.remove &&
      `${counts.remove} ${counts.remove === 1 ? "removal" : "removals"}`,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <section
      aria-label={title}
      className={cn(
        "w-full overflow-hidden rounded-lg bg-card ring-1 ring-border",
        className,
      )}
    >
      <div className="flex items-center justify-between gap-3 border-b px-3 py-2">
        <span className="text-sm font-medium">{title}</span>
        {!applied && changeable.length ? (
          <span className="text-xs text-muted-foreground">
            Select rows to include
          </span>
        ) : null}
      </div>
      <div className="overflow-x-auto">
        <table className="w-full border-collapse text-start">
          <thead>
            <tr className="border-b">
              {columns.map((column) => (
                <th
                  key={column}
                  className="px-3 py-2 text-start text-xs font-medium text-muted-foreground"
                >
                  {column}
                </th>
              ))}
              <th className="w-10 px-3 py-2">
                <span className="sr-only">Include</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => {
              const interactive = row.kind !== "same" && !applied;
              const on = row.kind !== "same" && !excluded.has(row.id);
              const tone = row.kind === "remove" ? "remove" : "add";
              return (
                <tr
                  key={row.id}
                  tabIndex={interactive ? 0 : undefined}
                  aria-selected={row.kind === "same" ? undefined : on}
                  onClick={interactive ? () => toggle(row.id) : undefined}
                  onKeyDown={
                    interactive
                      ? (event) => {
                          if (event.key === "Enter" || event.key === " ") {
                            event.preventDefault();
                            toggle(row.id);
                          }
                        }
                      : undefined
                  }
                  className={cn(
                    "border-b transition-colors duration-150 last:border-0 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring focus-visible:ring-inset",
                    interactive && "cursor-pointer",
                    on && row.kind === "remove" && "bg-(--destructive-tint)",
                    on &&
                      (row.kind === "add" || row.kind === "change") &&
                      "bg-(--success-tint)",
                  )}
                >
                  {row.cells.map((cell, index) => (
                    <td
                      key={index}
                      className={cn(
                        "px-3 py-2 align-top text-sm",
                        on &&
                          row.kind === "remove" &&
                          "text-destructive line-through decoration-destructive/50",
                        on && row.kind === "add" && "text-(--success)",
                      )}
                    >
                      {row.kind === "change" &&
                      row.before?.[index] != null &&
                      row.before[index] !== cell ? (
                        <span className="flex flex-col">
                          <span className="text-xs text-muted-foreground line-through">
                            {row.before[index]}
                          </span>
                          <span className={on ? "text-(--success)" : undefined}>
                            {cell}
                          </span>
                        </span>
                      ) : (
                        cell
                      )}
                    </td>
                  ))}
                  <td className="px-3 py-2 align-top">
                    {row.kind !== "same" ? (
                      <Mark included={on} tone={tone} />
                    ) : null}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {changeable.length ? (
        <div className="flex min-h-11 items-center justify-between gap-3 border-t bg-muted/40 px-3 py-2">
          {applied ? (
            <span className="bui-pop-in inline-flex items-center gap-1.5 rounded-full bg-(--success-tint) py-1 ps-1 pe-2.5 text-xs font-medium text-(--success)">
              <span className="flex size-4 items-center justify-center rounded-full bg-(--success) text-white">
                <Check aria-hidden className="size-2.5" strokeWidth={3} />
              </span>
              Applied
            </span>
          ) : (
            <>
              <span className="text-xs text-muted-foreground tabular-nums">
                {summary || "Nothing selected"}
              </span>
              <Button
                size="sm"
                disabled={!included.length || applying}
                onClick={() => onApply(included.map((row) => row.id))}
              >
                {applying
                  ? "Applying…"
                  : `Apply ${included.length} ${included.length === 1 ? "change" : "changes"}`}
              </Button>
            </>
          )}
        </div>
      ) : null}
    </section>
  );
}
