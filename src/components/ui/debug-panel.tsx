/**
 * Prints any set of values as an object literal. Adapted from Skiper UI
 * (skiper102) by @gurvinder-singh02 — https://skiper-ui.com.
 */
import { cn } from "@/lib/utils";

export function DebugValues({
  values,
  className,
}: {
  values: Record<string, unknown>;
  className?: string;
}) {
  return (
    <div className={cn("font-mono text-xs leading-relaxed", className)}>
      {"{"}
      {Object.entries(values).map(([key, value]) => (
        <div key={key} className="ms-4 break-all">
          <span className="text-muted-foreground">{key}</span>:{" "}
          <span
            className={cn(
              typeof value === "boolean" &&
                (value
                  ? "text-emerald-600 dark:text-emerald-400"
                  : "text-red-600 dark:text-red-400"),
            )}
          >
            {format(value)}
          </span>
          ;
        </div>
      ))}
      {"}"}
    </div>
  );
}

function format(value: unknown) {
  if (value === null || value === undefined) return String(value);
  if (typeof value === "object") {
    try {
      return JSON.stringify(value);
    } catch {
      return "[object]";
    }
  }
  return String(value);
}
