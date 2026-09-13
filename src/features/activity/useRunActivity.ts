import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { isActive, tenderPath, useApi } from "../../api";
import type { ActivityFilters, RunActivityPage } from "./types";

type Window = RunActivityPage & { windows?: RunActivityPage[] };
/** Keep opaque cursors at page boundaries, including when the opposite end is evicted. */
function merge(
  previous: Window | undefined,
  page: RunActivityPage,
  earlier = false,
): Window {
  if (
    !previous ||
    page.reset_required ||
    previous.history_key !== page.history_key
  )
    return { ...page, windows: [page] };
  let windows = [...(previous.windows ?? [previous])];
  const seen = new Set((previous.items ?? []).map((item) => item.event_id));
  const fresh = (page.items ?? []).filter((item) => !seen.has(item.event_id));
  if (fresh.length) {
    const edge = earlier ? windows[0] : windows.at(-1)!;
    const combined = (edge.items?.length ?? 0) + fresh.length <= 100;
    if (combined) {
      const joined = earlier
        ? {
            ...edge,
            before_cursor: page.before_cursor,
            has_earlier: page.has_earlier,
            items: [...fresh, ...(edge.items ?? [])],
          }
        : {
            ...page,
            before_cursor: edge.before_cursor,
            has_earlier: edge.has_earlier,
            items: [...(edge.items ?? []), ...fresh],
          };
      if (earlier) windows[0] = joined;
      else windows[windows.length - 1] = joined;
    } else if (earlier) windows.unshift({ ...page, items: fresh });
    else windows.push({ ...page, items: fresh });
  }
  const trimmed = windows.length > 3;
  if (trimmed) windows = earlier ? windows.slice(0, 3) : windows.slice(-3);
  const first = windows[0],
    last = windows.at(-1)!;
  const cursor = earlier ? last.cursor : page.cursor;
  return {
    ...page,
    windows,
    items: windows
      .flatMap((window) => window.items ?? [])
      .sort((a, b) => a.event_id - b.event_id),
    cursor,
    before_cursor: first.before_cursor,
    has_more: earlier ? trimmed || previous.has_more : page.has_more,
    has_earlier: earlier ? page.has_earlier : trimmed || previous.has_earlier,
  };
}

/** One observer owns live polling; both timeline surfaces receive this same state. */
export function useRunActivity(
  tenderId: string,
  runId: string,
  filters: ActivityFilters,
  enabled: boolean,
  followLive: boolean,
  onSettled?: () => void,
) {
  const api = useApi();
  const client = useQueryClient();
  const base = `${tenderPath(tenderId)}/runs/${encodeURIComponent(runId)}/activity`;
  const search = new URLSearchParams({ limit: "100" });
  if (filters.q.trim()) search.set("q", filters.q.trim());
  if (filters.actor_id) search.set("actor_id", filters.actor_id);
  if (filters.category) search.set("category", filters.category);
  if (filters.errors_only) search.set("errors_only", "true");
  const path = `${base}?${search}`;
  const key = ["run-activity", path];
  const [earlierBusy, setEarlierBusy] = useState(false);
  const [earlierError, setEarlierError] = useState<unknown>(null);
  const pendingEarlier = useRef<AbortController | null>(null);
  const settled = useRef(false);
  useEffect(() => {
    setEarlierError(null);
    setEarlierBusy(false);
    return () => {
      pendingEarlier.current?.abort();
      pendingEarlier.current = null;
    };
  }, [path]);
  const query = useQuery<Window>({
    queryKey: key,
    enabled,
    retry: false,
    staleTime: 2000,
    gcTime: 60_000,
    queryFn: async ({ signal }) => {
      const previous = client.getQueryData<Window>(key);
      let page = await api.get<RunActivityPage>(
        path +
          (previous?.cursor
            ? `&after=${encodeURIComponent(previous.cursor)}`
            : ""),
        signal,
      );
      if (page.reset_required && !page.items?.length)
        page = await api.get<RunActivityPage>(path, signal);
      return merge(client.getQueryData<Window>(key), page);
    },
    refetchInterval: ({ state }) =>
      !followLive
        ? false
        : state.error
          ? 3000
          : state.data?.has_more
            ? 100
            : isActive(state.data?.run_status ?? "running")
              ? 750
              : false,
    refetchOnWindowFocus: true,
  });
  useEffect(() => {
    if (query.data && !isActive(query.data.run_status) && !settled.current) {
      settled.current = true;
      onSettled?.();
    }
  }, [query.data, onSettled]);
  async function loadEarlier() {
    const previous = client.getQueryData<Window>(key);
    if (!previous?.before_cursor || pendingEarlier.current) return;
    const controller = new AbortController();
    pendingEarlier.current = controller;
    setEarlierBusy(true);
    setEarlierError(null);
    try {
      await client.cancelQueries({ queryKey: key, exact: true });
      if (controller.signal.aborted) return;
      const page = await api.get<RunActivityPage>(
        `${path}&before=${encodeURIComponent(previous.before_cursor)}`,
        controller.signal,
      );
      if (controller.signal.aborted) return;
      client.setQueryData<Window>(key, (current) => {
        if (current && current.history_key !== previous.history_key)
          return current;
        return merge(current, page, true);
      });
    } catch (error) {
      if (!controller.signal.aborted) setEarlierError(error);
    } finally {
      if (pendingEarlier.current === controller) {
        pendingEarlier.current = null;
        setEarlierBusy(false);
      }
    }
  }
  async function jumpToLatest() {
    pendingEarlier.current?.abort();
    pendingEarlier.current = null;
    setEarlierBusy(false);
    await client.resetQueries({
      queryKey: ["run-activity", `${base}?limit=100`],
      exact: true,
    });
  }
  return {
    ...query,
    base,
    loadEarlier,
    jumpToLatest,
    earlierBusy,
    earlierError,
  };
}
