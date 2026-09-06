import { useDeferredValue, useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { File, Search, Upload } from "lucide-react";
import { tenderPath, useApi, type Schema } from "../api";
import {
  Empty,
  ErrorNotice,
  Loading,
  Status,
  statusLabel,
} from "../components/ui";
import type { SourceSelection } from "./Sources";
import {
  SearchPreparation,
  searchExcerpt,
  type SearchMethod,
} from "./DocumentSearch";

export function Files({
  tenderId,
  artifacts,
  onImport,
  onSource,
  meaningAvailable = false,
  activeRuns = [],
  onWork,
}: {
  tenderId: string;
  artifacts: Schema<"Artifact">[];
  onImport: () => void;
  onSource: (source: SourceSelection) => void;
  meaningAvailable?: boolean;
  activeRuns?: Schema<"Run">[];
  onWork?: () => void;
}) {
  const api = useApi();
  const [query, setQuery] = useState(""),
    [status, setStatus] = useState("all"),
    [area, setArea] = useState("all"),
    [revision, setRevision] = useState("current");
  const [mode, setMode] = useState<SearchMethod>("words");
  const deferred = useDeferredValue(query.trim());
  const history = useQuery({
    queryKey: [`${tenderPath(tenderId)}/artifacts?include_history=true`],
    queryFn: () =>
      api.get<Schema<"Artifact">[]>(
        `${tenderPath(tenderId)}/artifacts?include_history=true`,
      ),
    enabled: revision === "all",
  });
  const registered = revision === "all" ? (history.data ?? []) : artifacts;
  const documentKey = artifacts
    .map((file) => `${file.id}:${file.status}`)
    .join("|");
  useEffect(() => {
    if (revision === "all") void history.refetch();
  }, [revision, documentKey, history.refetch]);
  const searchParams = new URLSearchParams({ q: deferred, mode });
  if (area !== "all") searchParams.set("area", area);
  if (status !== "all") searchParams.set("status", status);
  const search = useQuery({
    queryKey: [tenderPath(tenderId), "search", deferred, mode, area, status],
    queryFn: () =>
      api.get<Schema<"Evidence">[]>(
        `${tenderPath(tenderId)}/search?${searchParams}`,
      ),
    enabled: !!deferred,
    retry: false,
  });
  const visible = registered.filter(
    (file) =>
      (status === "all" || file.status === status) &&
      (area === "all" || file.area === area) &&
      (revision === "all" || file.is_current),
  );
  const visibleIds = new Set(
    artifacts
      .filter(
        (file) =>
          (status === "all" || file.status === status) &&
          (area === "all" || file.area === area),
      )
      .map((file) => file.id),
  );
  const matchingSources = search.data?.filter((hit) =>
    visibleIds.has(hit.artifact_id),
  );
  return (
    <div className="feature-page">
      <div className="section-heading">
        <div>
          <h2>Files</h2>
          <p className="muted">
            Original documents and the passages used in your tender.
          </p>
        </div>
        <button className="button" onClick={onImport}>
          <Upload size={18} />
          Import package
        </button>
      </div>
      <div className="file-filters">
        <label className="search-field">
          <Search size={18} />
          <input
            aria-label="Search source text"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search source text…"
          />
        </label>
        {meaningAvailable ? (
          <select
            aria-label="Search method"
            value={mode}
            onChange={(event) => setMode(event.target.value as SearchMethod)}
          >
            <option value="words">Words</option>
            <option value="meaning">Meaning</option>
            <option value="combined">Both</option>
          </select>
        ) : null}
        <select
          aria-label="Filter by processing status"
          value={status}
          onChange={(event) => setStatus(event.target.value)}
        >
          <option value="all">All statuses</option>
          {Array.from(new Set(registered.map((file) => file.status)))
            .sort()
            .map((value) => (
              <option key={value} value={value}>
                {statusLabel(value)}
              </option>
            ))}
        </select>
        <select
          aria-label="Filter by area"
          value={area}
          onChange={(event) => setArea(event.target.value)}
        >
          <option value="all">All areas</option>
          {Array.from(new Set(registered.map((file) => file.area)))
            .filter(Boolean)
            .sort()
            .map((value) => (
              <option key={value}>{value}</option>
            ))}
        </select>
        <select
          aria-label="Filter by revision"
          value={revision}
          onChange={(event) => setRevision(event.target.value)}
        >
          <option value="current">Current files</option>
          <option value="all">All revisions</option>
        </select>
      </div>
      {revision === "all" ? (
        <p className="field-help history-note">
          Search uses current file versions. Earlier versions can be opened from
          the register.
        </p>
      ) : null}
      {meaningAvailable ? (
        <SearchPreparation
          tenderId={tenderId}
          activeRuns={activeRuns}
          documentKey={documentKey}
          onWork={onWork}
        />
      ) : null}
      {revision === "all" && history.isPending ? (
        <Loading>Loading earlier file versions…</Loading>
      ) : null}
      <ErrorNotice error={revision === "all" ? history.error : null} />
      {deferred ? (
        <section className="search-results">
          <h3>Source matches</h3>
          {search.isPending ? <Loading>Searching documents…</Loading> : null}
          <ErrorNotice error={search.error} />
          {matchingSources?.length === 0 ? (
            <p className="muted">
              No source matches these search and document filters.
            </p>
          ) : null}
          {matchingSources?.map((hit) => (
            <button
              className="search-result"
              key={hit.id}
              onClick={() => onSource({ sourceId: hit.id })}
            >
              <strong>{hit.artifact_name}</strong>
              <span className="source-location">{hit.locator}</span>
              <p>{searchExcerpt(hit)}</p>
            </button>
          ))}
        </section>
      ) : registered.length ? (
        <div className="document-list">
          {visible.map((file) => (
            <article className="document-row" key={file.id}>
              <File size={26} />
              <div className="document-content">
                <button
                  className="file-title"
                  onClick={() =>
                    onSource({ artifactId: file.id, artifact: file })
                  }
                >
                  {file.name}
                </button>
                <p className="file-path">{file.relative_path}</p>
                <div className="file-meta">
                  <span>Revision {file.version}</span>
                  {file.area ? <span>{file.area}</span> : null}
                  <span>{formatSize(file.size)}</span>
                  {!file.is_current ? <span>Previous revision</span> : null}
                </div>
                {file.warnings?.length ? (
                  <details className="document-warnings">
                    <summary>
                      {file.warnings.length} reading{" "}
                      {file.warnings.length === 1 ? "issue" : "issues"}
                    </summary>
                    {file.warnings.map((warning, index) => (
                      <p key={index}>
                        {String(
                          warning.message ??
                            warning.code ??
                            "Document requires attention",
                        )}
                        {warning.locator ? ` · ${String(warning.locator)}` : ""}
                      </p>
                    ))}
                  </details>
                ) : null}
              </div>
              <Status value={file.status} />
            </article>
          ))}
          {visible.length === 0 ? (
            <p className="muted">No documents match these filters.</p>
          ) : null}
        </div>
      ) : revision === "all" && (history.isPending || history.error) ? null : (
        <Empty
          title="Add the tender documents"
          action={
            <button className="button primary" onClick={onImport}>
              Import package
            </button>
          }
        >
          Import a folder or ZIP file to build the document register and inspect
          the source evidence.
        </Empty>
      )}
    </div>
  );
}
function formatSize(bytes: number) {
  return bytes >= 1_048_576
    ? `${(bytes / 1_048_576).toFixed(1)} MB`
    : `${Math.max(1, Math.round(bytes / 1024))} KB`;
}
