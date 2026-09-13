/**
 * Document reading coverage, as a network of the
 * package. Documents group under their area and connect back to the Manager,
 * so an engineer sees coverage and gaps without reading a table.
 *
 * Drawn directly in SVG. React Flow was tried first and would not sync its
 * edges into its own store here, so the links never rendered; a fixed
 * three-column layout does not need a graph engine.
 */
import { useMemo } from "react";
import { BriefcaseBusiness, FileText } from "lucide-react";
import type { Schema } from "../api";
import { cn } from "@/lib/utils";
import type { SourceSelection } from "./Sources";

type Artifact = Schema<"Artifact">;

const ROW = 52;
const PAD = 12;
const MANAGER = { x: 8, width: 210, height: 62 };
const AREA = { x: 272, width: 180, height: 52 };
const DOCUMENT = { x: 516, width: 252, height: 40 };
const CANVAS_WIDTH = DOCUMENT.x + DOCUMENT.width + PAD;

type ReadingState = "read" | "attention" | "failed";
type Box = { x: number; width: number; top: number; height: number };

function readingState(artifact: Artifact): ReadingState {
  if (artifact.status === "extracted") return "read";
  if (artifact.status === "failed") return "failed";
  return "attention";
}

/** A curved link from the right edge of one card to the left edge of another. */
function link(from: Box, to: Box) {
  const x1 = from.x + from.width;
  const y1 = from.top + from.height / 2;
  const x2 = to.x;
  const y2 = to.top + to.height / 2;
  const bend = Math.max(24, (x2 - x1) / 2);
  return `M ${x1} ${y1} C ${x1 + bend} ${y1}, ${x2 - bend} ${y2}, ${x2} ${y2}`;
}

export function DocumentReadingMap({
  artifacts,
  working = false,
  onSource,
  className,
}: {
  artifacts: Artifact[];
  /** An import or indexing run is under way, so the links animate. */
  working?: boolean;
  onSource: (selection: SourceSelection) => void;
  className?: string;
}) {
  const map = useMemo(() => {
    const current = artifacts.filter((artifact) => artifact.is_current);
    const byArea = new Map<string, Artifact[]>();
    for (const artifact of current) {
      const area = artifact.area || "General";
      byArea.set(area, [...(byArea.get(area) ?? []), artifact]);
    }

    const documents: { top: number; artifact: Artifact }[] = [];
    const areas: { top: number; name: string; total: number; read: number }[] =
      [];
    const links: { key: string; d: string; live: boolean }[] = [];

    let row = 0;
    for (const [name, group] of byArea) {
      const placed = group.map((artifact) => {
        const entry = { top: PAD + row * ROW, artifact };
        row += 1;
        documents.push(entry);
        return entry;
      });
      const area = {
        top:
          (placed[0].top + placed[placed.length - 1].top) / 2 +
          DOCUMENT.height / 2 -
          AREA.height / 2,
        name,
        total: group.length,
        read: group.filter((item) => readingState(item) === "read").length,
      };
      areas.push(area);
      for (const entry of placed)
        links.push({
          key: `${name}-${entry.artifact.id}`,
          d: link({ ...AREA, top: area.top }, { ...DOCUMENT, top: entry.top }),
          live: working && readingState(entry.artifact) !== "read",
        });
    }

    const height = Math.max(PAD * 2 + row * ROW, MANAGER.height + PAD * 2, 160);
    const managerTop = height / 2 - MANAGER.height / 2;
    for (const area of areas)
      links.push({
        key: `manager-${area.name}`,
        d: link({ ...MANAGER, top: managerTop }, { ...AREA, top: area.top }),
        live: working,
      });

    return {
      documents,
      areas,
      links,
      height,
      managerTop,
      read: current.filter((artifact) => readingState(artifact) === "read")
        .length,
      total: current.length,
    };
  }, [artifacts, working]);

  return (
    <div
      className={cn(
        "overflow-auto rounded-xl border bg-muted/20 p-1",
        className,
      )}
      role="group"
      aria-label="Document reading map"
    >
      <div
        className="relative"
        style={{ width: CANVAS_WIDTH, height: map.height }}
      >
        <svg
          className="pointer-events-none absolute inset-0"
          width={CANVAS_WIDTH}
          height={map.height}
          aria-hidden="true"
        >
          {map.links.map((line) => (
            <path
              key={line.key}
              d={line.d}
              fill="none"
              strokeWidth={1.5}
              className={cn(
                "stroke-border",
                line.live && "quantix-link-flow stroke-primary/60",
              )}
            />
          ))}
        </svg>

        <div
          className="absolute flex items-center gap-3 rounded-xl border bg-card p-3 shadow-sm"
          style={{
            left: MANAGER.x,
            top: map.managerTop,
            width: MANAGER.width,
            height: MANAGER.height,
          }}
        >
          <span
            className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-primary text-primary-foreground"
            aria-hidden="true"
          >
            <BriefcaseBusiness className="size-4" />
          </span>
          <span className="flex min-w-0 flex-col">
            <strong className="truncate text-sm font-medium">
              Document reading
            </strong>
            <span className="text-xs text-muted-foreground">
              {`${map.read} of ${map.total} read`}
            </span>
          </span>
        </div>

        {map.areas.map((area) => (
          <div
            key={area.name}
            className="absolute flex flex-col justify-center rounded-lg border bg-muted/70 px-3"
            style={{
              left: AREA.x,
              top: area.top,
              width: AREA.width,
              height: AREA.height,
            }}
          >
            <span className="truncate text-xs font-medium" dir="auto">
              {area.name}
            </span>
            <span className="text-xs text-muted-foreground">
              {area.read} of {area.total} read
            </span>
          </div>
        ))}

        {map.documents.map((entry) => {
          const state = readingState(entry.artifact);
          return (
            <button
              key={entry.artifact.id}
              type="button"
              title={entry.artifact.relative_path}
              onClick={() =>
                onSource({
                  artifactId: entry.artifact.id,
                  artifact: entry.artifact,
                  version: entry.artifact.version,
                  contentHash: entry.artifact.content_hash,
                })
              }
              className={cn(
                "absolute flex items-center gap-2 rounded-lg border bg-card px-2.5 text-start shadow-xs transition-colors hover:bg-accent",
                state === "read" && "border-emerald-500/40",
                state === "attention" && "border-amber-500/50",
                state === "failed" && "border-destructive/50",
              )}
              style={{
                left: DOCUMENT.x,
                top: entry.top,
                width: DOCUMENT.width,
                height: DOCUMENT.height,
              }}
            >
              <FileText
                className="size-3.5 shrink-0 text-muted-foreground"
                aria-hidden="true"
              />
              <span className="min-w-0 flex-1 truncate text-xs" dir="auto">
                {entry.artifact.name}
              </span>
              <span
                aria-hidden="true"
                className={cn(
                  "size-1.5 shrink-0 rounded-full",
                  state === "read"
                    ? "bg-emerald-500"
                    : state === "failed"
                      ? "bg-destructive"
                      : "bg-amber-500",
                )}
              />
            </button>
          );
        })}
      </div>
    </div>
  );
}
