import { useState } from "react";
import { Check, ChevronDown, Ruler, X } from "lucide-react";
import {
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { ErrorNotice, Loading } from "../components/common";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";
import { Citations, type SourceSelection } from "./Sources";

type Line = Schema<"TakeoffLine">;
type Comparison = Line["comparison"];
type Filter = "attention" | Comparison | "all";

const comparisonText: Record<Comparison, string> = {
  matches: "Matches BOQ",
  differs: "Differs from BOQ",
  unit_differs: "Unit differs",
  no_boq_quantity: "BOQ has no quantity",
  not_in_boq: "Missing from BOQ",
  not_on_drawings: "Not on drawings",
};

const comparisonTone: Record<Comparison, string> = {
  matches: "bg-emerald-500",
  differs: "bg-amber-500",
  unit_differs: "bg-amber-500",
  no_boq_quantity: "bg-muted-foreground/60",
  not_in_boq: "bg-destructive",
  not_on_drawings: "bg-sky-500",
};

const methodText: Record<NonNullable<Line["method"]>, string> = {
  dimensions: "From printed dimensions",
  schedule: "From a schedule",
  scaled: "Scaled from the drawing",
  counted: "Counted on the drawing",
};

const filters: { id: Filter; label: string }[] = [
  { id: "attention", label: "Needs a decision" },
  { id: "differs", label: "Differs" },
  { id: "not_in_boq", label: "Missing from BOQ" },
  { id: "not_on_drawings", label: "Not on drawings" },
  { id: "matches", label: "Matches" },
  { id: "all", label: "All" },
];

const TAKEOFF_REQUEST =
  "Do a quantity takeoff from the drawings and check it against the BOQ. List quantities that differ, work on the drawings that the BOQ is missing, and BOQ items the drawings do not show.";

function needsDecision(line: Line) {
  return line.status === "proposed" && line.comparison !== "matches";
}

/** The team's quantity takeoff from the drawings, checked against the BOQ, for the engineer to review. */
export function Takeoff({
  tenderId,
  onSource,
}: {
  tenderId: string;
  onSource: (source: SourceSelection) => void;
}) {
  const api = useApi();
  const refresh = useRefresh();
  const takeoff = useResource<Line[]>(`${tenderPath(tenderId)}/takeoff`, true);
  const [filter, setFilter] = useState<Filter>("attention");
  const [asking, setAsking] = useState(false);
  const [asked, setAsked] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const lines = takeoff.data ?? [];

  const count = (id: Filter) =>
    id === "all"
      ? lines.length
      : id === "attention"
        ? lines.filter(needsDecision).length
        : lines.filter((line) => line.comparison === id).length;
  const shown = lines.filter((line) =>
    filter === "all"
      ? true
      : filter === "attention"
        ? needsDecision(line)
        : line.comparison === filter,
  );

  async function askForTakeoff() {
    setAsking(true);
    setError(null);
    try {
      await api.post<Schema<"MessageSubmission">>(
        `${tenderPath(tenderId)}/messages`,
        {
          content: TAKEOFF_REQUEST,
          idempotency_key: `takeoff-${tenderId}-${Date.now()}`,
        } satisfies Schema<"MessageRequest">,
      );
      setAsked(true);
      await refresh();
    } catch (failure) {
      setError(failure);
    } finally {
      setAsking(false);
    }
  }

  return (
    <section
      aria-label="Quantity takeoff"
      className="flex flex-col gap-4 @container"
    >
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-0.5">
          <h2 className="text-sm font-semibold tracking-tight">
            Quantity takeoff
          </h2>
          <p className="text-xs text-muted-foreground">
            Quantities the team took from the drawings, checked against the BOQ.
          </p>
        </div>
        <Button
          size="sm"
          variant="outline"
          disabled={asking}
          onClick={() => void askForTakeoff()}
        >
          <Ruler data-icon="inline-start" />
          {lines.length
            ? "Ask for a new takeoff"
            : "Ask the team for a takeoff"}
        </Button>
      </header>
      {asked ? (
        <p className="text-xs text-muted-foreground" role="status">
          Sent to the Tender Manager. Lines appear here as the team saves them.
        </p>
      ) : null}
      <ErrorNotice error={error || takeoff.error} />
      {takeoff.isPending ? <Loading>Loading the takeoff…</Loading> : null}
      {takeoff.data && !lines.length ? (
        <p className="rounded-xl border border-dashed p-6 text-center text-sm text-muted-foreground">
          No takeoff yet.
        </p>
      ) : null}
      {lines.length ? (
        <>
          <div
            className="flex flex-wrap gap-1.5"
            role="group"
            aria-label="Show takeoff lines"
          >
            {filters.map((item) => (
              <Button
                key={item.id}
                size="xs"
                variant={filter === item.id ? "secondary" : "ghost"}
                aria-pressed={filter === item.id}
                onClick={() => setFilter(item.id)}
              >
                {item.label}
                <span className="text-muted-foreground">{count(item.id)}</span>
              </Button>
            ))}
          </div>
          {shown.length ? (
            <ul className="flex flex-col gap-2" aria-label="Takeoff lines">
              {shown.map((line) => (
                <li key={line.id}>
                  <TakeoffRow
                    tenderId={tenderId}
                    line={line}
                    onSource={onSource}
                  />
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-sm text-muted-foreground">
              No lines in this view.
            </p>
          )}
        </>
      ) : null}
    </section>
  );
}

function quantity(value: string | null | undefined, unit: string) {
  return value == null ? "—" : `${value} ${unit}`;
}

function TakeoffRow({
  tenderId,
  line,
  onSource,
}: {
  tenderId: string;
  line: Line;
  onSource: (source: SourceSelection) => void;
}) {
  const api = useApi();
  const refresh = useRefresh();
  const [note, setNote] = useState(line.review_note);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<unknown>(null);

  async function review(decision: "accepted" | "rejected") {
    setSaving(true);
    setError(null);
    try {
      await api.post<Line>(
        `${tenderPath(tenderId)}/takeoff/${encodeURIComponent(line.id)}/review`,
        { decision, note: note.trim() } satisfies Schema<"TakeoffReview">,
      );
      await refresh();
    } catch (failure) {
      setError(failure);
    } finally {
      setSaving(false);
    }
  }

  return (
    <details
      className={cn(
        "group rounded-xl border bg-card text-sm",
        line.status === "rejected" && "opacity-60",
      )}
    >
      <summary className="flex cursor-pointer list-none flex-wrap items-start gap-x-3 gap-y-1 p-3">
        <span
          aria-hidden="true"
          className={cn(
            "mt-1.5 size-2 shrink-0 rounded-full",
            comparisonTone[line.comparison],
          )}
        />
        <span className="min-w-0 flex-1">
          <span className="block font-medium" dir="auto">
            {line.description}
          </span>
          <span className="block text-xs text-muted-foreground" dir="auto">
            {[comparisonText[line.comparison], line.location]
              .filter(Boolean)
              .join(" · ")}
          </span>
        </span>
        <span className="flex flex-col items-end text-xs tabular-nums">
          <span>Drawings {quantity(line.quantity, line.unit)}</span>
          {line.boq ? (
            <span className="text-muted-foreground">
              BOQ {quantity(line.boq.quantity, line.boq.unit)}
              {line.difference_percent
                ? ` (${line.difference_percent.startsWith("-") ? "" : "+"}${line.difference_percent}%)`
                : ""}
            </span>
          ) : null}
        </span>
        <span className="flex items-center gap-1.5">
          {!line.is_current ? (
            <Badge variant="outline" className="font-normal">
              Source changed
            </Badge>
          ) : null}
          {line.status !== "proposed" ? (
            <Badge variant="secondary" className="font-normal">
              {line.status === "accepted" ? "Accepted" : "Rejected"}
            </Badge>
          ) : null}
          <ChevronDown className="size-4 text-muted-foreground transition-transform group-open:rotate-180" />
        </span>
      </summary>
      <div className="flex flex-col gap-3 px-3 pb-3 ps-8">
        {line.boq ? (
          <p className="text-xs text-muted-foreground" dir="auto">
            BOQ item: {line.boq.description}
          </p>
        ) : null}
        <div className="flex flex-col gap-0.5">
          <span className="text-xs text-muted-foreground">
            {line.method ? methodText[line.method] : "Working"} · by{" "}
            {line.author}
          </span>
          <p className="whitespace-pre-wrap" dir="auto">
            {line.working}
          </p>
        </div>
        <Citations
          ids={line.source_ids}
          tenderId={tenderId}
          onOpen={onSource}
        />
        <label className="flex flex-col gap-1.5 text-xs text-muted-foreground">
          Your note (optional)
          <Textarea
            dir="auto"
            rows={2}
            maxLength={2000}
            value={note}
            onChange={(event) => setNote(event.target.value)}
          />
        </label>
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            disabled={saving || line.status === "accepted"}
            onClick={() => void review("accepted")}
          >
            <Check data-icon="inline-start" />
            Accept
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={saving || line.status === "rejected"}
            onClick={() => void review("rejected")}
          >
            <X data-icon="inline-start" />
            Reject
          </Button>
        </div>
        <ErrorNotice error={error} />
      </div>
    </details>
  );
}
