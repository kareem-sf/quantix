import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ChevronDown, RotateCcw } from "lucide-react";
import {
  isActive,
  tenderPath,
  useApi,
  useRefresh,
  type Schema,
} from "../../api";
import { ErrorNotice } from "../../components/common";
import { Button } from "@/components/ui/button";
import { FactList } from "@/components/beautiful/fact-list";
import {
  formatElapsed,
  LiveStatus,
  useElapsed,
} from "@/components/beautiful/live-status";
import { ThinkingTrace, WorkBlock } from "@/components/beautiful/work-block";
import { cn } from "@/lib/utils";
import { isStructured, plainText, RichText } from "@/components/rich-text";
import { textDirection } from "@/lib/text-direction";
import { useFullRunActivity } from "./useFullRunActivity";
import type { SourceSelection } from "../Sources";
import {
  blockHeading,
  buildWorkLog,
  stopReason,
  workSummary,
  type FactOpen,
  type WorkLog as WorkLogData,
  type WorkLogBlock,
} from "./model";

type WorkBriefState = {
  brief?: { steps?: { title: string; state: string }[] } | null;
};

function openAction(
  open: FactOpen | undefined,
  onSource?: (source: SourceSelection) => void,
) {
  if (!open || !onSource) return null;
  const page = typeof open.page === "number" ? open.page : undefined;
  const selection: SourceSelection | null = open.source_id
    ? { sourceId: open.source_id, page }
    : open.artifact_id
      ? { artifactId: open.artifact_id, page }
      : null;
  if (!selection) return null;
  return (
    <button
      type="button"
      onClick={() => onSource(selection)}
      className="text-xs font-medium text-brand-ink underline-offset-2 hover:underline"
    >
      {page ? `Open page ${page}` : "Open document"}
    </button>
  );
}

function Block({
  block,
  log,
  live,
  open,
  onOpenChange,
  onSource,
}: {
  block: WorkLogBlock;
  log: WorkLogData;
  live: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSource?: (source: SourceSelection) => void;
}) {
  const note = block.note.trim();
  const longNote = Boolean(note) && isStructured(note);
  // Long formatted text with no actions is the AI drafting its answer, not a
  // working note; it gets a plain title and shows formatted inside the step.
  const heading = note ? (
    longNote ? (
      block.facts.length ? (
        plainText(note, 160)
      ) : (
        "Wrote the answer"
      )
    ) : (
      <RichText inline text={note} />
    )
  ) : (
    blockHeading(block)
  );
  const working =
    live &&
    (!block.settled || block.facts.some((fact) => fact.state === "running"));
  const duration = block.endedAt
    ? Date.parse(block.endedAt) - Date.parse(block.startedAt)
    : null;
  return (
    <WorkBlock
      headingDir={
        note && !(longNote && !block.facts.length)
          ? textDirection(note)
          : undefined
      }
      heading={
        heading ||
        (block.thinking
          ? working
            ? "Thinking…"
            : `Thought for ${Math.max(1, Math.round((block.thinkingMs ?? duration ?? 0) / 1000))} seconds`
          : working
            ? "Working…"
            : "Wrote the answer")
      }
      working={working}
      failed={block.failed && !block.facts.length}
      durationMs={working ? null : duration}
      open={open}
      onOpenChange={onOpenChange}
    >
      {longNote ? (
        <RichText
          text={note}
          className="py-1 text-sm"
          onSource={onSource ? (sourceId) => onSource({ sourceId }) : undefined}
        />
      ) : null}
      {block.thinking ? (
        <ThinkingTrace
          text={block.thinking}
          streaming={live && !block.thinkingDone && !block.settled}
          durationMs={block.thinkingMs}
        />
      ) : null}
      <FactList
        facts={block.facts}
        renderAction={(fact) =>
          openAction((fact as { open?: FactOpen }).open, onSource)
        }
      />
      {block.facts
        .filter(
          (fact) =>
            fact.kind === "assign" &&
            fact.assignmentId &&
            log.staff.has(fact.assignmentId),
        )
        .map((fact) => {
          const staff = log.staff.get(fact.assignmentId!)!;
          return (
            <StaffSection
              key={staff.assignmentId}
              name={fact.subject ?? staff.name}
              blocks={staff.blocks}
              log={log}
              live={live}
              onSource={onSource}
            />
          );
        })}
    </WorkBlock>
  );
}

function StaffSection({
  name,
  blocks,
  log,
  live,
  onSource,
}: {
  name: string;
  blocks: WorkLogBlock[];
  log: WorkLogData;
  live: boolean;
  onSource?: (source: SourceSelection) => void;
}) {
  const [open, setOpen] = useState<Record<string, boolean>>({});
  if (!blocks.length) return null;
  return (
    <section
      aria-label={`${name}'s work`}
      className="mt-1 mb-1.5 flex flex-col gap-0.5 rounded-md bg-muted/40 px-2.5 py-1.5"
    >
      <p className="text-xs font-medium text-muted-foreground">{name}</p>
      {blocks.map((block, index) => (
        <Block
          key={block.id}
          block={block}
          log={log}
          live={live}
          open={open[block.id] ?? (live && index === blocks.length - 1)}
          onOpenChange={(value) =>
            setOpen((all) => ({ ...all, [block.id]: value }))
          }
          onSource={onSource}
        />
      ))}
    </section>
  );
}

/**
 * What the Tender Manager is doing for one request, live: its own notes, its
 * thinking, and the real actions it took with what they found. When the job
 * ends the log folds to one summary line; a stopped job says why in plain
 * terms and offers to continue.
 */
export function WorkLog({
  tenderId,
  runId,
  run,
  status,
  startedAt,
  onSource,
  onRaiseLimit,
  current = true,
}: {
  /** False for older jobs: a stopped one folds and offers no Continue. */
  current?: boolean;
  tenderId: string;
  runId: string;
  run?: Schema<"Run">;
  status?: string;
  startedAt?: string;
  onSource?: (source: SourceSelection) => void;
  onRaiseLimit?: () => void;
}) {
  const api = useApi();
  const refresh = useRefresh();
  const activity = useFullRunActivity(tenderId, runId);
  const page = activity.data;
  const runStatus = page?.run_status ?? run?.status ?? status ?? "";
  const active = isActive(runStatus);
  const stopped = ["failed", "cancelled", "interrupted"].includes(runStatus);
  const log = useMemo(() => buildWorkLog(page?.items ?? []), [page?.items]);
  const [expanded, setExpanded] = useState<boolean | null>(null);
  const [openBlocks, setOpenBlocks] = useState<Record<string, boolean>>({});
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>(null);

  const began = startedAt ?? run?.created_at ?? page?.items?.[0]?.created_at;
  const liveElapsed = useElapsed(began, active);
  const endedAt = page?.run_updated_at ?? run?.updated_at;
  const total = active
    ? liveElapsed
    : began && endedAt
      ? Date.parse(endedAt) - Date.parse(began)
      : null;

  const brief = useQuery<WorkBriefState>({
    queryKey: ["work-brief", tenderId, runId],
    enabled: stopped && current,
    staleTime: 30_000,
    queryFn: ({ signal }) =>
      api.get<WorkBriefState>(`${tenderPath(tenderId)}/work-brief`, signal),
  });

  // A finished job folds; live and stopped jobs stay open unless closed.
  const isOpen = expanded ?? (active || (stopped && current));
  useEffect(() => {
    if (!active) setOpenBlocks({});
  }, [active]);

  async function act(action: "cancel" | "resume") {
    setPending(true);
    setError(null);
    try {
      await api.post(`/runs/${encodeURIComponent(runId)}/${action}`);
      await refresh();
    } catch (failure) {
      setError(failure);
    } finally {
      setPending(false);
    }
  }

  const last = log.blocks.at(-1);
  const waiting =
    active &&
    (!last ||
      (last.settled && !last.facts.some((fact) => fact.state === "running")) ||
      (!last.settled && !last.note && !last.thinking && !last.facts.length));
  const waitingSince = !last
    ? began
    : last.settled
      ? last.endedAt
      : last.startedAt;
  const summary = workSummary(log);
  const stop = stopped ? stopReason(runStatus, run?.error) : null;
  const kept = summary.filter((part) => !part.startsWith("searched"));
  const notDone = (brief.data?.brief?.steps ?? [])
    .filter((step) => step.state !== "done")
    .map((step) => step.title);

  const title = active
    ? "Working"
    : stopped
      ? "Stopped"
      : `Worked for ${total != null ? formatElapsed(total) : "a moment"}`;

  return (
    <section aria-label="Work log" className="flex min-w-0 flex-col">
      <button
        type="button"
        aria-expanded={isOpen}
        onClick={() => setExpanded(!isOpen)}
        className="group -mx-1.5 flex min-w-0 items-start gap-2 rounded-md px-1.5 py-1 text-start transition-colors hover:bg-muted/60"
      >
        <ChevronDown
          aria-hidden
          className={cn(
            "mt-0.5 size-4 shrink-0 text-muted-foreground transition-transform duration-300",
            !isOpen && "-rotate-90",
          )}
        />
        <span className="min-w-0 flex-1 text-sm text-muted-foreground wrap-anywhere">
          <span
            className={cn(
              "font-medium",
              stopped ? "text-destructive" : "text-foreground",
            )}
          >
            {title}
          </span>
          {!active && !stopped && summary.length
            ? ` · ${summary.join(" · ")}`
            : null}
          {stop && !current ? ` · ${stop.reason}` : null}
        </span>
        {(active || stopped) && total != null ? (
          <span className="mt-0.5 shrink-0 font-mono text-xs text-muted-foreground tabular-nums">
            {formatElapsed(total)}
          </span>
        ) : null}
      </button>

      <div className="bui-collapse" data-open={isOpen}>
        <div>
          <div className="ms-[7px] flex flex-col gap-0.5 border-s ps-3 pt-0.5 pb-1">
            {log.blocks.map((block, index) => {
              const current = active && index === log.blocks.length - 1;
              return (
                <Block
                  key={block.id}
                  block={block}
                  log={log}
                  live={active}
                  open={openBlocks[block.id] ?? current}
                  onOpenChange={(value) =>
                    setOpenBlocks((all) => ({ ...all, [block.id]: value }))
                  }
                  onSource={onSource}
                />
              );
            })}

            {waiting ? (
              <LiveStatus
                className="py-1"
                label={
                  log.blocks.length
                    ? "Waiting for the Tender Manager's reply"
                    : "Starting"
                }
                since={waitingSince}
              />
            ) : null}

            {stop ? (
              <div
                role="alert"
                className="mt-1 flex flex-col gap-1 rounded-md bg-(--destructive-tint) px-3 py-2 text-sm"
              >
                <p className="font-medium text-destructive">
                  Stopped: {stop.reason}.
                </p>
                {kept.length ? (
                  <p className="text-foreground/80">
                    <span className="font-medium">Kept:</span> {kept.join(", ")}
                    .
                  </p>
                ) : null}
                {notDone.length ? (
                  <p className="text-foreground/80">
                    <span className="font-medium">Not done:</span>{" "}
                    {notDone.join(", ")}.
                  </p>
                ) : null}
                <div className="mt-1 flex flex-wrap gap-1.5">
                  {stop.limit && onRaiseLimit ? (
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      onClick={onRaiseLimit}
                    >
                      Raise limit
                    </Button>
                  ) : null}
                  {current && run?.kind !== "research" ? (
                    <Button
                      type="button"
                      size="sm"
                      disabled={pending}
                      onClick={() => void act("resume")}
                    >
                      <RotateCcw data-icon="inline-start" />
                      Continue
                    </Button>
                  ) : null}
                </div>
              </div>
            ) : null}
            <ErrorNotice error={error ?? activity.error} />
          </div>
        </div>
      </div>
    </section>
  );
}
