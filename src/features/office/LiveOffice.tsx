import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  BriefcaseBusiness,
  CircleAlert,
  CircleCheck,
  History,
  RefreshCw,
  Settings2,
  UserRound,
} from "lucide-react";
import { errorText, tenderPath, useApi, type Schema } from "../../api";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { SourceSelection } from "../Sources";
import { StaffPortrait } from "../StaffPortrait";
import { OfficeMessages } from "./OfficeMessages";
import { StaffDesk } from "./StaffDesk";
import { useOffice, type OfficeConnectionState } from "./useOffice";

type StaffFilter = "active" | "history";

function isHistoryProfile(profile: StaffProfile) {
  return profile.lifecycle === "retired" || profile.lifecycle === "archived";
}

function matchesStaffFilter(profile: StaffProfile, filter: StaffFilter) {
  return filter === "history"
    ? isHistoryProfile(profile)
    : !isHistoryProfile(profile);
}

function lifecycleQuery(filter: StaffFilter) {
  return filter === "history" ? "history" : "available";
}
type OfficeSnapshot = Schema<"OfficeSnapshot">;
type OfficeEvent = Schema<"OfficeEvent">;
type StaffProfile = Schema<"StaffProfileRecord">;
type StaffPage = Schema<"StaffPage">;
type StaffAssignment = Schema<"StaffAssignment">;
type ManagerProfile = Schema<"ManagerProfile">;

export type LiveOfficeProps = {
  tenderId: string;
  onSource: (source: SourceSelection) => void;
  onOpenResult?: (resultId: string) => void;
  onOpenOutput?: (outputId: string) => void;
  onCustomizeManager?: () => void;
  onOpenManager?: () => void;
  onExpand?: () => void;
  onClose?: () => void;
  expanded?: boolean;
  /** Rendered inside the Tender work pane, which supplies its own heading. */
  embedded?: boolean;
  className?: string;
};

export function LiveOffice({
  tenderId,
  onSource,
  onOpenResult,
  onOpenOutput,
  onCustomizeManager,
  onOpenManager,
  onExpand,
  onClose,
  expanded = true,
  embedded = false,
  className = "",
}: LiveOfficeProps) {
  const office = useOffice(tenderId, expanded);
  const [staffFilter, setStaffFilter] = useState<StaffFilter>("active");
  const roster = useStaffRoster(tenderId, office.snapshot, staffFilter);
  const [selectedStaffId, setSelectedStaffId] = useState<string | null>(null);
  const [motionMode, setMotionMode] = useState<"system" | "reduced" | "full">(
    "system",
  );
  const [hidden, setHidden] = useState(false);

  useEffect(() => {
    const onVisibilityChange = () =>
      setHidden(document.visibilityState === "hidden");
    onVisibilityChange();
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () =>
      document.removeEventListener("visibilitychange", onVisibilityChange);
  }, []);

  const staff = roster.staff;
  useEffect(() => {
    if (
      selectedStaffId &&
      !staff.some((profile) => profile.id === selectedStaffId)
    ) {
      setSelectedStaffId(null);
    }
  }, [selectedStaffId, staff]);

  const arrivingStaffIds = useMemo(
    () =>
      new Set(
        staff
          .filter((profile) =>
            office.arrivalIds.some((eventId) =>
              eventMatchesProfile(office.events, eventId, profile.id),
            ),
          )
          .map((profile) => profile.id),
      ),
    [office.arrivalIds, office.events, staff],
  );

  if (!expanded) {
    return (
      <section
        className={cn(
          "flex flex-wrap items-center justify-between gap-3 rounded-xl border bg-card p-4",
          className,
        )}
        aria-label="Live office"
      >
        <div className="flex min-w-0 items-start gap-3">
          <BriefcaseBusiness
            className="mt-0.5 size-5 shrink-0"
            aria-hidden="true"
          />
          <div className="flex min-w-0 flex-col gap-0.5">
            <h2 className="text-sm font-semibold tracking-tight">
              Live office
            </h2>
            <p className="text-sm text-muted-foreground">
              See the colleagues and actual exchanges working on this Tender.
            </p>
          </div>
        </div>
        <Button type="button" size="sm" onClick={onExpand}>
          Open live office
          <ArrowRight data-icon="inline-end" className="rtl:-scale-x-100" />
        </Button>
      </section>
    );
  }

  return (
    <section
      className={cn(
        "@container flex min-h-0 w-full min-w-0 flex-col gap-4",
        embedded ? "p-0" : "p-4",
        className,
      )}
      data-motion={motionMode}
      data-hidden={hidden ? "true" : "false"}
      aria-label="Live office"
    >
      {embedded ? null : (
        <header className="flex flex-wrap items-start justify-between gap-3">
          <div className="flex min-w-0 flex-col gap-1">
            <h1 className="text-lg font-semibold tracking-tight">
              Live office
            </h1>
            <p className="text-sm text-muted-foreground">
              Review your team's work and discuss next steps with the Tender
              Manager.
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {onClose ? (
              <Button type="button" variant="ghost" size="sm" onClick={onClose}>
                <ArrowLeft
                  data-icon="inline-start"
                  className="rtl:-scale-x-100"
                />
                Back to current work
              </Button>
            ) : null}
            {onCustomizeManager ? (
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={onCustomizeManager}
              >
                <Settings2 data-icon="inline-start" />
                Customize Manager
              </Button>
            ) : null}
          </div>
        </header>
      )}

      <ConnectionBanner
        state={office.connectionState}
        error={office.error}
        hasSnapshot={Boolean(office.snapshot)}
        onRetry={office.retry}
      />

      {!office.snapshot ? (
        office.connectionState === "connecting" ||
        office.connectionState === "reconnecting" ? (
          <div
            className="flex items-center gap-2 text-sm text-muted-foreground"
            role="status"
          >
            <RefreshCw className="size-4 animate-spin" />
            Loading the saved office…
          </div>
        ) : (
          <div
            className="flex items-start gap-3 rounded-xl border border-destructive/40 bg-destructive/5 p-4"
            role="alert"
          >
            <CircleAlert
              className="mt-0.5 size-4 shrink-0 text-destructive"
              aria-hidden="true"
            />
            <div className="flex min-w-0 flex-col items-start gap-2">
              <strong className="text-sm font-medium">
                The live office is unavailable.
              </strong>
              <p className="text-sm text-muted-foreground">
                {errorText(office.error)}
              </p>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={office.retry}
              >
                Try again
              </Button>
            </div>
          </div>
        )
      ) : (
        <OfficeContent
          tenderId={tenderId}
          embedded={embedded}
          snapshot={office.snapshot}
          staff={staff}
          staffFilter={staffFilter}
          onStaffFilterChange={setStaffFilter}
          staffLoading={roster.loading}
          staffError={roster.error}
          hasMoreStaff={roster.hasMore}
          onLoadStaff={roster.loadMore}
          onStaffReactivated={roster.refresh}
          events={office.events}
          arrivingStaffIds={arrivingStaffIds}
          selectedStaffId={selectedStaffId}
          setSelectedStaffId={setSelectedStaffId}
          onSource={onSource}
          onOpenResult={onOpenResult}
          onOpenOutput={onOpenOutput}
          onOpenManager={onOpenManager}
        />
      )}

      <details className="text-sm">
        <summary className="w-fit cursor-pointer text-xs text-muted-foreground">
          More options
        </summary>
        <div className="mt-2 flex flex-col gap-2">
          <label
            className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground"
            htmlFor={`office-motion-${tenderId}`}
          >
            Motion preference
            <select
              id={`office-motion-${tenderId}`}
              className="h-8 rounded-md border bg-background px-2 text-sm text-foreground"
              value={motionMode}
              onChange={(event) =>
                setMotionMode(event.target.value as typeof motionMode)
              }
            >
              <option value="system">System preference</option>
              <option value="reduced">Reduced</option>
              <option value="full">Full</option>
            </select>
          </label>
          <p className="text-xs text-muted-foreground">
            Motion only marks new saved office events. Hiding this window pauses
            visual motion while work continues in the service.
          </p>
        </div>
      </details>
    </section>
  );
}

function OfficeContent({
  tenderId,
  embedded,
  snapshot,
  staff,
  staffFilter,
  onStaffFilterChange,
  staffLoading,
  staffError,
  hasMoreStaff,
  onLoadStaff,
  onStaffReactivated,
  events,
  arrivingStaffIds,
  selectedStaffId,
  setSelectedStaffId,
  onSource,
  onOpenResult,
  onOpenOutput,
  onOpenManager,
}: {
  tenderId: string;
  embedded: boolean;
  snapshot: OfficeSnapshot;
  staff: StaffProfile[];
  staffFilter: StaffFilter;
  onStaffFilterChange: (filter: StaffFilter) => void;
  staffLoading: boolean;
  staffError: unknown;
  hasMoreStaff: boolean;
  onLoadStaff: () => void;
  onStaffReactivated: () => void;
  events: OfficeEvent[];
  arrivingStaffIds: Set<string>;
  selectedStaffId: string | null;
  setSelectedStaffId: (staffId: string | null) => void;
  onSource: (source: SourceSelection) => void;
  onOpenResult?: (resultId: string) => void;
  onOpenOutput?: (outputId: string) => void;
  onOpenManager?: () => void;
}) {
  const api = useApi();
  const assignments = snapshot.assignments ?? [];
  const selectedProfile =
    staff.find((profile) => profile.id === selectedStaffId) ?? null;
  const partial = Object.entries(snapshot.partial_flags ?? {})
    .filter(([, value]) => value)
    .map(([key]) => key);
  const [reactivatingId, setReactivatingId] = useState<string | null>(null);
  const [reactivateError, setReactivateError] = useState<unknown>(null);

  const reactivate = useCallback(
    async (profile: StaffProfile) => {
      setReactivatingId(profile.id);
      setReactivateError(null);
      try {
        await api.post(
          `${tenderPath(tenderId)}/staff/${encodeURIComponent(profile.id)}/lifecycle`,
          {
            staff_id: profile.id,
            expected_version: profile.version,
            target: "available",
            reason: "Reuse this colleague for later work.",
            idempotency_key: `roster-${profile.id}-available-${profile.version}`,
          },
        );
        onStaffReactivated();
      } catch (failure) {
        setReactivateError(failure);
      } finally {
        setReactivatingId(null);
      }
    },
    [api, onStaffReactivated, tenderId],
  );

  return (
    <>
      {!embedded || partial.length ? (
        <div
          className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground"
          aria-live="polite"
        >
          {embedded ? null : (
            <span className="flex items-center gap-1.5">
              <CircleCheck className="size-3.5" aria-hidden="true" />
              Saved office snapshot · {snapshot.staff_total}{" "}
              {snapshot.staff_total === 1 ? "colleague" : "colleagues"}
            </span>
          )}
          {partial.length ? (
            <span>
              Additional saved history is available through the Load more
              controls.
            </span>
          ) : null}
        </div>
      ) : null}

      <div
        className={cn(
          "grid min-w-0 gap-5",
          !embedded &&
            "@3xl:grid-cols-[minmax(0,1.05fr)_minmax(0,0.95fr)] @3xl:items-start",
        )}
      >
        <section
          className={cn(
            "flex min-w-0 flex-col gap-4",
            !embedded && "rounded-xl border bg-card p-4",
          )}
          aria-labelledby="office-work-heading"
        >
          <div hidden={embedded && Boolean(selectedProfile)}>
            <div className="flex min-w-0 flex-col gap-4">
              <div className="flex flex-wrap items-start justify-between gap-2">
                <div className="flex min-w-0 flex-col gap-0.5">
                  <h2
                    id="office-work-heading"
                    className="text-sm font-semibold tracking-tight"
                  >
                    {embedded ? "Colleagues" : "Shared work area"}
                  </h2>
                  <p className="text-xs text-muted-foreground">
                    The Manager coordinates the actual assignments below.
                  </p>
                </div>
                {embedded ? null : (
                  <span className="text-xs text-muted-foreground">
                    Office update {snapshot.sequence}
                  </span>
                )}
              </div>

              <ManagerAnchor
                manager={snapshot.manager}
                onOpenManager={onOpenManager}
                compact={embedded}
              />

              <div className="flex flex-col gap-2">
                <div
                  className="flex flex-wrap items-center gap-1.5"
                  role="group"
                  aria-label="Colleague list filter"
                >
                  <Button
                    type="button"
                    size="sm"
                    variant={staffFilter === "active" ? "default" : "outline"}
                    aria-pressed={staffFilter === "active"}
                    onClick={() => onStaffFilterChange("active")}
                  >
                    Active colleagues
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant={staffFilter === "history" ? "default" : "outline"}
                    aria-pressed={staffFilter === "history"}
                    onClick={() => onStaffFilterChange("history")}
                  >
                    History
                  </Button>
                </div>
                <span className="text-xs text-muted-foreground">
                  {staffFilter === "history"
                    ? "Retired and archived colleagues stay here. Archive is not deletion."
                    : "Colleagues available for new work."}
                </span>
              </div>

              {staff.length ? (
                <div
                  className={cn(
                    "grid min-w-0 gap-1",
                    !embedded && "gap-3 @2xl:grid-cols-2",
                  )}
                  role="list"
                  aria-label={
                    staffFilter === "history"
                      ? "Retired and archived staff"
                      : "Generated staff"
                  }
                >
                  {staff.map((profile) => {
                    const profileAssignments = assignments.filter(
                      (assignment) => assignment.staff_id === profile.id,
                    );
                    const currentAssignment =
                      latestAssignment(profileAssignments);
                    const selected = profile.id === selectedStaffId;
                    return (
                      <StaffCard
                        key={profile.id}
                        compact={embedded}
                        profile={profile}
                        assignment={currentAssignment}
                        selected={selected}
                        arriving={arrivingStaffIds.has(profile.id)}
                        onOpen={() => setSelectedStaffId(profile.id)}
                        showReactivate={staffFilter === "history"}
                        reactivating={reactivatingId === profile.id}
                        onReactivate={() => void reactivate(profile)}
                      />
                    );
                  })}
                </div>
              ) : (
                <div className="flex flex-col items-start gap-2 rounded-lg border border-dashed p-4">
                  {staffFilter === "history" ? (
                    <History
                      className="size-5 text-muted-foreground"
                      aria-hidden="true"
                    />
                  ) : (
                    <UserRound
                      className="size-5 text-muted-foreground"
                      aria-hidden="true"
                    />
                  )}
                  <h3 className="text-sm font-medium">
                    {staffFilter === "history"
                      ? "No retired or archived colleagues."
                      : "The Manager is the only colleague so far."}
                  </h3>
                  <p className="text-sm text-muted-foreground">
                    {staffFilter === "history"
                      ? "Retired and archived colleagues stay here with their history. Archive is not deletion."
                      : "Ask the Tender Manager to review work. It will create a complete AI staff profile only when the task needs one."}
                  </p>
                  {staffFilter !== "history" && onOpenManager ? (
                    <Button type="button" size="sm" onClick={onOpenManager}>
                      Open Manager conversation
                      <ArrowRight
                        data-icon="inline-end"
                        className="rtl:-scale-x-100"
                      />
                    </Button>
                  ) : null}
                </div>
              )}
              {staffLoading ? (
                <p className="flex items-center gap-2 text-xs text-muted-foreground">
                  <RefreshCw className="size-3.5 animate-spin" />
                  Loading saved colleagues…
                </p>
              ) : null}
              {staffError ? (
                <div
                  className="flex flex-wrap items-center gap-2 text-sm text-destructive"
                  role="alert"
                >
                  <span>{errorText(staffError)}</span>
                  <Button
                    type="button"
                    variant="link"
                    size="sm"
                    className="h-auto p-0"
                    onClick={onLoadStaff}
                  >
                    Try again
                  </Button>
                </div>
              ) : null}
              {hasMoreStaff ? (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="w-fit"
                  disabled={staffLoading}
                  onClick={onLoadStaff}
                >
                  {staffLoading ? (
                    <RefreshCw
                      data-icon="inline-start"
                      className="animate-spin"
                    />
                  ) : (
                    <History data-icon="inline-start" />
                  )}
                  {staffLoading
                    ? "Loading more colleagues…"
                    : "Load more colleagues"}
                </Button>
              ) : null}
              {reactivateError ? (
                <p className="text-sm text-destructive" role="alert">
                  {errorText(reactivateError)}
                </p>
              ) : null}
            </div>
          </div>
          {selectedProfile ? (
            <StaffDesk
              tenderId={tenderId}
              staffId={selectedProfile.id}
              refreshKey={snapshot.sequence}
              onSource={onSource}
              onOpenResult={onOpenResult}
              onOpenOutput={onOpenOutput}
              onOpenManager={onOpenManager}
              onClose={() => setSelectedStaffId(null)}
            />
          ) : null}
        </section>

        {embedded ? (
          <details className="border-t pt-4">
            <summary className="cursor-pointer text-xs text-muted-foreground">
              Office conversation
            </summary>
            <div className="quantix-reveal mt-4">
              <OfficeMessages
                tenderId={tenderId}
                page={snapshot.messages}
                onSource={onSource}
                onOpenResult={onOpenResult}
                onOpenOutput={onOpenOutput}
                compact
              />
            </div>
          </details>
        ) : (
          <OfficeMessages
            tenderId={tenderId}
            page={snapshot.messages}
            onSource={onSource}
            onOpenResult={onOpenResult}
            onOpenOutput={onOpenOutput}
          />
        )}
      </div>
    </>
  );
}

function ManagerAnchor({
  manager,
  onOpenManager,
  compact = false,
}: {
  manager: ManagerProfile;
  onOpenManager?: () => void;
  compact?: boolean;
}) {
  return (
    <article
      className={cn(
        "flex min-w-0 flex-wrap items-center gap-3 rounded-lg p-3",
        compact ? "bg-muted/40" : "border bg-muted/40",
      )}
    >
      <span
        className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-primary text-primary-foreground"
        aria-hidden="true"
      >
        <BriefcaseBusiness className="size-4" />
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <strong className="truncate text-sm font-medium">
          {manager.display_name}
        </strong>
        <span className="truncate text-xs text-muted-foreground">
          {manager.title && manager.title !== manager.display_name
            ? `${manager.title} · AI`
            : "Tender Manager · AI"}
        </span>
      </div>
      {onOpenManager ? (
        <Button
          type="button"
          variant="ghost"
          size={compact ? "icon-sm" : "sm"}
          aria-label={compact ? "Open Manager" : undefined}
          title="Open Manager"
          onClick={onOpenManager}
        >
          {compact ? null : "Open Manager"}
          <ArrowRight data-icon="inline-end" className="rtl:-scale-x-100" />
        </Button>
      ) : null}
    </article>
  );
}

function StaffCard({
  compact = false,
  profile,
  assignment,
  selected,
  arriving,
  onOpen,
  showReactivate = false,
  reactivating = false,
  onReactivate,
}: {
  compact?: boolean;
  profile: StaffProfile;
  assignment: StaffAssignment | null;
  selected: boolean;
  arriving: boolean;
  onOpen: () => void;
  showReactivate?: boolean;
  reactivating?: boolean;
  onReactivate?: () => void;
}) {
  const status = assignment?.status ?? "idle";
  const retired = isHistoryProfile(profile);
  return (
    <article
      className={cn(
        "flex min-w-0 flex-col overflow-hidden rounded-lg border bg-card transition-colors",
        compact && "border-transparent",
        selected && "border-primary bg-accent",
        arriving && "office-staff-card-arriving",
      )}
      role="listitem"
      data-staff-id={profile.id}
    >
      <button
        type="button"
        className="group flex min-w-0 flex-1 items-start gap-3 p-3 text-start outline-none hover:bg-accent/60 focus-visible:bg-accent/60"
        aria-pressed={selected}
        onClick={onOpen}
      >
        <StaffPortrait
          portrait={profile.portrait}
          name={profile.display_name}
          size={44}
          decorative
        />
        <span className="flex min-w-0 flex-1 flex-col gap-1">
          <span className="flex min-w-0 flex-col">
            <strong className="truncate text-sm font-medium">
              {profile.display_name}
            </strong>
            <span className="truncate text-xs text-muted-foreground">
              {profile.title} · {profile.role}
            </span>
          </span>
          <span
            className={cn(
              "w-fit rounded-full px-2 py-0.5 text-xs",
              status === "running"
                ? "bg-emerald-500/12 text-emerald-700 dark:text-emerald-400"
                : status === "failed" || status === "interrupted"
                  ? "bg-destructive/10 text-destructive"
                  : "bg-muted text-muted-foreground",
            )}
          >
            {assignment ? assignmentLabel(status) : "Idle"}
          </span>
          <span className="line-clamp-2 text-xs text-muted-foreground">
            {assignment?.detail ||
              // Before any work runs, say what this colleague was hired to do.
              (profile.objectives?.[0]
                ? `Planned: ${profile.objectives[0]}`
                : "No work has been planned for this colleague yet.")}
          </span>
          {retired ? (
            <span className="text-xs text-muted-foreground">
              {profile.lifecycle === "archived"
                ? "Archived. History is kept."
                : "Retired. History is kept."}
            </span>
          ) : null}
        </span>
        <ArrowRight
          className="mt-0.5 size-4 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 rtl:-scale-x-100"
          aria-hidden="true"
        />
        <span className="sr-only">Open staff desk</span>
      </button>
      {showReactivate && retired && onReactivate ? (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="m-2 mt-0 w-fit"
          disabled={reactivating}
          onClick={onReactivate}
          aria-label={`Reactivate ${profile.display_name} for later work`}
        >
          {reactivating ? "Reactivating…" : "Reactivate for later work"}
        </Button>
      ) : null}
    </article>
  );
}

function ConnectionBanner({
  state,
  error,
  hasSnapshot,
  onRetry,
}: {
  state: OfficeConnectionState;
  error: Error | null;
  hasSnapshot: boolean;
  onRetry: () => void;
}) {
  if (state === "connected") return null;
  if (state === "connecting" && !hasSnapshot) return null;
  const copy: Record<OfficeConnectionState, string> = {
    disabled: "Live office updates are paused.",
    connecting: "Reconnecting to the live office…",
    connected: "Live office connected.",
    reconnecting: "The saved office is shown while Quantix reconnects.",
    disconnected:
      "The live office could not be reached. The saved view is kept when available.",
  };
  return (
    <div
      className={cn(
        "flex flex-wrap items-center gap-2 rounded-lg border px-3 py-2 text-xs",
        state === "disconnected"
          ? "border-destructive/40 bg-destructive/5 text-destructive"
          : "bg-muted/50 text-muted-foreground",
      )}
      role={state === "disconnected" ? "alert" : "status"}
    >
      <CircleAlert className="size-3.5 shrink-0" aria-hidden="true" />
      <span className="min-w-0 flex-1">
        {copy[state]}
        {error ? ` ${errorText(error)}` : ""}
      </span>
      <Button
        type="button"
        variant="link"
        size="sm"
        className="h-auto p-0 text-xs"
        onClick={onRetry}
      >
        Retry
      </Button>
    </div>
  );
}

function latestAssignment(assignments: StaffAssignment[]) {
  return (
    [...assignments].sort((left, right) =>
      right.updated_at.localeCompare(left.updated_at),
    )[0] ?? null
  );
}

function assignmentLabel(status: StaffAssignment["status"] | "idle") {
  const labels: Record<StaffAssignment["status"] | "idle", string> = {
    idle: "Idle",
    queued: "Waiting to start",
    running: "Working",
    waiting: "Waiting for a reply",
    completed: "Completed",
    failed: "Failed",
    cancelled: "Cancelled",
    interrupted: "Interrupted",
  };
  return labels[status];
}

function eventMatchesProfile(
  events: OfficeEvent[],
  eventId: string,
  staffId: string,
) {
  const event = events.find((item) => item.event_id === eventId);
  if (!event || event.event_type !== "staff_created") return false;
  if (event.actor_id === staffId) return true;
  const ref = event.record_ref;
  return Boolean(ref && (ref.staff_id === staffId || ref.id === staffId));
}

function useStaffRoster(
  tenderId: string,
  snapshot: OfficeSnapshot | null,
  filter: StaffFilter,
) {
  const api = useApi();
  const requestRef = useRef<AbortController | null>(null);
  const generationRef = useRef(0);
  const identityRef = useRef("");
  const filterRef = useRef<StaffFilter>(filter);
  filterRef.current = filter;
  const instanceId = snapshot?.instance_id ?? "none";
  const identity = `${tenderId}\u0000${instanceId}\u0000${filter}`;
  const [staff, setStaff] = useState<StaffProfile[]>(() =>
    (snapshot?.staff ?? []).filter((profile) =>
      matchesStaffFilter(profile, filterRef.current),
    ),
  );
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(
    Boolean(snapshot?.partial_flags?.staff) ||
      (snapshot?.staff?.length ?? 0) >= 50,
  );
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const prevFilterRef = useRef<StaffFilter>(filter);

  useEffect(() => {
    const previousFilter = prevFilterRef.current;
    prevFilterRef.current = filter;
    generationRef.current += 1;
    const generation = generationRef.current;
    requestRef.current?.abort();
    requestRef.current = null;
    identityRef.current = identity;
    const base = (snapshot?.staff ?? []).filter((profile) =>
      matchesStaffFilter(profile, filter),
    );
    setStaff(base);
    setNextCursor(null);
    setHasMore(
      Boolean(snapshot?.partial_flags?.staff) ||
        (snapshot?.staff?.length ?? 0) >= 50,
    );
    setLoading(false);
    setError(null);
    if (previousFilter === filter) return;
    // The engineer switched between active colleagues and history: fetch the
    // first server page for the new view and merge it with the snapshot rows.
    const request = new AbortController();
    requestRef.current = request;
    setLoading(true);
    void (async () => {
      try {
        const query = new URLSearchParams({
          limit: "50",
          lifecycle: lifecycleQuery(filter),
        });
        const next = await api.get<StaffPage>(
          `${tenderPath(tenderId)}/staff?${query.toString()}`,
          request.signal,
        );
        if (
          request.signal.aborted ||
          generation !== generationRef.current ||
          identityRef.current !== identity
        )
          return;
        setStaff(
          mergeStaff(
            base,
            (next.items ?? []).filter((item) =>
              matchesStaffFilter(item, filter),
            ),
          ),
        );
        setNextCursor(next.next_cursor ?? null);
        setHasMore(Boolean(next.next_cursor));
      } catch (failure) {
        if (
          request.signal.aborted ||
          generation !== generationRef.current ||
          identityRef.current !== identity
        )
          return;
        setError(failure);
      } finally {
        if (requestRef.current === request) {
          requestRef.current = null;
          setLoading(false);
        }
      }
    })();
  }, [identity]);

  // A new office snapshot may add a colleague or replace a profile version.
  // Merge it into the retained roster so pagination and selection stay stable.
  useEffect(() => {
    if (identityRef.current !== identity || !snapshot) return;
    setStaff((current) =>
      mergeStaff(
        current,
        (snapshot.staff ?? []).filter((profile) =>
          matchesStaffFilter(profile, filterRef.current),
        ),
      ),
    );
  }, [identity, snapshot?.sequence, snapshot?.staff]);

  const loadPage = useCallback(
    async (cursor: string | null, activeFilter: StaffFilter) => {
      const query = new URLSearchParams({
        limit: "50",
        lifecycle: lifecycleQuery(activeFilter),
      });
      if (cursor) query.set("cursor", cursor);
      const request = new AbortController();
      requestRef.current?.abort();
      requestRef.current = request;
      return { query, request };
    },
    [],
  );

  const loadMore = useCallback(async () => {
    if (loading || !hasMore) return;
    const generation = generationRef.current;
    const activeFilter = filterRef.current;
    const { query, request } = await loadPage(nextCursor, activeFilter);
    setLoading(true);
    setError(null);
    try {
      const next = await api.get<StaffPage>(
        `${tenderPath(tenderId)}/staff?${query.toString()}`,
        request.signal,
      );
      if (
        request.signal.aborted ||
        generation !== generationRef.current ||
        identityRef.current !== identity
      )
        return;
      setStaff((current) =>
        mergeStaff(
          current,
          (next.items ?? []).filter((profile) =>
            matchesStaffFilter(profile, filterRef.current),
          ),
        ),
      );
      setNextCursor(next.next_cursor ?? null);
      setHasMore(Boolean(next.next_cursor));
    } catch (failure) {
      if (
        request.signal.aborted ||
        generation !== generationRef.current ||
        identityRef.current !== identity
      )
        return;
      setError(failure);
    } finally {
      if (requestRef.current === request) {
        requestRef.current = null;
        setLoading(false);
      }
    }
  }, [api, hasMore, identity, loadPage, loading, nextCursor, tenderId]);

  const refresh = useCallback(async () => {
    const generation = generationRef.current;
    const activeFilter = filterRef.current;
    const { query, request } = await loadPage(null, activeFilter);
    setLoading(true);
    setError(null);
    try {
      const next = await api.get<StaffPage>(
        `${tenderPath(tenderId)}/staff?${query.toString()}`,
        request.signal,
      );
      if (
        request.signal.aborted ||
        generation !== generationRef.current ||
        identityRef.current !== identity
      )
        return;
      setStaff(
        (next.items ?? []).filter((profile) =>
          matchesStaffFilter(profile, filterRef.current),
        ),
      );
      setNextCursor(next.next_cursor ?? null);
      setHasMore(Boolean(next.next_cursor));
    } catch (failure) {
      if (
        request.signal.aborted ||
        generation !== generationRef.current ||
        identityRef.current !== identity
      )
        return;
      setError(failure);
    } finally {
      if (requestRef.current === request) {
        requestRef.current = null;
        setLoading(false);
      }
    }
  }, [api, identity, loadPage, tenderId]);

  useEffect(
    () => () => {
      generationRef.current += 1;
      requestRef.current?.abort();
      requestRef.current = null;
    },
    [],
  );

  return { staff, hasMore, loading, error, loadMore, refresh };
}

function mergeStaff(current: StaffProfile[], additions: StaffProfile[]) {
  const byId = new Map<string, StaffProfile>();
  for (const profile of [...current, ...additions])
    byId.set(profile.id, profile);
  return [...byId.values()];
}
