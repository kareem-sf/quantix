import {
  Archive,
  Bot,
  ChevronRight,
  CircleAlert,
  DatabaseBackup,
  FileText,
  Folder,
  Info,
  ListChecks,
  LoaderCircle,
  MessageSquare,
  MoreHorizontal,
  Pencil,
  PanelRightOpen,
  Paperclip,
  Plus,
  Search,
  Send,
  Settings,
  ShieldAlert,
  ShieldCheck,
  Trash2,
  Undo2,
  Users,
  Wrench,
} from "lucide-react";
import { AnimatePresence, LayoutGroup, m } from "motion/react";
import {
  type KeyboardEvent as ReactKeyboardEvent,
  type RefObject,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import type { ManagerWorkspaceProjection } from "./bindings/ManagerWorkspaceProjection";
import type { ManagerWorkspaceTender } from "./bindings/ManagerWorkspaceTender";
import type { ApplicationSettingsView } from "./bindings/ApplicationSettingsView";
import type { DeletionReceipt } from "./bindings/DeletionReceipt";
import type { StartupReconciliationReport } from "./bindings/StartupReconciliationReport";
import type { TrashedTenderRecord } from "./bindings/TrashedTenderRecord";
import type { GeneralApplicationPreferences } from "./bindings/GeneralApplicationPreferences";
import type { RuntimePreparationProgress } from "./bindings/RuntimePreparationProgress";
import type { RuntimeReadiness } from "./bindings/RuntimeReadiness";
import type { SetupIssue } from "./bindings/SetupIssue";
import type { TenderOfficeMessage } from "./bindings/TenderOfficeMessage";
import type { TenderPackageSourceKind } from "./bindings/TenderPackageSourceKind";
import type { WorkspaceCurrentAction } from "./bindings/WorkspaceCurrentAction";
import type { PackageIntakeProgress } from "./bindings/PackageIntakeProgress";
import type { WorkspaceSearchProjection } from "./bindings/WorkspaceSearchProjection";
import type { WorkspaceSearchHit } from "./bindings/WorkspaceSearchHit";
import type { WorkspaceMessageReference } from "./bindings/WorkspaceMessageReference";
import type { AgentRunInspection } from "./bindings/AgentRunInspection";
import type { WorkspaceTaskRow } from "./bindings/WorkspaceTaskRow";
import type { WorkspaceTaskState } from "./bindings/WorkspaceTaskState";
import type { ProductionArtifactVersion } from "./bindings/ProductionArtifactVersion";
import type { ProductionReview } from "./bindings/ProductionReview";
import type { ProductionReviewFinding } from "./bindings/ProductionReviewFinding";
import type { ProductionTaskReviewInspection } from "./bindings/ProductionTaskReviewInspection";
import type { TenderProductionInspection } from "./bindings/TenderProductionInspection";
import type { TenderRecordEvidence } from "./bindings/TenderRecordEvidence";
import type { TenderRecordInspection } from "./bindings/TenderRecordInspection";
import type { ChangeAssessmentClassification } from "./bindings/ChangeAssessmentClassification";
import type { ChangeAssessmentPage } from "./bindings/ChangeAssessmentPage";
import type { ArtifactVersionSummary } from "./bindings/ArtifactVersionSummary";
import { ApplicationSettings } from "./ApplicationSettings";
import { exactApplicationAiSelectionIsReady } from "./applicationAiSelectionReadiness";
import { notifyAttentionRequired } from "./applicationNotifications";
import { ControlledCalculationView } from "./ControlledCalculationView";
import { TenderEstimateReview } from "./TenderEstimateReview";
import { TenderFocusedAction } from "./TenderFocusedAction";
import { TenderRfiReview } from "./TenderRfiReview";
import {
  type EvidenceReviewConflict,
  type EvidenceReviewTarget,
  TenderEvidenceReview,
} from "./TenderEvidenceReview";
import {
  type TenderRecordDecisionTarget,
  TenderRecordDecision,
} from "./TenderRecordDecision";
import { QuantixMark } from "./QuantixMark";
import { QuantixWindow } from "./QuantixWindow";
import { quantixSmoothEase } from "./motion/motionPresets";
import type { WindowTitleBarMenu } from "./WindowTitleBar";
import {
  WorkspaceContextPanel,
  type WorkspaceContextPresentation,
} from "./WorkspaceContextPanel";
import { WorkspaceRecoveryCenter } from "./WorkspaceRecoveryCenter";
import { QuantixDialog, QuantixMenu } from "./ui";
import {
  applyGeneralApplicationPreferences,
  DEFAULT_GENERAL_APPLICATION_PREFERENCES,
} from "./applicationPreferences";
import {
  archiveTender,
  cancelRuntimePreparation,
  chooseAndImportTenderPackage,
  createTenderBackup,
  decideChangeAssessment,
  ensureQuantixSetup,
  inspectArtifactVersions,
  inspectChangeAssessments,
  inspectManagerWorkspace,
  inspectDeletionReceipts,
  inspectApplicationSettings,
  inspectAgentRun,
  inspectRuntimePreparationProgress,
  inspectRuntimeReadiness,
  inspectPackageIntakeProgress,
  cancelPackageIntake,
  inspectStartupReconciliation,
  inspectTrashedTenders,
  rebindManagerIntakeProvider,
  recordEngineerWorkspaceMessage,
  repairRuntimeReadiness,
  resumeManagerIntakes,
  searchManagerWorkspace,
  reviseTender,
  retryManagerIntake,
  restoreArchivedTender,
  restoreTrashedTender,
  selectManagerWorkspaceTender,
  startManagerTender,
  approveProductionFindingException,
  inspectProductionTaskReview,
  inspectTenderProduction,
  inspectTenderRecord,
  interruptAgentRun,
  runProductionTask,
  trashRecoveryRequiredTender,
  trashTender,
  purgeRecoveryRequiredTender,
  purgeTrashedTender,
} from "./quantixHost";
import "./ManagerWorkspace.css";

type WorkspaceView = "manager" | "work" | "team" | "files";
type SettingsSection = "general" | "ai" | "about";
type WorkspaceSurface = WorkspaceView | "settings" | "retention";
type WorkspaceLocation = {
  tenderId: string | null;
  surface: WorkspaceSurface;
  settingsSection?: SettingsSection;
};
type NavigationHistory = {
  entries: WorkspaceLocation[];
  index: number;
};
type RetentionAction = {
  kind: "archive" | "restore" | "trash";
  tender: ManagerWorkspaceTender;
} | null;
type TrashAction = {
  kind: "restore" | "purge";
  record: TrashedTenderRecord;
} | null;
type RecoveryRequestedAction = "move_to_trash" | "delete_permanently" | null;
type EvidenceReviewState = {
  target: EvidenceReviewTarget;
  conflicts: EvidenceReviewConflict[];
};
type RecordDecisionState = {
  focusTarget: TenderRecordDecisionTarget | null;
};

type WorkspaceOperationKind =
  | "start_tender"
  | "add_package"
  | "select_tender"
  | "save_message"
  | "retry_intake"
  | "rebind_intake"
  | "rename_tender"
  | "create_backup"
  | "retention"
  | "trash"
  | "run_task"
  | "stop_task"
  | "request_change";

type WorkspaceOperation = {
  kind: WorkspaceOperationKind;
  label: string;
  startedAt: number;
};

type PackageTaskState = {
  kind: "start_tender" | "add_package";
  sourceKind: TenderPackageSourceKind;
  progress: PackageIntakeProgress | null;
  error: string | null;
  cancelled: boolean;
};

type WorkspaceOperationFailure = {
  message: string;
  label: string;
};

interface ManagerWorkspaceProps {
  initialProjection?: ManagerWorkspaceProjection;
  initialPreferences?: GeneralApplicationPreferences;
  setupWarnings?: SetupIssue[];
}

type CapabilityStatus = "checking" | "ready" | "unavailable";

const RUNTIME_READINESS_RETRY_DELAY_MS = 500;
const TENDER_SELECTION_TIMEOUT_MS = 10_000;
const TENDER_SELECTION_TIMEOUT_MESSAGE =
  "This Tender took too long to open. Please try again.";
const FOCUSED_ACTION_KINDS = [
  "review_bid_decision",
  "prepare_work_plan",
  "review_work_plan",
] as const;
type FocusedActionKind = (typeof FOCUSED_ACTION_KINDS)[number];

function isFocusedActionKind(
  kind: WorkspaceCurrentAction["kind"],
): kind is FocusedActionKind {
  return (FOCUSED_ACTION_KINDS as readonly string[]).includes(kind);
}

const RFI_ACTION_KINDS = [
  "draft_external_rfi",
  "review_external_rfi",
  "interpret_external_rfi_response",
] as const;
type RfiActionKind = (typeof RFI_ACTION_KINDS)[number];

function isRfiActionKind(
  kind: WorkspaceCurrentAction["kind"],
): kind is RfiActionKind {
  return (RFI_ACTION_KINDS as readonly string[]).includes(kind);
}

const ESTIMATE_REVIEW_ACTION_KINDS = ["review_basis_of_estimate"] as const;
type EstimateReviewActionKind = (typeof ESTIMATE_REVIEW_ACTION_KINDS)[number];

function isEstimateReviewActionKind(
  kind: WorkspaceCurrentAction["kind"],
): kind is EstimateReviewActionKind {
  return (ESTIMATE_REVIEW_ACTION_KINDS as readonly string[]).includes(kind);
}

const LOADING_TITLE_BAR_MENUS: readonly WindowTitleBarMenu[] = [
  "File",
  "Edit",
  "View",
  "Help",
].map((label) => ({
  id: label.toLowerCase(),
  label,
  items: [
    {
      id: `${label.toLowerCase()}-loading`,
      label: "Available after the workspace opens",
      disabled: true,
    },
  ],
}));

function sameWorkspaceLocation(
  left: WorkspaceLocation,
  right: WorkspaceLocation,
): boolean {
  return (
    left.tenderId === right.tenderId &&
    left.surface === right.surface &&
    left.settingsSection === right.settingsSection
  );
}

function latestManagerQuestion(
  projection: ManagerWorkspaceProjection | null,
): TenderOfficeMessage | null {
  const messages = projection?.conversation?.messages ?? [];
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (message.author === "manager" && message.kind === "question") {
      return message;
    }
  }
  return null;
}

function citedRecordTargets(
  projection: ManagerWorkspaceProjection | null,
): TenderRecordDecisionTarget[] {
  const question = latestManagerQuestion(projection);
  if (!question) return [];
  return question.references
    .filter((reference) => reference.kind === "tender_record")
    .map((reference) => ({
      recordId: reference.reference,
      version: reference.version,
    }));
}

function evidenceMatchesTarget(
  evidence: TenderRecordEvidence,
  target: EvidenceReviewTarget,
) {
  return (
    evidence.reference.artifact_id === target.artifactId &&
    evidence.reference.version === target.version &&
    (target.ordinal === null || evidence.reference.ordinal === target.ordinal)
  );
}

function evidenceConflictsForTarget(
  records: TenderRecordInspection[],
  target: EvidenceReviewTarget,
): EvidenceReviewConflict[] {
  const conflicts: EvidenceReviewConflict[] = [];
  const seen = new Set<string>();
  for (const record of records) {
    for (const contradiction of record.contradictions) {
      if (
        !contradiction.evidence.some((evidence) =>
          evidenceMatchesTarget(evidence, target),
        )
      ) {
        continue;
      }
      for (const evidence of contradiction.evidence) {
        if (evidenceMatchesTarget(evidence, target)) continue;
        const key = `${evidence.reference.artifact_id}:${evidence.reference.version}:${evidence.reference.ordinal}`;
        if (seen.has(key)) continue;
        seen.add(key);
        conflicts.push({
          artifactId: evidence.reference.artifact_id,
          version: evidence.reference.version,
          ordinal: evidence.reference.ordinal,
          label: evidence.package_path,
        });
      }
    }
  }
  return conflicts;
}

function initialNavigationHistory(
  projection?: ManagerWorkspaceProjection,
): NavigationHistory {
  const tenderId = projection?.selected_tender?.tender_id ?? null;
  return { entries: [{ tenderId, surface: "manager" }], index: 0 };
}

function setupWarningCopy(issue: SetupIssue): string {
  if (issue === "storage_permissions_unverified") {
    return "Quantix could not verify that Application Home is private to this device user.";
  }
  return "Review the local workspace security warning before adding confidential Tender material.";
}

function startupReconciliationNotice(
  report: StartupReconciliationReport,
): { summary: string; details: string } | null {
  const sentences: string[] = [];
  const {
    removed_tender_candidates,
    interrupted_backup_operations,
    interrupted_recovery_operations,
    completed_retention_operations,
  } = report;
  if (removed_tender_candidates === 1) {
    sentences.push(
      "One unfinished Tender registration was cleaned up during startup.",
    );
  } else if (removed_tender_candidates > 1) {
    sentences.push(
      `${removed_tender_candidates} unfinished Tender registrations were cleaned up during startup.`,
    );
  }
  if (interrupted_backup_operations === 1) {
    sentences.push("An interrupted backup was safely closed.");
  } else if (interrupted_backup_operations > 1) {
    sentences.push(
      `${interrupted_backup_operations} interrupted backups were safely closed.`,
    );
  }
  if (interrupted_recovery_operations === 1) {
    sentences.push("An interrupted recovery was safely closed.");
  } else if (interrupted_recovery_operations > 1) {
    sentences.push(
      `${interrupted_recovery_operations} interrupted recoveries were safely closed.`,
    );
  }
  if (completed_retention_operations === 1) {
    sentences.push(
      "An interrupted Archived & Trash change was finished safely.",
    );
  } else if (completed_retention_operations > 1) {
    sentences.push(
      `${completed_retention_operations} interrupted Archived & Trash changes were finished safely.`,
    );
  }
  if (sentences.length === 0) return null;
  const details = [
    `Unfinished Tender registrations removed: ${removed_tender_candidates}`,
    `Interrupted backups closed: ${interrupted_backup_operations}`,
    `Interrupted recoveries closed: ${interrupted_recovery_operations}`,
    `Archived & Trash changes finished: ${completed_retention_operations}`,
  ].join(" · ");
  return { summary: sentences.join(" "), details };
}

function formatRuntimeBytes(bytes: number | bigint) {
  const value = Number(bytes);
  if (value < 1024 * 1024) {
    return `${Math.max(0, Math.round(value / 1024))} KB`;
  }
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

function mediaQueryMatches(query: string) {
  return (
    typeof window.matchMedia === "function" && window.matchMedia(query).matches
  );
}

function useMediaQuery(query: string) {
  const [matches, setMatches] = useState(() => mediaQueryMatches(query));

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const mediaQuery = window.matchMedia(query);
    const updateMatch = (event: MediaQueryListEvent) =>
      setMatches(event.matches);

    setMatches(mediaQuery.matches);
    mediaQuery.addEventListener("change", updateMatch);
    return () => mediaQuery.removeEventListener("change", updateMatch);
  }, [query]);

  return matches;
}

const phaseLabel: Record<ManagerWorkspaceTender["phase"], string> = {
  intake: "Intake",
  bid_decision: "Bid decision",
  tender_planning: "Planning",
  active_production: "In progress",
  integrated_review: "Integrated review",
  change_assessment: "Change review",
  package_production: "Package production",
  final_review: "Final review",
  declined: "Declined",
};

function readableError(error: unknown): string {
  if (
    error instanceof Error &&
    error.message === TENDER_SELECTION_TIMEOUT_MESSAGE
  ) {
    return TENDER_SELECTION_TIMEOUT_MESSAGE;
  }
  if (typeof error === "object" && error !== null && "code" in error) {
    switch (error.code) {
      case "local_document_tools_required":
        return "Prepare document tools before continuing local processing.";
      case "ai_provider_required":
        return "The approved AI Provider is not ready. Open Settings to reconnect it or explicitly choose another live model.";
      case "recovery_required":
        return "This Tender needs recovery before it can be opened.";
      case "invalid_command":
        return "Quantix could not use that selection.";
      case "store_unavailable":
        return "The local Tender record is temporarily unavailable.";
    }
    if (typeof error.code === "string") {
      return `Quantix could not complete that action (${error.code.replace(/_/g, " ")}).`;
    }
  }
  if (typeof error === "string" && error.trim()) return error;
  return "Quantix could not complete that action.";
}

async function selectManagerWorkspaceTenderWithTimeout(
  tenderId: string,
): Promise<ManagerWorkspaceProjection> {
  let timeout: number | null = null;
  try {
    return await Promise.race([
      selectManagerWorkspaceTender(tenderId),
      new Promise<never>((_, reject) => {
        timeout = window.setTimeout(
          () => reject(new Error(TENDER_SELECTION_TIMEOUT_MESSAGE)),
          TENDER_SELECTION_TIMEOUT_MS,
        );
      }),
    ]);
  } finally {
    if (timeout !== null) window.clearTimeout(timeout);
  }
}

function TenderButton({
  tender,
  selected,
  busy,
  onSelect,
  onCreateBackup,
  onInspectBackups,
  onRename,
  onArchive,
  onTrash,
  onPurge,
}: {
  tender: ManagerWorkspaceTender;
  selected: boolean;
  busy: boolean;
  onSelect: () => void;
  onCreateBackup: () => void;
  onInspectBackups: () => void;
  onRename: () => void;
  onArchive: () => void;
  onTrash: () => void;
  onPurge: () => void;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const unavailableReason =
    "Available after decline, withdrawal, expiry, or final approval when no protected work is active.";

  return (
    <div
      className="manager-workspace__tender-row"
      onContextMenu={(event) => {
        event.preventDefault();
        if (!busy) setMenuOpen(true);
      }}
      onKeyDown={(event) => {
        if (
          !busy &&
          ((event.shiftKey && event.key === "F10") ||
            event.key === "ContextMenu")
        ) {
          event.preventDefault();
          setMenuOpen(true);
        }
      }}
    >
      <button
        className="manager-workspace__tender"
        type="button"
        aria-current={selected ? "page" : undefined}
        aria-label={
          tender.state === "recovery_required"
            ? `Open recovery center for ${tender.name}`
            : undefined
        }
        disabled={busy}
        onClick={onSelect}
      >
        <span>
          <strong>{tender.name}</strong>
          <small>
            {tender.state === "recovery_required"
              ? "Needs recovery"
              : tender.state === "archived"
                ? "Archived"
                : phaseLabel[tender.phase]}
          </small>
        </span>
        {tender.needs_engineer ? (
          <span className="manager-workspace__attention">Needs you</span>
        ) : null}
        {selected ? (
          <m.span
            className="manager-workspace__selection-indicator"
            layoutId="active-tender-indicator"
            aria-hidden="true"
          />
        ) : null}
      </button>
      <QuantixMenu
        label={`Manage ${tender.name}`}
        isOpen={menuOpen}
        onOpenChange={setMenuOpen}
        items={[
          ...(tender.state === "recovery_required"
            ? [
                {
                  id: "recovery",
                  label: "Inspect recovery",
                  icon: <Wrench size={15} aria-hidden="true" />,
                  disabled: busy,
                  description:
                    "Review the integrity finding and verified recovery options.",
                },
              ]
            : [
                {
                  id: "create_backup",
                  label: "Create verified backup",
                  icon: <DatabaseBackup size={15} aria-hidden="true" />,
                  disabled: busy,
                  description: "Verify this Tender and save a restorable copy.",
                },
                {
                  id: "inspect_backups",
                  label: "Inspect backups",
                  icon: <ShieldCheck size={15} aria-hidden="true" />,
                  disabled: busy,
                  description:
                    "Review verified backups and recovery candidates.",
                },
              ]),
          {
            id: "rename",
            label: "Rename",
            icon: <Pencil size={15} aria-hidden="true" />,
            disabled: busy || tender.state === "recovery_required",
            description:
              tender.state === "recovery_required"
                ? "Recover this Tender before changing its name."
                : undefined,
          },
          {
            id: "archive",
            label: "Archive",
            icon: <Archive size={15} aria-hidden="true" />,
            disabled: busy || !tender.can_archive,
            description: tender.can_archive
              ? undefined
              : tender.state === "recovery_required"
                ? "Recovery-required Tenders cannot be archived."
                : unavailableReason,
          },
          {
            id: "trash",
            label: "Move to Trash",
            icon: <Trash2 size={15} aria-hidden="true" />,
            disabled: busy || !tender.can_delete,
            description: tender.can_delete ? undefined : unavailableReason,
          },
          ...(tender.state === "recovery_required"
            ? [
                {
                  id: "purge",
                  label: "Delete Permanently",
                  icon: <Trash2 size={15} aria-hidden="true" />,
                  disabled: busy || !tender.can_delete,
                  description:
                    "Irreversibly delete every Quantix-controlled copy after exact-name confirmation.",
                },
              ]
            : []),
        ]}
        onAction={(action) => {
          setMenuOpen(false);
          if (action === "recovery") onSelect();
          else if (action === "create_backup") onCreateBackup();
          else if (action === "inspect_backups") onInspectBackups();
          else if (action === "rename") onRename();
          else if (action === "archive") onArchive();
          else if (action === "trash") onTrash();
          else if (action === "purge") onPurge();
        }}
      >
        <MoreHorizontal size={17} aria-hidden="true" />
      </QuantixMenu>
    </div>
  );
}

const PACKAGE_STAGE_LABELS: Record<string, string> = {
  checking_source: "Checking the selected package",
  reading_package: "Copying and verifying documents",
  recording_documents: "Recording documents",
  opening_workspace: "Opening the Tender workspace",
};

function formatElapsed(epochMs: number | bigint | null | undefined): string {
  if (epochMs == null) return "";
  const value = Number(epochMs);
  if (!Number.isFinite(value)) return "";
  const seconds = Math.max(0, Math.floor((Date.now() - value) / 1000));
  return seconds < 60
    ? `${seconds}s`
    : `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
}

function PackageTaskPanel({
  task,
  onCancel,
  onChooseAgain,
}: {
  task: PackageTaskState;
  onCancel: () => void;
  onChooseAgain: () => void;
}) {
  const progress = task.progress;
  const stage = progress?.stage ?? "checking_source";
  const stageLabel = PACKAGE_STAGE_LABELS[stage] ?? "Preparing the package";
  const sourceName = progress?.source_name ?? "selected package";
  const processed = progress?.processed_count ?? 0;
  const discovered = progress?.discovered_count ?? 0;
  const registered = progress?.registered_count ?? 0;
  const exceptions = progress?.exception_count ?? 0;
  const total = progress?.total_count;
  const elapsed = formatElapsed(progress?.started_at_epoch_ms);
  const stalled = Boolean(
    progress?.updated_at_epoch_ms != null &&
    Date.now() - Number(progress.updated_at_epoch_ms) > 4_000,
  );
  const isWaiting = !progress;
  const canCancel =
    Boolean(progress?.cancellable) &&
    !progress?.cancellation_requested &&
    !task.cancelled;

  return (
    <section
      className="workspace-operation-panel"
      aria-labelledby="package-task-title"
    >
      <span className="start-tender__icon" aria-hidden="true">
        <Folder size={22} />
      </span>
      <h2 id="package-task-title">
        {task.error
          ? "Package could not be opened"
          : isWaiting
            ? "Choose a Tender Package"
            : stageLabel}
      </h2>
      <p className="workspace-operation-panel__summary">
        {task.error ??
          (isWaiting
            ? "Waiting for you to choose a folder or ZIP file."
            : `Working with ${sourceName}.`)}
      </p>
      {!task.error ? (
        <>
          <div
            className="workspace-operation-panel__stage"
            role="status"
            aria-live="polite"
          >
            <strong>
              {progress?.cancellation_requested
                ? "Cancel requested — finishing the current file"
                : stageLabel}
            </strong>
            {progress?.current_relative_path ? (
              <code title={progress.current_relative_path}>
                {progress.current_relative_path}
              </code>
            ) : null}
            {stalled && progress?.current_relative_path ? (
              <small>
                Still working on this file ·{" "}
                {formatElapsed(progress.updated_at_epoch_ms)} since the last
                update
              </small>
            ) : null}
          </div>
          {total != null && total > 0 ? (
            <progress
              className="workspace-operation-panel__progress"
              value={processed}
              max={total}
              aria-label={`${processed} of ${total} documents processed`}
            />
          ) : (
            <div
              className="workspace-operation-panel__indeterminate"
              aria-label="Package processing in progress"
            >
              <span />
            </div>
          )}
          <div
            className="workspace-operation-panel__counts"
            aria-label="Package progress"
          >
            <span>{discovered.toLocaleString()} discovered</span>
            <span>
              {processed.toLocaleString()}
              {total != null ? ` of ${total.toLocaleString()}` : ""} processed
            </span>
            <span>{registered.toLocaleString()} recorded</span>
            {exceptions > 0 ? (
              <span>{exceptions.toLocaleString()} exceptions</span>
            ) : null}
          </div>
          {elapsed ? (
            <small className="workspace-operation-panel__elapsed">
              Elapsed {elapsed}
            </small>
          ) : null}
          <button
            type="button"
            className="manager-workspace__secondary"
            disabled={!canCancel}
            onClick={onCancel}
          >
            {progress?.cancellation_requested
              ? "Finishing cancellation…"
              : "Cancel"}
          </button>
        </>
      ) : (
        <button
          type="button"
          className="manager-workspace__primary"
          onClick={onChooseAgain}
        >
          Choose package again
        </button>
      )}
    </section>
  );
}

function NavigationTaskPanel({ label }: { label: string }) {
  return (
    <section
      className="workspace-operation-panel workspace-operation-panel--navigation"
      role="status"
      aria-live="polite"
    >
      <LoaderCircle
        className="workspace-operation-panel__spinner"
        size={24}
        aria-hidden="true"
      />
      <h2>{label}…</h2>
      <p className="workspace-operation-panel__summary">
        Loading the selected Tender workspace.
      </p>
    </section>
  );
}

function StartTender({
  action,
  busy,
  onStart,
  focusRef,
}: {
  action: WorkspaceCurrentAction;
  busy: boolean;
  onStart: (kind: TenderPackageSourceKind) => Promise<boolean>;
  focusRef?: RefObject<HTMLButtonElement | null>;
}) {
  const actionRef = useRef<HTMLButtonElement>(null);

  return (
    <div className="start-tender">
      <span className="start-tender__icon" aria-hidden="true">
        <Folder size={22} />
      </span>
      <h2>{action.title}</h2>
      <p>{action.summary}</p>
      <button
        ref={(node) => {
          actionRef.current = node;
          if (focusRef) focusRef.current = node;
        }}
        className="manager-workspace__primary"
        type="button"
        disabled={busy}
        onClick={async () => {
          if (!(await onStart("directory"))) actionRef.current?.focus();
        }}
      >
        {busy ? "Opening package…" : action.action_label}
      </button>
    </div>
  );
}

const MESSAGE_KIND_LABELS: Partial<
  Record<TenderOfficeMessage["kind"], string>
> = {
  status: "Status",
  question: "Question",
  finding: "Finding",
  handoff: "Handoff",
  blocker: "Blocker",
  output: "Output",
};

function Message({
  message,
  meaningful,
  onOpenReference,
}: {
  message: TenderOfficeMessage;
  meaningful: boolean;
  onOpenReference: (reference: WorkspaceMessageReference) => void;
}) {
  const isEngineer = message.author === "engineer";
  const isSystem = message.author === "system";
  const kindLabel = MESSAGE_KIND_LABELS[message.kind];
  return (
    <article
      id={`manager-message-${message.message_id}`}
      className={`manager-message manager-message--${message.author}${meaningful ? " is-meaningful" : ""}`}
      tabIndex={-1}
    >
      <div className="manager-message__identity">
        <span className="manager-message__avatar" aria-hidden="true">
          {isEngineer ? "You" : "Q"}
        </span>
        <div>
          <strong>
            {isEngineer ? "You" : isSystem ? "Quantix" : "Tendering Manager"}
          </strong>
          {kindLabel ? (
            <span className="manager-message__kind">{kindLabel}</span>
          ) : null}
          <time dateTime={message.created_at}>
            {new Intl.DateTimeFormat(undefined, {
              hour: "numeric",
              minute: "2-digit",
            }).format(new Date(message.created_at))}
          </time>
        </div>
      </div>
      <div className="manager-message__body">
        <p>{message.body}</p>
        {message.references.length > 0 ? (
          <details className="manager-message__references">
            <summary>
              {message.references.length} reference
              {message.references.length === 1 ? "" : "s"}
            </summary>
            <ul>
              {message.references.map((reference, index) => {
                const opensReview = reference.kind === "source_evidence";
                const opensDecision = reference.kind === "tender_record";
                const referenceBody = (
                  <>
                    <strong>{reference.label}</strong>
                    {reference.detail ? <span>{reference.detail}</span> : null}
                    <code>
                      {reference.reference} · v{reference.version}
                      {reference.evidence_ordinal
                        ? ` · evidence ${reference.evidence_ordinal}`
                        : ""}
                    </code>
                  </>
                );
                return (
                  <li
                    key={`${reference.kind}-${reference.reference}-${reference.version}-${reference.evidence_ordinal ?? 0}-${index}`}
                  >
                    {opensReview || opensDecision ? (
                      <button
                        type="button"
                        className="manager-message__reference"
                        onClick={() => onOpenReference(reference)}
                      >
                        {referenceBody}
                        <span className="manager-message__reference-action">
                          {opensReview ? "Review evidence" : "Review record"}
                        </span>
                      </button>
                    ) : (
                      referenceBody
                    )}
                  </li>
                );
              })}
            </ul>
          </details>
        ) : null}
      </div>
    </article>
  );
}

function ManagerView({
  projection,
  composer,
  onComposerChange,
  aiStatus,
  runtimeStatus,
  runtimeReadiness,
  runtimeProgress,
  runtimePreparing,
  runtimeNotice,
  busy,
  onImport,
  onPrepareRuntime,
  onDeferRuntime,
  onCancelRuntime,
  onRetry,
  onRebindProvider,
  onSend,
  onOpenSettings,
  onOpenAction,
  onOpenFocusedAction,
  onOpenRfiReview,
  onOpenEstimateReview,
  onOpenRecordDecision,
  onOpenChangeReview,
  canDecideCitedRecords,
  onOpenReference,
  onOpenSearch,
  contextRefs,
  onRemoveContext,
  readOnly,
}: {
  projection: ManagerWorkspaceProjection;
  composer: string;
  onComposerChange: (value: string) => void;
  aiStatus: CapabilityStatus;
  runtimeStatus: CapabilityStatus;
  runtimeReadiness: RuntimeReadiness | null;
  runtimeProgress: RuntimePreparationProgress | null;
  runtimePreparing: boolean;
  runtimeNotice: string | null;
  busy: boolean;
  onImport: (kind: TenderPackageSourceKind) => void;
  onPrepareRuntime: () => Promise<void>;
  onDeferRuntime: () => void;
  onCancelRuntime: () => Promise<void>;
  onRetry: () => Promise<void>;
  onRebindProvider: () => Promise<void>;
  onSend: (body: string) => Promise<boolean>;
  onOpenSettings: () => void;
  onOpenAction: () => void;
  onOpenFocusedAction: () => void;
  onOpenRfiReview: () => void;
  onOpenEstimateReview: () => void;
  onOpenRecordDecision: () => void;
  onOpenChangeReview: () => void;
  canDecideCitedRecords: boolean;
  onOpenReference: (reference: WorkspaceMessageReference) => void;
  onOpenSearch: () => void;
  contextRefs: WorkspaceMessageReference[];
  onRemoveContext: (reference: string) => void;
  readOnly: boolean;
}) {
  const [showEarlier, setShowEarlier] = useState(false);
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const conversationRef = useRef<HTMLDivElement>(null);
  const messages = projection.conversation?.messages ?? [];
  const visibleMessages = useMemo(() => {
    if (showEarlier) return messages;
    const recent = messages.slice(-8);
    const meaningful = messages.find(
      (message) =>
        message.message_id ===
        projection.conversation?.latest_meaningful_message_id,
    );
    if (
      !meaningful ||
      recent.some((message) => message.message_id === meaningful.message_id)
    ) {
      return recent;
    }
    return [meaningful, ...recent].sort(
      (left, right) => left.sequence - right.sequence,
    );
  }, [
    messages,
    projection.conversation?.latest_meaningful_message_id,
    showEarlier,
  ]);

  useEffect(() => {
    const conversation = conversationRef.current;
    if (!conversation || showEarlier) return;
    conversation.scrollTop = conversation.scrollHeight;
  }, [
    messages[messages.length - 1]?.sequence,
    projection.current_action.kind,
    showEarlier,
  ]);

  const send = async () => {
    const body = composer.trim();
    if (!body || busy) return;
    if (await onSend(body)) onComposerChange("");
  };

  const onKeyDown = (event: ReactKeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void send();
    }
  };

  const hasRegisteredDocuments = projection.files.tender_document_count > 0;
  const intakeNeedsDocumentTools =
    !projection.intake ||
    [
      "waiting_for_local_tools",
      "package_registered",
      "reading_documents",
    ].includes(projection.intake.stage);
  const documentToolsRequired =
    hasRegisteredDocuments &&
    intakeNeedsDocumentTools &&
    runtimeStatus !== "ready";
  const preparationActive =
    runtimePreparing || runtimeReadiness?.state === "preparing";
  const providerCheckPending =
    !documentToolsRequired &&
    aiStatus === "checking" &&
    projection.current_action.kind === "configure_ai_provider";
  const aiAvailable = projection.ai_execution?.readiness === "ready";
  const aiBlockers = projection.doctor_blockers.filter(
    (blocker) => blocker.area === "ai_execution",
  );
  const capabilityGaps =
    projection.capability_readiness?.state === "blocked"
      ? projection.capability_readiness.gaps
      : [];

  const openCurrentAction = () => {
    if (documentToolsRequired) {
      if (!preparationActive && runtimeStatus !== "checking") {
        void onPrepareRuntime();
      }
      return;
    }
    if (providerCheckPending) return;
    switch (projection.current_action.kind) {
      case "answer_manager_question":
        composerRef.current?.focus();
        break;
      case "retry_intake":
        void onRetry();
        break;
      case "configure_ai_provider":
        if (aiAvailable) void onRebindProvider();
        else onOpenSettings();
        break;
      case "review_bid_decision": {
        onOpenFocusedAction();
        break;
      }
      case "draft_external_rfi":
      case "review_external_rfi":
      case "interpret_external_rfi_response":
        onOpenRfiReview();
        break;
      case "review_basis_of_estimate":
        onOpenEstimateReview();
        break;
      case "review_change":
        onOpenChangeReview();
        break;
      case "prepare_work_plan":
      case "review_work_plan":
        onOpenFocusedAction();
        break;
      case "add_tender_package":
        onImport("directory");
        break;
      default:
        onOpenAction();
    }
  };

  const hasActionButton = [
    "add_tender_package",
    "review_intake",
    "configure_ai_provider",
    "answer_manager_question",
    "retry_intake",
    "review_bid_decision",
    "review_change",
    "prepare_work_plan",
    "review_work_plan",
    "review_work",
    ...RFI_ACTION_KINDS,
    ...ESTIMATE_REVIEW_ACTION_KINDS,
  ].includes(projection.current_action.kind);
  const intakeWorking =
    !documentToolsRequired &&
    aiAvailable &&
    projection.intake?.status === "working";
  const intakePaused =
    (documentToolsRequired || !aiAvailable) &&
    projection.intake?.status === "working";
  const activeRuntimeActivity = runtimeProgress?.activities.find(
    (activity) => activity.status === "active",
  );
  const currentActionTitle = documentToolsRequired
    ? preparationActive
      ? "Preparing document tools"
      : runtimeStatus === "checking"
        ? "Checking document tools"
        : "Prepare document tools"
    : providerCheckPending
      ? "Checking your AI connection"
      : projection.current_action.title;
  const currentActionSummary = documentToolsRequired
    ? preparationActive
      ? (activeRuntimeActivity?.detail ??
        "Quantix is preparing the approved local document environment. You can keep using the workspace or switch Tenders while this continues.")
      : runtimeStatus === "checking"
        ? "The registered Tender Package is safe while Quantix checks the local document environment."
        : "Prepare the managed Python environment and about 500 MB of approved document models under Application Home. This needs an internet connection and roughly 2 GB of free space; your registered Tender remains safe if you do this later."
    : providerCheckPending
      ? "The Tender Package is registered safely while Quantix checks the exact provider, model, and reasoning selection for future work."
      : projection.intake && !projection.current_action.requires_engineer
        ? projection.intake.summary
        : projection.current_action.summary;
  const showActionButton =
    (documentToolsRequired &&
      runtimeStatus !== "checking" &&
      !preparationActive) ||
    (!documentToolsRequired && !providerCheckPending && hasActionButton);
  const intakeProgress = projection.intake
    ? projection.intake.stage === "reading_documents" &&
      projection.intake.parseable_document_count > 0
      ? `${projection.intake.parsed_document_count.toLocaleString()} of ${projection.intake.parseable_document_count.toLocaleString()} supported documents safely read`
      : projection.intake.stage === "extracting_tender_facts" &&
          projection.intake.extraction_run_count > 0
        ? `${projection.intake.extraction_run_count.toLocaleString()} evidence batches completed`
        : null
    : null;
  const activityState = projection.current_action.requires_engineer
    ? "Needs you"
    : intakeWorking
      ? "Working"
      : intakePaused
        ? "Paused"
        : "Ready";
  const currentActionLabel = documentToolsRequired
    ? "Prepare document tools"
    : projection.current_action.kind === "configure_ai_provider" && !aiAvailable
      ? "Open Settings"
      : projection.current_action.action_label;
  const managerQuiet =
    !documentToolsRequired &&
    !providerCheckPending &&
    !preparationActive &&
    aiAvailable &&
    aiStatus !== "unavailable" &&
    !projection.intake &&
    projection.team.active_agent_runs === 0 &&
    !projection.current_action.requires_engineer &&
    !hasActionButton &&
    projection.doctor_blockers.length === 0;
  const capabilityGatesCurrentAction =
    capabilityGaps.length > 0 &&
    ["prepare_work_plan", "review_work_plan"].includes(
      projection.current_action.kind,
    );
  const capabilityGapList = capabilityGaps
    .map((gap) => gap.capability.replace(/_/g, " "))
    .join(", ");
  const showBlockerRecovery =
    aiBlockers.length > 0 &&
    !(
      showActionButton &&
      !aiAvailable &&
      projection.current_action.kind === "configure_ai_provider"
    );
  const sendSuggestion = (body: string) => {
    if (!busy) void onSend(body);
  };

  return (
    <div className="manager-view">
      {managerQuiet ? null : (
        <div className="manager-view__status">
          <span
            className={
              projection.team.active_agent_runs > 0 || intakeWorking
                ? "is-working"
                : ""
            }
          />
          <div>
            <strong>Tendering Manager</strong>
            <small>
              {documentToolsRequired
                ? preparationActive
                  ? "Preparing local document tools"
                  : runtimeStatus === "checking"
                    ? "Checking local document tools"
                    : "Waiting for local document tools"
                : projection.intake
                  ? intakePaused
                    ? "Paused — AI office unavailable"
                    : projection.intake.label
                  : projection.team.active_agent_runs > 0
                    ? "Coordinating the Tender team"
                    : aiAvailable
                      ? "Ready"
                      : aiStatus === "checking"
                        ? "Checking AI connection"
                        : "AI office unavailable — records remain accessible"}
            </small>
          </div>
        </div>
      )}
      {!readOnly && aiBlockers.length > 0 ? (
        <section className="manager-view__blocker" role="status">
          {aiBlockers.map((blocker) => (
            <div key={blocker.code}>
              <strong>{blocker.title}</strong>
              <details className="manager-view__blocker__details">
                <summary>Technical details</summary>
                <p>{blocker.detail}</p>
              </details>
            </div>
          ))}
          {showBlockerRecovery ? (
            <button type="button" onClick={onOpenSettings}>
              Restore AI connection
            </button>
          ) : null}
        </section>
      ) : null}
      {!readOnly && capabilityGatesCurrentAction ? (
        <p className="manager-view__capability" role="note">
          These skills still need a specialist before the work plan can
          continue: {capabilityGapList}.
        </p>
      ) : null}
      <div
        ref={conversationRef}
        className="manager-view__conversation"
        role="log"
        aria-label="Tender conversation"
        aria-live="polite"
      >
        {!showEarlier && messages.length > visibleMessages.length ? (
          <button
            className="manager-workspace__text-button manager-view__earlier"
            type="button"
            onClick={() => setShowEarlier(true)}
          >
            Show {messages.length - visibleMessages.length} earlier messages
          </button>
        ) : null}
        {visibleMessages.map((message) => (
          <Message
            key={message.message_id}
            message={message}
            meaningful={
              message.message_id ===
              projection.conversation?.latest_meaningful_message_id
            }
            onOpenReference={onOpenReference}
          />
        ))}
        {!readOnly && !managerQuiet ? (
          <article
            className="manager-message manager-message--activity"
            aria-label="Current Tender activity"
          >
            <div className="manager-message__identity">
              <span className="manager-message__avatar" aria-hidden="true">
                Q
              </span>
              <div>
                <strong>Tendering Manager</strong>
                <span
                  className={
                    "manager-activity__state is-" +
                    activityState.toLowerCase().replace(" ", "-")
                  }
                >
                  {activityState}
                </span>
              </div>
            </div>
            <div className="manager-message__body manager-activity__body">
              <h2>{currentActionTitle}</h2>
              <p>{currentActionSummary}</p>
              {projection.intake?.stage === "reading_documents" &&
              projection.intake.parseable_document_count > 0 ? (
                <div className="manager-activity__progress">
                  <progress
                    aria-label="Tender documents read"
                    max={projection.intake.parseable_document_count}
                    value={projection.intake.parsed_document_count}
                  />
                  <small>{intakeProgress}</small>
                </div>
              ) : intakeProgress ? (
                <small className="manager-activity__progress-label">
                  {intakeProgress}
                </small>
              ) : null}
              {documentToolsRequired && preparationActive ? (
                <div className="document-tools-progress" role="status">
                  <span>
                    {activeRuntimeActivity?.title ?? "Preparing approved files"}
                  </span>
                  {runtimeProgress?.model_files_written !== null &&
                  runtimeProgress?.model_files_written !== undefined &&
                  runtimeProgress.model_bytes_written !== null ? (
                    <small>
                      {runtimeProgress.model_files_written.toLocaleString()}{" "}
                      files ·{" "}
                      {formatRuntimeBytes(runtimeProgress.model_bytes_written)}
                      written
                    </small>
                  ) : null}
                </div>
              ) : null}
              {runtimeNotice && documentToolsRequired ? (
                <p className="manager-activity__notice" role="status">
                  {runtimeNotice}
                </p>
              ) : null}
              <div
                className="manager-activity__suggestions"
                aria-label="Suggested next steps"
              >
                {showActionButton ? (
                  <button
                    className="is-primary"
                    type="button"
                    disabled={busy}
                    onClick={openCurrentAction}
                  >
                    {currentActionLabel}
                  </button>
                ) : null}
                {canDecideCitedRecords ? (
                  <button
                    type="button"
                    disabled={busy}
                    onClick={onOpenRecordDecision}
                  >
                    Review cited Tender records
                  </button>
                ) : null}
                {documentToolsRequired && showActionButton ? (
                  <button
                    type="button"
                    disabled={busy}
                    onClick={onDeferRuntime}
                  >
                    Not now
                  </button>
                ) : null}
                {documentToolsRequired && preparationActive ? (
                  <button type="button" onClick={() => void onCancelRuntime()}>
                    Cancel preparation
                  </button>
                ) : null}
                {intakePaused &&
                !documentToolsRequired &&
                !showActionButton &&
                aiBlockers.length === 0 ? (
                  <button type="button" onClick={onOpenSettings}>
                    Restore AI connection
                  </button>
                ) : null}
                {intakeWorking ? (
                  <>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() =>
                        sendSuggestion(
                          "What have you found so far in this Tender?",
                        )
                      }
                    >
                      What have you found so far?
                    </button>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() =>
                        sendSuggestion("Show me the files that need attention.")
                      }
                    >
                      Show files needing attention
                    </button>
                  </>
                ) : !showActionButton && !providerCheckPending ? (
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() =>
                      sendSuggestion(
                        "Explain what happens next for this Tender.",
                      )
                    }
                  >
                    Explain what happens next
                  </button>
                ) : null}
              </div>
            </div>
          </article>
        ) : null}
      </div>

      {!readOnly ? (
        <div className="manager-composer">
          {contextRefs.length ? (
            <div
              className="manager-composer__context"
              aria-label="Attached context"
            >
              {contextRefs.map((reference) => (
                <button
                  key={[
                    reference.kind,
                    reference.reference,
                    reference.version,
                    reference.evidence_ordinal ?? 0,
                  ].join("-")}
                  type="button"
                  title="Remove attached context"
                  onClick={() => onRemoveContext(reference.reference)}
                >
                  {reference.label} ×
                </button>
              ))}
            </div>
          ) : null}
          <div className="manager-composer__surface">
            <button
              className="manager-composer__attach"
              type="button"
              aria-label="Add Tender context"
              title="Add Tender context"
              disabled={busy}
              onClick={onOpenSearch}
            >
              <Paperclip size={18} aria-hidden="true" />
            </button>
            <textarea
              ref={composerRef}
              rows={1}
              value={composer}
              aria-label="Message your Tendering Manager"
              placeholder="Ask your Tendering Manager…"
              disabled={busy}
              onChange={(event) => onComposerChange(event.target.value)}
              onKeyDown={onKeyDown}
            />
            <button
              className="manager-composer__send"
              type="button"
              aria-label="Send message"
              disabled={busy || !composer.trim()}
              onClick={() => void send()}
            >
              <Send size={18} aria-hidden="true" />
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}

function ArchivedTenders({
  tenders,
  trash,
  receipts,
  busy,
  onSelect,
  onTrashAction,
}: {
  tenders: ManagerWorkspaceTender[];
  trash: TrashedTenderRecord[];
  receipts: DeletionReceipt[];
  busy: boolean;
  onSelect: (tenderId: string) => void;
  onTrashAction: (
    kind: "restore" | "purge",
    record: TrashedTenderRecord,
  ) => void;
}) {
  return (
    <main className="retention-view">
      <div className="retention-view__heading">
        <Archive size={20} aria-hidden="true" />
        <div>
          <h1>Archived &amp; Trash</h1>
          <p>Healthy archived Tenders remain complete and read-only.</p>
        </div>
      </div>
      <section aria-labelledby="archived-tenders-title">
        <h2 id="archived-tenders-title">Archived</h2>
        {tenders.length ? (
          <div className="retention-view__list">
            {tenders.map((tender) => (
              <button
                key={tender.tender_id}
                type="button"
                disabled={busy}
                onClick={() => onSelect(tender.tender_id)}
              >
                <span>
                  <strong>{tender.name}</strong>
                  <small>{phaseLabel[tender.phase]} · read-only</small>
                </span>
                <ChevronRight size={17} aria-hidden="true" />
              </button>
            ))}
          </div>
        ) : (
          <p className="retention-view__empty">No archived Tenders.</p>
        )}
      </section>
      <section aria-labelledby="trashed-tenders-title">
        <div className="retention-view__section-heading">
          <div>
            <h2 id="trashed-tenders-title">Trash</h2>
            <p>
              Recoverable Tender Stores stay here until you explicitly restore
              or permanently delete them.
            </p>
          </div>
        </div>
        {trash.length ? (
          <div className="retention-view__trash-list">
            {trash.map((record) => {
              const ready = record.state === "trashed";
              return (
                <article key={record.deletion_id}>
                  <div>
                    <strong>{record.tender_name}</strong>
                    <small>
                      Deleted {new Date(record.created_at).toLocaleString()}
                    </small>
                    {record.deletion_source === "recovery_required" ? (
                      <span>Recovery-required Store</span>
                    ) : null}
                    {!ready ? (
                      <span>
                        {record.state === "failed"
                          ? "Move needs attention"
                          : `Deletion state: ${record.state}`}
                      </span>
                    ) : null}
                  </div>
                  {ready ? (
                    <div>
                      <button
                        type="button"
                        disabled={busy}
                        onClick={() => onTrashAction("restore", record)}
                      >
                        <Undo2 size={16} aria-hidden="true" /> Restore
                      </button>
                      <button
                        className="retention-view__danger"
                        type="button"
                        disabled={busy}
                        onClick={() => onTrashAction("purge", record)}
                      >
                        <Trash2 size={16} aria-hidden="true" /> Permanent Delete
                      </button>
                    </div>
                  ) : null}
                </article>
              );
            })}
          </div>
        ) : (
          <p className="retention-view__empty">Trash is empty.</p>
        )}
        <p className="retention-view__note">
          Quantix never purges Tender Trash automatically. This is not the
          operating-system recycle bin.
        </p>
      </section>
      <section aria-labelledby="deletion-receipts-title">
        <div className="retention-view__section-heading">
          <div>
            <h2 id="deletion-receipts-title">Deletion Receipts</h2>
            <p>
              Minimal proof of completed local deletion. Receipts contain no
              Tender content and cannot restore a Tender.
            </p>
          </div>
        </div>
        {receipts.length ? (
          <div className="retention-view__receipt-list">
            {receipts.map((receipt) => (
              <article key={receipt.receipt_id}>
                <div>
                  <strong>
                    Deleted Tender {receipt.tender_id.slice(0, 8)}
                  </strong>
                  <small>{new Date(receipt.purged_at).toLocaleString()}</small>
                </div>
                <dl>
                  <div>
                    <dt>Local copies</dt>
                    <dd>
                      {receipt.local_deletion_completed ? "Deleted" : "Pending"}
                    </dd>
                  </div>
                  <div>
                    <dt>Provider cleanup</dt>
                    <dd>
                      {receipt.provider_cleanup_status.replace(/_/g, " ")}
                    </dd>
                  </div>
                  <div>
                    <dt>Deletion source</dt>
                    <dd>
                      {receipt.deletion_source === "recovery_required"
                        ? "Recovery-required deletion"
                        : "Verified Store"}
                    </dd>
                  </div>
                  <div>
                    <dt>Threads</dt>
                    <dd>
                      {receipt.confirmed_provider_thread_deletions}/
                      {receipt.provider_thread_count}
                    </dd>
                  </div>
                </dl>
                {receipt.provider_cleanup_status === "incomplete" ? (
                  <p className="retention-view__provider-note">
                    Quantix could not discover every provider reference from the
                    damaged Store. Local deletion is complete; manual provider
                    review may still be required.
                  </p>
                ) : null}
                <details>
                  <summary>Deletion scope</summary>
                  <p>
                    Checked:{" "}
                    {receipt.erased_copy_classes.join(", ").replace(/_/g, " ")}.
                  </p>
                  <p>
                    Outside Quantix control:{" "}
                    {receipt.external_copy_exclusions
                      .join(", ")
                      .replace(/_/g, " ")}
                    .
                  </p>
                </details>
              </article>
            ))}
          </div>
        ) : (
          <p className="retention-view__empty">
            No permanent deletion receipts.
          </p>
        )}
      </section>
    </main>
  );
}

function TenderSearch({
  query,
  results,
  busy,
  inputRef,
  onQueryChange,
  onSearch,
  onOpenResult,
  onAttach,
}: {
  query: string;
  results: WorkspaceSearchProjection | null;
  busy: boolean;
  inputRef: RefObject<HTMLInputElement | null>;
  onQueryChange: (value: string) => void;
  onSearch: () => void;
  onOpenResult: (kind: string) => void;
  onAttach: (hit: WorkspaceSearchHit) => void;
}) {
  const hitCount =
    results?.groups.reduce((total, group) => total + group.hits.length, 0) ?? 0;
  return (
    <div className="tender-search">
      <form
        role="search"
        onSubmit={(event) => {
          event.preventDefault();
          onSearch();
        }}
      >
        <Search size={16} aria-hidden="true" />
        <input
          ref={inputRef}
          type="search"
          value={query}
          aria-label="Search this Tender"
          placeholder="Search conversation, work, files, evidence, and agents"
          onChange={(event) => onQueryChange(event.target.value)}
        />
        <button type="submit" disabled={busy || query.trim().length < 2}>
          {busy ? "Searching…" : "Search"}
        </button>
      </form>
      {results ? (
        <div
          className="tender-search__results"
          role="region"
          aria-label="Tender search results"
          aria-live="polite"
        >
          <div>
            <strong>{hitCount} results</strong>
            <span>Grouped by their canonical Tender source.</span>
          </div>
          {results.groups.map((group) => (
            <section key={group.kind}>
              <h3>{group.kind[0].toUpperCase() + group.kind.slice(1)}</h3>
              {group.hits.length ? (
                <ul>
                  {group.hits.map((hit) => (
                    <li
                      key={`${hit.kind}-${hit.reference}-${hit.version ?? 0}`}
                    >
                      <button
                        type="button"
                        onClick={() => onOpenResult(hit.kind)}
                      >
                        <strong>{hit.title}</strong>
                        <span>{hit.detail}</span>
                        <code>
                          {hit.reference}
                          {hit.version === null ? "" : ` · v${hit.version}`}
                        </code>
                      </button>
                      {hit.kind === "evidence" && hit.version !== null ? (
                        <button
                          className="tender-search__attach"
                          type="button"
                          onClick={() => onAttach(hit)}
                        >
                          Attach allowed context
                        </button>
                      ) : null}
                    </li>
                  ))}
                </ul>
              ) : (
                <p>No matches.</p>
              )}
            </section>
          ))}
        </div>
      ) : null}
    </div>
  );
}

const WORK_GROUP_ORDER: readonly WorkspaceTaskState[] = [
  "waiting",
  "working",
  "needs_engineer",
  "paused",
  "done",
  "failed",
];

const WORK_GROUP_LABELS: Record<WorkspaceTaskState, string> = {
  waiting: "Waiting",
  working: "Working",
  needs_engineer: "Needs you",
  paused: "Paused",
  done: "Done",
  failed: "Failed",
};

function humanizeToken(value: string): string {
  return value.replace(/_/g, " ");
}

function taskStateSentence(task: WorkspaceTaskRow): string {
  switch (task.status_detail) {
    case "blocked":
      return "Waiting for earlier work to finish first.";
    case "ready":
      return "Ready. The Manager's coordinator starts this automatically.";
    case "running":
      return "A specialist is working on this right now.";
    case "reviewing":
      return "An independent reviewer is checking the finished work.";
    case "review_ready":
      return "The work is finished. Its review waits for your decision.";
    case "remediation_ready":
      return "The reviewer asked for changes. The rework starts next.";
    case "query_blocked":
      return "On hold until a Tender question is answered.";
    case "attempt_limit_reached":
      return "Tried too many times without a verified result. It needs your decision.";
    case "indeterminate":
      return "The last attempt ended without a clear result. It needs your decision.";
    case "ready_for_integration":
      return "Done. The Manager folds this output into the next stage.";
    case "suspended":
      return "Paused because the Work Plan changed. The Manager will resume or re-plan it.";
    case "cancelled":
      return "Stopped before finishing. The recorded outcome stays on file.";
    case "failed":
      return "Could not finish. Decide what happens next with the Manager.";
    default:
      return WORK_GROUP_LABELS[task.state] + ".";
  }
}

function waitingForLabel(task: WorkspaceTaskRow): string | null {
  if (task.state !== "waiting" || task.dependencies.length === 0) return null;
  return task.dependencies.map(humanizeToken).join(", ");
}

function workDependents(
  tasks: WorkspaceTaskRow[],
  task: WorkspaceTaskRow,
): WorkspaceTaskRow[] {
  if (!task.task_key) return [];
  return tasks.filter(
    (candidate) =>
      candidate.production_task_id !== task.production_task_id &&
      candidate.dependencies.includes(task.task_key),
  );
}

function taskChangeRequestBody(task: WorkspaceTaskRow): string {
  const objective = task.objective ?? humanizeToken(task.task_key);
  return `Please amend the Work Plan for task "${task.task_key}" (${objective}). I want to change what this task covers. Propose the amendment for my approval, and restart this task only after the amended plan is approved.`;
}

function WorkView({
  projection,
  onOpenTask,
}: {
  projection: ManagerWorkspaceProjection;
  onOpenTask: (task: WorkspaceTaskRow) => void;
}) {
  const tasks = projection.work.tasks;
  const groups = WORK_GROUP_ORDER.map(
    (state) => [state, tasks.filter((task) => task.state === state)] as const,
  );
  const total = tasks.length;
  return (
    <section className="workspace-summary" aria-labelledby="work-title">
      <div className="workspace-summary__heading">
        <ListChecks size={22} aria-hidden="true" />
        <div>
          <h2 id="work-title">Work</h2>
          <p>The approved Tender plan, grouped by what is happening now.</p>
        </div>
      </div>
      {total === 0 ? (
        <div className="workspace-summary__empty">
          <p>No work has been delegated yet.</p>
          <span>The Tender Manager will propose the complete plan first.</span>
        </div>
      ) : (
        <div className="workspace-work-groups">
          {groups
            .filter(([, groupTasks]) => groupTasks.length > 0)
            .map(([state, groupTasks]) => (
              <section key={state} aria-label={WORK_GROUP_LABELS[state]}>
                <div className="workspace-work-groups__heading">
                  <h3>{WORK_GROUP_LABELS[state]}</h3>
                  <span>{groupTasks.length}</span>
                </div>
                <ul>
                  {groupTasks.map((task) => {
                    const waitingFor = waitingForLabel(task);
                    return (
                      <li
                        key={task.production_task_id}
                        className={
                          task.state === "done" ? "is-quiet" : undefined
                        }
                      >
                        <button
                          type="button"
                          className="workspace-task__open"
                          onClick={() => onOpenTask(task)}
                        >
                          <span
                            className="workspace-task__state"
                            data-state={task.state}
                            aria-hidden="true"
                          />
                          <div>
                            <strong>
                              {task.objective ?? humanizeToken(task.task_key)}
                            </strong>
                            <span>{taskStateSentence(task)}</span>
                            {waitingFor ? (
                              <span className="workspace-task__waiting">
                                Waiting for: {waitingFor}
                              </span>
                            ) : null}
                            <dl>
                              <div>
                                <dt>Specialist</dt>
                                <dd>
                                  {task.agent?.identity ?? "Tendering Manager"}
                                </dd>
                              </div>
                              <div>
                                <dt>Outputs</dt>
                                <dd>{task.output_count}</dd>
                              </div>
                            </dl>
                          </div>
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </section>
            ))}
        </div>
      )}
    </section>
  );
}

function WorkTaskDetail({
  tenderId,
  task,
  tasks,
  busy,
  readOnly,
  onRefresh,
  onStart,
  onStop,
  onRequestChange,
  onOpenCalculations,
  reportCommandFailure,
  onClose,
}: {
  tenderId: string;
  task: WorkspaceTaskRow;
  tasks: WorkspaceTaskRow[];
  busy: boolean;
  readOnly: boolean;
  onRefresh: () => Promise<void>;
  onStart: () => Promise<boolean>;
  onStop: () => Promise<boolean>;
  onRequestChange: () => Promise<boolean>;
  onOpenCalculations: () => void;
  reportCommandFailure: () => void;
  onClose: () => void;
}) {
  const [production, setProduction] =
    useState<TenderProductionInspection | null>(null);
  const [review, setReview] = useState<ProductionTaskReviewInspection | null>(
    null,
  );
  const [loading, setLoading] = useState(true);
  const [confirmStop, setConfirmStop] = useState(false);
  const [exceptionDrafts, setExceptionDrafts] = useState<
    Record<string, { rationale: string; consequence: string }>
  >({});
  const [approvingFindingId, setApprovingFindingId] = useState<string | null>(
    null,
  );

  const dependents = useMemo(() => workDependents(tasks, task), [tasks, task]);
  const productionTask =
    production?.tasks.find(
      (candidate) => candidate.production_task_id === task.production_task_id,
    ) ?? null;
  const isEstimatorTask =
    productionTask?.task.exact_inputs.some(
      (input) =>
        input.kind === "calculation_scenario_version" ||
        input.kind === "basis_of_estimate_request",
    ) ?? false;
  const canStart = !readOnly && task.status_detail === "ready";
  const canRequestQueryControl =
    !readOnly &&
    task.status_detail === "query_blocked" &&
    productionTask?.query_control_available === true;
  const canStop =
    !readOnly && task.state === "working" && task.current_run_id !== null;
  const exceptionAllowed =
    productionTask?.task.major_finding_policy === "engineer_exception_allowed";

  useEffect(() => {
    let active = true;
    setLoading(true);
    setConfirmStop(false);
    setExceptionDrafts({});
    const load = async () => {
      try {
        const [nextProduction, nextReview] = await Promise.all([
          inspectTenderProduction(tenderId),
          inspectProductionTaskReview(tenderId, task.production_task_id),
        ]);
        if (!active) return;
        setProduction(nextProduction);
        setReview(nextReview);
      } catch {
        if (active) reportCommandFailure();
      } finally {
        if (active) setLoading(false);
      }
    };
    void load();
    return () => {
      active = false;
    };
  }, [
    tenderId,
    task.production_task_id,
    task.state,
    task.current_run_id,
    reportCommandFailure,
  ]);

  const requestStart = async () => {
    await onStart();
  };

  const requestStop = async () => {
    await onStop();
  };

  const approveException = async (
    targetReview: ProductionReview,
    finding: ProductionReviewFinding,
  ) => {
    const draft = exceptionDrafts[finding.finding_id];
    const artifact = review?.artifact_versions.find(
      (candidate) =>
        candidate.artifact_id === targetReview.target_artifact_id &&
        candidate.version === targetReview.target_version,
    );
    if (!draft?.rationale.trim() || !draft.consequence.trim() || !artifact) {
      return;
    }
    setApprovingFindingId(finding.finding_id);
    try {
      const next = await approveProductionFindingException(
        tenderId,
        task.production_task_id,
        finding.finding_id,
        targetReview.review_id,
        artifact.artifact_id,
        artifact.version,
        artifact.payload_sha256,
        draft.rationale.trim(),
        draft.consequence.trim(),
      );
      setReview(next);
      setExceptionDrafts((current) => {
        const nextDrafts = { ...current };
        delete nextDrafts[finding.finding_id];
        return nextDrafts;
      });
      await onRefresh();
    } catch {
      reportCommandFailure();
    } finally {
      setApprovingFindingId(null);
    }
  };

  const artifacts = review?.artifact_versions ?? [];
  const evidenceReferences = [
    ...new Set(
      artifacts.flatMap((artifact) => artifact.payload.evidence_references),
    ),
  ];

  return (
    <section
      className="work-task-detail"
      data-testid="work-task-detail"
      aria-labelledby="work-task-detail-title"
    >
      <header className="work-task-detail__header">
        <div>
          <p className="section-label">Work task</p>
          <h2 id="work-task-detail-title">
            {task.objective ?? humanizeToken(task.task_key)}
          </h2>
          <p>{taskStateSentence(task)}</p>
          <p>
            Specialist: {task.agent?.identity ?? "Tendering Manager"}
            {task.agent ? ` · ${task.agent.profession}` : ""}
          </p>
        </div>
        <button
          type="button"
          className="manager-workspace__secondary"
          onClick={onClose}
        >
          Back to Work
        </button>
      </header>

      {loading ? (
        <p className="work-task-detail__loading" role="status">
          Opening the task detail…
        </p>
      ) : null}

      {!readOnly && (canStart || canRequestQueryControl || canStop) ? (
        <section className="work-task-detail__section" aria-label="Actions">
          {canStart || canRequestQueryControl ? (
            <div className="work-task-detail__actions">
              <button
                type="button"
                className="manager-workspace__primary"
                disabled={busy}
                onClick={() => void requestStart()}
              >
                {canStart ? "Start work now" : "Request specialist update"}
              </button>
            </div>
          ) : null}
          {canStop ? (
            confirmStop ? (
              <div className="work-task-detail__stop" role="alert">
                <strong>Stop this work?</strong>
                <p>
                  {dependents.length > 0
                    ? `${dependents.length === 1 ? "1 task waits" : `${dependents.length} tasks wait`} for this work: ${dependents
                        .map(
                          (dependent) =>
                            dependent.objective ??
                            humanizeToken(dependent.task_key),
                        )
                        .join("; ")}. `
                    : "Nothing else waits on this task right now. "}
                  Stopping now records the outcome as it stands, and dependent
                  work stays waiting until the Manager re-plans or the task
                  starts again.
                </p>
                <div className="work-task-detail__actions">
                  <button
                    type="button"
                    className="manager-workspace__primary"
                    disabled={busy}
                    onClick={() => void requestStop()}
                  >
                    Stop the work
                  </button>
                  <button
                    type="button"
                    className="manager-workspace__secondary"
                    disabled={busy}
                    onClick={() => setConfirmStop(false)}
                  >
                    Keep working
                  </button>
                </div>
              </div>
            ) : (
              <div className="work-task-detail__actions">
                <button
                  type="button"
                  className="manager-workspace__secondary"
                  disabled={busy}
                  onClick={() => setConfirmStop(true)}
                >
                  Stop
                </button>
              </div>
            )
          ) : null}
        </section>
      ) : null}

      {productionTask && productionTask.task.exact_inputs.length > 0 ? (
        <section className="work-task-detail__section">
          <h3>What this task works from</h3>
          <ul className="work-task-detail__references">
            {productionTask.task.exact_inputs.map((input) => (
              <li key={`${input.kind}-${input.reference}-${input.version}`}>
                <strong>{humanizeToken(input.kind)}</strong>
                <code>
                  {input.reference} · v{input.version}
                </code>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {isEstimatorTask ? (
        <section
          className="work-task-detail__section"
          aria-label="Controlled estimating"
          data-testid="work-task-detail__estimating"
        >
          <h3>Controlled estimating</h3>
          <p>
            This task records its quantities, rates, and results as controlled
            calculation records. Open the controlled calculations to read the
            exact inputs, rounding, and results as the engine produced them.
          </p>
          <button
            type="button"
            className="manager-workspace__secondary"
            disabled={busy}
            onClick={onOpenCalculations}
          >
            Open controlled calculations
          </button>
        </section>
      ) : null}

      {evidenceReferences.length > 0 ? (
        <section className="work-task-detail__section">
          <h3>Evidence used</h3>
          <ul className="work-task-detail__references">
            {evidenceReferences.map((reference) => (
              <li key={reference}>
                <code>{reference}</code>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      <section className="work-task-detail__section">
        <h3>Current output</h3>
        {artifacts.length > 0 ? (
          artifacts.map((artifact: ProductionArtifactVersion) => (
            <article
              key={`${artifact.artifact_id}-${artifact.version}`}
              className="work-task-detail__output"
            >
              <strong>
                Output v{artifact.version}
                {artifact.version === artifacts[artifacts.length - 1].version
                  ? " · latest"
                  : ""}
              </strong>
              {isEstimatorTask ? (
                <p>
                  Published by Agent Run <code>{artifact.author_run_id}</code>.
                </p>
              ) : null}
              <p>{artifact.payload.summary}</p>
              <p>
                Output checks{" "}
                {artifact.output_validation_passed ? "passed" : "failed"} ·
                Evidence verification{" "}
                {artifact.evidence_verified ? "passed" : "failed"}
              </p>
              {artifact.payload.gaps.length > 0 ? (
                <p>Disclosed gaps: {artifact.payload.gaps.join(", ")}</p>
              ) : null}
            </article>
          ))
        ) : (
          <p>
            No output yet ({task.output_count} recorded). It appears here as
            soon as the specialist records one.
          </p>
        )}
      </section>

      {review && review.reviews.length > 0 ? (
        <section className="work-task-detail__section">
          <h3>Independent reviews</h3>
          {review.reviews.map((targetReview) => (
            <article
              key={targetReview.review_id}
              className="work-task-detail__review"
            >
              <strong>
                Output v{targetReview.target_version} ·{" "}
                {humanizeToken(targetReview.result)}
              </strong>
              <p>
                {humanizeToken(targetReview.capability)} · criteria:{" "}
                {targetReview.criteria.join(", ")}
              </p>
              {targetReview.findings.length === 0 ? (
                <p>No findings.</p>
              ) : (
                <ul className="work-task-detail__findings">
                  {targetReview.findings.map((finding) => {
                    const draft = exceptionDrafts[finding.finding_id];
                    return (
                      <li key={finding.finding_id}>
                        <strong>{humanizeToken(finding.severity)}</strong> ·{" "}
                        {finding.summary}
                        {finding.evidence_references.length > 0 ? (
                          <span>
                            {" "}
                            · Evidence {finding.evidence_references.join(", ")}
                          </span>
                        ) : null}
                        {finding.disposition ? (
                          <p>
                            Settled: {humanizeToken(finding.disposition.kind)}{" "}
                            by {humanizeToken(finding.disposition.decided_by)}.
                            Consequence: {finding.disposition.consequence}
                          </p>
                        ) : finding.severity === "minor" ? (
                          <p>Disclosed; does not block the work.</p>
                        ) : finding.severity === "critical" ? (
                          <p>
                            Open and nonwaivable; the author must fix it and a
                            new independent review is required.
                          </p>
                        ) : finding.severity === "major" && exceptionAllowed ? (
                          <div className="work-task-detail__exception">
                            <label>
                              Exception rationale
                              <textarea
                                value={draft?.rationale ?? ""}
                                maxLength={4000}
                                disabled={busy || approvingFindingId !== null}
                                onChange={(event) => {
                                  const rationale = event.currentTarget.value;
                                  setExceptionDrafts((current) => ({
                                    ...current,
                                    [finding.finding_id]: {
                                      rationale,
                                      consequence:
                                        current[finding.finding_id]
                                          ?.consequence ?? "",
                                    },
                                  }));
                                }}
                              />
                            </label>
                            <label>
                              Exact consequence
                              <textarea
                                value={draft?.consequence ?? ""}
                                maxLength={4000}
                                disabled={busy || approvingFindingId !== null}
                                onChange={(event) => {
                                  const consequence = event.currentTarget.value;
                                  setExceptionDrafts((current) => ({
                                    ...current,
                                    [finding.finding_id]: {
                                      rationale:
                                        current[finding.finding_id]
                                          ?.rationale ?? "",
                                      consequence,
                                    },
                                  }));
                                }}
                              />
                            </label>
                            <button
                              type="button"
                              className="manager-workspace__primary"
                              disabled={
                                busy ||
                                approvingFindingId !== null ||
                                !draft?.rationale.trim() ||
                                !draft?.consequence.trim()
                              }
                              onClick={() =>
                                void approveException(targetReview, finding)
                              }
                            >
                              Approve this exception
                            </button>
                          </div>
                        ) : (
                          <p>
                            Open; the approved Review Policy requires the author
                            to fix it and run a new review.
                          </p>
                        )}
                      </li>
                    );
                  })}
                </ul>
              )}
            </article>
          ))}
        </section>
      ) : null}

      {!readOnly ? (
        <section className="work-task-detail__section work-task-detail__change">
          <h3>Need this task to do something different?</h3>
          <p>
            What a task covers comes from the approved Work Plan. Only the
            Manager can change it, through a Work Plan amendment. Sending the
            request opens the Manager conversation with this exact task named.
          </p>
          <button
            type="button"
            className="manager-workspace__secondary"
            disabled={busy}
            onClick={() => void onRequestChange()}
          >
            Request a change through the Manager
          </button>
        </section>
      ) : null}
    </section>
  );
}

type TeamRoomFilter = "all" | "needs_you" | "handoffs" | "outputs";

const TEAM_ROOM_FILTERS: readonly { id: TeamRoomFilter; label: string }[] = [
  { id: "all", label: "All messages" },
  { id: "needs_you", label: "Needs you" },
  { id: "handoffs", label: "Handoffs" },
  { id: "outputs", label: "Outputs" },
];

const NEEDS_YOU_MESSAGE_KINDS: readonly TenderOfficeMessage["kind"][] = [
  "question",
  "finding",
  "blocker",
];

function roomFilter(
  messages: TenderOfficeMessage[],
  filter: TeamRoomFilter,
  latestMeaningfulMessageId: string | null | undefined,
): TenderOfficeMessage[] {
  switch (filter) {
    case "handoffs":
      return messages.filter((message) => message.kind === "handoff");
    case "outputs":
      return messages.filter((message) => message.kind === "output");
    case "needs_you":
      return messages.filter((message) => {
        if (!NEEDS_YOU_MESSAGE_KINDS.includes(message.kind)) return false;
        if (message.message_id === latestMeaningfulMessageId) return true;
        return !messages.some(
          (later) =>
            later.sequence > message.sequence && later.author === "engineer",
        );
      });
    default:
      return messages;
  }
}

type ConversationSection = {
  key: string;
  heading: string | null;
  messages: TenderOfficeMessage[];
};

function conversationDayKey(at: string): string {
  const date = new Date(at);
  if (Number.isNaN(date.getTime())) return "";
  return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`;
}

function groupConversationByDay(
  messages: TenderOfficeMessage[],
): ConversationSection[] {
  const today = new Date();
  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);
  const todayKey = conversationDayKey(today.toISOString());
  const yesterdayKey = conversationDayKey(yesterday.toISOString());
  const formatDate = new Intl.DateTimeFormat(undefined, { dateStyle: "long" });
  const sections: ConversationSection[] = [];
  for (const message of messages) {
    const key = conversationDayKey(message.created_at);
    const section = sections[sections.length - 1];
    if (section && section.key === key) {
      section.messages.push(message);
      continue;
    }
    const date = new Date(message.created_at);
    const heading =
      !key || Number.isNaN(date.getTime())
        ? null
        : key === todayKey
          ? null
          : key === yesterdayKey
            ? "Yesterday"
            : formatDate.format(date);
    sections.push({ key, heading, messages: [message] });
  }
  return sections;
}

function TeamRoom({
  projection,
  busy,
  readOnly,
  composer,
  onComposerChange,
  onSend,
  onOpenSearch,
  contextRefs,
  onRemoveContext,
  onOpenReference,
  onClose,
}: {
  projection: ManagerWorkspaceProjection;
  busy: boolean;
  readOnly: boolean;
  composer: string;
  onComposerChange: (value: string) => void;
  onSend: (body: string) => Promise<boolean>;
  onOpenSearch: () => void;
  contextRefs: WorkspaceMessageReference[];
  onRemoveContext: (reference: string) => void;
  onOpenReference: (reference: WorkspaceMessageReference) => void;
  onClose: () => void;
}) {
  const tenderId = projection.selected_tender?.tender_id ?? null;
  const [filter, setFilter] = useState<TeamRoomFilter>("all");
  const [selectedRun, setSelectedRun] = useState<AgentRunInspection | null>(
    null,
  );
  const [loadingRunId, setLoadingRunId] = useState<string | null>(null);
  const [workroomTab, setWorkroomTab] = useState<
    "conversation" | "context" | "activity" | "outputs"
  >("conversation");
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const conversationRef = useRef<HTMLDivElement>(null);

  const messages = projection.conversation?.messages ?? [];
  const filteredMessages = useMemo(
    () =>
      roomFilter(
        messages,
        filter,
        projection.conversation?.latest_meaningful_message_id,
      ),
    [filter, messages, projection.conversation?.latest_meaningful_message_id],
  );
  const sections = useMemo(
    () => groupConversationByDay(filteredMessages),
    [filteredMessages],
  );

  useEffect(() => {
    const conversation = conversationRef.current;
    if (conversation) conversation.scrollTop = conversation.scrollHeight;
  }, [filter, messages[messages.length - 1]?.sequence]);

  const openWorkroom = async (runId: string) => {
    if (!tenderId) return;
    setLoadingRunId(runId);
    try {
      setSelectedRun(await inspectAgentRun(tenderId, runId));
      setWorkroomTab("conversation");
    } finally {
      setLoadingRunId(null);
    }
  };

  const send = async () => {
    const body = composer.trim();
    if (!body || busy) return;
    if (await onSend(body)) onComposerChange("");
  };

  const onKeyDown = (event: ReactKeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void send();
    }
  };

  return (
    <section className="team-room" aria-labelledby="team-room-title">
      <header className="team-room__header">
        <div className="team-room__heading">
          <Users size={22} aria-hidden="true" />
          <div>
            <h2 id="team-room-title">Team working</h2>
            <p>
              The live room for this Tender. Closing it returns you to the
              Manager conversation.
            </p>
          </div>
        </div>
        <button
          type="button"
          className="manager-workspace__secondary"
          onClick={onClose}
        >
          Back to Manager
        </button>
      </header>
      <div
        className="team-room__filters"
        role="group"
        aria-label="Message filters"
      >
        {TEAM_ROOM_FILTERS.map(({ id, label }) => (
          <button
            key={id}
            type="button"
            aria-pressed={filter === id}
            onClick={() => setFilter(id)}
          >
            {label}
          </button>
        ))}
      </div>
      <div className="team-room__body">
        <div
          ref={conversationRef}
          className="team-room__conversation"
          role="log"
          aria-label="Team room conversation"
          aria-live="polite"
        >
          {sections.length === 0 ? (
            <p className="team-room__empty">
              {filter === "all"
                ? "No messages yet. The Manager and specialists post here as the work moves."
                : "No messages match this filter."}
            </p>
          ) : (
            sections.map((section) => (
              <div key={section.key} className="team-room__section">
                {section.heading ? (
                  <h3 className="team-room__section-heading">
                    {section.heading}
                  </h3>
                ) : null}
                {section.messages.map((message) => (
                  <Message
                    key={message.message_id}
                    message={message}
                    meaningful={
                      message.message_id ===
                      projection.conversation?.latest_meaningful_message_id
                    }
                    onOpenReference={onOpenReference}
                  />
                ))}
              </div>
            ))
          )}
        </div>
        <aside className="team-room__rail" aria-label="Agent workrooms">
          <h3>Agent workrooms</h3>
          {projection.team.agent_runs.length ? (
            <ul className="workspace-team__runs">
              {projection.team.agent_runs.map((run) => (
                <li key={run.run_id}>
                  <div>
                    <strong>{run.agent.identity}</strong>
                    <span>{run.agent.profession}</span>
                    <small>{run.state.replace(/_/g, " ")}</small>
                  </div>
                  <button
                    type="button"
                    disabled={loadingRunId === run.run_id}
                    onClick={() => void openWorkroom(run.run_id)}
                  >
                    {loadingRunId === run.run_id ? "Opening…" : "Open workroom"}
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="team-room__empty">No Agent Runs yet.</p>
          )}
          {selectedRun ? (
            <section className="agent-workroom" aria-label="Agent workroom">
              <div className="agent-workroom__heading">
                <div>
                  <strong>{selectedRun.profile.identity}</strong>
                  <span>{selectedRun.task.objective}</span>
                </div>
                <button type="button" onClick={() => setSelectedRun(null)}>
                  Close
                </button>
              </div>
              <nav aria-label="Agent workroom sections">
                {(
                  ["conversation", "context", "activity", "outputs"] as const
                ).map((tab) => (
                  <button
                    key={tab}
                    type="button"
                    aria-current={workroomTab === tab ? "page" : undefined}
                    onClick={() => setWorkroomTab(tab)}
                  >
                    {tab[0].toUpperCase() + tab.slice(1)}
                  </button>
                ))}
              </nav>
              {workroomTab === "conversation" ? (
                <div className="agent-workroom__panel">
                  <p>
                    Run <code>{selectedRun.run_id}</code> is{" "}
                    {selectedRun.state.replace(/_/g, " ")}.
                  </p>
                  <p>
                    {selectedRun.failure?.required_user_action ??
                      "No provider failure recorded."}
                  </p>
                </div>
              ) : null}
              {workroomTab === "context" ? (
                <div className="agent-workroom__panel">
                  <h4>Exact inputs</h4>
                  <ul>
                    {selectedRun.task.exact_inputs.map((input) => (
                      <li
                        key={`${input.kind}-${input.reference}-${input.version}`}
                      >
                        {input.kind}: <code>{input.reference}</code> · v
                        {input.version}
                      </li>
                    ))}
                  </ul>
                  <h4>Granted access</h4>
                  <p>
                    {selectedRun.permission_grant.data_scopes.join(", ") ||
                      "No data scopes"}
                  </p>
                  <h4>Requested but not granted</h4>
                  <p>
                    {selectedRun.access_requests
                      .filter((request) => request.status !== "approved")
                      .flatMap((request) => request.request.data_scopes)
                      .join(", ") || "None"}
                  </p>
                </div>
              ) : null}
              {workroomTab === "activity" ? (
                <ol className="agent-workroom__panel agent-workroom__events">
                  {selectedRun.events.map((event) => (
                    <li key={event.sequence}>
                      <strong>{event.kind.replace(/_/g, " ")}</strong>
                      <span>{event.summary}</span>
                    </li>
                  ))}
                </ol>
              ) : null}
              {workroomTab === "outputs" ? (
                <div className="agent-workroom__panel">
                  {selectedRun.proposed_result ? (
                    <>
                      <strong>
                        {selectedRun.proposed_result.verification_status.replace(
                          /_/g,
                          " ",
                        )}
                      </strong>
                      <pre>{selectedRun.proposed_result.payload_json}</pre>
                    </>
                  ) : (
                    <p>No output has been proposed.</p>
                  )}
                </div>
              ) : null}
            </section>
          ) : null}
        </aside>
      </div>
      {!readOnly ? (
        <div className="manager-composer team-room__composer">
          {contextRefs.length ? (
            <div
              className="manager-composer__context"
              aria-label="Attached context"
            >
              {contextRefs.map((reference) => (
                <button
                  key={[
                    reference.kind,
                    reference.reference,
                    reference.version,
                    reference.evidence_ordinal ?? 0,
                  ].join("-")}
                  type="button"
                  title="Remove attached context"
                  onClick={() => onRemoveContext(reference.reference)}
                >
                  {reference.label} ×
                </button>
              ))}
            </div>
          ) : null}
          <div className="manager-composer__surface">
            <button
              className="manager-composer__attach"
              type="button"
              aria-label="Add Tender context"
              title="Add Tender context"
              disabled={busy}
              onClick={onOpenSearch}
            >
              <Paperclip size={18} aria-hidden="true" />
            </button>
            <textarea
              ref={composerRef}
              rows={1}
              value={composer}
              aria-label="Message the Team"
              placeholder="Message the Team…"
              disabled={busy}
              onChange={(event) => onComposerChange(event.target.value)}
              onKeyDown={onKeyDown}
            />
            <button
              className="manager-composer__send"
              type="button"
              aria-label="Send message"
              disabled={busy || !composer.trim()}
              onClick={() => void send()}
            >
              <Send size={18} aria-hidden="true" />
            </button>
          </div>
        </div>
      ) : null}
    </section>
  );
}

function documentFolder(packagePath: string): string {
  const separator = packagePath.lastIndexOf("/");
  return separator === -1 ? "" : packagePath.slice(0, separator);
}

function ArtifactHistoryDisclosure({
  tenderId,
  artifactId,
}: {
  tenderId: string;
  artifactId: string;
}) {
  const [versions, setVersions] = useState<ArtifactVersionSummary[] | null>(
    null,
  );
  const [failed, setFailed] = useState(false);

  return (
    <details
      className="workspace-documents__history"
      onToggle={(event) => {
        const open = event.currentTarget.open;
        if (!open || versions !== null || failed) return;
        inspectArtifactVersions(tenderId, artifactId)
          .then((history) => setVersions(history.versions))
          .catch(() => setFailed(true));
      }}
    >
      <summary>History</summary>
      {versions === null ? (
        <p>{failed ? "Version history is unavailable." : "Loading history…"}</p>
      ) : (
        <ul>
          {versions.map((version, index) => (
            <li key={`${version.artifact_id}-${version.version}`}>
              <strong>
                {index === 0 ? "Current version" : "Prior version"} · v
                {version.version}
              </strong>
              <code>{version.digest ?? "not registered"}</code>
              <span>{new Date(version.created_at).toLocaleString()}</span>
            </li>
          ))}
        </ul>
      )}
    </details>
  );
}

function FilesView({
  projection,
  busy,
  onImport,
  readOnly,
}: {
  projection: ManagerWorkspaceProjection;
  busy: boolean;
  onImport: (kind: TenderPackageSourceKind) => void;
  readOnly: boolean;
}) {
  const tenderId = projection.selected_tender?.tender_id ?? null;
  const intakeWorking = ["working", "waiting"].includes(
    projection.intake?.status ?? "",
  );
  const registeredDocuments = projection.files.tender_documents.filter(
    (document) => document.registration_state === "registered",
  );
  const exceptionDocuments = projection.files.tender_documents.filter(
    (document) => document.registration_state === "exception",
  );
  const registeredFolders = new Map<
    string,
    typeof projection.files.tender_documents
  >();
  for (const document of registeredDocuments) {
    const folder = documentFolder(document.package_path);
    const group = registeredFolders.get(folder);
    if (group) {
      group.push(document);
    } else {
      registeredFolders.set(folder, [document]);
    }
  }
  const folders = [...registeredFolders.entries()].sort(([left], [right]) =>
    left.localeCompare(right),
  );
  for (const [, documents] of folders) {
    documents.sort((left, right) =>
      left.package_path.localeCompare(right.package_path),
    );
  }

  const renderDocument = (
    document: (typeof projection.files.tender_documents)[number],
  ) => (
    <li key={`${document.artifact_id}-${document.version}`}>
      <FileText size={17} aria-hidden="true" />
      <div>
        <strong>{document.package_path}</strong>
        <span>
          {document.registration_state === "registered" ? (
            <>Registered · {document.parse_state.replace(/_/g, " ")}</>
          ) : (
            <>
              Registration exception
              {document.exception ? ` · ${document.exception}` : ""}
            </>
          )}
        </span>
        <details>
          <summary>Provenance</summary>
          <dl>
            <div>
              <dt>Registration</dt>
              <dd>{document.registration_state.replace(/_/g, " ")}</dd>
            </div>
            {document.exception ? (
              <div>
                <dt>Exception</dt>
                <dd>
                  <code>{document.exception}</code>
                </dd>
              </div>
            ) : null}
            <div>
              <dt>Type</dt>
              <dd>{document.document_type.replace(/_/g, " ")}</dd>
            </div>
            <div>
              <dt>Size</dt>
              <dd>{document.size_bytes.toLocaleString()} bytes</dd>
            </div>
            {document.sha256 ? (
              <div>
                <dt>SHA-256</dt>
                <dd>
                  <code>{document.sha256}</code>
                </dd>
              </div>
            ) : null}
            <div>
              <dt>Artifact</dt>
              <dd>
                <code>
                  {document.artifact_id} · v{document.version}
                </code>
              </dd>
            </div>
          </dl>
        </details>
        {tenderId && document.registration_state === "registered" ? (
          <ArtifactHistoryDisclosure
            tenderId={tenderId}
            artifactId={document.artifact_id}
          />
        ) : null}
      </div>
    </li>
  );

  return (
    <section className="workspace-summary" aria-labelledby="files-title">
      <div className="workspace-summary__heading">
        <Folder size={22} aria-hidden="true" />
        <div>
          <h2 id="files-title">Files</h2>
          <p>Tender sources and Quantix outputs stay separate and traceable.</p>
        </div>
      </div>
      <div className="workspace-files">
        <article>
          <FileText size={20} aria-hidden="true" />
          <div>
            <strong>{registeredDocuments.length}</strong>
            <span>Registered documents</span>
          </div>
        </article>
        <article>
          <CircleAlert size={20} aria-hidden="true" />
          <div>
            <strong>{exceptionDocuments.length}</strong>
            <span>Registration exceptions</span>
          </div>
        </article>
        <article>
          <Bot size={20} aria-hidden="true" />
          <div>
            <strong>{projection.files.quantix_output_count}</strong>
            <span>Quantix outputs</span>
          </div>
        </article>
      </div>
      {registeredDocuments.length > 0 ? (
        <div className="workspace-documents">
          <h3>Registered source documents</h3>
          {folders.map(([folder, documents]) => (
            <section
              key={folder || "package-root"}
              className="workspace-documents__folder"
              aria-label={folder || "Package root"}
            >
              <h4>{folder || "Package root"}</h4>
              <ul>{documents.map(renderDocument)}</ul>
            </section>
          ))}
        </div>
      ) : null}
      {exceptionDocuments.length > 0 ? (
        <div className="workspace-documents">
          <h3>Registration exceptions</h3>
          <p>
            These package paths were discovered but not registered. Review the
            exact Host-provided exception before deciding how to proceed.
          </p>
          <ul>{exceptionDocuments.map(renderDocument)}</ul>
        </div>
      ) : null}
      {projection.files.quantix_outputs.length > 0 ? (
        <div className="workspace-documents workspace-outputs">
          <h3>Quantix-generated outputs</h3>
          <ul>
            {projection.files.quantix_outputs.map((output) => (
              <li key={`${output.artifact_id}-${output.version}`}>
                <Bot size={17} aria-hidden="true" />
                <div>
                  <strong>{output.artifact_id}</strong>
                  <span>
                    Version {output.version} · task {output.production_task_id}
                  </span>
                  <details>
                    <summary>Evidence &amp; provenance</summary>
                    <dl>
                      <div>
                        <dt>Agent Run</dt>
                        <dd>
                          <code>{output.author_run_id}</code>
                        </dd>
                      </div>
                      <div>
                        <dt>Payload SHA-256</dt>
                        <dd>
                          <code>{output.payload_sha256}</code>
                        </dd>
                      </div>
                      <div>
                        <dt>Created</dt>
                        <dd>{new Date(output.created_at).toLocaleString()}</dd>
                      </div>
                    </dl>
                  </details>
                  {tenderId ? (
                    <ArtifactHistoryDisclosure
                      tenderId={tenderId}
                      artifactId={output.artifact_id}
                    />
                  ) : null}
                </div>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
      {!readOnly ? (
        <div className="workspace-files__actions">
          <button
            className="manager-workspace__secondary"
            type="button"
            disabled={busy || intakeWorking}
            onClick={() => onImport("directory")}
          >
            Add package folder
          </button>
          <button
            className="manager-workspace__text-button"
            type="button"
            disabled={busy || intakeWorking}
            onClick={() => onImport("zip_archive")}
          >
            Add ZIP
          </button>
        </div>
      ) : null}
    </section>
  );
}

function ChangeReviewSurface({
  tenderId,
  busy,
  reportCommandFailure,
  onDecided,
  onClose,
}: {
  tenderId: string;
  busy: boolean;
  reportCommandFailure: () => void;
  onDecided: () => void;
  onClose: () => void;
}) {
  const [page, setPage] = useState<ChangeAssessmentPage | null>(null);
  const [classification, setClassification] =
    useState<ChangeAssessmentClassification>("material");
  const [rationale, setRationale] = useState("");
  const [deciding, setDeciding] = useState(false);
  const [loading, setLoading] = useState(true);
  const requestGeneration = useRef(0);

  useEffect(() => {
    const generation = ++requestGeneration.current;
    setLoading(true);
    inspectChangeAssessments(tenderId, null, 4)
      .then((next) => {
        if (generation === requestGeneration.current) setPage(next);
      })
      .catch(() => {
        if (generation === requestGeneration.current) reportCommandFailure();
      })
      .finally(() => {
        if (generation === requestGeneration.current) setLoading(false);
      });
    return () => {
      requestGeneration.current += 1;
    };
  }, [reportCommandFailure, tenderId]);

  const active = page?.active ?? null;

  const decide = async () => {
    if (!active || active.status !== "pending" || deciding || !rationale.trim())
      return;
    setDeciding(true);
    try {
      await decideChangeAssessment(
        tenderId,
        active.assessment_id,
        active.manifest_sha256,
        classification,
        rationale.trim(),
      );
      onDecided();
    } catch {
      reportCommandFailure();
      setDeciding(false);
    }
  };

  return (
    <section
      className="change-review"
      data-testid="change-review"
      aria-labelledby="change-review-title"
    >
      <header className="change-review__header">
        <div>
          <h2 id="change-review-title">Review the Tender change</h2>
          <p>
            A new source document version is waiting for your decision. The
            earlier version stays preserved unchanged, and work that depends on
            it is explained below.
          </p>
        </div>
        <button
          type="button"
          className="manager-workspace__secondary"
          onClick={onClose}
        >
          Back to Manager conversation
        </button>
      </header>
      {loading ? (
        <p className="change-review__loading">
          <LoaderCircle size={16} aria-hidden="true" /> Loading the change
          assessment…
        </p>
      ) : !active ? (
        <p className="change-review__empty">
          No change is waiting for a decision.
        </p>
      ) : (
        <>
          <article className="change-review__card">
            <h3>What changed</h3>
            <p>
              {active.relationship_kind === "addendum" ? (
                <>A new addendum arrived for </>
              ) : (
                <>A replacement document arrived for </>
              )}
              <strong>{active.prior_source.package_path}</strong> and is
              registered as{" "}
              <strong>{active.replacement_source.package_path}</strong>.
            </p>
            <dl>
              <div>
                <dt>Earlier version</dt>
                <dd>
                  <code>
                    {active.prior_source.artifact_id} · v
                    {active.prior_source.version}
                  </code>
                </dd>
              </div>
              <div>
                <dt>New version</dt>
                <dd>
                  <code>
                    {active.replacement_source.artifact_id} · v
                    {active.replacement_source.version}
                  </code>
                </dd>
              </div>
              <div>
                <dt>SHA-256</dt>
                <dd>
                  <code>{active.replacement_source.sha256}</code>
                </dd>
              </div>
            </dl>
          </article>
          <article className="change-review__card">
            <h3>What is now out of date</h3>
            {active.impacts.length ? (
              <ul className="change-review__impacts">
                {active.impacts.map((impact) => (
                  <li
                    key={`${impact.kind}-${impact.object_id}-${impact.object_version}`}
                  >
                    <div>
                      <strong>{impact.kind.replace(/_/g, " ")}</strong>
                      <span>{impact.consequence.replace(/_/g, " ")}</span>
                    </div>
                    <p>{impact.summary}</p>
                    <code>
                      {impact.object_id}
                      {impact.object_version
                        ? ` · v${impact.object_version}`
                        : ""}
                    </code>
                  </li>
                ))}
              </ul>
            ) : (
              <p>
                No current records, work, or calculations depend on the replaced
                document.
              </p>
            )}
          </article>
          <article className="change-review__card">
            <h3>Affected work</h3>
            <ul>
              {active.proposed_rework.map((item, index) => (
                <li key={`${index}-${item}`}>{item}</li>
              ))}
            </ul>
            <p>{active.deadline_effect}</p>
            <h4>Unchanged</h4>
            <ul>
              {active.unchanged_scope.map((item, index) => (
                <li key={`${index}-${item}`}>{item}</li>
              ))}
            </ul>
            {active.approval_consequences.length ? (
              <>
                <h4>Approvals affected</h4>
                <ul>
                  {active.approval_consequences.map((item) => (
                    <li key={item.reference}>
                      {item.reference}: {item.consequence}
                    </li>
                  ))}
                </ul>
              </>
            ) : null}
          </article>
          {active.status === "pending" ? (
            <form
              className="change-review__form"
              onSubmit={(event) => {
                event.preventDefault();
                void decide();
              }}
            >
              <label>
                Classification
                <select
                  value={classification}
                  disabled={busy || deciding}
                  onChange={(event) =>
                    setClassification(
                      event.target.value as ChangeAssessmentClassification,
                    )
                  }
                >
                  <option value="material">
                    Material — targeted rework is required
                  </option>
                  <option value="irrelevant">
                    Irrelevant — current work stays valid
                  </option>
                </select>
              </label>
              <label>
                Decision rationale
                <textarea
                  value={rationale}
                  disabled={busy || deciding}
                  onChange={(event) => setRationale(event.target.value)}
                />
              </label>
              <button
                className="manager-workspace__primary"
                type="submit"
                disabled={busy || deciding || !rationale.trim()}
              >
                Record decision
              </button>
            </form>
          ) : (
            <p className="change-review__empty">
              A decision was already recorded for this change.
            </p>
          )}
        </>
      )}
    </section>
  );
}

export function ManagerWorkspace({
  initialProjection,
  initialPreferences = DEFAULT_GENERAL_APPLICATION_PREFERENCES,
  setupWarnings = [],
}: ManagerWorkspaceProps) {
  const [aiStatus, setAiStatus] = useState<CapabilityStatus>("checking");
  const [runtimeStatus, setRuntimeStatus] =
    useState<CapabilityStatus>("checking");
  const [runtimeReadiness, setRuntimeReadiness] =
    useState<RuntimeReadiness | null>(null);
  const [runtimeProgress, setRuntimeProgress] =
    useState<RuntimePreparationProgress | null>(null);
  const [runtimePreparing, setRuntimePreparing] = useState(false);
  const [runtimeNotice, setRuntimeNotice] = useState<string | null>(null);
  const [preferences, setPreferences] = useState(initialPreferences);
  const [settingsSnapshot, setSettingsSnapshot] =
    useState<ApplicationSettingsView | null>(null);
  const [projection, setProjection] =
    useState<ManagerWorkspaceProjection | null>(initialProjection ?? null);
  const [recoveryTarget, setRecoveryTarget] =
    useState<ManagerWorkspaceTender | null>(null);
  const [recoveryRequestedAction, setRecoveryRequestedAction] =
    useState<RecoveryRequestedAction>(null);
  const [view, setView] = useState<WorkspaceView>("manager");
  const [focusedActionTenderId, setFocusedActionTenderId] = useState<
    string | null
  >(null);
  const [evidenceReview, setEvidenceReview] =
    useState<EvidenceReviewState | null>(null);
  const [recordDecision, setRecordDecision] =
    useState<RecordDecisionState | null>(null);
  const [changeReviewOpen, setChangeReviewOpen] = useState(false);
  const [rfiReviewOpen, setRfiReviewOpen] = useState(false);
  const [estimateReviewOpen, setEstimateReviewOpen] = useState(false);
  const [calculationsOpen, setCalculationsOpen] = useState(false);
  const [focusedTaskId, setFocusedTaskId] = useState<string | null>(null);
  const [tenderViews, setTenderViews] = useState<Record<string, WorkspaceView>>(
    {},
  );
  const [composerDrafts, setComposerDrafts] = useState<Record<string, string>>(
    {},
  );
  const [searchQueries, setSearchQueries] = useState<Record<string, string>>(
    {},
  );
  const [searchResults, setSearchResults] = useState<
    Record<string, WorkspaceSearchProjection | null>
  >({});
  const [contextRefsByTender, setContextRefsByTender] = useState<
    Record<string, WorkspaceMessageReference[]>
  >({});
  const [searchingTenderId, setSearchingTenderId] = useState<string | null>(
    null,
  );
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsSection, setSettingsSection] =
    useState<SettingsSection>("general");
  const [retentionOpen, setRetentionOpen] = useState(false);
  const [navigationHistory, setNavigationHistory] = useState<NavigationHistory>(
    () => initialNavigationHistory(initialProjection),
  );
  const [retentionAction, setRetentionAction] = useState<RetentionAction>(null);
  const [renameTarget, setRenameTarget] =
    useState<ManagerWorkspaceTender | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [pendingTenderStart, setPendingTenderStart] =
    useState<TenderPackageSourceKind | null>(null);
  const [aiPreflightOpen, setAiPreflightOpen] = useState(false);
  const [aiPreflightReason, setAiPreflightReason] = useState(
    "Choose and approve a provider, model, and reasoning setting before AI-assisted Tender work begins.",
  );
  const [trashAction, setTrashAction] = useState<TrashAction>(null);
  const [trashedTenders, setTrashedTenders] = useState<TrashedTenderRecord[]>(
    [],
  );
  const [deletionReceipts, setDeletionReceipts] = useState<DeletionReceipt[]>(
    [],
  );
  const [permanentDeleteConfirmation, setPermanentDeleteConfirmation] =
    useState("");
  const [retentionRationale, setRetentionRationale] = useState("");
  const contextRail = useMediaQuery("(min-width: 1280px)");
  const contextPresentation: WorkspaceContextPresentation = contextRail
    ? "rail"
    : "drawer";
  const [sidebarOpen, setSidebarOpen] = useState(
    () =>
      typeof window.matchMedia !== "function" ||
      mediaQueryMatches("(min-width: 820px)"),
  );
  const [contextOpen, setContextOpen] = useState(() => contextRail);
  const [operation, setOperation] = useState<WorkspaceOperation | null>(null);
  const [aiPreflightChecking, setAiPreflightChecking] = useState(false);
  const isBusy = operation !== null || aiPreflightChecking;
  const [showNavigationPanel, setShowNavigationPanel] = useState(false);
  const [packageTask, setPackageTask] = useState<PackageTaskState | null>(null);
  const [operationFailure, setOperationFailure] =
    useState<WorkspaceOperationFailure | null>(null);
  const [backupConfirmation, setBackupConfirmation] = useState<string | null>(
    null,
  );
  const [startupReconciliation, setStartupReconciliation] =
    useState<StartupReconciliationReport | null>(null);
  const startupReconciliationFetched = useRef(false);
  const [error, setError] = useState<string | null>(null);
  const refreshRunning = useRef(false);
  const settingsSnapshotRequestVersion = useRef(0);
  const aiStatusRequestVersion = useRef(0);
  const aiPreflightInFlight = useRef(false);
  const operationRef = useRef<WorkspaceOperation | null>(null);
  const failedOperationRef = useRef<{
    work: () => Promise<unknown>;
    details: Pick<WorkspaceOperation, "kind" | "label">;
    complete?: (result: unknown) => Promise<void> | void;
  } | null>(null);
  const packageFocusRef = useRef<HTMLButtonElement | null>(null);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const projectionEpoch = useRef(0);
  const initialProjectionAccepted = useRef(Boolean(initialProjection));
  const resumedIntakes = useRef<CapabilityStatus | null>(null);
  const previousAttentionTenderIds = useRef<Set<string> | null>(null);
  const contextTriggerRef = useRef<HTMLButtonElement | null>(null);
  const previousContextRail = useRef(contextRail);
  const runtimeProbeInFlight = useRef(false);
  const runtimeProbeDisposed = useRef(false);
  const runtimeRetryTimer = useRef<number | null>(null);
  const archivedInspectionTenderId =
    projection?.selected_tender?.state === "archived"
      ? projection.selected_tender.tender_id
      : null;

  useEffect(() => {
    if (
      focusedActionTenderId &&
      (projection?.selected_tender?.tender_id !== focusedActionTenderId ||
        !projection ||
        !isFocusedActionKind(projection.current_action.kind))
    ) {
      setFocusedActionTenderId(null);
    }
  }, [focusedActionTenderId, projection]);

  const handleContextOpenChange = useCallback(
    (open: boolean) => {
      setContextOpen(open);
      if (open && !contextRail) setSidebarOpen(false);
      if (!open) {
        window.setTimeout(() => contextTriggerRef.current?.focus(), 0);
      }
    },
    [contextRail],
  );

  const toggleContext = useCallback(() => {
    const next = !contextOpen;
    setContextOpen(next);
    if (next && !contextRail) setSidebarOpen(false);
  }, [contextOpen, contextRail]);

  useEffect(() => {
    if (
      rfiReviewOpen &&
      (!projection?.selected_tender ||
        !isRfiActionKind(projection.current_action.kind))
    ) {
      setRfiReviewOpen(false);
    }
  }, [rfiReviewOpen, projection]);

  useEffect(() => {
    if (
      estimateReviewOpen &&
      (!projection?.selected_tender ||
        !isEstimateReviewActionKind(projection.current_action.kind))
    ) {
      setEstimateReviewOpen(false);
    }
  }, [estimateReviewOpen, projection]);

  useEffect(() => {
    if (
      !evidenceReview &&
      !recordDecision &&
      !changeReviewOpen &&
      !rfiReviewOpen &&
      !estimateReviewOpen &&
      !calculationsOpen &&
      !focusedTaskId
    )
      return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !operationRef.current) {
        if (calculationsOpen) setCalculationsOpen(false);
        else if (evidenceReview) setEvidenceReview(null);
        else if (recordDecision) setRecordDecision(null);
        else if (changeReviewOpen) setChangeReviewOpen(false);
        else if (rfiReviewOpen) setRfiReviewOpen(false);
        else if (estimateReviewOpen) setEstimateReviewOpen(false);
        else setFocusedTaskId(null);
      }
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [
    evidenceReview,
    recordDecision,
    changeReviewOpen,
    rfiReviewOpen,
    estimateReviewOpen,
    calculationsOpen,
    focusedTaskId,
  ]);

  useEffect(() => {
    setEvidenceReview(null);
    setRecordDecision(null);
    setChangeReviewOpen(false);
    setRfiReviewOpen(false);
    setEstimateReviewOpen(false);
    setCalculationsOpen(false);
    setFocusedTaskId(null);
  }, [projection?.selected_tender?.tender_id]);

  useEffect(() => {
    const wasRail = previousContextRail.current;
    previousContextRail.current = contextRail;
    if (wasRail && !contextRail) setContextOpen(false);
    if (!wasRail && contextRail) setSidebarOpen(true);
  }, [contextRail]);

  useEffect(() => {
    if (!retentionAction && !trashAction && !renameTarget) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !operationRef.current) {
        setRetentionAction(null);
        setRenameTarget(null);
        setRenameValue("");
        setTrashAction(null);
        setRetentionRationale("");
        setPermanentDeleteConfirmation("");
      }
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [renameTarget, retentionAction, trashAction]);

  useEffect(() => {
    if (!contextOpen || !contextRail) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        handleContextOpenChange(false);
      }
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [contextOpen, contextRail, handleContextOpenChange]);

  useEffect(() => {
    const current = new Set(
      projection?.catalogue
        .filter((tender) => tender.state === "active" && tender.needs_engineer)
        .map((tender) => tender.tender_id) ?? [],
    );
    const previous = previousAttentionTenderIds.current;
    previousAttentionTenderIds.current = current;
    if (
      !previous ||
      !preferences.notify_when_attention_needed ||
      document.hasFocus()
    ) {
      return;
    }
    if ([...current].some((tenderId) => !previous.has(tenderId))) {
      void notifyAttentionRequired().catch(() => undefined);
    }
  }, [preferences.notify_when_attention_needed, projection?.catalogue]);

  const load = useCallback(async () => {
    const epoch = ++projectionEpoch.current;
    setError(null);
    try {
      const next = await inspectManagerWorkspace();
      if (epoch === projectionEpoch.current) setProjection(next);
    } catch (reason) {
      if (epoch === projectionEpoch.current) setError(readableError(reason));
    }
  }, []);

  useEffect(() => {
    if (!projection || startupReconciliationFetched.current) return;
    startupReconciliationFetched.current = true;
    let disposed = false;
    void inspectStartupReconciliation()
      .then((report) => {
        if (!disposed) setStartupReconciliation(report);
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
    };
  }, [projection]);

  const acceptRuntimeReadiness = useCallback((readiness: RuntimeReadiness) => {
    const runtimeProbePending =
      readiness.state === "repair_required" &&
      !readiness.repair_available &&
      readiness.issues.includes("runtime_probe_failed");
    setRuntimeReadiness(readiness);
    setRuntimeStatus(
      runtimeProbePending
        ? "checking"
        : readiness.state === "ready"
          ? "ready"
          : "unavailable",
    );
    if (runtimeProbePending) setRuntimeNotice(null);
    return runtimeProbePending;
  }, []);

  const probeRuntimeReadiness = useCallback(() => {
    if (runtimeProbeDisposed.current) return;
    if (runtimeProbeInFlight.current || runtimeRetryTimer.current !== null) {
      return;
    }
    runtimeProbeInFlight.current = true;
    void inspectRuntimeReadiness()
      .then((readiness) => {
        if (runtimeProbeDisposed.current) return;
        if (!readiness) {
          setRuntimeStatus("unavailable");
          setRuntimeNotice(
            "Quantix could not check the local document environment. Try preparation again when you are ready.",
          );
          return;
        }
        if (acceptRuntimeReadiness(readiness)) {
          runtimeRetryTimer.current = window.setTimeout(() => {
            runtimeRetryTimer.current = null;
            probeRuntimeReadiness();
          }, RUNTIME_READINESS_RETRY_DELAY_MS);
        }
      })
      .catch(() => {
        if (runtimeProbeDisposed.current) return;
        setRuntimeStatus("unavailable");
        setRuntimeNotice(
          "Quantix could not check the local document environment. Try preparation again when you are ready.",
        );
      })
      .finally(() => {
        runtimeProbeInFlight.current = false;
      });
  }, [acceptRuntimeReadiness]);

  const refreshSettingsSnapshot = useCallback(async () => {
    const snapshotRequestVersion = ++settingsSnapshotRequestVersion.current;
    const statusRequestVersion = ++aiStatusRequestVersion.current;
    try {
      const settings = await inspectApplicationSettings();
      if (!settings) {
        if (statusRequestVersion === aiStatusRequestVersion.current) {
          setAiStatus("unavailable");
        }
        return;
      }
      let ready = false;
      try {
        ready = await exactApplicationAiSelectionIsReady(settings);
      } catch {
        // A failed local approval check is unavailable, while the settings
        // snapshot itself remains useful and current.
      }
      if (snapshotRequestVersion === settingsSnapshotRequestVersion.current) {
        const nextPreferences = settings.general_preferences;
        setSettingsSnapshot(settings);
        setPreferences(nextPreferences);
        applyGeneralApplicationPreferences(nextPreferences);
      }
      if (statusRequestVersion === aiStatusRequestVersion.current) {
        setAiStatus(ready ? "ready" : "unavailable");
      }
    } catch {
      if (statusRequestVersion === aiStatusRequestVersion.current) {
        setAiStatus("unavailable");
      }
    }
  }, []);

  const loadCapabilities = useCallback(() => {
    probeRuntimeReadiness();
    void refreshSettingsSnapshot();
  }, [probeRuntimeReadiness, refreshSettingsSnapshot]);

  const handlePreferencesChange = useCallback(
    (nextPreferences: GeneralApplicationPreferences) => {
      setPreferences(nextPreferences);
      applyGeneralApplicationPreferences(nextPreferences);
    },
    [],
  );
  const handleAiAvailabilityChange = useCallback(
    (available: boolean) => {
      aiStatusRequestVersion.current += 1;
      setAiStatus(available ? "ready" : "unavailable");
      if (available && pendingTenderStart) {
        setAiPreflightReason(
          "The default provider, model, and reasoning selection is ready for the new Tender.",
        );
        setAiPreflightOpen(true);
      }
    },
    [pendingTenderStart],
  );
  const handleSettingsChange = useCallback(
    (settings: ApplicationSettingsView) => setSettingsSnapshot(settings),
    [],
  );

  const loadTrash = useCallback(async () => {
    try {
      const [trash, receipts] = await Promise.all([
        inspectTrashedTenders(),
        inspectDeletionReceipts(),
      ]);
      setTrashedTenders(trash);
      setDeletionReceipts(receipts);
    } catch (reason) {
      setError(readableError(reason));
    }
  }, []);

  useEffect(() => {
    if (initialProjectionAccepted.current) {
      initialProjectionAccepted.current = false;
      return;
    }
    void load();
  }, [load]);

  useEffect(() => {
    runtimeProbeDisposed.current = false;
    void loadCapabilities();
    return () => {
      runtimeProbeDisposed.current = true;
      if (runtimeRetryTimer.current !== null) {
        window.clearTimeout(runtimeRetryTimer.current);
        runtimeRetryTimer.current = null;
      }
    };
  }, [loadCapabilities]);

  useEffect(() => {
    const tenderId = projection?.selected_tender?.tender_id;
    if (!tenderId) return;
    setNavigationHistory((current) =>
      current.entries.length
        ? current
        : { entries: [{ tenderId, surface: "manager" }], index: 0 },
    );
  }, [projection?.selected_tender?.tender_id]);

  useEffect(() => {
    if (!runtimePreparing && runtimeReadiness?.state !== "preparing") return;
    let disposed = false;
    const inspectPreparation = async () => {
      const [progressResult, readinessResult] = await Promise.allSettled([
        inspectRuntimePreparationProgress(),
        inspectRuntimeReadiness(),
      ]);
      if (disposed) return;
      if (progressResult.status === "fulfilled") {
        setRuntimeProgress(progressResult.value);
      }
      if (readinessResult.status === "fulfilled") {
        acceptRuntimeReadiness(readinessResult.value);
      }
    };
    void inspectPreparation();
    const timer = window.setInterval(() => void inspectPreparation(), 500);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [acceptRuntimeReadiness, runtimePreparing, runtimeReadiness?.state]);

  // Package selection/import is intentionally polled from the one native
  // progress record. This also reconnects to an import that was already in
  // flight when the renderer mounted again.
  useEffect(() => {
    let disposed = false;
    const inspectPackage = async () => {
      try {
        const progress = await inspectPackageIntakeProgress();
        if (disposed || !progress) return;
        const kind: WorkspaceOperationKind =
          progress.kind === "start_tender" ? "start_tender" : "add_package";
        if (!operationRef.current) {
          const reattached = {
            kind,
            label:
              kind === "start_tender" ? "Opening package" : "Adding package",
            startedAt: Date.now(),
          };
          operationRef.current = reattached;
          setOperation(reattached);
        }
        setPackageTask((current) => ({
          kind,
          sourceKind:
            current?.sourceKind ?? progress.source_kind ?? "directory",
          progress,
          error: current?.error ?? null,
          cancelled: current?.cancelled ?? false,
        }));
      } catch {
        // Package failures are surfaced by the operation promise; polling is
        // best-effort and must never replace the last truthful task state.
      }
    };
    void inspectPackage();
    const timer =
      operation?.kind === "start_tender" || operation?.kind === "add_package"
        ? window.setInterval(() => void inspectPackage(), 250)
        : undefined;
    return () => {
      disposed = true;
      if (timer !== undefined) window.clearInterval(timer);
    };
  }, [operation?.kind]);

  useEffect(() => {
    if (operation?.kind !== "select_tender") {
      setShowNavigationPanel(false);
      return;
    }
    const timer = window.setTimeout(() => setShowNavigationPanel(true), 400);
    return () => window.clearTimeout(timer);
  }, [operation?.kind, operation?.startedAt]);

  const cancelPackage = useCallback(async () => {
    const operationId = packageTask?.progress?.operation_id;
    if (!operationId || packageTask?.progress?.cancellation_requested) return;
    setPackageTask((current) =>
      current ? { ...current, cancelled: true } : current,
    );
    try {
      const accepted = await cancelPackageIntake(operationId);
      if (!accepted) {
        setPackageTask((current) =>
          current ? { ...current, cancelled: false } : current,
        );
      }
    } catch (reason) {
      setPackageTask((current) =>
        current
          ? { ...current, cancelled: false, error: readableError(reason) }
          : current,
      );
    }
  }, [
    packageTask?.progress?.cancellation_requested,
    packageTask?.progress?.operation_id,
  ]);

  useEffect(() => {
    if (
      runtimeStatus === "checking" ||
      resumedIntakes.current === runtimeStatus
    ) {
      return;
    }
    resumedIntakes.current = runtimeStatus;
    void resumeManagerIntakes().catch(() => {
      if (resumedIntakes.current === runtimeStatus) {
        resumedIntakes.current = null;
      }
    });
  }, [aiStatus, runtimeStatus]);

  useEffect(() => {
    if (retentionOpen) void loadTrash();
  }, [loadTrash, retentionOpen]);

  useEffect(() => {
    if (
      !retentionOpen ||
      !deletionReceipts.some(
        (receipt) => receipt.provider_cleanup_status === "pending",
      )
    ) {
      return;
    }
    const timer = window.setInterval(() => void loadTrash(), 5_000);
    return () => window.clearInterval(timer);
  }, [deletionReceipts, loadTrash, retentionOpen]);

  useEffect(() => {
    const refresh = async () => {
      if (
        isBusy ||
        (document.hidden && !preferences.notify_when_attention_needed) ||
        refreshRunning.current
      ) {
        return;
      }
      refreshRunning.current = true;
      const epoch = projectionEpoch.current;
      try {
        const next = await inspectManagerWorkspace(archivedInspectionTenderId);
        if (epoch === projectionEpoch.current && !operationRef.current) {
          setProjection(next);
        }
      } catch {
        // Keep the last canonical projection visible; explicit actions surface errors.
      } finally {
        refreshRunning.current = false;
      }
    };
    const timer = window.setInterval(() => void refresh(), 2_500);
    return () => window.clearInterval(timer);
  }, [
    archivedInspectionTenderId,
    isBusy,
    preferences.notify_when_attention_needed,
  ]);

  const run = useCallback(
    async <T,>(
      work: () => Promise<T>,
      details: Pick<WorkspaceOperation, "kind" | "label"> = {
        kind: "select_tender",
        label: "Working",
      },
      complete?: (result: T) => Promise<void> | void,
    ) => {
      projectionEpoch.current += 1;
      const nextOperation: WorkspaceOperation = {
        ...details,
        startedAt: Date.now(),
      };
      operationRef.current = nextOperation;
      setOperation(nextOperation);
      setError(null);
      setOperationFailure(null);
      failedOperationRef.current = {
        work,
        details,
        complete: complete as
          ((result: unknown) => Promise<void> | void) | undefined,
      };
      try {
        return await work();
      } catch (reason) {
        const message = readableError(reason);
        if (
          nextOperation.kind === "start_tender" ||
          nextOperation.kind === "add_package"
        ) {
          setPackageTask((current) =>
            current ? { ...current, error: message } : current,
          );
        } else {
          setOperationFailure({ message, label: nextOperation.label });
        }
        return null;
      } finally {
        operationRef.current = null;
        setOperation(null);
      }
    },
    [],
  );

  const retryFailedOperation = useCallback(async () => {
    const failed = failedOperationRef.current;
    if (!failed || operationRef.current) return;
    const result = await run(failed.work, failed.details, failed.complete);
    if (result !== null && failed.complete) await failed.complete(result);
  }, [run]);

  const recordLocation = useCallback((location: WorkspaceLocation) => {
    setNavigationHistory((current) => {
      const active = current.entries[current.index];
      if (active && sameWorkspaceLocation(active, location)) return current;

      // The empty workspace is not a recoverable Host selection once the first
      // Tender is created, so start history at that first real destination.
      if (
        location.tenderId &&
        current.entries.every((entry) => !entry.tenderId)
      ) {
        return { entries: [location], index: 0 };
      }

      const entries = [
        ...current.entries.slice(0, current.index + 1),
        location,
      ];
      return { entries, index: entries.length - 1 };
    });
  }, []);

  const displayLocation = useCallback((location: WorkspaceLocation) => {
    setRecoveryTarget(null);
    setSettingsOpen(location.surface === "settings");
    setRetentionOpen(location.surface === "retention");
    setSettingsSection(location.settingsSection ?? "general");
    if (
      location.surface === "manager" ||
      location.surface === "work" ||
      location.surface === "team" ||
      location.surface === "files"
    ) {
      setView(location.surface);
    }
    if (location.surface === "settings" || location.surface === "retention") {
      setContextOpen(false);
      if (mediaQueryMatches("(max-width: 819px)")) setSidebarOpen(false);
    }
  }, []);

  const navigateToView = useCallback(
    (destination: WorkspaceView) => {
      const tenderId = projection?.selected_tender?.tender_id ?? null;
      if (!tenderId && destination !== "manager") return;
      const location: WorkspaceLocation = {
        tenderId,
        surface: destination,
      };
      if (tenderId) {
        setTenderViews((current) => ({
          ...current,
          [tenderId]: destination,
        }));
      }
      displayLocation(location);
      recordLocation(location);
    },
    [displayLocation, projection?.selected_tender?.tender_id, recordLocation],
  );

  const navigateFromWorkspaceTools = useCallback(
    (destination: WorkspaceView) => {
      navigateToView(destination);
      if (!contextRail) handleContextOpenChange(false);
    },
    [contextRail, handleContextOpenChange, navigateToView],
  );

  const openSettings = useCallback(
    (section: SettingsSection = "general") => {
      setRecoveryTarget(null);
      const location: WorkspaceLocation = {
        tenderId: projection?.selected_tender?.tender_id ?? null,
        surface: "settings",
        settingsSection: section,
      };
      displayLocation(location);
      recordLocation(location);
    },
    [displayLocation, projection?.selected_tender?.tender_id, recordLocation],
  );

  const openRetention = useCallback(() => {
    setRecoveryTarget(null);
    const location: WorkspaceLocation = {
      tenderId: projection?.selected_tender?.tender_id ?? null,
      surface: "retention",
    };
    displayLocation(location);
    recordLocation(location);
    void loadTrash();
  }, [
    displayLocation,
    loadTrash,
    projection?.selected_tender?.tender_id,
    recordLocation,
  ]);

  const travelHistory = useCallback(
    async (direction: -1 | 1) => {
      const targetIndex = navigationHistory.index + direction;
      const location = navigationHistory.entries[targetIndex];
      if (!location || operationRef.current) return;

      const currentTenderId = projection?.selected_tender?.tender_id ?? null;
      if (!location.tenderId && currentTenderId) return;
      if (location.tenderId && location.tenderId !== currentTenderId) {
        const complete = (next: ManagerWorkspaceProjection) => {
          setProjection(next);
          displayLocation(location);
          if (location.surface === "retention") void loadTrash();
          setNavigationHistory((current) =>
            current.entries[targetIndex] &&
            sameWorkspaceLocation(current.entries[targetIndex], location)
              ? { ...current, index: targetIndex }
              : current,
          );
        };
        const next = await run(
          () =>
            selectManagerWorkspaceTenderWithTimeout(
              location.tenderId as string,
            ),
          {
            kind: "select_tender",
            label: `Opening ${projection?.catalogue.find((tender) => tender.tender_id === location.tenderId)?.name ?? "Tender"}`,
          },
          complete,
        );
        if (!next) return;
        complete(next);
        return;
      }
      displayLocation(location);
      if (location.surface === "retention") void loadTrash();
      setNavigationHistory((current) =>
        current.entries[targetIndex] &&
        sameWorkspaceLocation(current.entries[targetIndex], location)
          ? { ...current, index: targetIndex }
          : current,
      );
    },
    [
      displayLocation,
      loadTrash,
      navigationHistory.entries,
      navigationHistory.index,
      projection?.selected_tender?.tender_id,
      run,
    ],
  );

  const toggleSidebarFromTitleBar = useCallback(() => {
    const next = !sidebarOpen;
    setSidebarOpen(next);
    if (next && !contextRail) setContextOpen(false);
  }, [contextRail, sidebarOpen]);

  useEffect(() => {
    const navigateWithKeyboard = (event: KeyboardEvent) => {
      if (!event.altKey || event.ctrlKey || event.metaKey) return;
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        void travelHistory(-1);
      } else if (event.key === "ArrowRight") {
        event.preventDefault();
        void travelHistory(1);
      }
    };
    window.addEventListener("keydown", navigateWithKeyboard);
    return () => window.removeEventListener("keydown", navigateWithKeyboard);
  }, [travelHistory]);

  const prepareRuntime = useCallback(async () => {
    if (runtimePreparing) return;
    setRuntimePreparing(true);
    setRuntimeNotice(null);
    setRuntimeProgress(null);
    try {
      const readiness = await repairRuntimeReadiness();
      acceptRuntimeReadiness(readiness);
      setRuntimeNotice(
        readiness.state === "ready"
          ? "Document tools are ready. Quantix can continue this Tender when the exact AI selection is ready."
          : readiness.issues.includes("insufficient_disk_space")
            ? "Quantix needs at least 2 GB of free space in the Application Home, but that amount is not currently available. Free some space, then try again. Your Tender remains safe."
            : "Document-tool preparation did not complete. Your Tender remains safe; review Diagnostics or try again.",
      );
    } catch {
      setRuntimeStatus("unavailable");
      setRuntimeNotice(
        "Document-tool preparation stopped before anything was published. Your Tender remains safe; try again when ready.",
      );
    } finally {
      setRuntimePreparing(false);
    }
  }, [acceptRuntimeReadiness, runtimePreparing]);

  const deferRuntimePreparation = useCallback(() => {
    setRuntimeNotice(
      "Not now. Your registered Tender remains readable while document tools wait for preparation.",
    );
  }, []);

  const cancelRuntime = useCallback(async () => {
    try {
      const cancelled = await cancelRuntimePreparation();
      setRuntimeNotice(
        cancelled
          ? "Preparation cancelled. Completed verified downloads are kept for a later retry; your Tender is unchanged."
          : "No active document-tool preparation needed cancellation.",
      );
    } catch {
      setRuntimeNotice(
        "Quantix could not request cancellation. Preparation may still be finishing safely in the background.",
      );
    }
  }, []);

  const continueStartTender = useCallback(
    async (
      kind: TenderPackageSourceKind,
      localOnly = false,
    ): Promise<boolean> => {
      setPackageTask({
        kind: "start_tender",
        sourceKind: kind,
        progress: null,
        error: null,
        cancelled: false,
      });
      const setup = await ensureQuantixSetup();
      if (setup.state !== "ready" && setup.state !== "warning") {
        setPackageTask({
          kind: "start_tender",
          sourceKind: kind,
          progress: null,
          error: `Quantix workspace check failed: ${setup.issues.join(", ").replace(/_/g, " ")}.`,
          cancelled: false,
        });
        return false;
      }
      const next = await run(
        () => startManagerTender(kind, localOnly),
        {
          kind: "start_tender",
          label: "Opening package",
        },
        async (result) => {
          if (!result) return;
          setPackageTask(null);
          setProjection(result);
          const tenderId = result.selected_tender?.tender_id ?? null;
          const location: WorkspaceLocation = { tenderId, surface: "manager" };
          displayLocation(location);
          recordLocation(location);
        },
      );
      if (next) {
        setPackageTask(null);
        setProjection(next);
        const tenderId = next.selected_tender?.tender_id ?? null;
        const location: WorkspaceLocation = { tenderId, surface: "manager" };
        displayLocation(location);
        recordLocation(location);
      } else if (!operationRef.current) {
        setPackageTask((current) => (current?.error ? current : null));
        window.setTimeout(() => packageFocusRef.current?.focus(), 0);
      }
      return Boolean(next);
    },
    [displayLocation, recordLocation, run],
  );

  const startTender = useCallback(
    async (kind: TenderPackageSourceKind): Promise<boolean> => {
      if (aiPreflightInFlight.current) return false;
      aiPreflightInFlight.current = true;
      const snapshotRequestVersion = ++settingsSnapshotRequestVersion.current;
      const statusRequestVersion = ++aiStatusRequestVersion.current;
      setAiPreflightChecking(true);
      setRecoveryTarget(null);
      setError(null);
      try {
        const settings = await inspectApplicationSettings();
        if (snapshotRequestVersion === settingsSnapshotRequestVersion.current) {
          setSettingsSnapshot(settings);
        }
        const ready = await exactApplicationAiSelectionIsReady(settings);
        if (statusRequestVersion === aiStatusRequestVersion.current) {
          setAiStatus(ready ? "ready" : "unavailable");
        }
        if (ready) return await continueStartTender(kind);
        setPendingTenderStart(kind);
        setAiPreflightReason(
          settings.ai_execution_selection
            ? "The default provider, model, or reasoning selection is not currently approved and available."
            : "No default AI provider and model is selected for new Tenders.",
        );
        setAiPreflightOpen(true);
        return false;
      } catch {
        setPendingTenderStart(kind);
        setAiPreflightReason(
          "Quantix could not verify the default AI setup. You can review AI & Models or continue with local-only Tender work.",
        );
        setAiPreflightOpen(true);
        return false;
      } finally {
        aiPreflightInFlight.current = false;
        setAiPreflightChecking(false);
      }
    },
    [continueStartTender],
  );

  const selectTender = useCallback(
    async (tenderId: string) => {
      setPendingTenderStart(null);
      setAiPreflightOpen(false);
      const tender = projection?.catalogue.find(
        (candidate) => candidate.tender_id === tenderId,
      );
      if (tender?.state === "recovery_required") {
        setRecoveryRequestedAction(null);
        setRecoveryTarget(tender);
        setSettingsOpen(false);
        setRetentionOpen(false);
        setContextOpen(false);
        setOperationFailure(null);
        if (mediaQueryMatches("(max-width: 819px)")) setSidebarOpen(false);
        return;
      }
      setRecoveryRequestedAction(null);
      setRecoveryTarget(null);
      const tenderName = tender?.name ?? "Tender";
      const next = await run(
        () => selectManagerWorkspaceTenderWithTimeout(tenderId),
        {
          kind: "select_tender",
          label: `Opening ${tenderName}`,
        },
        async (result) => {
          setProjection(result);
          const destination = tenderViews[tenderId] ?? "manager";
          const location: WorkspaceLocation = {
            tenderId,
            surface: destination,
          };
          displayLocation(location);
          recordLocation(location);
          if (mediaQueryMatches("(max-width: 819px)")) setSidebarOpen(false);
        },
      );
      if (next) {
        setProjection(next);
        const destination = tenderViews[tenderId] ?? "manager";
        const location: WorkspaceLocation = {
          tenderId,
          surface: destination,
        };
        displayLocation(location);
        recordLocation(location);
        if (mediaQueryMatches("(max-width: 819px)")) {
          setSidebarOpen(false);
        }
      }
    },
    [displayLocation, projection?.catalogue, recordLocation, run, tenderViews],
  );

  const openRecoveredTender = useCallback(
    async (tenderId: string) => {
      setRecoveryTarget(null);
      const next = await run(
        () => selectManagerWorkspaceTenderWithTimeout(tenderId),
        {
          kind: "select_tender",
          label: "Opening recovered Tender",
        },
      );
      if (!next) return;
      setProjection(next);
      const destination = tenderViews[tenderId] ?? "manager";
      const location: WorkspaceLocation = {
        tenderId,
        surface: destination,
      };
      displayLocation(location);
      recordLocation(location);
      if (mediaQueryMatches("(max-width: 819px)")) setSidebarOpen(false);
    },
    [displayLocation, recordLocation, run, tenderViews],
  );

  const importPackage = useCallback(
    async (kind: TenderPackageSourceKind) => {
      const tenderId = projection?.selected_tender?.tender_id;
      if (!tenderId) return;
      setPackageTask({
        kind: "add_package",
        sourceKind: kind,
        progress: null,
        error: null,
        cancelled: false,
      });
      const result = await run(
        () => chooseAndImportTenderPackage(tenderId, kind),
        { kind: "add_package", label: "Adding package" },
      );
      if (result) {
        setPackageTask(null);
        await load();
      }
      if (!result && !packageTask?.error) {
        setPackageTask((current) => (current?.error ? current : null));
        window.setTimeout(() => packageFocusRef.current?.focus(), 0);
      }
    },
    [load, packageTask?.error, projection?.selected_tender?.tender_id, run],
  );

  const sendMessage = useCallback(
    async (body: string) => {
      const tenderId = projection?.selected_tender?.tender_id;
      if (!tenderId) return false;
      const next = await run(
        () =>
          recordEngineerWorkspaceMessage(
            tenderId,
            body,
            [],
            contextRefsByTender[tenderId] ?? [],
          ),
        { kind: "save_message", label: "Saving message" },
        (result) => setProjection(result),
      );
      if (!next) return false;
      setProjection(next);
      setContextRefsByTender((current) => ({ ...current, [tenderId]: [] }));
      return true;
    },
    [contextRefsByTender, projection?.selected_tender?.tender_id, run],
  );

  const retryIntake = useCallback(async () => {
    const tenderId = projection?.selected_tender?.tender_id;
    if (!tenderId) return;
    const succeeded = await run(
      async () => {
        await retryManagerIntake(tenderId);
        return true;
      },
      { kind: "retry_intake", label: "Retrying Tender intake" },
      async () => load(),
    );
    if (succeeded) await load();
  }, [load, projection?.selected_tender?.tender_id, run]);

  const reportFocusedCommandFailure = useCallback(() => {
    setOperationFailure({
      message: "Quantix could not complete that action.",
      label: "Tender decision workspace",
    });
  }, []);

  const startFocusedTask = useCallback(async () => {
    const tenderId = projection?.selected_tender?.tender_id;
    const task = projection?.work.tasks.find(
      (candidate) => candidate.production_task_id === focusedTaskId,
    );
    if (!tenderId || !task) return false;
    const succeeded = await run(
      async () => {
        await runProductionTask(tenderId, task.production_task_id);
        return true;
      },
      { kind: "run_task", label: "Starting the task" },
      async () => load(),
    );
    if (succeeded) await load();
    return succeeded === true;
  }, [
    focusedTaskId,
    load,
    projection?.selected_tender?.tender_id,
    projection?.work.tasks,
    run,
  ]);

  const stopFocusedTask = useCallback(async () => {
    const tenderId = projection?.selected_tender?.tender_id;
    const task = projection?.work.tasks.find(
      (candidate) => candidate.production_task_id === focusedTaskId,
    );
    if (!tenderId || !task?.current_run_id) return false;
    const succeeded = await run(
      async () => {
        await interruptAgentRun(tenderId, task.current_run_id!);
        return true;
      },
      { kind: "stop_task", label: "Stopping the run" },
      async () => load(),
    );
    if (succeeded) await load();
    return succeeded === true;
  }, [
    focusedTaskId,
    load,
    projection?.selected_tender?.tender_id,
    projection?.work.tasks,
    run,
  ]);

  const requestFocusedTaskChange = useCallback(async () => {
    const tenderId = projection?.selected_tender?.tender_id;
    const task = projection?.work.tasks.find(
      (candidate) => candidate.production_task_id === focusedTaskId,
    );
    if (!tenderId || !task) return false;
    const succeeded = await run(
      () =>
        recordEngineerWorkspaceMessage(
          tenderId,
          taskChangeRequestBody(task),
          [],
          [],
        ),
      { kind: "request_change", label: "Sending the change request" },
      (next) => setProjection(next),
    );
    if (!succeeded) return false;
    setFocusedTaskId(null);
    navigateToView("manager");
    return true;
  }, [
    focusedTaskId,
    navigateToView,
    projection?.selected_tender?.tender_id,
    projection?.work.tasks,
    run,
  ]);

  const openEvidenceReview = useCallback(
    (target: EvidenceReviewTarget, conflicts?: EvidenceReviewConflict[]) => {
      const tenderId = projection?.selected_tender?.tender_id;
      if (!tenderId) return;
      setEvidenceReview({ target, conflicts: conflicts ?? [] });
      if (conflicts) return;
      const recordTargets = citedRecordTargets(projection);
      if (recordTargets.length === 0) return;
      void (async () => {
        try {
          const inspections = await Promise.all(
            recordTargets.map((recordTarget) =>
              inspectTenderRecord(
                tenderId,
                recordTarget.recordId,
                recordTarget.version,
              ),
            ),
          );
          const nextConflicts = evidenceConflictsForTarget(inspections, target);
          setEvidenceReview((current) =>
            current && current.target === target
              ? { target, conflicts: nextConflicts }
              : current,
          );
        } catch {
          // The review stays open without the conflicting-source context.
        }
      })();
    },
    [projection],
  );

  const handleMessageReference = useCallback(
    (reference: WorkspaceMessageReference) => {
      if (reference.kind === "source_evidence") {
        openEvidenceReview({
          artifactId: reference.reference,
          version: reference.version,
          ordinal: reference.evidence_ordinal,
          label: reference.label,
        });
      } else if (reference.kind === "tender_record") {
        setRecordDecision({
          focusTarget: {
            recordId: reference.reference,
            version: reference.version,
          },
        });
      }
    },
    [openEvidenceReview],
  );

  const closeEvidenceReview = useCallback(() => setEvidenceReview(null), []);
  const closeRecordDecision = useCallback(() => setRecordDecision(null), []);
  const closeChangeReview = useCallback(() => setChangeReviewOpen(false), []);
  const handleChangeDecided = useCallback(async () => {
    setChangeReviewOpen(false);
    await load();
  }, [load]);
  const handleRecordDecided = useCallback(async () => {
    setRecordDecision(null);
    setEvidenceReview(null);
    await load();
  }, [load]);

  const decisionTargets = useMemo(() => {
    const focus = recordDecision?.focusTarget;
    const cited = citedRecordTargets(projection);
    if (!focus) return cited;
    return [
      focus,
      ...cited.filter(
        (target) =>
          !(
            target.recordId === focus.recordId &&
            target.version === focus.version
          ),
      ),
    ];
  }, [projection, recordDecision]);
  const canDecideCitedRecords =
    projection?.current_action.kind === "answer_manager_question" &&
    citedRecordTargets(projection).length > 0;
  const focusedTask =
    focusedTaskId && projection
      ? (projection.work.tasks.find(
          (task) => task.production_task_id === focusedTaskId,
        ) ?? null)
      : null;
  const evidenceReviewOriginLabel = recordDecision
    ? "record decision"
    : "Manager conversation";

  const rebindIntakeProvider = useCallback(async () => {
    const tenderId = projection?.selected_tender?.tender_id;
    if (!tenderId) return;
    const succeeded = await run(
      async () => {
        await rebindManagerIntakeProvider(tenderId);
        return true;
      },
      { kind: "rebind_intake", label: "Reconnecting Tender intake" },
      async () => load(),
    );
    if (succeeded) await load();
  }, [load, projection?.selected_tender?.tender_id, run]);

  const createBackup = useCallback(
    async (tender: ManagerWorkspaceTender) => {
      setBackupConfirmation(null);
      const record = await run(
        () => createTenderBackup(tender.tender_id),
        { kind: "create_backup", label: "Creating verified backup" },
        async () => load(),
      );
      if (record) setBackupConfirmation(record.created_at);
    },
    [load, run],
  );

  const inspectBackups = useCallback((tender: ManagerWorkspaceTender) => {
    setBackupConfirmation(null);
    setRecoveryRequestedAction(null);
    setRecoveryTarget(tender);
    setSettingsOpen(false);
    setRetentionOpen(false);
    setContextOpen(false);
    if (mediaQueryMatches("(max-width: 819px)")) setSidebarOpen(false);
  }, []);

  const applyRetentionAction = useCallback(async () => {
    const tenderId = retentionAction?.tender.tender_id;
    const action = retentionAction?.kind;
    const rationale = retentionRationale.trim();
    if (!tenderId || !action || !rationale) return;
    const complete = async () => {
      setRetentionAction(null);
      setRetentionRationale("");
      if (action === "restore") {
        await selectTender(tenderId);
      } else {
        await Promise.all([load(), loadTrash()]);
        if (action === "trash") {
          const location: WorkspaceLocation = {
            tenderId: null,
            surface: "retention",
          };
          displayLocation(location);
          setNavigationHistory((current) => {
            const entries = current.entries.filter(
              (entry) => entry.tenderId !== tenderId,
            );
            if (
              !entries.some((entry) => sameWorkspaceLocation(entry, location))
            )
              entries.push(location);
            return { entries, index: entries.length - 1 };
          });
        }
      }
    };
    const succeeded = await run(
      async () => {
        if (action === "archive") {
          await archiveTender(tenderId, rationale);
        } else if (action === "trash") {
          await trashTender(tenderId, rationale);
        } else {
          await restoreArchivedTender(tenderId, rationale);
        }
        return true;
      },
      { kind: "retention", label: "Recording retention decision" },
      complete,
    );
    if (!succeeded) return;
    await complete();
  }, [
    displayLocation,
    load,
    loadTrash,
    projection?.selected_tender?.tender_id,
    retentionAction,
    retentionRationale,
    run,
    selectTender,
  ]);

  const applyRename = useCallback(async () => {
    const name = renameValue.trim();
    if (!renameTarget || !name || name === renameTarget.name) return;
    const result = await run(() => reviseTender(renameTarget.tender_id, name), {
      kind: "rename_tender",
      label: "Renaming Tender",
    });
    if (!result) return;
    setRenameTarget(null);
    setRenameValue("");
    await load();
  }, [load, renameTarget, renameValue, run]);

  const applyTrashAction = useCallback(async () => {
    const rationale = retentionRationale.trim();
    if (
      !trashAction ||
      !rationale ||
      (trashAction.kind === "purge" &&
        permanentDeleteConfirmation !== trashAction.record.tender_name)
    ) {
      return;
    }
    const { kind, record } = trashAction;
    const complete = async () => {
      setTrashAction(null);
      setRetentionRationale("");
      setPermanentDeleteConfirmation("");
      await loadTrash();
      if (kind === "restore") {
        await load();
        if (record.deletion_source === "recovery_required") {
          setRecoveryTarget({
            tender_id: record.tender_id,
            name: record.tender_name,
            revision: 0,
            phase: "intake",
            needs_engineer: true,
            state: "recovery_required",
            can_archive: false,
            can_delete: true,
            last_activity_at: null,
          });
          setRetentionOpen(false);
          setSettingsOpen(false);
        } else {
          await selectTender(record.tender_id);
        }
      }
    };
    const succeeded = await run(
      async () => {
        if (kind === "restore") {
          await restoreTrashedTender(record.deletion_id, rationale);
        } else if (record.deletion_source === "recovery_required") {
          await purgeRecoveryRequiredTender(
            record.tender_id,
            rationale,
            permanentDeleteConfirmation,
          );
        } else {
          await purgeTrashedTender(
            record.deletion_id,
            rationale,
            permanentDeleteConfirmation,
          );
        }
        return true;
      },
      {
        kind: kind === "restore" ? "retention" : "trash",
        label: kind === "restore" ? "Restoring Tender" : "Updating Trash",
      },
      complete,
    );
    if (!succeeded) return;
    await complete();
  }, [
    load,
    loadTrash,
    permanentDeleteConfirmation,
    retentionRationale,
    run,
    selectTender,
    trashAction,
  ]);

  const moveRecoveryRequiredTenderToTrash = useCallback(
    async (rationale: string) => {
      if (!recoveryTarget) throw new Error("No recovery Tender is selected.");
      const moved = await run(
        () => trashRecoveryRequiredTender(recoveryTarget.tender_id, rationale),
        { kind: "trash", label: "Moving recovery Tender to Trash" },
      );
      if (!moved)
        throw new Error("Quantix could not move this Tender to Trash.");
      setRecoveryRequestedAction(null);
      await Promise.all([load(), loadTrash()]);
      openRetention();
    },
    [load, loadTrash, openRetention, recoveryTarget, run],
  );

  const permanentlyDeleteRecoveryRequiredTender = useCallback(
    async (rationale: string, confirmationTenderName: string) => {
      if (!recoveryTarget) throw new Error("No recovery Tender is selected.");
      const receipt = await run(
        () =>
          purgeRecoveryRequiredTender(
            recoveryTarget.tender_id,
            rationale,
            confirmationTenderName,
          ),
        { kind: "trash", label: "Permanently deleting recovery Tender" },
      );
      if (!receipt) {
        throw new Error("Quantix could not permanently delete this Tender.");
      }
      setRecoveryRequestedAction(null);
      await Promise.all([load(), loadTrash()]);
      openRetention();
    },
    [load, loadTrash, openRetention, recoveryTarget, run],
  );

  const selected = projection?.selected_tender ?? null;
  const displayedTender = recoveryTarget ?? selected;
  const runTenderSearch = useCallback(async () => {
    const tenderId = projection?.selected_tender?.tender_id;
    if (!tenderId) return;
    const query = (searchQueries[tenderId] ?? "").trim();
    if (query.length < 2 || searchingTenderId) return;
    setSearchingTenderId(tenderId);
    setError(null);
    try {
      const results = await searchManagerWorkspace(tenderId, query);
      setSearchResults((current) => ({ ...current, [tenderId]: results }));
    } catch (reason) {
      setError(readableError(reason));
    } finally {
      setSearchingTenderId(null);
    }
  }, [
    projection?.selected_tender?.tender_id,
    searchQueries,
    searchingTenderId,
  ]);
  const activeTenders =
    projection?.catalogue.filter((tender) => tender.state !== "archived") ?? [];
  const archivedTenders =
    projection?.catalogue.filter((tender) => tender.state === "archived") ?? [];
  const visibleTrash = trashedTenders.filter(
    (record) => !["restored", "purged"].includes(record.state),
  );
  const reconciliationNotice = useMemo(
    () =>
      startupReconciliation
        ? startupReconciliationNotice(startupReconciliation)
        : null,
    [startupReconciliation],
  );
  const activeCounts = useMemo(() => {
    if (!projection) return 0;
    return (
      projection.team.active_agent_runs +
      projection.team.waiting_tasks +
      projection.team.needs_engineer
    );
  }, [projection]);
  const selectedIsReadOnly = selected?.state === "archived";
  const intakeWorking = ["working", "waiting"].includes(
    projection?.intake?.status ?? "",
  );
  const titleBarMenus = useMemo<readonly WindowTitleBarMenu[]>(
    () => [
      {
        id: "file",
        label: "File",
        items: [
          {
            id: "new-tender",
            label: "New Tender",
            disabled: isBusy,
            onSelect: () => void startTender("directory"),
          },
          { id: "file-package-separator", type: "separator" },
          {
            id: "add-package-folder",
            label: "Add package folder",
            disabled:
              !selected || selectedIsReadOnly || isBusy || intakeWorking,
            onSelect: () => void importPackage("directory"),
          },
          {
            id: "add-package-zip",
            label: "Add ZIP",
            disabled:
              !selected || selectedIsReadOnly || isBusy || intakeWorking,
            onSelect: () => void importPackage("zip_archive"),
          },
          { id: "file-places-separator", type: "separator" },
          {
            id: "archived-trash",
            label: "Archived & Trash",
            disabled: isBusy,
            onSelect: openRetention,
          },
          {
            id: "settings",
            label: "Settings",
            disabled: isBusy,
            onSelect: () => openSettings(),
          },
        ],
      },
      {
        id: "edit",
        label: "Edit",
        items: [
          {
            id: "undo",
            label: "Undo",
            shortcut: "Ctrl+Z",
            editCommand: "undo",
          },
          {
            id: "redo",
            label: "Redo",
            shortcut: "Ctrl+Y",
            editCommand: "redo",
          },
          { id: "edit-history-separator", type: "separator" },
          {
            id: "cut",
            label: "Cut",
            shortcut: "Ctrl+X",
            editCommand: "cut",
          },
          {
            id: "copy",
            label: "Copy",
            shortcut: "Ctrl+C",
            editCommand: "copy",
          },
          {
            id: "paste",
            label: "Paste",
            shortcut: "Ctrl+V",
            editCommand: "paste",
          },
          { id: "edit-select-separator", type: "separator" },
          {
            id: "select-all",
            label: "Select all",
            shortcut: "Ctrl+A",
            editCommand: "selectAll",
          },
        ],
      },
      {
        id: "view",
        label: "View",
        items: [
          {
            id: "toggle-tenders",
            label: sidebarOpen ? "Hide Tenders" : "Show Tenders",
            onSelect: toggleSidebarFromTitleBar,
          },
          { id: "view-workspace-separator", type: "separator" },
          ...(
            [
              ["manager", "Manager"],
              ["work", "Work"],
              ["team", "Team"],
              ["files", "Files"],
            ] as const
          ).map(([destination, label]) => ({
            id: `view-${destination}`,
            label,
            disabled: (!selected && destination !== "manager") || isBusy,
            onSelect: () => navigateToView(destination),
          })),
          { id: "view-context-separator", type: "separator" },
          {
            id: "toggle-context",
            label: contextOpen
              ? "Hide Tender workspace"
              : "Show Tender workspace",
            disabled: !selected || isBusy,
            onSelect: toggleContext,
          },
        ],
      },
      {
        id: "help",
        label: "Help",
        items: [
          {
            id: "about-quantix",
            label: "About Quantix & Diagnostics",
            disabled: isBusy,
            onSelect: () => openSettings("about"),
          },
        ],
      },
    ],
    [
      isBusy,
      contextOpen,
      importPackage,
      intakeWorking,
      navigateToView,
      openRetention,
      openSettings,
      selected,
      selectedIsReadOnly,
      sidebarOpen,
      startTender,
      toggleContext,
      toggleSidebarFromTitleBar,
    ],
  );

  if (!projection && !error) {
    return (
      <QuantixWindow
        menus={LOADING_TITLE_BAR_MENUS}
        canToggleSidebar={false}
        motionState="static"
      >
        <div className="workspace-loading" aria-live="polite">
          <LoaderCircle size={22} aria-hidden="true" />
          Opening your Tender workspace…
        </div>
      </QuantixWindow>
    );
  }

  const workspaceTools =
    selected && projection ? (
      <>
        <TenderSearch
          query={searchQueries[selected.tender_id] ?? ""}
          results={searchResults[selected.tender_id] ?? null}
          busy={searchingTenderId === selected.tender_id}
          inputRef={searchInputRef}
          onQueryChange={(value) =>
            setSearchQueries((current) => ({
              ...current,
              [selected.tender_id]: value,
            }))
          }
          onSearch={() => void runTenderSearch()}
          onOpenResult={(kind) => {
            setSearchResults((current) => ({
              ...current,
              [selected.tender_id]: null,
            }));
            navigateFromWorkspaceTools(
              kind === "conversation"
                ? "manager"
                : kind === "work"
                  ? "work"
                  : kind === "agents"
                    ? "team"
                    : "files",
            );
          }}
          onAttach={(hit) => {
            const separator = hit.reference.lastIndexOf(":");
            if (separator < 1 || hit.version === null) return;
            const artifactId = hit.reference.slice(0, separator);
            const ordinal = Number(hit.reference.slice(separator + 1));
            if (!Number.isInteger(ordinal) || ordinal < 1) return;
            const reference: WorkspaceMessageReference = {
              kind: "source_evidence",
              reference: artifactId,
              version: hit.version,
              evidence_ordinal: ordinal,
              label: hit.title,
              detail: hit.detail,
            };
            setContextRefsByTender((current) => {
              const existing = current[selected.tender_id] ?? [];
              if (
                existing.some(
                  (candidate) =>
                    candidate.reference === reference.reference &&
                    candidate.version === reference.version &&
                    candidate.evidence_ordinal === reference.evidence_ordinal,
                )
              ) {
                return current;
              }
              return {
                ...current,
                [selected.tender_id]: [...existing, reference],
              };
            });
          }}
        />
        <nav
          className="tender-workspace-tools__navigation"
          aria-label="Tender workspace navigation"
        >
          {(
            [
              ["manager", "Manager", MessageSquare],
              ["work", "Work", ListChecks],
              ["team", "Team", Users],
              ["files", "Files", Folder],
            ] as const
          ).map(([destination, label, Icon]) => (
            <button
              key={destination}
              type="button"
              aria-current={view === destination ? "page" : undefined}
              onClick={() => navigateFromWorkspaceTools(destination)}
            >
              <Icon size={17} aria-hidden="true" />
              {label}
              {view === destination ? (
                <m.span
                  className="tender-workspace-tools__indicator"
                  layoutId="active-workspace-tab"
                  aria-hidden="true"
                />
              ) : null}
            </button>
          ))}
        </nav>
      </>
    ) : null;

  return (
    <QuantixWindow
      menus={titleBarMenus}
      motionState="ready"
      reducedMotion={preferences.reduced_motion}
      sidebarVisible={sidebarOpen}
      canToggleSidebar={!isBusy}
      onToggleSidebar={toggleSidebarFromTitleBar}
      canGoBack={!isBusy && navigationHistory.index > 0}
      onBack={() => void travelHistory(-1)}
      canGoForward={
        !isBusy &&
        navigationHistory.index < navigationHistory.entries.length - 1
      }
      onForward={() => void travelHistory(1)}
    >
      <LayoutGroup id="quantix-workspace">
        <div
          className={`manager-workspace${sidebarOpen ? " is-sidebar-open" : " is-sidebar-closed"}${selected && !recoveryTarget && projection && !settingsOpen && !retentionOpen && contextOpen ? " has-context" : ""}${displayedTender && !settingsOpen && !retentionOpen ? " has-workspace-bar" : ""}`}
          data-testid="manager-workspace"
        >
          {displayedTender && !settingsOpen && !retentionOpen ? (
            <m.header className="manager-workspace__bar" layout="position">
              <div className="manager-workspace__bar-title">
                <h1>{displayedTender.name}</h1>
              </div>
              {projection && selected && !recoveryTarget ? (
                <div className="manager-workspace__bar-actions">
                  <button
                    ref={contextTriggerRef}
                    className="manager-workspace__context-toggle"
                    type="button"
                    aria-label={
                      contextOpen
                        ? "Hide Tender workspace"
                        : "Show Tender workspace"
                    }
                    aria-expanded={contextOpen}
                    aria-controls="tender-workspace-panel"
                    onClick={toggleContext}
                  >
                    <PanelRightOpen size={18} aria-hidden="true" />
                    <span>Workspace</span>
                    {activeCounts > 0 ? <small>{activeCounts}</small> : null}
                  </button>
                </div>
              ) : null}
            </m.header>
          ) : null}

          <AnimatePresence initial={false}>
            {sidebarOpen ? (
              <m.aside
                key="workspace-sidebar"
                className="manager-workspace__sidebar"
                aria-label="Tenders"
                layout="position"
                initial={{ opacity: 0, x: -14 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, x: -14 }}
                transition={quantixSmoothEase}
              >
                <div className="manager-workspace__sidebar-brand">
                  <QuantixMark
                    className="manager-workspace__sidebar-brand-mark"
                    label="Quantix"
                  />
                </div>
                <button
                  className="manager-workspace__new-tender"
                  type="button"
                  disabled={isBusy}
                  onClick={() => void startTender("directory")}
                >
                  <Plus size={17} aria-hidden="true" />
                  New Tender
                </button>
                <div className="manager-workspace__sidebar-heading">
                  <span>Tenders</span>
                </div>
                <nav aria-label="Tenders">
                  {activeTenders.map((tender) => (
                    <TenderButton
                      key={tender.tender_id}
                      tender={tender}
                      selected={
                        recoveryTarget?.tender_id === tender.tender_id ||
                        (!recoveryTarget &&
                          selected?.tender_id === tender.tender_id)
                      }
                      busy={isBusy}
                      onSelect={() => void selectTender(tender.tender_id)}
                      onCreateBackup={() => void createBackup(tender)}
                      onInspectBackups={() => inspectBackups(tender)}
                      onRename={() => {
                        setRenameTarget(tender);
                        setRenameValue(tender.name);
                      }}
                      onArchive={() => {
                        setRetentionAction({ kind: "archive", tender });
                        setRetentionRationale("");
                      }}
                      onTrash={() => {
                        if (tender.state === "recovery_required") {
                          setRecoveryTarget(tender);
                          setRecoveryRequestedAction("move_to_trash");
                          setSettingsOpen(false);
                          setRetentionOpen(false);
                          setContextOpen(false);
                        } else {
                          setRetentionAction({ kind: "trash", tender });
                          setRetentionRationale("");
                        }
                      }}
                      onPurge={() => {
                        setRecoveryTarget(tender);
                        setRecoveryRequestedAction("delete_permanently");
                        setSettingsOpen(false);
                        setRetentionOpen(false);
                        setContextOpen(false);
                      }}
                    />
                  ))}
                </nav>
                <div className="manager-workspace__sidebar-footer">
                  <button
                    type="button"
                    aria-current={retentionOpen ? "page" : undefined}
                    onClick={openRetention}
                  >
                    <Archive size={17} aria-hidden="true" />
                    Archived &amp; Trash
                    {archivedTenders.length + visibleTrash.length ? (
                      <span>
                        {archivedTenders.length + visibleTrash.length}
                      </span>
                    ) : null}
                  </button>
                  <button
                    type="button"
                    aria-current={settingsOpen ? "page" : undefined}
                    onClick={() => openSettings()}
                  >
                    <Settings size={17} aria-hidden="true" />
                    Settings
                  </button>
                </div>
              </m.aside>
            ) : null}
          </AnimatePresence>

          <m.div className="manager-workspace__main" layout="position">
            {setupWarnings.length > 0 ? (
              <aside
                className="manager-workspace__warning"
                aria-label="Workspace security warning"
              >
                <ShieldAlert size={18} aria-hidden="true" />
                <div>
                  <strong>Review workspace security</strong>
                  <span>{setupWarnings.map(setupWarningCopy).join(" ")}</span>
                </div>
                <button type="button" onClick={() => openSettings()}>
                  Review Settings
                </button>
              </aside>
            ) : null}
            {reconciliationNotice ? (
              <aside
                className="manager-workspace__warning"
                role="status"
                aria-label="Startup cleanup"
              >
                <Info size={18} aria-hidden="true" />
                <div>
                  <span>{reconciliationNotice.summary}</span>
                  <details>
                    <summary>Technical details</summary>
                    <small>{reconciliationNotice.details}</small>
                  </details>
                </div>
              </aside>
            ) : null}
            {error ? (
              <div className="manager-workspace__error" role="alert">
                <CircleAlert size={18} aria-hidden="true" />
                <span>{error}</span>
                <button type="button" onClick={() => void load()}>
                  Try again
                </button>
              </div>
            ) : null}
            {operationFailure ? (
              <div className="manager-workspace__error" role="alert">
                <CircleAlert size={18} aria-hidden="true" />
                <span>
                  {operationFailure.label} failed: {operationFailure.message}
                </span>
                <button
                  type="button"
                  onClick={() => void retryFailedOperation()}
                >
                  Retry {operationFailure.label.toLowerCase()}
                </button>
              </div>
            ) : null}
            <div
              className="manager-workspace__live-region"
              aria-live="polite"
              aria-atomic="true"
            >
              {operation ? `${operation.label}…` : null}
            </div>
            {operation &&
            operation.kind !== "start_tender" &&
            operation.kind !== "add_package" &&
            operation.kind !== "select_tender" ? (
              <div
                className="manager-workspace__operation-feedback"
                role="status"
              >
                {operation.label}…
              </div>
            ) : null}
            {!operation && backupConfirmation ? (
              <div
                className="manager-workspace__operation-feedback"
                role="status"
              >
                {`Verified backup created at ${new Date(backupConfirmation).toLocaleString()}. Find it under Inspect backups.`}
              </div>
            ) : null}

            <m.div
              key={
                recoveryTarget
                  ? `recovery-${recoveryTarget.tender_id}`
                  : settingsOpen
                    ? `settings-${settingsSection}`
                    : retentionOpen
                      ? "retention"
                      : selected && projection
                        ? `${selected.tender_id}-${view}`
                        : "empty"
              }
              className={`manager-workspace__surface${!selected && !recoveryTarget && projection ? " manager-workspace__surface--empty" : ""}`}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.22, ease: [0.2, 0, 0, 1] }}
            >
              {showNavigationPanel && operation?.kind === "select_tender" ? (
                <NavigationTaskPanel label={operation.label} />
              ) : packageTask ? (
                <PackageTaskPanel
                  task={packageTask}
                  onCancel={() => void cancelPackage()}
                  onChooseAgain={() => {
                    const taskKind = packageTask.kind;
                    const sourceKind = packageTask.sourceKind;
                    setPackageTask(null);
                    setError(null);
                    window.setTimeout(() => {
                      if (taskKind === "start_tender") {
                        void startTender(sourceKind);
                      } else {
                        void importPackage(sourceKind);
                      }
                    }, 0);
                  }}
                />
              ) : recoveryTarget ? (
                <WorkspaceRecoveryCenter
                  tenderId={recoveryTarget.tender_id}
                  tenderName={recoveryTarget.name}
                  variant={
                    recoveryTarget.state === "recovery_required"
                      ? "recovery"
                      : "backups"
                  }
                  requestedAction={recoveryRequestedAction}
                  onRequestedActionHandled={() =>
                    setRecoveryRequestedAction(null)
                  }
                  onClose={() => {
                    setRecoveryRequestedAction(null);
                    setRecoveryTarget(null);
                  }}
                  onRecovered={async () => {
                    const recoveredTenderId = recoveryTarget.tender_id;
                    await openRecoveredTender(recoveredTenderId);
                  }}
                  onMoveToTrash={moveRecoveryRequiredTenderToTrash}
                  onDeletePermanently={permanentlyDeleteRecoveryRequiredTender}
                />
              ) : settingsOpen ? (
                <ApplicationSettings
                  onAiAvailabilityChange={handleAiAvailabilityChange}
                  onSettingsChange={handleSettingsChange}
                  onPreferencesChange={handlePreferencesChange}
                  initialSection={settingsSection}
                  selectedTenderId={
                    projection?.selected_tender?.tender_id ?? null
                  }
                  onClose={() => {
                    void refreshSettingsSnapshot().then(() =>
                      navigateToView(view),
                    );
                  }}
                />
              ) : retentionOpen ? (
                <ArchivedTenders
                  tenders={archivedTenders}
                  trash={visibleTrash}
                  receipts={deletionReceipts}
                  busy={isBusy}
                  onSelect={(tenderId) => void selectTender(tenderId)}
                  onTrashAction={(kind, record) => {
                    setTrashAction({ kind, record });
                    setRetentionRationale("");
                    setPermanentDeleteConfirmation("");
                  }}
                />
              ) : selected && projection ? (
                <>
                  {selected.state === "archived" ? (
                    <div className="manager-workspace__archived" role="status">
                      <Archive size={18} aria-hidden="true" />
                      <div>
                        <strong>Archived · read-only</strong>
                        <span>
                          Manager, Work, and Files remain available without
                          changing this Tender.
                        </span>
                      </div>
                      <button
                        type="button"
                        disabled={isBusy}
                        onClick={() =>
                          setRetentionAction({
                            kind: "restore",
                            tender: selected,
                          })
                        }
                      >
                        <Undo2 size={16} aria-hidden="true" /> Restore
                      </button>
                    </div>
                  ) : null}
                  <main className="manager-workspace__content">
                    {view === "manager" ? (
                      rfiReviewOpen &&
                      isRfiActionKind(projection.current_action.kind) ? (
                        <TenderRfiReview
                          tenderId={selected.tender_id}
                          summaries={projection.external_rfis}
                          interpretationFirst={
                            projection.current_action.kind ===
                            "interpret_external_rfi_response"
                          }
                          runtimeReady={runtimeStatus === "ready"}
                          reportCommandFailure={reportFocusedCommandFailure}
                          onRefresh={load}
                          onClose={() => setRfiReviewOpen(false)}
                        />
                      ) : estimateReviewOpen &&
                        isEstimateReviewActionKind(
                          projection.current_action.kind,
                        ) ? (
                        <TenderEstimateReview
                          tenderId={selected.tender_id}
                          runtimeReady={runtimeStatus === "ready"}
                          reportCommandFailure={reportFocusedCommandFailure}
                          onRefresh={load}
                          onOpenCalculations={() => setCalculationsOpen(true)}
                          onClose={() => setEstimateReviewOpen(false)}
                        />
                      ) : focusedActionTenderId === selected.tender_id &&
                        isFocusedActionKind(projection.current_action.kind) ? (
                        <TenderFocusedAction
                          tenderId={selected.tender_id}
                          actionKind={projection.current_action.kind}
                          runtimeReady={runtimeStatus === "ready"}
                          reportCommandFailure={() =>
                            setOperationFailure({
                              message:
                                "Quantix could not complete that action.",
                              label: "Tender decision workspace",
                            })
                          }
                          onManagerRefresh={load}
                          onClose={() => setFocusedActionTenderId(null)}
                        />
                      ) : (
                        <ManagerView
                          key={selected.tender_id}
                          projection={projection}
                          composer={composerDrafts[selected.tender_id] ?? ""}
                          onComposerChange={(value) =>
                            setComposerDrafts((current) => ({
                              ...current,
                              [selected.tender_id]: value,
                            }))
                          }
                          aiStatus={aiStatus}
                          runtimeStatus={runtimeStatus}
                          runtimeReadiness={runtimeReadiness}
                          runtimeProgress={runtimeProgress}
                          runtimePreparing={runtimePreparing}
                          runtimeNotice={runtimeNotice}
                          busy={isBusy}
                          onImport={importPackage}
                          onPrepareRuntime={prepareRuntime}
                          onDeferRuntime={deferRuntimePreparation}
                          onCancelRuntime={cancelRuntime}
                          onRetry={retryIntake}
                          onRebindProvider={rebindIntakeProvider}
                          onSend={sendMessage}
                          onOpenSettings={() => openSettings()}
                          onOpenFocusedAction={() =>
                            setFocusedActionTenderId(selected.tender_id)
                          }
                          onOpenRfiReview={() => setRfiReviewOpen(true)}
                          onOpenEstimateReview={() =>
                            setEstimateReviewOpen(true)
                          }
                          onOpenRecordDecision={() =>
                            setRecordDecision({ focusTarget: null })
                          }
                          onOpenChangeReview={() => setChangeReviewOpen(true)}
                          canDecideCitedRecords={canDecideCitedRecords}
                          onOpenReference={handleMessageReference}
                          onOpenSearch={() => {
                            searchInputRef.current?.focus();
                            searchInputRef.current?.scrollIntoView({
                              behavior: "smooth",
                              block: "center",
                            });
                          }}
                          contextRefs={
                            contextRefsByTender[selected.tender_id] ?? []
                          }
                          onRemoveContext={(reference) =>
                            setContextRefsByTender((current) => ({
                              ...current,
                              [selected.tender_id]: (
                                current[selected.tender_id] ?? []
                              ).filter(
                                (candidate) =>
                                  candidate.reference !== reference,
                              ),
                            }))
                          }
                          onOpenAction={() =>
                            navigateToView(
                              projection.current_action.kind.includes(
                                "package",
                              ) ||
                                projection.current_action.kind ===
                                  "review_intake"
                                ? "files"
                                : "work",
                            )
                          }
                          readOnly={selected.state === "archived"}
                        />
                      )
                    ) : null}
                    {view === "work" ? (
                      <WorkView
                        projection={projection}
                        onOpenTask={(task) =>
                          setFocusedTaskId(task.production_task_id)
                        }
                      />
                    ) : null}
                    {view === "team" ? (
                      <TeamRoom
                        projection={projection}
                        busy={isBusy}
                        readOnly={selected.state === "archived"}
                        composer={composerDrafts[selected.tender_id] ?? ""}
                        onComposerChange={(value) =>
                          setComposerDrafts((current) => ({
                            ...current,
                            [selected.tender_id]: value,
                          }))
                        }
                        onSend={sendMessage}
                        onOpenSearch={() => {
                          searchInputRef.current?.focus();
                          searchInputRef.current?.scrollIntoView({
                            behavior: "smooth",
                            block: "center",
                          });
                        }}
                        contextRefs={
                          contextRefsByTender[selected.tender_id] ?? []
                        }
                        onRemoveContext={(reference) =>
                          setContextRefsByTender((current) => ({
                            ...current,
                            [selected.tender_id]: (
                              current[selected.tender_id] ?? []
                            ).filter(
                              (candidate) => candidate.reference !== reference,
                            ),
                          }))
                        }
                        onOpenReference={handleMessageReference}
                        onClose={() => navigateToView("manager")}
                      />
                    ) : null}
                    {view === "files" ? (
                      <FilesView
                        projection={projection}
                        busy={isBusy}
                        onImport={importPackage}
                        readOnly={selected.state === "archived"}
                      />
                    ) : null}
                  </main>
                  {changeReviewOpen ? (
                    <div className="manager-workspace__overlay">
                      <ChangeReviewSurface
                        tenderId={selected.tender_id}
                        busy={isBusy}
                        reportCommandFailure={reportFocusedCommandFailure}
                        onDecided={() => void handleChangeDecided()}
                        onClose={closeChangeReview}
                      />
                    </div>
                  ) : null}
                  {recordDecision ? (
                    <div className="manager-workspace__overlay">
                      <TenderRecordDecision
                        tenderId={selected.tender_id}
                        recordTargets={decisionTargets}
                        busy={isBusy}
                        onReviewEvidence={openEvidenceReview}
                        reportCommandFailure={reportFocusedCommandFailure}
                        onDecided={() => void handleRecordDecided()}
                        onClose={closeRecordDecision}
                      />
                    </div>
                  ) : null}
                  {focusedTask ? (
                    <div className="manager-workspace__overlay">
                      <WorkTaskDetail
                        tenderId={selected.tender_id}
                        task={focusedTask}
                        tasks={projection.work.tasks}
                        busy={isBusy}
                        readOnly={selected.state === "archived"}
                        onRefresh={load}
                        onStart={() => startFocusedTask()}
                        onStop={() => stopFocusedTask()}
                        onRequestChange={() => requestFocusedTaskChange()}
                        onOpenCalculations={() => setCalculationsOpen(true)}
                        reportCommandFailure={reportFocusedCommandFailure}
                        onClose={() => setFocusedTaskId(null)}
                      />
                    </div>
                  ) : null}
                  {calculationsOpen ? (
                    <div className="manager-workspace__overlay is-raised">
                      <ControlledCalculationView
                        tenderId={selected.tender_id}
                        reportCommandFailure={reportFocusedCommandFailure}
                        onClose={() => setCalculationsOpen(false)}
                      />
                    </div>
                  ) : null}
                  {evidenceReview ? (
                    <div className="manager-workspace__overlay is-raised">
                      <TenderEvidenceReview
                        tenderId={selected.tender_id}
                        target={evidenceReview.target}
                        conflicts={evidenceReview.conflicts}
                        originLabel={evidenceReviewOriginLabel}
                        onOpenTarget={(target) => openEvidenceReview(target)}
                        onClose={closeEvidenceReview}
                      />
                    </div>
                  ) : null}
                </>
              ) : projection ? (
                <main className="manager-workspace__empty-main">
                  <StartTender
                    action={projection.current_action}
                    busy={isBusy}
                    focusRef={packageFocusRef}
                    onStart={startTender}
                  />
                  {aiPreflightOpen && pendingTenderStart ? (
                    <section
                      className="start-tender__ai-note"
                      aria-label="AI setup for the new Tender"
                    >
                      <h3>
                        {aiStatus === "ready"
                          ? "AI is ready for this Tender"
                          : "AI & Models is not fully set up"}
                      </h3>
                      <p>
                        {aiPreflightReason} Local package registration and
                        document work remain available without AI; only
                        AI-required work will wait.
                      </p>
                      {settingsSnapshot?.ai_execution_selection ? (
                        <p>
                          Current default:{" "}
                          {settingsSnapshot.ai_execution_selection.model_id}
                        </p>
                      ) : null}
                      <div className="start-tender__ai-actions">
                        <button
                          type="button"
                          disabled={isBusy}
                          onClick={() => {
                            setAiPreflightOpen(false);
                            setPendingTenderStart(null);
                          }}
                        >
                          Cancel
                        </button>
                        {aiStatus !== "ready" ? (
                          <button
                            type="button"
                            disabled={isBusy}
                            onClick={() => {
                              setAiPreflightOpen(false);
                              openSettings("ai");
                            }}
                          >
                            Set up AI &amp; Models
                          </button>
                        ) : null}
                        <button
                          className="manager-workspace__primary"
                          type="button"
                          disabled={isBusy}
                          onClick={() => {
                            const kind = pendingTenderStart;
                            if (!kind) return;
                            setAiPreflightOpen(false);
                            setPendingTenderStart(null);
                            void continueStartTender(
                              kind,
                              aiStatus !== "ready",
                            );
                          }}
                        >
                          {aiStatus === "ready"
                            ? "Continue New Tender"
                            : "Continue without AI"}
                        </button>
                      </div>
                    </section>
                  ) : null}
                </main>
              ) : null}
            </m.div>
          </m.div>

          <AnimatePresence initial={false}>
            {selected &&
            !recoveryTarget &&
            projection &&
            !settingsOpen &&
            !retentionOpen &&
            contextOpen &&
            contextPresentation === "rail" ? (
              <m.div
                key="workspace-context"
                className="manager-workspace__context-motion"
                initial={{ opacity: 0, x: 18, scale: 0.985 }}
                animate={{ opacity: 1, x: 0, scale: 1 }}
                exit={{ opacity: 0, x: 18, scale: 0.985 }}
                transition={quantixSmoothEase}
              >
                <WorkspaceContextPanel
                  projection={projection}
                  phase={phaseLabel[selected.phase]}
                  tools={workspaceTools}
                  isOpen={contextOpen}
                  onOpenChange={handleContextOpenChange}
                  presentation="rail"
                />
              </m.div>
            ) : null}
          </AnimatePresence>

          {selected &&
          !recoveryTarget &&
          projection &&
          !settingsOpen &&
          !retentionOpen &&
          contextPresentation === "drawer" ? (
            <WorkspaceContextPanel
              projection={projection}
              phase={phaseLabel[selected.phase]}
              tools={workspaceTools}
              isOpen={contextOpen}
              onOpenChange={handleContextOpenChange}
              presentation="drawer"
            />
          ) : null}

          <QuantixDialog
            isOpen={renameTarget !== null}
            title={
              renameTarget ? `Rename ${renameTarget.name}` : "Rename Tender"
            }
            onOpenChange={(open) => {
              if (!open && !isBusy) {
                setRenameTarget(null);
                setRenameValue("");
              }
            }}
          >
            <label
              className="manager-workspace__dialog-field"
              htmlFor="rename-tender-name"
            >
              Tender name
              <input
                id="rename-tender-name"
                autoFocus
                maxLength={200}
                value={renameValue}
                disabled={isBusy}
                onChange={(event) => setRenameValue(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    void applyRename();
                  }
                }}
              />
            </label>
            <div className="retention-dialog__actions">
              <button
                type="button"
                disabled={isBusy}
                onClick={() => {
                  setRenameTarget(null);
                  setRenameValue("");
                }}
              >
                Cancel
              </button>
              <button
                className="manager-workspace__primary"
                type="button"
                disabled={
                  isBusy ||
                  !renameValue.trim() ||
                  renameValue.trim() === renameTarget?.name
                }
                onClick={() => void applyRename()}
              >
                {isBusy ? "Renaming…" : "Rename Tender"}
              </button>
            </div>
          </QuantixDialog>

          {retentionAction ? (
            <m.div
              key="retention-dialog"
              className="retention-dialog__backdrop"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={{ duration: 0.18 }}
            >
              <m.section
                className="retention-dialog"
                role="dialog"
                aria-modal="true"
                aria-labelledby="retention-dialog-title"
                initial={{ opacity: 0, y: 14, scale: 0.975 }}
                animate={{ opacity: 1, y: 0, scale: 1 }}
                transition={quantixSmoothEase}
              >
                <div>
                  <h2 id="retention-dialog-title">
                    {retentionAction.kind === "archive"
                      ? `Archive ${retentionAction.tender.name}?`
                      : retentionAction.kind === "trash"
                        ? `Move ${retentionAction.tender.name} to Trash?`
                        : `Restore ${retentionAction.tender.name}?`}
                  </h2>
                  <p>
                    {retentionAction.kind === "archive"
                      ? "This keeps the complete Tender Store and history in place, but makes the Tender read-only until you restore it."
                      : retentionAction.kind === "trash"
                        ? "This moves the complete Tender Store into recoverable Quantix Trash. It disappears from Tenders, but you can restore it until you separately choose Permanent Delete."
                        : "This returns the same Tender and conversation context to active work."}
                  </p>
                </div>
                <label htmlFor="retention-rationale">
                  Decision rationale
                  <textarea
                    id="retention-rationale"
                    autoFocus
                    rows={3}
                    maxLength={4000}
                    value={retentionRationale}
                    disabled={isBusy}
                    onChange={(event) =>
                      setRetentionRationale(event.target.value)
                    }
                  />
                </label>
                <div className="retention-dialog__actions">
                  <button
                    type="button"
                    disabled={isBusy}
                    onClick={() => {
                      setRetentionAction(null);
                      setRetentionRationale("");
                    }}
                  >
                    Cancel
                  </button>
                  <button
                    className="manager-workspace__primary"
                    type="button"
                    disabled={isBusy || !retentionRationale.trim()}
                    onClick={() => void applyRetentionAction()}
                  >
                    {isBusy
                      ? "Recording decision…"
                      : retentionAction.kind === "archive"
                        ? "Archive Tender"
                        : retentionAction.kind === "trash"
                          ? "Move to Trash"
                          : "Restore Tender"}
                  </button>
                </div>
              </m.section>
            </m.div>
          ) : null}

          {trashAction ? (
            <m.div
              key="trash-dialog"
              className="retention-dialog__backdrop"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={{ duration: 0.18 }}
            >
              <m.section
                className="retention-dialog"
                role="dialog"
                aria-modal="true"
                aria-labelledby="trash-action-dialog-title"
                initial={{ opacity: 0, y: 14, scale: 0.975 }}
                animate={{ opacity: 1, y: 0, scale: 1 }}
                transition={quantixSmoothEase}
              >
                <div>
                  <h2 id="trash-action-dialog-title">
                    {trashAction.kind === "restore"
                      ? `Restore ${trashAction.record.tender_name}?`
                      : `Permanently delete ${trashAction.record.tender_name}?`}
                  </h2>
                  <p>
                    {trashAction.kind === "restore"
                      ? trashAction.record.deletion_source ===
                        "recovery_required"
                        ? "Quantix returns the same Tender Store and identity to the active list as Needs recovery. It does not repair, verify, or open the damaged Store."
                        : "Quantix verifies and republishes the same Tender Store without merging, overwriting, or changing its identity."
                      : "This is irreversible. Quantix deletes the Tender Store and every identifiable Tender backup, Portable Tender Archive, delivery export, Agent workspace, staging or quarantine item, and Tender-specific log it controls."}
                  </p>
                  {trashAction.kind === "purge" ? (
                    <p>
                      Original source packages, copies made outside Quantix,
                      recipient copies, operating-system or third-party backups,
                      and application-wide Provider Credentials are outside this
                      deletion. Provider-thread cleanup is tracked separately
                      and cannot hold local deletion open.
                    </p>
                  ) : null}
                </div>
                <label htmlFor="trash-action-rationale">
                  Decision rationale
                  <textarea
                    id="trash-action-rationale"
                    autoFocus
                    rows={3}
                    maxLength={4000}
                    value={retentionRationale}
                    disabled={isBusy}
                    onChange={(event) =>
                      setRetentionRationale(event.target.value)
                    }
                  />
                </label>
                {trashAction.kind === "purge" ? (
                  <label htmlFor="permanent-delete-confirmation">
                    Type {trashAction.record.tender_name} to confirm
                    <input
                      id="permanent-delete-confirmation"
                      value={permanentDeleteConfirmation}
                      disabled={isBusy}
                      autoComplete="off"
                      onChange={(event) =>
                        setPermanentDeleteConfirmation(event.target.value)
                      }
                    />
                  </label>
                ) : null}
                <div className="retention-dialog__actions">
                  <button
                    type="button"
                    disabled={isBusy}
                    onClick={() => {
                      setTrashAction(null);
                      setRetentionRationale("");
                      setPermanentDeleteConfirmation("");
                    }}
                  >
                    Cancel
                  </button>
                  <button
                    className={
                      trashAction.kind === "purge"
                        ? "manager-workspace__danger-confirm"
                        : "manager-workspace__primary"
                    }
                    type="button"
                    disabled={
                      isBusy ||
                      !retentionRationale.trim() ||
                      (trashAction.kind === "purge" &&
                        permanentDeleteConfirmation !==
                          trashAction.record.tender_name)
                    }
                    onClick={() => void applyTrashAction()}
                  >
                    {isBusy
                      ? "Recording decision…"
                      : trashAction.kind === "restore"
                        ? "Restore Tender"
                        : "Permanent Delete"}
                  </button>
                </div>
              </m.section>
            </m.div>
          ) : null}
        </div>
      </LayoutGroup>
    </QuantixWindow>
  );
}
