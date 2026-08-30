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
import { ApplicationSettings } from "./ApplicationSettings";
import { exactApplicationAiSelectionIsReady } from "./applicationAiSelectionReadiness";
import { notifyAttentionRequired } from "./applicationNotifications";
import { TenderFocusedAction } from "./TenderFocusedAction";
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
  ensureQuantixSetup,
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
  | "trash";

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

function Message({
  message,
  meaningful,
}: {
  message: TenderOfficeMessage;
  meaningful: boolean;
}) {
  const isEngineer = message.author === "engineer";
  const isSystem = message.author === "system";
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
              {message.references.map((reference, index) => (
                <li
                  key={`${reference.kind}-${reference.reference}-${reference.version}-${reference.evidence_ordinal ?? 0}-${index}`}
                >
                  <strong>{reference.label}</strong>
                  {reference.detail ? <span>{reference.detail}</span> : null}
                  <code>
                    {reference.reference} · v{reference.version}
                    {reference.evidence_ordinal
                      ? ` · evidence ${reference.evidence_ordinal}`
                      : ""}
                  </code>
                </li>
              ))}
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
    "prepare_work_plan",
    "review_work_plan",
    "review_work",
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
  const sendSuggestion = (body: string) => {
    if (!busy) void onSend(body);
  };

  return (
    <div className="manager-view">
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
          />
        ))}
        {!readOnly ? (
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
                {intakePaused && !documentToolsRequired && !showActionButton ? (
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

function WorkView({ projection }: { projection: ManagerWorkspaceProjection }) {
  const groups: Array<[string, WorkspaceTaskRow[]]> = [
    [
      "Needs you",
      projection.work.tasks.filter((task) => task.state === "needs_engineer"),
    ],
    [
      "Working",
      projection.work.tasks.filter((task) => task.state === "working"),
    ],
    [
      "Waiting",
      projection.work.tasks.filter((task) =>
        ["waiting", "paused"].includes(task.state),
      ),
    ],
    ["Done", projection.work.tasks.filter((task) => task.state === "done")],
    [
      "Needs attention",
      projection.work.tasks.filter((task) => task.state === "failed"),
    ],
  ];
  const total = projection.work.tasks.length;
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
            .filter(([, tasks]) => tasks.length > 0)
            .map(([label, tasks]) => (
              <section key={label} aria-label={label}>
                <div className="workspace-work-groups__heading">
                  <h3>{label}</h3>
                  <span>{tasks.length}</span>
                </div>
                <ul>
                  {tasks.map((task) => (
                    <li key={task.production_task_id}>
                      <div
                        className="workspace-task__state"
                        data-state={task.state}
                      />
                      <div>
                        <strong>{task.objective ?? task.task_key}</strong>
                        <span>{task.status_detail}</span>
                        <dl>
                          <div>
                            <dt>Specialist</dt>
                            <dd>
                              {task.agent?.identity ?? "Tendering Manager"}
                            </dd>
                          </div>
                          <div>
                            <dt>Blocker</dt>
                            <dd>
                              {task.dependencies.length
                                ? task.dependencies.join(", ")
                                : "None"}
                            </dd>
                          </div>
                          <div>
                            <dt>Outputs</dt>
                            <dd>{task.output_count}</dd>
                          </div>
                        </dl>
                      </div>
                    </li>
                  ))}
                </ul>
              </section>
            ))}
        </div>
      )}
    </section>
  );
}

function TeamView({ projection }: { projection: ManagerWorkspaceProjection }) {
  const tenderId = projection.selected_tender?.tender_id ?? null;
  const [selectedRun, setSelectedRun] = useState<AgentRunInspection | null>(
    null,
  );
  const [loadingRunId, setLoadingRunId] = useState<string | null>(null);
  const [workroomTab, setWorkroomTab] = useState<
    "conversation" | "context" | "activity" | "outputs"
  >("conversation");

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

  return (
    <section
      className="workspace-summary workspace-team"
      aria-labelledby="team-title"
    >
      <div className="workspace-summary__heading">
        <Users size={22} aria-hidden="true" />
        <div>
          <h2 id="team-title">Team</h2>
          <p>
            Attributable questions, findings, handoffs, blockers, and outputs.
          </p>
        </div>
      </div>
      <div className="workspace-team__columns">
        <section>
          <h3>Team stream</h3>
          {projection.team.events.length ? (
            <ol className="workspace-team__stream">
              {projection.team.events.map((event) => (
                <li key={event.message_id}>
                  <span>{event.kind.replace(/_/g, " ")}</span>
                  <strong>
                    {event.author === "engineer" ? "Engineer" : "Quantix"}
                  </strong>
                  <p>{event.body}</p>
                  <time>{new Date(event.created_at).toLocaleString()}</time>
                </li>
              ))}
            </ol>
          ) : (
            <p className="workspace-summary__empty">No Team events yet.</p>
          )}
        </section>
        <section>
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
            <p className="workspace-summary__empty">No Agent Runs yet.</p>
          )}
        </section>
      </div>
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
            {(["conversation", "context", "activity", "outputs"] as const).map(
              (tab) => (
                <button
                  key={tab}
                  type="button"
                  aria-current={workroomTab === tab ? "page" : undefined}
                  onClick={() => setWorkroomTab(tab)}
                >
                  {tab[0].toUpperCase() + tab.slice(1)}
                </button>
              ),
            )}
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
                  <li key={`${input.kind}-${input.reference}-${input.version}`}>
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
    </section>
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
  const intakeWorking = ["working", "waiting"].includes(
    projection.intake?.status ?? "",
  );
  const registeredDocuments = projection.files.tender_documents.filter(
    (document) => document.registration_state === "registered",
  );
  const exceptionDocuments = projection.files.tender_documents.filter(
    (document) => document.registration_state === "exception",
  );

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
              {document.exception ? ` · ${document.exception}` : null}
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
          <ul>{registeredDocuments.map(renderDocument)}</ul>
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
                      focusedActionTenderId === selected.tender_id &&
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
                      <WorkView projection={projection} />
                    ) : null}
                    {view === "team" ? (
                      <TeamView projection={projection} />
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
                </>
              ) : projection ? (
                <main className="manager-workspace__empty-main">
                  <StartTender
                    action={projection.current_action}
                    busy={isBusy}
                    focusRef={packageFocusRef}
                    onStart={startTender}
                  />
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
            isOpen={aiPreflightOpen}
            title={
              aiStatus === "ready"
                ? "AI is ready for this Tender"
                : "AI & Models is not fully set up"
            }
            onOpenChange={(open) => {
              setAiPreflightOpen(open);
              if (!open && aiStatus !== "ready") {
                setPendingTenderStart(null);
              }
            }}
          >
            <p className="manager-workspace__dialog-copy">
              {aiPreflightReason} Local package registration and document work
              remain available without AI; only AI-required work will wait.
            </p>
            {settingsSnapshot?.ai_execution_selection ? (
              <p className="manager-workspace__dialog-detail">
                Current default:{" "}
                {settingsSnapshot.ai_execution_selection.model_id}
              </p>
            ) : null}
            <div className="retention-dialog__actions">
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
                disabled={isBusy || pendingTenderStart === null}
                onClick={() => {
                  const kind = pendingTenderStart;
                  if (!kind) return;
                  setAiPreflightOpen(false);
                  setPendingTenderStart(null);
                  void continueStartTender(kind, aiStatus !== "ready");
                }}
              >
                {aiStatus === "ready"
                  ? "Continue New Tender"
                  : "Continue without AI"}
              </button>
            </div>
          </QuantixDialog>

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
