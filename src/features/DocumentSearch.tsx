import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CircleCheck, Sparkles } from "lucide-react";
import type { operations } from "../bindings/api";
import { isActive, tenderPath, useApi, useRefresh, type Schema } from "../api";
import { ErrorNotice, Loading } from "../components/common";
import { Button } from "@/components/ui/button";
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemMedia,
  ItemTitle,
} from "@/components/ui/item";
import { RunRow } from "./RunRow";

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

  async function prepare() {
    setStarting(true);
    setError(null);
    try {
      const job = await api.post<Schema<"Run">>(`${base}/search-index`);
      setRunId(job.id);
      await refresh();
    } catch (failure) {
      setError(failure);
    } finally {
      setStarting(false);
    }
  }

  return (
    <div className="flex flex-col gap-3">
      <ErrorNotice error={status.error || started.error || error} />
      {status.isPending ? (
        <Loading>Checking the evidence index…</Loading>
      ) : null}
      {status.data?.ready ? (
        <p className="flex items-center gap-2 text-xs text-muted-foreground">
          <CircleCheck
            className="size-3.5 text-emerald-600 dark:text-emerald-400"
            aria-hidden="true"
          />
          Tender evidence is indexed for meaning search.
        </p>
      ) : status.data ? (
        <Item variant="outline" className="bg-card">
          <ItemMedia variant="icon" className="size-9 rounded-lg bg-muted">
            <Sparkles />
          </ItemMedia>
          <ItemContent>
            <ItemTitle>
              {status.data.status === "empty"
                ? "Add documents before indexing"
                : status.data.status === "stale"
                  ? "The evidence index needs an update"
                  : status.data.status === "limit_exceeded"
                    ? "The evidence index cannot cover this package yet"
                    : "Tender evidence is not indexed yet"}
            </ItemTitle>
            <ItemDescription>{status.data.detail}</ItemDescription>
          </ItemContent>
          <ItemActions>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={
                starting ||
                !!active ||
                (!!run && isActive(run.status)) ||
                unavailable
              }
              onClick={() => void prepare()}
            >
              {starting ? "Starting…" : "Index tender evidence"}
            </Button>
          </ItemActions>
        </Item>
      ) : null}
      {run ? <RunRow run={run} compact /> : null}
      {run && onWork ? (
        <Button
          type="button"
          variant="link"
          size="sm"
          className="h-auto self-start p-0"
          onClick={onWork}
        >
          View progress in Work
        </Button>
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
