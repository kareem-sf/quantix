import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CircleCheck, Sparkles } from "lucide-react";
import type { operations } from "../bindings/api";
import { tenderPath, useApi, useRefresh, type Schema } from "../api";
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

export type SearchMethod = NonNullable<
  NonNullable<
    operations["search_api_tenders__tender_id__search_get"]["parameters"]["query"]
  >["mode"]
>;

const PREPARING = new Set(["preparing", "updating"]);

export function SearchPreparation({
  tenderId,
  documentKey,
}: {
  tenderId: string;
  activeRuns?: Schema<"Run">[];
  documentKey: string;
  onWork?: () => void;
}) {
  const api = useApi(),
    refresh = useRefresh(),
    client = useQueryClient(),
    base = tenderPath(tenderId);
  const [starting, setStarting] = useState(false),
    [error, setError] = useState<unknown>(null);
  const status = useQuery({
    queryKey: [`${base}/search-status`],
    queryFn: () => api.get<Schema<"SemanticStatus">>(`${base}/search-status`),
    refetchInterval: (query) =>
      query.state.data && PREPARING.has(query.state.data.status) ? 1500 : false,
  });
  useEffect(() => {
    void status.refetch();
  }, [documentKey, status.refetch]);
  useEffect(() => {
    if (status.data?.ready) {
      void client.invalidateQueries({ queryKey: [base, "search"] });
    }
  }, [status.data?.ready, status.data?.published_generation, client, base]);
  const unavailable =
    status.data?.status === "empty" || status.data?.status === "limit_exceeded";
  const preparing = !!status.data && PREPARING.has(status.data.status);

  async function prepare() {
    setStarting(true);
    setError(null);
    try {
      await api.post<Schema<"SemanticStatus">>(`${base}/search-index`);
      await status.refetch();
      await refresh();
    } catch (failure) {
      setError(failure);
    } finally {
      setStarting(false);
    }
  }

  async function stop() {
    setError(null);
    try {
      await api.post<Schema<"SemanticStatus">>(`${base}/search-index/cancel`);
      await status.refetch();
    } catch (failure) {
      setError(failure);
    }
  }

  const title =
    status.data?.status === "empty"
      ? "Add documents before indexing"
      : status.data?.status === "stale"
        ? "The evidence index needs an update"
        : status.data?.status === "limit_exceeded"
          ? "The evidence index cannot cover this package yet"
          : status.data?.status === "failed"
            ? "Meaning search could not be prepared"
            : status.data?.status === "stopped"
              ? "Meaning search preparation was stopped"
              : preparing
                ? "Preparing meaning search"
                : "Tender evidence is not indexed yet";

  return (
    <div className="flex flex-col gap-3">
      <ErrorNotice error={status.error || error} />
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
            <ItemTitle>{title}</ItemTitle>
            <ItemDescription>{status.data.detail}</ItemDescription>
          </ItemContent>
          <ItemActions>
            {preparing ? (
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => void stop()}
              >
                Stop preparation
              </Button>
            ) : (
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={starting || unavailable}
                onClick={() => void prepare()}
              >
                {starting ? "Starting…" : "Index tender evidence"}
              </Button>
            )}
          </ItemActions>
        </Item>
      ) : null}
    </div>
  );
}

export function searchHits(
  data: Schema<"RetrievalResponse"> | undefined,
): Schema<"RetrievalHit">[] {
  return data?.hits ?? [];
}

export function searchExcerpt(
  hit: Schema<"RetrievalHit"> | Schema<"Evidence">,
) {
  const match = hit.metadata?.semantic_match;
  return typeof match === "object" &&
    match !== null &&
    "text" in match &&
    typeof match.text === "string"
    ? match.text.slice(0, 480)
    : hit.text.slice(0, 480);
}
