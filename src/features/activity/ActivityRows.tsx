import { useState } from "react";
import { Button } from "../../components/ui/button";
import type { RunActivity } from "./types";
export function ActivityRows({
  items,
  onSelect,
  compact = false,
}: {
  items: RunActivity[];
  onSelect: (item: RunActivity) => void;
  compact?: boolean;
}) {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const operations = new Map(
    items
      .filter((item) => item.operation_id)
      .map((item) => [item.operation_id!, item]),
  );
  const parents = new Set(
    items.map((item) => item.parent_operation_id).filter(Boolean),
  );
  function hidden(item: RunActivity) {
    let parent = item.parent_operation_id;
    const visited = new Set<string>();
    while (parent && !visited.has(parent)) {
      if (collapsed.has(parent)) return true;
      visited.add(parent);
      parent = operations.get(parent)?.parent_operation_id;
    }
    return false;
  }
  return (
    <ol
      className="space-y-1"
      aria-label={compact ? "Recent activity" : "Activity history"}
    >
      {items
        .filter((item) => !hidden(item))
        .map((item) => (
          <li
            key={item.event_id}
            id={`activity-${compact ? "inline" : "history"}-${item.event_id}`}
            className={`min-w-0 border-s-2 py-2 ps-3 ${item.phase === "failed" || item.category === "error" ? "border-destructive/70" : "border-border"} ${item.parent_operation_id ? "ms-4" : ""}`}
            style={{
              contentVisibility: "auto",
              containIntrinsicSize: "auto 90px",
            }}
          >
            <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1 text-xs">
              <span className="font-medium">{item.actor_label}</span>
              <span className="text-muted-foreground">
                {categoryLabel(item.category)} ·{" "}
                {item.phase === "delta" ? "Writing" : item.phase}
              </span>
              <time
                className="ms-auto text-muted-foreground tabular-nums"
                dateTime={item.created_at}
                title={new Date(item.created_at).toLocaleString()}
              >
                {new Date(item.created_at).toLocaleTimeString()}
              </time>
            </div>
            {item.parent_operation_id ? (
              <p className="mt-1 text-xs text-muted-foreground">
                Part of{" "}
                {operations.get(item.parent_operation_id)?.message ??
                  "a parent operation in this work"}
              </p>
            ) : null}
            <p className="mt-1 text-sm leading-relaxed wrap-anywhere">
              {item.message}
            </p>
            {item.tool ? (
              <p className="mt-1 text-xs text-muted-foreground wrap-anywhere">
                Tool: {item.tool}
              </p>
            ) : null}
            {item.elapsed_ms != null ? (
              <p className="mt-1 text-xs text-muted-foreground">
                {(item.elapsed_ms / 1000).toFixed(1)}s elapsed
              </p>
            ) : null}
            {item.operation_id && parents.has(item.operation_id) ? (
              <Button
                variant="ghost"
                size="sm"
                className="me-3 h-auto px-0 py-1 text-xs"
                aria-expanded={!collapsed.has(item.operation_id)}
                aria-label={`${collapsed.has(item.operation_id) ? "Show" : "Hide"} child activity: ${item.message}`}
                onClick={() =>
                  setCollapsed((current) => {
                    const next = new Set(current);
                    if (next.has(item.operation_id!))
                      next.delete(item.operation_id!);
                    else next.add(item.operation_id!);
                    return next;
                  })
                }
              >
                {collapsed.has(item.operation_id)
                  ? "Show child activity"
                  : "Hide child activity"}
              </Button>
            ) : null}
            {item.preview && item.preview !== item.message ? (
              <div>
                <p className="mt-1 text-xs text-muted-foreground">
                  Preview · open captured details for the full record
                </p>
                <p
                  className={`mt-1 whitespace-pre-wrap text-xs leading-relaxed text-muted-foreground wrap-anywhere ${compact ? "line-clamp-3" : "line-clamp-5"}`}
                >
                  {readablePreview(item.preview)}
                </p>
              </div>
            ) : null}
            {item.preview_truncated ? (
              <p className="mt-1 text-xs text-muted-foreground">
                Preview shortened · open captured details for more
              </p>
            ) : null}
            {item.provider ? (
              <p className="mt-1 text-xs text-muted-foreground wrap-anywhere">
                {item.provider}
                {item.model ? ` · ${item.model}` : ""}
              </p>
            ) : null}
            {item.detail_available ? (
              <Button
                variant="link"
                size="sm"
                className="h-auto px-0 py-1 text-xs"
                aria-label={`Open details: ${item.message}`}
                onClick={() => onSelect(item)}
              >
                Open captured details
              </Button>
            ) : (
              <p className="mt-1 text-xs text-muted-foreground">
                {item.capture_status.replaceAll("_", " ")} · no further detail
                captured
              </p>
            )}
          </li>
        ))}
    </ol>
  );
}

function categoryLabel(category: string) {
  return (
    (
      {
        reasoning_summary: "Thinking summary",
        reasoning: "Thinking summary",
        draft: "Draft · awaiting checks",
        model_request: "AI request",
        model_output: "AI response",
        capability: "AI information",
        source: "Source",
        assignment: "Staff assignment",
        tool: "Tool action",
      } as Record<string, string>
    )[category] ?? category.replaceAll("_", " ")
  );
}

function readablePreview(preview: string) {
  const text = preview.trim();
  if (text.startsWith("{") || text.startsWith("[")) {
    try {
      const parsed: unknown = JSON.parse(text);
      if (parsed && typeof parsed === "object")
        return "Structured detail captured. Open details to inspect it.";
    } catch {
      /* Text, code and partially captured content remain readable. */
    }
  }
  return preview;
}
