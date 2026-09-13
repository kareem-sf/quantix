import { useCallback, useEffect, useRef, useState } from "react";
import { useApi, type Schema } from "../api";

export function useWorkProductRows(base: string) {
  const api = useApi();
  const [retryIndex, setRetryIndex] = useState(0);
  const request = useRef({ base, retryIndex, generation: 0 });
  if (
    request.current.base !== base ||
    request.current.retryIndex !== retryIndex
  ) {
    request.current = {
      base,
      retryIndex,
      generation: request.current.generation + 1,
    };
  }
  const [items, setItems] = useState<Record<string, unknown>[]>([]);
  const [total, setTotal] = useState(0);
  const [missing, setMissing] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);

  const read = useCallback(
    async (offset: number, generation: number, signal?: AbortSignal) => {
      const page = await api.get<Schema<"WorkProductRowPage">>(
        `${base}?offset=${offset}&limit=50`,
        signal,
      );
      if (generation !== request.current.generation) return;
      setItems((current) =>
        offset === 0 ? (page.items ?? []) : [...current, ...(page.items ?? [])],
      );
      setTotal(page.total);
      setMissing(page.missing);
    },
    [api, base],
  );

  useEffect(() => {
    const controller = new AbortController();
    const generation = request.current.generation;
    setItems([]);
    setTotal(0);
    setMissing(0);
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
  }, [read, retryIndex]);

  async function loadMore() {
    if (items.length >= total || loading) return;
    const generation = request.current.generation;
    setLoading(true);
    setError(null);
    try {
      await read(items.length, generation);
    } catch (failure) {
      if (generation === request.current.generation) setError(failure);
    } finally {
      if (generation === request.current.generation) setLoading(false);
    }
  }

  function refresh() {
    if (!loading) setRetryIndex((current) => current + 1);
  }

  return { items, total, missing, loading, error, loadMore, refresh };
}
