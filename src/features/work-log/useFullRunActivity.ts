import { useQuery, useQueryClient } from "@tanstack/react-query";
import { isActive, tenderPath, useApi } from "../../api";
import type { RunActivity, RunActivityPage } from "../activity/types";

type Full = {
  items: RunActivity[];
  cursor: string | null;
  run_status: string;
  run_updated_at: string;
  history_key: string;
};

const PAGE = 200;
// A long job can record a few thousand events; stop well before memory matters.
const MAX_ITEMS = 20_000;

/**
 * Every event of one run, oldest first. The shared activity hook keeps only a
 * recent window for the technical log, which dropped the start of long jobs
 * from the work log. The first page returns the newest events, so this pages
 * back to the start once, then follows new events forward.
 */
export function useFullRunActivity(
  tenderId: string,
  runId: string,
  enabled = true,
) {
  const api = useApi();
  const client = useQueryClient();
  const base = `${tenderPath(tenderId)}/runs/${encodeURIComponent(runId)}/activity?limit=${PAGE}`;
  const key = ["run-activity-full", tenderId, runId];
  return useQuery<Full>({
    queryKey: key,
    enabled,
    retry: false,
    refetchInterval: ({ state }) =>
      state.error
        ? 3000
        : isActive(state.data?.run_status ?? "running")
          ? 1000
          : false,
    queryFn: async ({ signal }) => {
      const cached = client.getQueryData<Full>(key);
      if (cached?.cursor) {
        let current = cached;
        for (;;) {
          const page = await api.get<RunActivityPage>(
            `${base}&after=${encodeURIComponent(current.cursor!)}`,
            signal,
          );
          if (page.reset_required || page.history_key !== current.history_key)
            break;
          const seen = new Set(current.items.map((item) => item.event_id));
          current = {
            items: [
              ...current.items,
              ...(page.items ?? []).filter((item) => !seen.has(item.event_id)),
            ].slice(-MAX_ITEMS),
            cursor: page.cursor ?? current.cursor,
            run_status: page.run_status,
            run_updated_at: page.run_updated_at,
            history_key: page.history_key,
          };
          if (!page.has_more) return current;
        }
      }
      const latest = await api.get<RunActivityPage>(base, signal);
      let items = latest.items ?? [];
      let before = latest.before_cursor;
      let earlier = latest.has_earlier;
      while (earlier && before && items.length < MAX_ITEMS) {
        const page = await api.get<RunActivityPage>(
          `${base}&before=${encodeURIComponent(before)}`,
          signal,
        );
        const seen = new Set(items.map((item) => item.event_id));
        items = [
          ...(page.items ?? []).filter((item) => !seen.has(item.event_id)),
          ...items,
        ];
        before = page.before_cursor;
        earlier = page.has_earlier && Boolean(page.items?.length);
      }
      return {
        items,
        cursor: latest.cursor ?? null,
        run_status: latest.run_status,
        run_updated_at: latest.run_updated_at,
        history_key: latest.history_key,
      };
    },
  });
}
