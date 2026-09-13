import { useCallback, useEffect, useRef, useState } from "react";
import { errorText, tenderPath, useApi, type Schema } from "../../api";
import { Citations, type SourceSelection } from "../Sources";

type NotebookEntry = Schema<"NotebookEntry">;
type NotebookPage = Schema<"NotebookPage">;

const PAGE_SIZE = 50;

type NotebookView = "all" | "open" | "history";

export function StaffNotebook({
  tenderId,
  staffId,
  refreshKey,
  onSource,
  onOpenResult,
  onOpenOutput,
}: {
  tenderId: string;
  staffId: string;
  refreshKey?: number;
  onSource: (source: SourceSelection) => void;
  onOpenResult?: (resultId: string) => void;
  onOpenOutput?: (outputId: string) => void;
}) {
  const api = useApi();
  const identityKey = `${tenderId}\u0000${staffId}`;
  const requestControllerRef = useRef<AbortController | null>(null);
  const generationRef = useRef(0);
  const [view, setView] = useState<NotebookView>("all");
  const [searchDraft, setSearchDraft] = useState("");
  const [search, setSearch] = useState("");
  const [items, setItems] = useState<NotebookEntry[]>([]);
  const [total, setTotal] = useState(0);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [retryCursor, setRetryCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);

  const loadPage = useCallback(
    async (
      cursor: string | null,
      append: boolean,
      activeView: NotebookView,
      activeSearch: string,
    ) => {
      requestControllerRef.current?.abort();
      const controller = new AbortController();
      requestControllerRef.current = controller;
      const generation = generationRef.current;
      const requestIdentity = identityKey;
      const query = new URLSearchParams();
      query.set("limit", String(PAGE_SIZE));
      if (cursor) query.set("cursor", cursor);
      if (activeView === "open") query.set("kind", "open_question");
      if (activeView === "history") query.set("current", "false");
      if (activeSearch.trim()) query.set("query", activeSearch.trim());
      setLoading(true);
      setError(null);
      try {
        const page = await api.get<NotebookPage>(
          `${tenderPath(tenderId)}/staff/${encodeURIComponent(staffId)}/notebook?${query.toString()}`,
          controller.signal,
        );
        if (
          controller.signal.aborted ||
          generation !== generationRef.current ||
          requestIdentity !== identityKey
        )
          return;
        setItems((current) =>
          append ? [...current, ...(page.items ?? [])] : (page.items ?? []),
        );
        setTotal(page.total ?? 0);
        setNextCursor(page.next_cursor ?? null);
        setRetryCursor(cursor);
      } catch (failure) {
        if (
          controller.signal.aborted ||
          generation !== generationRef.current ||
          requestIdentity !== identityKey
        )
          return;
        setRetryCursor(cursor);
        setError(failure);
      } finally {
        if (
          !controller.signal.aborted &&
          generation === generationRef.current &&
          requestIdentity === identityKey
        ) {
          setLoading(false);
        }
        if (requestControllerRef.current === controller) {
          requestControllerRef.current = null;
        }
      }
    },
    [api, identityKey, staffId, tenderId],
  );

  useEffect(() => {
    generationRef.current += 1;
    requestControllerRef.current?.abort();
    requestControllerRef.current = null;
    setItems([]);
    setTotal(0);
    setNextCursor(null);
    setRetryCursor(null);
    setError(null);
    setLoading(true);
    void loadPage(null, false, view, search);
    return () => {
      generationRef.current += 1;
      requestControllerRef.current?.abort();
      requestControllerRef.current = null;
    };
  }, [identityKey, loadPage, refreshKey, search, view]);

  return (
    <section
      className="office-desk-section"
      aria-labelledby="staff-notebook-title"
    >
      <h3 id="staff-notebook-title">Work notes</h3>
      <p className="office-muted">
        What this colleague wrote down while working, in the order written.
        {total
          ? ` ${total} ${total === 1 ? "note" : "notes"} in this view.`
          : ""}
      </p>
      <div
        className="office-staff-filter"
        role="group"
        aria-label="Work notes view"
      >
        <button
          type="button"
          className={view === "all" ? "button primary" : "button"}
          aria-pressed={view === "all"}
          onClick={() => setView("all")}
        >
          All notes
        </button>
        <button
          type="button"
          className={view === "open" ? "button primary" : "button"}
          aria-pressed={view === "open"}
          onClick={() => setView("open")}
        >
          Open questions
        </button>
        <button
          type="button"
          className={view === "history" ? "button primary" : "button"}
          aria-pressed={view === "history"}
          onClick={() => setView("history")}
        >
          History
        </button>
      </div>
      <form
        className="office-notebook-search"
        onSubmit={(event) => {
          event.preventDefault();
          setSearch(searchDraft);
        }}
      >
        <label htmlFor={`notebook-search-${staffId}`}>
          Search notes
          <input
            id={`notebook-search-${staffId}`}
            type="search"
            value={searchDraft}
            onChange={(event) => setSearchDraft(event.target.value)}
            placeholder="Search this colleague's notes"
          />
        </label>
        <button type="submit" className="button" disabled={loading}>
          Search
        </button>
      </form>
      {error ? (
        <div className="office-inline-error" role="alert">
          <span>{errorText(error)}</span>
          <button
            type="button"
            className="text-button"
            disabled={loading}
            onClick={() =>
              void loadPage(retryCursor, Boolean(retryCursor), view, search)
            }
          >
            Retry work notes
          </button>
        </div>
      ) : null}
      {loading && !items.length ? (
        <p className="office-muted office-inline-loading" role="status">
          Loading work notes…
        </p>
      ) : null}
      {!loading && !error && !items.length ? (
        <p className="office-muted">{emptyCopy(view)}</p>
      ) : null}
      {items.length ? (
        <div className="office-notebook-list">
          {items.map((item) => (
            <NotebookCard
              key={item.id}
              entry={item}
              tenderId={tenderId}
              onSource={onSource}
              onOpenResult={onOpenResult}
              onOpenOutput={onOpenOutput}
            />
          ))}
        </div>
      ) : null}
      {nextCursor ? (
        <button
          type="button"
          className="office-history-button"
          disabled={loading}
          onClick={() => void loadPage(nextCursor, true, view, search)}
        >
          {loading ? "Loading older notes…" : "Load older notes"}
        </button>
      ) : null}
    </section>
  );
}

function NotebookCard({
  entry,
  tenderId,
  onSource,
  onOpenResult,
  onOpenOutput,
}: {
  entry: NotebookEntry;
  tenderId: string;
  onSource: (source: SourceSelection) => void;
  onOpenResult?: (resultId: string) => void;
  onOpenOutput?: (outputId: string) => void;
}) {
  const sourceRefs: string[] = [];
  const resultRefs: string[] = [];
  const outputRefs: string[] = [];
  for (const ref of entry.refs ?? []) {
    const [kind, identifier] = splitRef(ref);
    if (kind === "staff_result") resultRefs.push(identifier);
    else if (kind === "output") outputRefs.push(identifier);
    else sourceRefs.push(identifier);
  }
  return (
    <article className="office-notebook-note">
      <div className="office-row-heading">
        <strong>{kindLabel(entry.kind)}</strong>
        {entry.stale ? (
          <span className="office-ai-label">Needs review</span>
        ) : null}
        {!entry.current ? (
          <span className="office-ai-label">Corrected — history is kept</span>
        ) : null}
      </div>
      <p>{entry.text}</p>
      {sourceRefs.length ? (
        <Citations ids={sourceRefs} tenderId={tenderId} onOpen={onSource} />
      ) : null}
      {resultRefs.map((resultId) => (
        <button
          key={`result-${resultId}`}
          type="button"
          className="text-button"
          onClick={() => onOpenResult?.(resultId)}
        >
          Open saved staff result
        </button>
      ))}
      {outputRefs.map((outputId) => (
        <button
          key={`output-${outputId}`}
          type="button"
          className="text-button"
          onClick={() => onOpenOutput?.(outputId)}
        >
          Open work output
        </button>
      ))}
      <details className="office-more-options">
        <summary>More options</summary>
        <dl className="office-facts-grid">
          <Fact label="Kind" value={entry.kind} />
          <Fact label="Applies to" value={entry.applicability} />
          <Fact
            label="Assignment"
            value={entry.assignment_id ?? "Not assigned"}
          />
          <Fact
            label="Corrects note"
            value={entry.supersedes_id ?? "This is the original note"}
          />
          <Fact label="Source scope" value={entry.source_scope} />
          <Fact
            label="Profile version"
            value={
              entry.profile_version
                ? String(entry.profile_version)
                : "Not recorded"
            }
          />
          <Fact label="Written" value={formatDate(entry.created_at)} />
        </dl>
      </details>
    </article>
  );
}

function splitRef(ref: string): [string, string] {
  const separator = ref.indexOf(":");
  if (separator < 0) return ["source", ref];
  return [ref.slice(0, separator), ref.slice(separator + 1)];
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value || "Not recorded."}</dd>
    </div>
  );
}

function kindLabel(kind: string) {
  if (kind === "finding") return "Finding";
  if (kind === "assumption") return "Assumption";
  if (kind === "open_question") return "Open question";
  if (kind === "failed_approach") return "Failed approach";
  if (kind === "handover") return "Handover";
  if (kind === "preference") return "Preference";
  return kind;
}

function emptyCopy(view: NotebookView) {
  if (view === "open")
    return "No open questions. Nothing needs an answer here.";
  if (view === "history")
    return "No corrected notes. Earlier notes stay here after a correction.";
  return "No work notes recorded for this colleague yet.";
}

function formatDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : date.toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
}
