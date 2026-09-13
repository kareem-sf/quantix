import { useCallback, useEffect, useRef, useState } from "react";
import { useApi } from "../api";

export function useOffsetList<T extends { id: string }>({
  path,
  refreshKey,
  limit = 50,
}: {
  path: string;
  refreshKey: string;
  limit?: number;
}) {
  const api = useApi();
  const lastPath = useRef(path);
  const identity = `${path}:${refreshKey}:${limit}`;
  const request = useRef({ identity, generation: 0 });
  if (request.current.identity !== identity) {
    request.current = {
      identity,
      generation: request.current.generation + 1,
    };
  }
  const [items, setItems] = useState<T[]>([]);
  const [nextOffset, setNextOffset] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);

  const read = useCallback(
    async (offset: number, generation: number, signal?: AbortSignal) => {
      const separator = path.includes("?") ? "&" : "?";
      const page = await api.get<T[]>(
        `${path}${separator}offset=${offset}&limit=${limit}`,
        signal,
      );
      if (generation !== request.current.generation) return;
      setItems((current) => (offset === 0 ? page : mergeById(current, page)));
      setNextOffset(page.length === limit ? offset + page.length : null);
    },
    [api, limit, path],
  );

  const reload = useCallback(async () => {
    const generation = ++request.current.generation;
    setLoading(true);
    setError(null);
    try {
      await read(0, generation);
    } catch (failure) {
      if (generation === request.current.generation) setError(failure);
    } finally {
      if (generation === request.current.generation) setLoading(false);
    }
  }, [read]);

  useEffect(() => {
    const controller = new AbortController();
    const generation = request.current.generation;
    if (lastPath.current !== path) {
      setItems([]);
      setNextOffset(null);
      lastPath.current = path;
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
  }, [identity, read]);

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

  const prepend = useCallback((item: T, newest = false) => {
    setItems((current) => {
      const exists = current.some((value) => value.id === item.id);
      if (newest && !exists)
        setNextOffset((offset) => (offset == null ? null : offset + 1));
      return [item, ...current.filter((value) => value.id !== item.id)];
    });
  }, []);

  return { items, nextOffset, loading, error, loadMore, reload, prepend };
}

function mergeById<T extends { id: string }>(current: T[], additions: T[]) {
  const values = new Map<string, T>();
  for (const item of [...current, ...additions]) values.set(item.id, item);
  return [...values.values()];
}
