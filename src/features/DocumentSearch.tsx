import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { operations } from "../bindings/api";
import { isActive, tenderPath, useApi, useRefresh, type Schema } from "../api";
import { ErrorNotice, Loading } from "../components/ui";
import { RunRow } from "./Work";

export type SearchMethod = NonNullable<
  NonNullable<
    operations["search_api_tenders__tender_id__search_get"]["parameters"]["query"]
  >["mode"]
>;

export function SearchPreparation({
  tenderId,
  activeRuns,
  documentKey,
  onWork,
}: {
  tenderId: string;
  activeRuns: Schema<"Run">[];
  documentKey: string;
  onWork?: () => void;
}) {
  const api = useApi(),
    refresh = useRefresh(),
    client = useQueryClient(),
    base = tenderPath(tenderId);
  const [starting, setStarting] = useState(false),
    [error, setError] = useState<unknown>(null),
    [runId, setRunId] = useState<string | null>(null);
  const active = activeRuns.find((run) => run.kind === "index");
  const status = useQuery({
    queryKey: [`${base}/search-status`],
    queryFn: () => api.get<Schema<"SemanticStatus">>(`${base}/search-status`),
  });
  const started = useQuery({
    queryKey: ["search-preparation", runId],
    queryFn: () => api.get<Schema<"Run">>(`/runs/${runId}`),
    enabled: !!runId,
    refetchInterval: (query) =>
      query.state.data && isActive(query.state.data.status) ? 1500 : false,
  });
  const run = active ?? started.data;
  useEffect(() => {
    void status.refetch();
  }, [documentKey, active?.id, status.refetch]);
  useEffect(() => {
    if (started.data && !isActive(started.data.status)) {
      void status.refetch();
      void client.invalidateQueries({ queryKey: [base, "search"] });
    }
  }, [started.data?.status, status.refetch, client, base]);
  const unavailable =
    status.data?.status === "empty" || status.data?.status === "limit_exceeded";
  return (
    <div className="search-preparation">
      <ErrorNotice error={status.error || started.error || error} />
      {status.isPending ? <Loading>Checking meaning search…</Loading> : null}
      {status.data?.ready ? (
        <p className="field-help">
          Meaning search is ready for the current documents.
        </p>
      ) : status.data ? (
        <div className="search-readiness">
          <div>
            <strong>
              {status.data.status === "empty"
                ? "Add documents before preparing search"
                : status.data.status === "stale"
                  ? "Meaning search needs an update"
                  : status.data.status === "limit_exceeded"
                    ? "Meaning search cannot cover this package yet"
                    : "Meaning search is not prepared"}
            </strong>
            <p>{status.data.detail}</p>
          </div>
          <button
            type="button"
            className="button"
            disabled={
              starting ||
              !!active ||
              (!!run && isActive(run.status)) ||
              unavailable
            }
            onClick={async () => {
              setStarting(true);
              setError(null);
              try {
                const job = await api.post<Schema<"Run">>(
                  `${base}/search-index`,
                );
                setRunId(job.id);
                await refresh();
              } catch (failure) {
                setError(failure);
              } finally {
                setStarting(false);
              }
            }}
          >
            {starting ? "Starting…" : "Prepare search"}
          </button>
        </div>
      ) : null}
      {run ? <RunRow run={run} compact /> : null}
      {run && onWork ? (
        <button type="button" className="text-button" onClick={onWork}>
          View progress in Work
        </button>
      ) : null}
    </div>
  );
}

export function searchExcerpt(hit: Schema<"Evidence">) {
  const match = hit.metadata?.semantic_match;
  return typeof match === "object" &&
    match !== null &&
    "text" in match &&
    typeof match.text === "string"
    ? match.text.slice(0, 480)
    : hit.text.slice(0, 480);
}
