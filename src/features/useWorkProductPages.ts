import { useCallback, useEffect, useRef, useState } from "react";
import { useApi, type Schema } from "../api";

type Summary = Schema<"WorkProductVersionSummary">;

export function useWorkProductPages(
  base: string,
  revision: number | string = 0,
) {
  const api = useApi();
  const [retryIndex, setRetryIndex] = useState(0);
  const request = useRef({ base, revision, retryIndex, generation: 0 });
  const displayedBase = useRef(base);
  if (
    request.current.base !== base ||
    request.current.revision !== revision ||
    request.current.retryIndex !== retryIndex
  ) {
    request.current = {
      base,
      revision,
      retryIndex,
      generation: request.current.generation + 1,
    };
  }
  const [items, setItems] = useState<Summary[]>([]);
  const [nextOffset, setNextOffset] = useState<number | null>(null);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);

  const read = useCallback(
    async (offset: number, generation: number, signal?: AbortSignal) => {
      const page = await api.get<Schema<"WorkProductPage">>(
        `${base}?offset=${offset}&limit=50`,
        signal,
      );
      if (generation !== request.current.generation) return;
      setItems((current) =>
        offset === 0
          ? (page.items ?? [])
          : mergeVersions(current, page.items ?? []),
      );
      setNextOffset(page.next_offset ?? null);
      setTotal(page.total);
    },
    [api, base],
  );

  useEffect(() => {
    const controller = new AbortController();
    const generation = request.current.generation;
    if (displayedBase.current !== base) {
      setItems([]);
      setNextOffset(null);
      setTotal(0);
      displayedBase.current = base;
    }
    setLoading(true);
    setError(null);
    void read(0, generation, controller.signal)
      .catch((failure) => {
        if (
          generation === request.current.generation &&
          !(failure instanceof DOMException && failure.name === "AbortError")
        )
          setError(failure);
      })
      .finally(() => {
        if (
          generation === request.current.generation &&
          !controller.signal.aborted
        )
          setLoading(false);
      });
    return () => controller.abort();
  }, [read, base, revision, retryIndex]);

  async function loadMore() {
    if (nextOffset == null || loading) return;
    const generation = request.current.generation;
    setLoading(true);
    setError(null);
    try {
      await read(nextOffset, generation);
    } catch (failure) {
      if (generation === request.current.generation) setError(failure);
    } finally {
      if (generation === request.current.generation) setLoading(false);
    }
  }

  function refresh() {
    if (!loading) setRetryIndex((current) => current + 1);
  }

  return { items, nextOffset, total, loading, error, loadMore, refresh };
}

function mergeVersions(current: Summary[], additions: Summary[]) {
  const values = new Map(current.map((item) => [item.id, item]));
  for (const item of additions) values.set(item.id, item);
  return [...values.values()];
}
