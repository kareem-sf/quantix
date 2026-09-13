import { useCallback, useEffect, useRef, useState } from "react";
import {
  ChevronLeft,
  FileCheck2,
  FileText,
  History,
  LoaderCircle,
  X,
} from "lucide-react";
import { errorText, tenderPath, useApi, type Schema } from "../../api";
import { StaffPortrait } from "../StaffPortrait";
import type { SourceSelection } from "../Sources";
import { OfficeMessages } from "./OfficeMessages";
import { StaffDraftContent } from "./StaffDraftContent";
import { StaffHandoffs } from "./StaffHandoffs";
import { NativeActivityInspector } from "../NativeActivityInspector";
import { LocalCodeInspector } from "../LocalCodeInspector";
import { StaffNotebook } from "./StaffNotebook";

type StaffDeskRecord = Schema<"StaffDesk">;
type StaffProfile = Schema<"StaffProfileRecord">;
type StaffAssignment = Schema<"StaffAssignment">;
type StaffResult = Schema<"StaffResult">;
type StaffVersionPage = Schema<"StaffVersionPage">;
type StaffWorkOrderPage = Schema<"StaffWorkOrderPage">;
type StaffResultPage = Schema<"StaffResultPage">;
type StaffReceiptPage = Schema<"StaffReceiptPage">;
type AssignmentPage = Schema<"AssignmentPage">;

export type StaffDeskProps = {
  tenderId: string;
  staffId: string;
  onSource: (source: SourceSelection) => void;
  onOpenResult?: (resultId: string) => void;
  onOpenOutput?: (outputId: string) => void;
  onOpenManager?: () => void;
  onClose?: () => void;
  /** Snapshot sequence used to reconcile current assignment/receipt state. */
  refreshKey?: number;
};

const PAGE_SIZE = 20;

export function StaffDesk({
  tenderId,
  staffId,
  onSource,
  onOpenResult,
  onOpenOutput,
  onOpenManager,
  onClose,
  refreshKey,
}: StaffDeskProps) {
  const api = useApi();
  const [desk, setDesk] = useState<StaffDeskRecord | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);
  const [selectedVersion, setSelectedVersion] = useState<number | null>(null);
  const [versionNumbers, setVersionNumbers] = useState<number[]>([]);
  const [versionPage, setVersionPage] = useState<StaffVersionPage | null>(null);
  const [versionsLoading, setVersionsLoading] = useState(false);
  const [workOrdersLoading, setWorkOrdersLoading] = useState(false);
  const [workOrdersHasMore, setWorkOrdersHasMore] = useState(false);
  const [workOrdersOffset, setWorkOrdersOffset] = useState(0);
  const [assignmentsLoading, setAssignmentsLoading] = useState(false);
  const [assignmentsHasMore, setAssignmentsHasMore] = useState(false);
  const [assignmentsOffset, setAssignmentsOffset] = useState(0);
  const [resultsLoading, setResultsLoading] = useState(false);
  const [resultsHasMore, setResultsHasMore] = useState(false);
  const [resultsOffset, setResultsOffset] = useState(0);
  const [receiptsLoading, setReceiptsLoading] = useState(false);
  const [receiptsHasMoreByAssignment, setReceiptsHasMoreByAssignment] =
    useState<Record<string, boolean>>({});
  const [receiptOffsets, setReceiptOffsets] = useState<Record<string, number>>(
    {},
  );
  const [selectedResult, setSelectedResult] = useState<StaffResult | null>(
    null,
  );
  const [resultLoading, setResultLoading] = useState(false);
  const [actionError, setActionError] = useState<unknown>(null);
  const [lifecycleBusy, setLifecycleBusy] = useState(false);
  const [preferenceText, setPreferenceText] = useState(
    "Be shorter\nKeep source references",
  );
  const deskRequestRef = useRef<AbortController | null>(null);
  const requestControllersRef = useRef(new Set<AbortController>());
  const generationRef = useRef(0);
  const identityRef = useRef(`${tenderId}\u0000${staffId}`);
  const selectedVersionRef = useRef<number | null>(null);
  const seenRefreshKeyRef = useRef(refreshKey);
  const identityKey = `${tenderId}\u0000${staffId}`;

  const isCurrentRequest = useCallback(
    (generation: number) =>
      generation === generationRef.current &&
      identityRef.current === identityKey,
    [identityKey],
  );

  const loadDesk = useCallback(
    async (version?: number, preserveLoadedData = false) => {
      deskRequestRef.current?.abort();
      const controller = new AbortController();
      deskRequestRef.current = controller;
      requestControllersRef.current.add(controller);
      const generation = generationRef.current;
      setLoading(true);
      setError(null);
      setActionError(null);
      const suffix = version === undefined ? "" : `?version=${version}`;
      try {
        const next = await api.get<StaffDeskRecord>(
          `${tenderPath(tenderId)}/staff/${encodeURIComponent(staffId)}${suffix}`,
          controller.signal,
        );
        if (controller.signal.aborted || !isCurrentRequest(generation)) return;
        setDesk((current) =>
          preserveLoadedData && current ? mergeDesk(current, next) : next,
        );
        setWorkOrdersHasMore((current) =>
          preserveLoadedData
            ? current || Boolean(next.partial_flags?.work_orders)
            : Boolean(next.partial_flags?.work_orders) ||
              (next.work_orders?.length ?? 0) >= PAGE_SIZE,
        );
        setResultsHasMore((current) =>
          preserveLoadedData
            ? current || Boolean(next.partial_flags?.results)
            : Boolean(next.partial_flags?.results) ||
              (next.results?.length ?? 0) >= PAGE_SIZE,
        );
        setAssignmentsHasMore((current) =>
          preserveLoadedData
            ? current || Boolean(next.partial_flags?.assignments)
            : Boolean(next.partial_flags?.assignments) ||
              (next.assignments?.length ?? 0) >= PAGE_SIZE,
        );
        setReceiptsHasMoreByAssignment((current) => {
          const baseline = Object.fromEntries(
            (next.assignments ?? []).map((assignment) => [
              assignment.id,
              Boolean(next.partial_flags?.receipts) ||
                (next.receipts?.filter(
                  (receipt) => receipt.assignment_id === assignment.id,
                ).length ?? 0) >= PAGE_SIZE,
            ]),
          );
          return preserveLoadedData ? { ...baseline, ...current } : baseline;
        });
        setVersionNumbers((current) => {
          const supplied = next.version_numbers ?? [];
          const values = supplied.length ? supplied : [next.profile.version];
          return [...new Set([...current, ...values])].sort(
            (left, right) => right - left,
          );
        });
      } catch (failure) {
        if (controller.signal.aborted || !isCurrentRequest(generation)) return;
        setError(failure);
      } finally {
        requestControllersRef.current.delete(controller);
        if (deskRequestRef.current === controller) {
          deskRequestRef.current = null;
          setLoading(false);
        }
      }
    },
    [api, isCurrentRequest, staffId, tenderId],
  );

  useEffect(() => {
    generationRef.current += 1;
    requestControllersRef.current.forEach((controller) => controller.abort());
    requestControllersRef.current.clear();
    identityRef.current = identityKey;
    seenRefreshKeyRef.current = refreshKey;
    selectedVersionRef.current = null;
    setDesk(null);
    setSelectedVersion(null);
    setVersionNumbers([]);
    setVersionPage(null);
    setWorkOrdersOffset(0);
    setAssignmentsOffset(0);
    setResultsOffset(0);
    setReceiptOffsets({});
    setWorkOrdersHasMore(false);
    setAssignmentsHasMore(false);
    setResultsHasMore(false);
    setReceiptsHasMoreByAssignment({});
    setVersionsLoading(false);
    setWorkOrdersLoading(false);
    setAssignmentsLoading(false);
    setResultsLoading(false);
    setReceiptsLoading(false);
    setResultLoading(false);
    setSelectedResult(null);
    void loadDesk();
  }, [identityKey, loadDesk]);

  useEffect(() => {
    if (seenRefreshKeyRef.current === refreshKey) return;
    seenRefreshKeyRef.current = refreshKey;
    void loadDesk(selectedVersionRef.current ?? undefined, true);
  }, [loadDesk, refreshKey]);

  useEffect(() => () => deskRequestRef.current?.abort(), []);
  useEffect(
    () => () => {
      requestControllersRef.current.forEach((controller) => controller.abort());
      requestControllersRef.current.clear();
      generationRef.current += 1;
    },
    [],
  );

  const changeLifecycle = useCallback(
    async (target: "available" | "retired" | "archived") => {
      if (!desk?.profile) return;
      setLifecycleBusy(true);
      setActionError(null);
      try {
        await api.post(
          `${tenderPath(tenderId)}/staff/${encodeURIComponent(staffId)}/lifecycle`,
          {
            staff_id: staffId,
            expected_version: desk.profile.version,
            target,
            reason:
              target === "available"
                ? "Reuse this colleague for later work."
                : target === "retired"
                  ? "Retire this colleague. Active work is unchanged."
                  : "Archive this colleague. This is not deletion.",
            idempotency_key: `desk-${staffId}-${target}-${desk.profile.version}`,
          },
        );
        await loadDesk(undefined, true);
      } catch (failure) {
        setActionError(failure);
      } finally {
        setLifecycleBusy(false);
      }
    },
    [api, desk, loadDesk, staffId, tenderId],
  );

  const savePreferences = useCallback(async () => {
    if (!desk?.profile) return;
    const preferences = preferenceText
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
    if (!preferences.length) return;
    setLifecycleBusy(true);
    setActionError(null);
    try {
      await api.patch(
        `${tenderPath(tenderId)}/staff/${encodeURIComponent(staffId)}/preferences`,
        {
          staff_id: staffId,
          expected_version: desk.profile.version,
          preferences,
          applies_from: "subsequent",
          idempotency_key: `desk-pref-${staffId}-${desk.profile.version}`,
        },
      );
      await loadDesk(undefined, true);
    } catch (failure) {
      setActionError(failure);
    } finally {
      setLifecycleBusy(false);
    }
  }, [api, desk, loadDesk, preferenceText, staffId, tenderId]);

  const loadAssignments = useCallback(async () => {
    if (!desk || assignmentsLoading || !assignmentsHasMore) return;
    const controller = new AbortController();
    requestControllersRef.current.add(controller);
    const generation = generationRef.current;
    const offset = assignmentsOffset;
    setAssignmentsLoading(true);
    setActionError(null);
    try {
      const next = await api.get<AssignmentPage>(
        `${tenderPath(tenderId)}/office/assignments?staff_id=${encodeURIComponent(staffId)}&offset=${offset}&limit=${PAGE_SIZE}`,
        controller.signal,
      );
      if (controller.signal.aborted || !isCurrentRequest(generation)) return;
      setDesk((current) =>
        current
          ? {
              ...current,
              assignments: mergeRecords(
                current.assignments ?? [],
                next.items ?? [],
              ),
            }
          : current,
      );
      setAssignmentsOffset(
        next.next_offset ?? offset + (next.items?.length ?? 0),
      );
      setAssignmentsHasMore(next.has_more);
      if (next.items?.length) {
        setReceiptsHasMoreByAssignment((current) => {
          const nextMap = { ...current };
          for (const assignment of next.items ?? []) {
            if (!(assignment.id in nextMap)) {
              nextMap[assignment.id] = Boolean(desk.partial_flags?.receipts);
            }
          }
          return nextMap;
        });
      }
    } catch (failure) {
      if (controller.signal.aborted || !isCurrentRequest(generation)) return;
      setActionError(failure);
    } finally {
      requestControllersRef.current.delete(controller);
      if (isCurrentRequest(generation)) setAssignmentsLoading(false);
    }
  }, [
    api,
    assignmentsHasMore,
    assignmentsLoading,
    assignmentsOffset,
    desk,
    isCurrentRequest,
    staffId,
    tenderId,
  ]);

  const loadVersions = useCallback(
    async (offset = versionPage?.next_offset ?? 0) => {
      const controller = new AbortController();
      requestControllersRef.current.add(controller);
      const generation = generationRef.current;
      setVersionsLoading(true);
      setActionError(null);
      try {
        const next = await api.get<StaffVersionPage>(
          `${tenderPath(tenderId)}/staff/${encodeURIComponent(staffId)}/versions?offset=${offset}&limit=${PAGE_SIZE}`,
          controller.signal,
        );
        if (controller.signal.aborted || !isCurrentRequest(generation)) return;
        setVersionPage((current) => ({
          items: [...(current?.items ?? []), ...(next.items ?? [])],
          next_offset: next.next_offset,
          has_more: next.has_more,
        }));
        setVersionNumbers((current) =>
          [
            ...new Set([
              ...current,
              ...(next.items ?? []).map((item) => item.version),
            ]),
          ].sort((left, right) => right - left),
        );
      } catch (failure) {
        if (controller.signal.aborted || !isCurrentRequest(generation)) return;
        setActionError(failure);
      } finally {
        requestControllersRef.current.delete(controller);
        if (isCurrentRequest(generation)) setVersionsLoading(false);
      }
    },
    [api, isCurrentRequest, staffId, tenderId, versionPage?.next_offset],
  );

  const loadWorkOrders = useCallback(async () => {
    if (!desk || workOrdersLoading) return;
    const controller = new AbortController();
    requestControllersRef.current.add(controller);
    const generation = generationRef.current;
    const offset = workOrdersOffset;
    setWorkOrdersLoading(true);
    setActionError(null);
    try {
      const next = await api.get<StaffWorkOrderPage>(
        `${tenderPath(tenderId)}/staff/${encodeURIComponent(staffId)}/work-orders?offset=${offset}&limit=${PAGE_SIZE}`,
        controller.signal,
      );
      if (controller.signal.aborted || !isCurrentRequest(generation)) return;
      setDesk((current) =>
        current
          ? {
              ...current,
              work_orders: mergeRecords(
                current.work_orders ?? [],
                next.items ?? [],
              ),
            }
          : current,
      );
      setWorkOrdersOffset(
        next.next_offset ?? offset + (next.items?.length ?? 0),
      );
      setWorkOrdersHasMore(next.has_more);
    } catch (failure) {
      if (controller.signal.aborted || !isCurrentRequest(generation)) return;
      setActionError(failure);
    } finally {
      requestControllersRef.current.delete(controller);
      if (isCurrentRequest(generation)) setWorkOrdersLoading(false);
    }
  }, [
    api,
    desk,
    isCurrentRequest,
    staffId,
    tenderId,
    workOrdersLoading,
    workOrdersOffset,
  ]);

  const loadResults = useCallback(async () => {
    if (!desk || resultsLoading) return;
    const controller = new AbortController();
    requestControllersRef.current.add(controller);
    const generation = generationRef.current;
    const offset = resultsOffset;
    setResultsLoading(true);
    setActionError(null);
    try {
      const next = await api.get<StaffResultPage>(
        `${tenderPath(tenderId)}/staff/${encodeURIComponent(staffId)}/results?offset=${offset}&limit=${PAGE_SIZE}`,
        controller.signal,
      );
      if (controller.signal.aborted || !isCurrentRequest(generation)) return;
      setDesk((current) =>
        current
          ? {
              ...current,
              results: mergeRecords(current.results ?? [], next.items ?? []),
            }
          : current,
      );
      setResultsOffset(next.next_offset ?? offset + (next.items?.length ?? 0));
      setResultsHasMore(next.has_more);
    } catch (failure) {
      if (controller.signal.aborted || !isCurrentRequest(generation)) return;
      setActionError(failure);
    } finally {
      requestControllersRef.current.delete(controller);
      if (isCurrentRequest(generation)) setResultsLoading(false);
    }
  }, [
    api,
    desk,
    isCurrentRequest,
    resultsLoading,
    resultsOffset,
    staffId,
    tenderId,
  ]);

  const loadReceipts = useCallback(
    async (assignmentId: string) => {
      if (
        !desk ||
        receiptsLoading ||
        !receiptsHasMoreByAssignment[assignmentId]
      )
        return;
      const controller = new AbortController();
      requestControllersRef.current.add(controller);
      const generation = generationRef.current;
      const offset = receiptOffsets[assignmentId] ?? 0;
      setReceiptsLoading(true);
      setActionError(null);
      try {
        const next = await api.get<StaffReceiptPage>(
          `${tenderPath(tenderId)}/office/assignments/${encodeURIComponent(assignmentId)}/receipts?offset=${offset}&limit=${PAGE_SIZE}`,
          controller.signal,
        );
        if (controller.signal.aborted || !isCurrentRequest(generation)) return;
        setDesk((current) =>
          current
            ? {
                ...current,
                receipts: mergeRecords(
                  current.receipts ?? [],
                  next.items ?? [],
                ),
              }
            : current,
        );
        setReceiptOffsets((current) => ({
          ...current,
          [assignmentId]:
            next.next_offset ?? offset + (next.items?.length ?? 0),
        }));
        setReceiptsHasMoreByAssignment((current) => ({
          ...current,
          [assignmentId]: next.has_more,
        }));
      } catch (failure) {
        if (controller.signal.aborted || !isCurrentRequest(generation)) return;
        setActionError(failure);
      } finally {
        requestControllersRef.current.delete(controller);
        if (isCurrentRequest(generation)) setReceiptsLoading(false);
      }
    },
    [
      api,
      desk,
      isCurrentRequest,
      receiptOffsets,
      receiptsHasMoreByAssignment,
      receiptsLoading,
      tenderId,
    ],
  );

  const inspectResult = useCallback(
    async (resultId: string) => {
      if (onOpenResult) {
        onOpenResult(resultId);
        return;
      }
      setResultLoading(true);
      setActionError(null);
      // A standalone desk owns this read. When the workspace handles the
      // route, its result pane owns the request and this desk unmounts.
      const controller = new AbortController();
      requestControllersRef.current.add(controller);
      const generation = generationRef.current;
      try {
        const result = await api.get<StaffResult>(
          `${tenderPath(tenderId)}/office/results/${encodeURIComponent(resultId)}`,
          controller.signal,
        );
        if (controller.signal.aborted || !isCurrentRequest(generation)) return;
        setSelectedResult(result);
      } catch (failure) {
        if (controller.signal.aborted || !isCurrentRequest(generation)) return;
        setActionError(failure);
      } finally {
        requestControllersRef.current.delete(controller);
        if (isCurrentRequest(generation)) setResultLoading(false);
      }
    },
    [api, isCurrentRequest, onOpenResult, tenderId],
  );

  if (loading && !desk)
    return (
      <div className="office-loading">
        <LoaderCircle size={18} className="spin" />
        Loading staff desk…
      </div>
    );
  if (error && !desk) {
    return (
      <section
        className="office-desk office-desk-error"
        aria-label="Staff desk"
      >
        <div className="office-inline-error" role="alert">
          <span>{errorText(error)}</span>
          <button
            type="button"
            className="text-button"
            onClick={() => void loadDesk(selectedVersion ?? undefined)}
          >
            Try again
          </button>
        </div>
        {onClose ? (
          <button type="button" className="text-button" onClick={onClose}>
            <ChevronLeft size={16} />
            Back to office
          </button>
        ) : null}
      </section>
    );
  }
  if (!desk) return null;

  const profile = desk.profile;
  const assignments = desk.assignments ?? [];
  const currentAssignment = latestAssignment(assignments);
  const hasMoreVersions = versionPage?.has_more ?? false;

  return (
    <section
      className="office-desk"
      aria-label={`${profile.display_name} staff desk`}
    >
      <header className="office-desk-header">
        <div className="office-desk-identity">
          <StaffPortrait
            portrait={profile.portrait}
            name={profile.display_name}
            size={76}
          />
          <div>
            <h2>{profile.display_name}</h2>
            <p className="office-desk-role">
              {profile.title} · {profile.role} · AI staff
            </p>
            <p className="office-muted">
              Profile version {profile.version} · {profile.lifecycle}
            </p>
          </div>
        </div>
        <div className="office-desk-actions">
          {profile.lifecycle === "available" ? (
            <button
              type="button"
              className="text-button"
              disabled={lifecycleBusy}
              onClick={() => void changeLifecycle("retired")}
            >
              Retire colleague
            </button>
          ) : (
            <button
              type="button"
              className="text-button"
              disabled={lifecycleBusy}
              onClick={() => void changeLifecycle("available")}
            >
              Reactivate colleague
            </button>
          )}
          {onOpenManager ? (
            <button
              type="button"
              className="text-button"
              onClick={onOpenManager}
            >
              Return to Manager
            </button>
          ) : null}
          {onClose ? (
            <button
              type="button"
              className="icon-button"
              aria-label="Close staff desk"
              onClick={onClose}
            >
              <X size={18} />
            </button>
          ) : null}
        </div>
      </header>

      <div className="office-desk-state-row">
        <span
          className={`office-status office-status-${currentAssignment?.status ?? "idle"}`}
        >
          {currentAssignment
            ? assignmentLabel(currentAssignment.status)
            : "Idle"}
        </span>
        <span className="office-muted">
          {currentAssignment?.detail ||
            "No current assignment detail is recorded."}
        </span>
      </div>

      {loading ? (
        <p className="office-muted office-inline-loading">
          <LoaderCircle size={15} className="spin" />
          Refreshing profile…
        </p>
      ) : null}
      {actionError ? (
        <div className="office-inline-error" role="alert">
          <span>{errorText(actionError)}</span>
          <button
            type="button"
            className="text-button"
            onClick={() => setActionError(null)}
          >
            Dismiss
          </button>
        </div>
      ) : null}

      <div className="office-desk-sections">
        <section className="office-desk-section">
          <div className="office-section-heading">
            <h3>Why this colleague was created</h3>
          </div>
          <p>{profile.creation_reason}</p>
          <p className="office-muted">
            Created {formatDate(profile.created_at)} for this Tender.
          </p>
        </section>

        <ProfileSection title="Professional profile">
          <p>{profile.persona}</p>
          <DetailList label="Specialisms" values={profile.specialisms} />
          <DetailList
            label="Responsibilities"
            values={profile.responsibilities}
          />
          <DetailList label="Objectives" values={profile.objectives} />
          <DetailList label="Working methods" values={profile.methods} />
          <DetailList label="Deliverables" values={profile.deliverables} />
          <DetailList
            label="Success checks"
            values={profile.success_criteria}
          />
          <DetailList label="Context needed" values={profile.context_needs} />
        </ProfileSection>

        <ProfileSection title="Working style">
          <p>{profile.personality.description}</p>
          <div className="office-facts-grid">
            <Fact
              label="Traits"
              value={profile.personality.traits.join(", ")}
            />
            <Fact
              label="Communication"
              value={profile.personality.communication_style}
            />
            <Fact
              label="Problem solving"
              value={profile.personality.problem_solving_style}
            />
            <Fact
              label="Collaboration"
              value={profile.personality.collaboration_style}
            />
            <Fact
              label="Uncertainty"
              value={profile.personality.uncertainty_handling}
            />
            <Fact label="Initiative" value={profile.personality.initiative} />
            <Fact
              label="Explanation"
              value={profile.personality.explanation_style}
            />
            <Fact
              label="Languages"
              value={profile.personality.language_preferences.join(", ")}
            />
            <Fact
              label="Working habits"
              value={profile.personality.working_habits.join(", ")}
            />
          </div>
        </ProfileSection>

        <StaffNotebook
          tenderId={tenderId}
          staffId={staffId}
          refreshKey={refreshKey}
          onSource={onSource}
          onOpenResult={onOpenResult}
          onOpenOutput={onOpenOutput}
        />

        <StaffHandoffs
          tenderId={tenderId}
          staffId={staffId}
          refreshKey={refreshKey}
        />

        <details className="office-more-options">
          <summary>More options</summary>
          <div className="office-facts-grid">
            <Fact
              label="Requested tools"
              value={profile.requested_tool_ids.join(", ")}
            />
            <Fact label="Creator run" value={profile.creator_run_id} />
            <Fact
              label="Manager profile"
              value={`Version ${profile.manager_profile_version}`}
            />
            <Fact label="Lifecycle" value={profile.lifecycle} />
            <label className="office-field-label">
              Working preferences for later work
              <textarea
                value={preferenceText}
                onChange={(event) => setPreferenceText(event.target.value)}
                rows={3}
              />
            </label>
            <button
              type="button"
              className="text-button"
              disabled={lifecycleBusy}
              onClick={() => void savePreferences()}
            >
              Save preferences
            </button>
            {profile.lifecycle !== "archived" ? (
              <button
                type="button"
                className="text-button"
                disabled={lifecycleBusy}
                onClick={() => void changeLifecycle("archived")}
              >
                Archive colleague
              </button>
            ) : null}
          </div>
          {[...new Set(assignments.map((item) => item.root_run_id))].map(
            (runId) => (
              <div key={runId} className="flex flex-col gap-3">
                <NativeActivityInspector
                  tenderId={tenderId}
                  runId={runId}
                  profileId={staffId}
                />
                <LocalCodeInspector
                  tenderId={tenderId}
                  runId={runId}
                  actorId={staffId}
                  onStopped={() => loadDesk(selectedVersion ?? undefined, true)}
                />
              </div>
            ),
          )}
        </details>

        <WorkOrderSection
          desk={desk}
          hasMore={workOrdersHasMore}
          loading={workOrdersLoading}
          onLoad={() => void loadWorkOrders()}
        />

        <AssignmentSection
          assignments={assignments}
          receipts={desk.receipts ?? []}
          hasMoreAssignments={assignmentsHasMore}
          assignmentsLoading={assignmentsLoading}
          onLoadAssignments={() => void loadAssignments()}
          hasMoreReceiptsByAssignment={receiptsHasMoreByAssignment}
          receiptsLoading={receiptsLoading}
          onSource={onSource}
          onLoadReceipts={(assignmentId) => void loadReceipts(assignmentId)}
        />

        <ResultSection
          results={desk.results ?? []}
          hasMore={resultsHasMore}
          loading={resultsLoading}
          resultLoading={resultLoading}
          selectedResult={selectedResult}
          onInspect={(resultId) => void inspectResult(resultId)}
          onLoad={() => void loadResults()}
          onSource={onSource}
        />

        <ProfileSection title="Profile versions">
          <label
            className="office-field-label"
            htmlFor={`staff-version-${staffId}`}
          >
            Inspect saved profile version
            <select
              id={`staff-version-${staffId}`}
              value={String(profile.version)}
              onChange={(event) => {
                const version = Number(event.target.value);
                setSelectedVersion(version);
                selectedVersionRef.current = version;
                void loadDesk(version, true);
              }}
            >
              {versionNumbers.map((version) => (
                <option key={version} value={version}>
                  Version {version}
                </option>
              ))}
            </select>
          </label>
          {!versionPage ? (
            <button
              type="button"
              className="office-history-button"
              disabled={versionsLoading}
              onClick={() => void loadVersions(0)}
            >
              {versionsLoading ? (
                <LoaderCircle size={16} className="spin" />
              ) : (
                <History size={16} />
              )}
              {versionsLoading ? "Loading versions…" : "Load profile versions"}
            </button>
          ) : null}
          {versionPage?.items?.length ? (
            <div className="office-version-list">
              {versionPage.items.map((version) => (
                <span
                  key={`${version.id}:${version.version}`}
                  className="office-version-chip"
                >
                  v{version.version} · {formatDate(version.created_at)}
                </span>
              ))}
            </div>
          ) : null}
          {hasMoreVersions ? (
            <button
              type="button"
              className="text-button"
              disabled={versionsLoading}
              onClick={() => void loadVersions()}
            >
              {versionsLoading ? "Loading…" : "Load older versions"}
            </button>
          ) : null}
        </ProfileSection>

        <OfficeMessages
          tenderId={tenderId}
          page={desk.messages}
          staffId={staffId}
          onSource={onSource}
          onOpenResult={onOpenResult}
          onOpenOutput={onOpenOutput}
          compact
        />
      </div>
    </section>
  );
}

function ProfileSection({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="office-desk-section">
      <h3>{title}</h3>
      {children}
    </section>
  );
}

function DetailList({ label, values }: { label: string; values: string[] }) {
  return (
    <div className="office-detail-list">
      <h4>{label}</h4>
      {values.length ? (
        <ul>
          {values.map((value, index) => (
            <li key={`${label}:${index}`}>{value}</li>
          ))}
        </ul>
      ) : (
        <p className="office-muted">None recorded.</p>
      )}
    </div>
  );
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value || "Not recorded."}</dd>
    </div>
  );
}

function WorkOrderSection({
  desk,
  hasMore,
  loading,
  onLoad,
}: {
  desk: StaffDeskRecord;
  hasMore: boolean;
  loading: boolean;
  onLoad: () => void;
}) {
  const workOrders = desk.work_orders ?? [];
  return (
    <ProfileSection title="Work orders">
      {workOrders.length ? (
        workOrders.map((record) => (
          <article key={record.id} className="office-work-order">
            <div className="office-row-heading">
              <strong>{record.work_order.goal}</strong>
              <span className="office-muted">Order {record.id}</span>
            </div>
            <p>{record.work_order.brief}</p>
            <DetailList
              label="Expected outputs"
              values={record.work_order.expected_outputs}
            />
            <DetailList
              label="Completion checks"
              values={record.work_order.completion_checks}
            />
            {record.work_order.source_ids.length ? (
              <p className="office-muted">
                Starting sources: {record.work_order.source_ids.join(", ")}
              </p>
            ) : null}
          </article>
        ))
      ) : (
        <p className="office-muted">
          No work orders are recorded for this colleague.
        </p>
      )}
      {hasMore ? (
        <button
          type="button"
          className="office-history-button"
          disabled={loading}
          onClick={onLoad}
        >
          {loading ? (
            <LoaderCircle size={16} className="spin" />
          ) : (
            <History size={16} />
          )}
          {loading ? "Loading older work orders…" : "Load older work orders"}
        </button>
      ) : null}
    </ProfileSection>
  );
}

function AssignmentSection({
  assignments,
  receipts,
  hasMoreAssignments,
  assignmentsLoading,
  onLoadAssignments,
  hasMoreReceiptsByAssignment,
  receiptsLoading,
  onSource,
  onLoadReceipts,
}: {
  assignments: StaffAssignment[];
  receipts: Schema<"StaffSourceReceipt">[];
  hasMoreAssignments: boolean;
  assignmentsLoading: boolean;
  onLoadAssignments: () => void;
  hasMoreReceiptsByAssignment: Record<string, boolean>;
  receiptsLoading: boolean;
  onSource: (source: SourceSelection) => void;
  onLoadReceipts: (assignmentId: string) => void;
}) {
  return (
    <ProfileSection title="Assignments and source inspections">
      {assignments.length ? (
        assignments.map((assignment) => {
          const assignmentReceipts = receipts.filter(
            (receipt) => receipt.assignment_id === assignment.id,
          );
          return (
            <article key={assignment.id} className="office-assignment">
              <div className="office-row-heading">
                <strong>{assignmentLabel(assignment.status)}</strong>
                <span className="office-muted">Assignment {assignment.id}</span>
              </div>
              <p>{assignment.detail || "No assignment detail is recorded."}</p>
              <dl className="office-facts-grid">
                <Fact
                  label="Profile version"
                  value={`Version ${assignment.staff_version}`}
                />
                <Fact label="Work order" value={assignment.work_order_id} />
              </dl>
              {assignment.result_id ? (
                <p className="office-muted">
                  Draft result: {assignment.result_id}
                </p>
              ) : null}
              <div className="office-receipt-list">
                {assignmentReceipts.length ? (
                  assignmentReceipts.map((receipt) => (
                    <button
                      key={receipt.id}
                      type="button"
                      className="office-reference-link"
                      onClick={() =>
                        onSource({
                          sourceId: receipt.source_id,
                          artifactId: receipt.artifact_id,
                          version: receipt.artifact_version,
                          contentHash: receipt.content_hash,
                          page: receipt.page ?? undefined,
                        })
                      }
                    >
                      <FileText size={15} aria-hidden="true" />
                      <span>
                        {receipt.source_id} · {receipt.locator} ·{" "}
                        {receipt.method}
                      </span>
                    </button>
                  ))
                ) : (
                  <p className="office-muted">
                    No source inspections are recorded for this assignment.
                  </p>
                )}
              </div>
              {hasMoreReceiptsByAssignment[assignment.id] ? (
                <button
                  type="button"
                  className="text-button"
                  disabled={receiptsLoading}
                  onClick={() => onLoadReceipts(assignment.id)}
                >
                  {receiptsLoading
                    ? "Loading inspections…"
                    : "Load more source inspections"}
                </button>
              ) : null}
            </article>
          );
        })
      ) : (
        <p className="office-muted">
          No assignments are recorded for this colleague.
        </p>
      )}
      {hasMoreAssignments ? (
        <button
          type="button"
          className="office-history-button"
          disabled={assignmentsLoading}
          onClick={onLoadAssignments}
        >
          {assignmentsLoading ? (
            <LoaderCircle size={16} className="spin" />
          ) : (
            <History size={16} />
          )}
          {assignmentsLoading
            ? "Loading older assignments…"
            : "Load older assignments"}
        </button>
      ) : null}
    </ProfileSection>
  );
}

function ResultSection({
  results,
  hasMore,
  loading,
  resultLoading,
  selectedResult,
  onInspect,
  onLoad,
  onSource,
}: {
  results: StaffResult[];
  hasMore: boolean;
  loading: boolean;
  resultLoading: boolean;
  selectedResult: StaffResult | null;
  onInspect: (resultId: string) => void;
  onLoad: () => void;
  onSource: (source: SourceSelection) => void;
}) {
  return (
    <ProfileSection title="Draft results">
      <p className="office-draft-note">
        <FileCheck2 size={16} aria-hidden="true" /> Staff results are drafts for
        engineer review. A source receipt shows what was inspected; it is not
        engineer approval.
      </p>
      {results.length ? (
        results.map((result) => (
          <article key={result.id} className="office-result">
            <div className="office-row-heading">
              <strong>{result.office_output.summary}</strong>
              <span
                className={`office-currentness office-currentness-${result.currentness}`}
              >
                {result.currentness === "current"
                  ? "Current draft"
                  : "Needs review"}
              </span>
            </div>
            <p className="office-muted">
              Result {result.id} · saved {formatDate(result.created_at)}
            </p>
            <button
              type="button"
              className="office-reference-link"
              disabled={resultLoading}
              onClick={() => onInspect(result.id)}
            >
              <FileText size={15} aria-hidden="true" />
              {resultLoading ? "Opening draft…" : "Open exact draft result"}
            </button>
            {result.office_output.source_ids?.length ? (
              <div className="office-source-id-list">
                <strong>Sources used</strong>
                {result.office_output.source_ids.map((sourceId) => (
                  <button
                    key={sourceId}
                    type="button"
                    className="text-button"
                    onClick={() => onSource({ sourceId })}
                  >
                    {sourceId}
                  </button>
                ))}
              </div>
            ) : null}
          </article>
        ))
      ) : (
        <p className="office-muted">
          No draft results are recorded for this colleague.
        </p>
      )}
      {selectedResult ? (
        <ResultDetail result={selectedResult} onSource={onSource} />
      ) : null}
      {hasMore ? (
        <button
          type="button"
          className="office-history-button"
          disabled={loading}
          onClick={onLoad}
        >
          {loading ? (
            <LoaderCircle size={16} className="spin" />
          ) : (
            <History size={16} />
          )}
          {loading ? "Loading older results…" : "Load older results"}
        </button>
      ) : null}
    </ProfileSection>
  );
}

function ResultDetail({
  result,
  onSource,
}: {
  result: StaffResult;
  onSource: (source: SourceSelection) => void;
}) {
  return <StaffDraftContent result={result} onSource={onSource} compact />;
}

function assignmentLabel(status: StaffAssignment["status"]) {
  const labels: Record<StaffAssignment["status"], string> = {
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

function formatDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(date);
}

function mergeDesk(
  current: StaffDeskRecord,
  next: StaffDeskRecord,
): StaffDeskRecord {
  return {
    ...next,
    version_numbers: [
      ...new Set([
        ...(current.version_numbers ?? []),
        ...(next.version_numbers ?? []),
      ]),
    ].sort((left, right) => right - left),
    work_orders: mergeRecords(
      current.work_orders ?? [],
      next.work_orders ?? [],
    ),
    assignments: mergeRecords(
      current.assignments ?? [],
      next.assignments ?? [],
    ),
    results: mergeRecords(current.results ?? [], next.results ?? []),
    receipts: mergeRecords(current.receipts ?? [], next.receipts ?? []),
    messages: {
      items: mergeRecords(current.messages.items, next.messages.items),
      next_cursor: next.messages.next_cursor ?? current.messages.next_cursor,
    },
  };
}

function latestAssignment(assignments: StaffAssignment[]) {
  return (
    [...assignments].sort((left, right) =>
      right.updated_at.localeCompare(left.updated_at),
    )[0] ?? null
  );
}

function mergeRecords<T extends { id: string; created_at: string }>(
  current: T[],
  additions: T[],
) {
  const values = new Map<string, T>();
  for (const item of [...current, ...additions]) values.set(item.id, item);
  return [...values.values()].sort((left, right) =>
    left.created_at.localeCompare(right.created_at),
  );
}
