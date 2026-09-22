import { useState } from "react";
import { ChevronDown } from "lucide-react";
import { ErrorNotice, Loading } from "../../components/common";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { SourceSelection } from "../Sources";
import { ActivityDetail } from "./ActivityDetail";
import { ActivityRows } from "./ActivityRows";
import { emptyFilters, type RunActivity } from "./types";
import { useRunActivity } from "./useRunActivity";

/**
 * The raw captured events of one run: the only place provider, model, request
 * and tool details appear. Collapsed by default and loaded only when opened.
 */
export function TechnicalLog({
  tenderId,
  runId,
  label = "Technical log",
  onSource,
}: {
  tenderId: string;
  runId: string;
  label?: string;
  onSource?: (source: SourceSelection) => void;
}) {
  const [open, setOpen] = useState(false);
  const [selected, setSelected] = useState<RunActivity | null>(null);
  const activity = useRunActivity(tenderId, runId, emptyFilters, open, open);
  const items = activity.data?.items ?? [];
  return (
    <div className="flex flex-col gap-2">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1 self-start text-xs text-muted-foreground hover:text-foreground"
      >
        <ChevronDown
          aria-hidden
          className={cn("size-3.5 transition-transform", !open && "-rotate-90")}
        />
        {label}
      </button>
      {open ? (
        selected ? (
          <ActivityDetail
            base={activity.base}
            activity={selected}
            historyKey={activity.data?.history_key ?? ""}
            onSource={onSource}
            onBack={() => setSelected(null)}
          />
        ) : (
          <div className="flex flex-col gap-2">
            <ErrorNotice error={activity.error ?? activity.earlierError} />
            {activity.data?.has_earlier ? (
              <Button
                variant="outline"
                size="xs"
                className="self-start"
                disabled={activity.earlierBusy}
                onClick={() => void activity.loadEarlier()}
              >
                Show earlier events
              </Button>
            ) : null}
            {activity.isPending ? (
              <Loading>Loading the technical log…</Loading>
            ) : items.length ? (
              <ActivityRows items={items} onSelect={setSelected} />
            ) : activity.data ? (
              <p className="text-xs text-muted-foreground">
                No technical events were recorded.
              </p>
            ) : null}
          </div>
        )
      ) : null}
    </div>
  );
}
