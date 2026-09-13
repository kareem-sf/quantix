import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, Circle, Search } from "lucide-react";
import { isActive } from "../api";
import { ErrorNotice, Loading, Modal } from "../components/common";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { ActivityRows } from "./activity/ActivityRows";
import { ActivityDetail } from "./activity/ActivityDetail";
import { emptyFilters, type RunActivity } from "./activity/types";
import { useRunActivity } from "./activity/useRunActivity";
import type { SourceSelection } from "./Sources";

const noActivity: RunActivity[] = [];

/** Monitoring observes existing work; the Manager composer owns work admission. */
export function LiveRunStream({
  tenderId,
  runId,
  status,
  startedAt,
  onSettled,
  onSource,
  onRunDetails,
}: {
  tenderId: string;
  runId: string;
  status?: string;
  startedAt?: string;
  onSettled?: () => void;
  onSource?: (source: SourceSelection) => void;
  onRunDetails?: () => void;
}) {
  const [expanded, setExpanded] = useState(true);
  const [inspecting, setInspecting] = useState(false);
  const [selected, setSelected] = useState<RunActivity | null>(null);
  const [filters, setFilters] = useState(emptyFilters);
  const [searchText, setSearchText] = useState("");
  const [visible, setVisible] = useState(
    () => typeof IntersectionObserver === "undefined",
  );
  const section = useRef<HTMLElement>(null);
  const feed = useRef<HTMLDivElement>(null);
  const inlineFeed = useRef<HTMLDivElement>(null);
  const inlineAtLive = useRef(true);
  const [inlineFollowing, setInlineFollowing] = useState(true);
  const atLive = useRef(true);
  const [following, setFollowing] = useState(true);
  const [now, setNow] = useState(Date.now());
  const currentHistory = useRef<string | null>(null);
  const [knownActors, setKnownActors] = useState<Map<string, string>>(
    new Map(),
  );
  useEffect(() => {
    const timer = setTimeout(
      () => setFilters((current) => ({ ...current, q: searchText })),
      250,
    );
    return () => clearTimeout(timer);
  }, [searchText]);
  useEffect(() => {
    if (typeof IntersectionObserver === "undefined" || !section.current) return;
    const observer = new IntersectionObserver(
      ([entry]) => setVisible(entry.isIntersecting),
      { rootMargin: "200px" },
    );
    observer.observe(section.current);
    return () => observer.disconnect();
  }, []);
  const activity = useRunActivity(
    tenderId,
    runId,
    filters,
    visible || inspecting || isActive(status ?? ""),
    following,
    onSettled,
  );
  const page = activity.data;
  useEffect(() => {
    if (
      page?.history_key &&
      currentHistory.current &&
      currentHistory.current !== page.history_key
    ) {
      setSelected(null);
      setKnownActors(new Map());
    }
    if (page?.history_key) currentHistory.current = page.history_key;
  }, [page?.history_key]);
  const active = isActive(page?.run_status ?? status ?? "");
  useEffect(() => {
    if (!active || (!visible && !inspecting)) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [active, visible, inspecting]);
  useLayoutEffect(() => {
    if (atLive.current && feed.current)
      feed.current.scrollTop = feed.current.scrollHeight;
    if (inlineAtLive.current && inlineFeed.current)
      inlineFeed.current.scrollTop = inlineFeed.current.scrollHeight;
  }, [page?.items, inspecting]);
  const items = page?.items ?? noActivity;
  const actors = useMemo(
    () => [
      ...new Map(
        items.map((item) => [item.actor_id ?? "", item.actor_label]),
      ).entries(),
    ],
    [items],
  );
  useEffect(() => {
    setKnownActors((current) => {
      if (actors.every(([id, label]) => current.get(id) === label))
        return current;
      return new Map([...current, ...actors]);
    });
  }, [actors]);
  const providers = useMemo(
    () => [
      ...new Set(
        items
          .filter((item) => item.provider)
          .map(
            (item) => `${item.provider}${item.model ? ` · ${item.model}` : ""}`,
          ),
      ),
    ],
    [items],
  );
  const inlineItems = useMemo(() => {
    const groups = new Map<string, RunActivity>();
    for (const item of items) {
      const key = item.operation_id
        ? `${item.operation_id}:${item.category}`
        : `event:${item.event_id}`;
      const previous = groups.get(key);
      if (previous?.phase === "delta" && item.phase === "delta") {
        const preview = previous.preview + item.preview;
        groups.set(key, {
          ...item,
          preview: preview.slice(0, 2000),
          preview_truncated:
            preview.length > 2000 ||
            previous.preview_truncated ||
            item.preview_truncated,
        });
      } else groups.set(key, item);
    }
    return [...groups.values()].sort((a, b) => a.event_id - b.event_id);
  }, [items]);
  function closeInspector() {
    const returnToLatest =
      !following ||
      filters.q ||
      filters.actor_id ||
      filters.category ||
      filters.errors_only;
    setInspecting(false);
    setSelected(null);
    setSearchText("");
    setFilters(emptyFilters);
    atLive.current = true;
    setFollowing(true);
    if (returnToLatest) void activity.jumpToLatest();
  }
  function jumpToLive() {
    setSearchText("");
    setFilters(emptyFilters);
    atLive.current = true;
    inlineAtLive.current = true;
    setInlineFollowing(true);
    setFollowing(true);
    if (feed.current) feed.current.scrollTop = feed.current.scrollHeight;
    void activity.jumpToLatest();
  }
  const lastUpdate = page?.run_updated_at;
  const firstTime = startedAt || items[0]?.created_at;
  const elapsed = firstTime
    ? Math.max(
        0,
        Math.floor(
          ((active ? now : Date.parse(lastUpdate ?? firstTime)) -
            Date.parse(firstTime)) /
            1000,
        ),
      )
    : null;
  const connection = activity.error
    ? "Updates paused"
    : active && !following
      ? "Viewing earlier activity · updates paused"
      : active && !following
        ? "Reading saved activity · live work continues"
        : active
          ? "Connected · checking live work"
          : "Saved history";
  const filtered =
    filters.q || filters.actor_id || filters.category || filters.errors_only;
  const history = (
    <>
      <div className="grid gap-2 sm:grid-cols-2">
        <label className="relative sm:col-span-2">
          <Search
            className="absolute start-3 top-3 size-4 text-muted-foreground"
            aria-hidden="true"
          />
          <Input
            type="search"
            aria-label="Search activity"
            placeholder="Search all captured activity"
            className="ps-9"
            value={searchText}
            onChange={(event) => setSearchText(event.target.value)}
          />
        </label>
        <label className="space-y-1 text-xs text-muted-foreground">
          Person
          <select
            aria-label="Filter by person"
            className="h-9 w-full rounded-md border bg-background px-2 text-sm text-foreground"
            value={filters.actor_id}
            onChange={(event) =>
              setFilters((current) => ({
                ...current,
                actor_id: event.target.value,
              }))
            }
          >
            <option value="">Everyone</option>
            {[...knownActors]
              .filter(([id]) => id)
              .map(([id, label]) => (
                <option key={id} value={id}>
                  {label}
                </option>
              ))}
          </select>
        </label>
        <label className="space-y-1 text-xs text-muted-foreground">
          Activity type
          <select
            aria-label="Filter by category"
            className="h-9 w-full rounded-md border bg-background px-2 text-sm text-foreground"
            value={filters.category}
            onChange={(event) =>
              setFilters((current) => ({
                ...current,
                category: event.target.value,
              }))
            }
          >
            <option value="">All activity</option>
            {[
              ...new Set([
                "tool",
                "source",
                "assignment",
                "model_request",
                "draft",
                "reasoning_summary",
                "capability",
                "model_output",
                "error",
                ...items.map((item) => item.category),
              ]),
            ].map((category) => (
              <option key={category} value={category}>
                {category.replaceAll("_", " ")}
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={filters.errors_only}
            onChange={(event) =>
              setFilters((current) => ({
                ...current,
                errors_only: event.target.checked,
              }))
            }
          />
          Errors only
        </label>
        <Button variant="ghost" size="sm" onClick={jumpToLive}>
          {active ? "Jump to live" : "Jump to latest"}
        </Button>
      </div>
      <div className="rounded-md bg-muted/30 p-3 text-xs text-muted-foreground">
        <p>
          People in this view:{" "}
          {actors.map(([, label]) => label).join(", ") ||
            "No person reported yet"}
        </p>
        <p className="mt-1 wrap-anywhere">
          AI reported in this view:{" "}
          {providers.join("; ") || "No provider reported yet"}
        </p>
      </div>
      <ErrorNotice error={activity.error || activity.earlierError} />
      {activity.error ? (
        <Button
          variant="outline"
          size="sm"
          onClick={() => void activity.refetch()}
        >
          Reconnect updates
        </Button>
      ) : null}
      <div
        ref={feed}
        className="max-h-[55dvh] min-h-36 overflow-y-auto overscroll-contain"
        aria-label="Chronological activity"
        onScroll={() => {
          const node = feed.current;
          if (node) {
            atLive.current =
              node.scrollHeight - node.scrollTop - node.clientHeight < 60;
            setFollowing(atLive.current);
          }
        }}
      >
        {page?.has_earlier ? (
          <Button
            variant="ghost"
            size="sm"
            disabled={activity.earlierBusy}
            onClick={async () => {
              atLive.current = false;
              setFollowing(false);
              const node = feed.current;
              const previousHeight = node?.scrollHeight ?? 0;
              const previousTop = node?.scrollTop ?? 0;
              await activity.loadEarlier();
              requestAnimationFrame(() => {
                if (node)
                  node.scrollTop =
                    previousTop + node.scrollHeight - previousHeight;
              });
            }}
          >
            {activity.earlierBusy
              ? "Loading earlier activity…"
              : "Load earlier activity"}
          </Button>
        ) : null}
        {activity.isPending ? (
          <Loading>Loading activity…</Loading>
        ) : (
          <ActivityRows items={items} onSelect={setSelected} />
        )}
        {page?.has_more ? (
          <Button
            variant="ghost"
            size="sm"
            disabled={activity.isFetching}
            onClick={() => void activity.refetch()}
          >
            Load later activity
          </Button>
        ) : null}
        {!activity.isPending && !activity.error && !items.length ? (
          <p className="p-4 text-sm text-muted-foreground">
            {filtered
              ? "No captured activity matches these filters."
              : "No activity has been captured for this work yet."}
          </p>
        ) : null}
      </div>
      {onRunDetails ? (
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            closeInspector();
            onRunDetails();
          }}
        >
          Open code and native work history
        </Button>
      ) : null}
      {!following && active ? (
        <p className="text-xs text-muted-foreground">
          Reading earlier activity. New work continues below.
        </p>
      ) : null}
    </>
  );
  return (
    <section
      ref={section}
      aria-label="Work activity"
      className="min-w-0 rounded-lg border border-border/60 bg-muted/10"
    >
      <div className="flex flex-wrap items-center justify-between gap-2 px-3 py-2">
        <Button
          variant="ghost"
          size="sm"
          className="px-0"
          aria-expanded={expanded}
          onClick={() => setExpanded((value) => !value)}
        >
          <ChevronDown
            className={`size-4 transition-transform ${expanded ? "" : "-rotate-90"}`}
          />
          {active ? "Work in progress" : "Work activity"}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            setInspecting(true);
            if (activity.isStale) void activity.refetch();
          }}
        >
          Inspect activity
        </Button>
      </div>
      {expanded ? (
        <div className="space-y-2 border-t px-3 py-3">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
            <span className="flex items-center gap-1.5">
              <Circle
                aria-hidden="true"
                className={`size-2 fill-current ${active && !activity.error ? "text-emerald-600 dark:text-emerald-400" : ""}`}
              />
              {connection}
            </span>
            {elapsed != null ? (
              <span>
                {elapsed < 60
                  ? `${elapsed}s`
                  : `${Math.floor(elapsed / 60)}m ${elapsed % 60}s`}{" "}
                {startedAt ? "elapsed" : "elapsed in loaded history"}
              </span>
            ) : null}
            {lastUpdate ? (
              <time
                dateTime={lastUpdate}
                title={new Date(lastUpdate).toLocaleString()}
              >
                Updated {new Date(lastUpdate).toLocaleTimeString()}
              </time>
            ) : null}
          </div>
          {page?.run_detail ? (
            <p className="text-xs text-muted-foreground wrap-anywhere">
              {page.run_detail}
            </p>
          ) : null}
          <ErrorNotice error={activity.error} />
          {activity.error ? (
            <Button
              variant="outline"
              size="sm"
              onClick={() => void activity.refetch()}
            >
              Reconnect updates
            </Button>
          ) : null}
          {activity.isPending ? (
            <Loading>Loading activity…</Loading>
          ) : (
            <div
              ref={inlineFeed}
              className="max-h-[55dvh] overflow-y-auto overscroll-contain"
              aria-label="Inline activity timeline"
              onScroll={() => {
                const node = inlineFeed.current;
                if (!node) return;
                inlineAtLive.current =
                  node.scrollHeight - node.scrollTop - node.clientHeight < 60;
                setInlineFollowing(inlineAtLive.current);
              }}
            >
              <ActivityRows
                items={inlineItems}
                onSelect={(item) => {
                  setSelected(item);
                  setInspecting(true);
                }}
                compact
              />
            </div>
          )}
          {!activity.isPending && !activity.error && !items.length ? (
            <p className="text-xs text-muted-foreground">
              No activity has been captured for this work yet.
            </p>
          ) : null}
          {!inlineFollowing && active ? (
            <Button variant="outline" size="sm" onClick={jumpToLive}>
              Jump to live
            </Button>
          ) : null}
          {items.length > inlineItems.length || page?.has_earlier ? (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setInspecting(true)}
            >
              View chronological history
            </Button>
          ) : null}
        </div>
      ) : null}
      {inspecting ? (
        <Modal
          title="Work activity"
          onClose={closeInspector}
          drawer
          drawerClassName="data-[side=right]:w-full data-[side=right]:sm:max-w-none data-[side=right]:lg:max-w-2xl"
          legacy={false}
        >
          <div className="min-w-0 space-y-4">
            <p className="text-xs text-muted-foreground">
              {connection} · {page?.run_status ?? status ?? "Loading"}
            </p>
            {selected ? (
              <ActivityDetail
                key={`${page?.history_key}:${selected.event_id}`}
                base={activity.base}
                historyKey={page?.history_key ?? "unavailable"}
                activity={selected}
                onSource={
                  onSource
                    ? (source) => {
                        closeInspector();
                        onSource(source);
                      }
                    : undefined
                }
                onBack={() => setSelected(null)}
              />
            ) : (
              history
            )}
          </div>
        </Modal>
      ) : null}
    </section>
  );
}
