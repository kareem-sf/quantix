import {
  Archive,
  Bot,
  ChevronLeft,
  ChevronRight,
  CircleAlert,
  FileText,
  Folder,
  ListChecks,
  LoaderCircle,
  Menu,
  MessageSquare,
  MoreHorizontal,
  Plus,
  Send,
  Settings,
  Undo2,
  Users,
  X,
} from "lucide-react";
import {
  type KeyboardEvent as ReactKeyboardEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import type { ManagerWorkspaceProjection } from "./bindings/ManagerWorkspaceProjection";
import type { ManagerWorkspaceTender } from "./bindings/ManagerWorkspaceTender";
import type { GeneralApplicationPreferences } from "./bindings/GeneralApplicationPreferences";
import type { TenderOfficeMessage } from "./bindings/TenderOfficeMessage";
import type { TenderPackageSourceKind } from "./bindings/TenderPackageSourceKind";
import type { WorkspaceCurrentAction } from "./bindings/WorkspaceCurrentAction";
import { ApplicationSettings } from "./ApplicationSettings";
import { notifyAttentionRequired } from "./applicationNotifications";
import { DEFAULT_GENERAL_APPLICATION_PREFERENCES } from "./applicationPreferences";
import {
  archiveTender,
  chooseAndImportTenderPackage,
  inspectManagerWorkspace,
  rebindManagerIntakeProvider,
  recordEngineerWorkspaceMessage,
  retryManagerIntake,
  restoreArchivedTender,
  selectManagerWorkspaceTender,
  startManagerTender,
} from "./quantixHost";
import "./ManagerWorkspace.css";

type WorkspaceView = "manager" | "work" | "files";
type RetentionAction = "archive" | "restore" | null;

interface ManagerWorkspaceProps {
  aiAvailable: boolean;
  initialPreferences?: GeneralApplicationPreferences;
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
  if (typeof error === "object" && error !== null && "code" in error) {
    switch (error.code) {
      case "runtime_required":
        return "The selected AI Provider is not ready. Open Settings to reconnect it or explicitly choose another live model.";
      case "recovery_required":
        return "This Tender needs recovery before it can be opened.";
      case "invalid_command":
        return "Quantix could not use that selection.";
      case "store_unavailable":
        return "The local Tender record is temporarily unavailable.";
    }
  }
  return "Quantix could not complete that action.";
}

function TenderButton({
  tender,
  selected,
  busy,
  onSelect,
}: {
  tender: ManagerWorkspaceTender;
  selected: boolean;
  busy: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      className="manager-workspace__tender"
      type="button"
      aria-current={selected ? "page" : undefined}
      disabled={busy || tender.state === "recovery_required"}
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
    </button>
  );
}

function StartTender({
  action,
  aiAvailable,
  busy,
  onStart,
}: {
  action: WorkspaceCurrentAction;
  aiAvailable: boolean;
  busy: boolean;
  onStart: (kind: TenderPackageSourceKind) => void;
}) {
  return (
    <div className="start-tender">
      <span className="start-tender__icon" aria-hidden="true">
        <Folder size={22} />
      </span>
      <h2>{action.title}</h2>
      <p>
        {aiAvailable
          ? action.summary
          : "Choose the Tender Package now. Quantix will register it safely and wait for an AI Provider without losing your place."}
      </p>
      <button
        className="manager-workspace__primary"
        type="button"
        disabled={busy}
        onClick={() => onStart("directory")}
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
          {isEngineer ? "You" : isSystem ? "Q" : <Bot size={17} />}
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
    </article>
  );
}

function ManagerView({
  projection,
  aiAvailable,
  busy,
  onImport,
  onRetry,
  onRebindProvider,
  onSend,
  onOpenSettings,
  onOpenAction,
  readOnly,
}: {
  projection: ManagerWorkspaceProjection;
  aiAvailable: boolean;
  busy: boolean;
  onImport: (kind: TenderPackageSourceKind) => void;
  onRetry: () => Promise<void>;
  onRebindProvider: () => Promise<void>;
  onSend: (body: string) => Promise<boolean>;
  onOpenSettings: () => void;
  onOpenAction: () => void;
  readOnly: boolean;
}) {
  const [composer, setComposer] = useState("");
  const [showEarlier, setShowEarlier] = useState(false);
  const composerRef = useRef<HTMLTextAreaElement>(null);
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

  const send = async () => {
    const body = composer.trim();
    if (!body || busy) return;
    if (await onSend(body)) setComposer("");
  };

  const onKeyDown = (event: ReactKeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void send();
    }
  };

  const openCurrentAction = () => {
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
        const messageId = projection.conversation?.latest_meaningful_message_id;
        const message = messageId
          ? document.getElementById(`manager-message-${messageId}`)
          : null;
        message?.scrollIntoView({ behavior: "smooth", block: "center" });
        message?.focus({ preventScroll: true });
        break;
      }
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
    "review_work",
  ].includes(projection.current_action.kind);
  const intakeWorking = aiAvailable && projection.intake?.status === "working";
  const intakePaused = !aiAvailable && projection.intake?.status === "working";
  const currentActionTitle = intakePaused
    ? "Tender intake paused"
    : projection.current_action.title;
  const currentActionSummary = intakePaused
    ? "Your registered Tender and completed work are safe. Restore the local AI runtime to continue this exact stage."
    : projection.current_action.summary;

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
            {projection.intake
              ? intakePaused
                ? "Paused — AI office unavailable"
                : projection.intake.label
              : projection.team.active_agent_runs > 0
                ? "Coordinating the Tender team"
                : aiAvailable
                  ? "Ready"
                  : "AI office unavailable — records remain accessible"}
          </small>
        </div>
      </div>
      {projection.intake ? (
        <p className="manager-view__stage-summary">
          {intakePaused
            ? "Your registered Tender and completed work are safe. Quantix will resume this stage after the local AI runtime is restored."
            : projection.intake.summary}
        </p>
      ) : null}

      <div className="manager-view__conversation" aria-live="polite">
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
      </div>

      {!readOnly ? (
        <>
          <section
            className="current-action"
            aria-labelledby="current-action-title"
          >
            <span className="current-action__eyebrow">
              {projection.current_action.requires_engineer
                ? "Your next step"
                : "Current focus"}
            </span>
            <h2 id="current-action-title">{currentActionTitle}</h2>
            <p>{currentActionSummary}</p>
            {hasActionButton ? (
              <div className="current-action__footer">
                <button
                  className="manager-workspace__primary"
                  type="button"
                  disabled={busy}
                  onClick={openCurrentAction}
                >
                  {projection.current_action.kind === "configure_ai_provider" &&
                  !aiAvailable
                    ? "Open Settings"
                    : projection.current_action.action_label}
                </button>
              </div>
            ) : null}
          </section>

          <div className="manager-composer">
            <textarea
              ref={composerRef}
              rows={1}
              value={composer}
              aria-label="Message your Tendering Manager"
              placeholder="Message your Tendering Manager…"
              disabled={busy}
              onChange={(event) => setComposer(event.target.value)}
              onKeyDown={onKeyDown}
            />
            <button
              type="button"
              aria-label="Send message"
              disabled={busy || !composer.trim()}
              onClick={() => void send()}
            >
              <Send size={18} aria-hidden="true" />
            </button>
          </div>
        </>
      ) : null}
    </div>
  );
}

function ArchivedTenders({
  tenders,
  busy,
  onSelect,
}: {
  tenders: ManagerWorkspaceTender[];
  busy: boolean;
  onSelect: (tenderId: string) => void;
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
    </main>
  );
}

function WorkView({ projection }: { projection: ManagerWorkspaceProjection }) {
  const groups = [
    ["Needs you", projection.work.needs_engineer],
    ["Working", projection.work.working],
    ["Waiting", projection.work.waiting],
    ["Done", projection.work.done],
    ["Cancelled", projection.work.cancelled],
    ["Failed", projection.work.failed],
  ] as const;
  const total = groups.reduce((sum, [, count]) => sum + count, 0);
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
        <dl className="workspace-summary__grid">
          {groups.map(([label, count]) => (
            <div key={label}>
              <dt>{label}</dt>
              <dd>{count}</dd>
            </div>
          ))}
        </dl>
      )}
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
            <strong>{projection.files.tender_document_count}</strong>
            <span>Tender documents</span>
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
      {projection.files.tender_documents.length > 0 ? (
        <div className="workspace-documents">
          <h3>Source package</h3>
          <ul>
            {projection.files.tender_documents.map((document) => (
              <li key={`${document.artifact_id}-${document.version}`}>
                <FileText size={17} aria-hidden="true" />
                <div>
                  <strong>{document.package_path}</strong>
                  <span>{document.parse_state.replace(/_/g, " ")}</span>
                  <details>
                    <summary>Provenance</summary>
                    <dl>
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
  aiAvailable: initialAiAvailable,
  initialPreferences = DEFAULT_GENERAL_APPLICATION_PREFERENCES,
}: ManagerWorkspaceProps) {
  const [aiAvailable, setAiAvailable] = useState(initialAiAvailable);
  const [preferences, setPreferences] = useState(initialPreferences);
  const [projection, setProjection] =
    useState<ManagerWorkspaceProjection | null>(null);
  const [view, setView] = useState<WorkspaceView>("manager");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [retentionOpen, setRetentionOpen] = useState(false);
  const [retentionAction, setRetentionAction] = useState<RetentionAction>(null);
  const [retentionRationale, setRetentionRationale] = useState("");
  const [sidebarOpen, setSidebarOpen] = useState(
    () => window.matchMedia("(min-width: 820px)").matches,
  );
  const [teamOpen, setTeamOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refreshRunning = useRef(false);
  const busyRef = useRef(false);
  const projectionEpoch = useRef(0);
  const previousAttentionTenderIds = useRef<Set<string> | null>(null);
  const archivedInspectionTenderId =
    projection?.selected_tender?.state === "archived"
      ? projection.selected_tender.tender_id
      : null;

  useEffect(() => {
    if (!retentionAction) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busyRef.current) {
        setRetentionAction(null);
        setRetentionRationale("");
      }
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [retentionAction]);

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
    void load();
  }, [load]);

  useEffect(() => {
    const refresh = async () => {
      if (
        busy ||
        (document.hidden && !preferences.notify_when_attention_needed) ||
        refreshRunning.current
      ) {
        return;
      }
      refreshRunning.current = true;
      const epoch = projectionEpoch.current;
      try {
        const next = await inspectManagerWorkspace(archivedInspectionTenderId);
        if (epoch === projectionEpoch.current && !busyRef.current) {
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
    busy,
    preferences.notify_when_attention_needed,
  ]);

  const run = useCallback(async <T,>(operation: () => Promise<T>) => {
    projectionEpoch.current += 1;
    busyRef.current = true;
    setBusy(true);
    setError(null);
    try {
      return await operation();
    } catch (reason) {
      setError(readableError(reason));
      return null;
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }, []);

  const startTender = useCallback(
    async (kind: TenderPackageSourceKind) => {
      const next = await run(() => startManagerTender(kind));
      if (next) {
        setProjection(next);
        setView("manager");
        setSettingsOpen(false);
        setRetentionOpen(false);
      }
    },
    [run],
  );

  const selectTender = useCallback(
    async (tenderId: string) => {
      const next = await run(() => selectManagerWorkspaceTender(tenderId));
      if (next) {
        setProjection(next);
        setView("manager");
        setSettingsOpen(false);
        setRetentionOpen(false);
        if (window.matchMedia("(max-width: 819px)").matches) {
          setSidebarOpen(false);
        }
      }
    },
    [run],
  );

  const importPackage = useCallback(
    async (kind: TenderPackageSourceKind) => {
      const tenderId = projection?.selected_tender?.tender_id;
      if (!tenderId) return;
      const result = await run(() =>
        chooseAndImportTenderPackage(tenderId, kind),
      );
      if (result) await load();
    },
    [load, projection?.selected_tender?.tender_id, run],
  );

  const sendMessage = useCallback(
    async (body: string) => {
      const tenderId = projection?.selected_tender?.tender_id;
      if (!tenderId) return false;
      const next = await run(() =>
        recordEngineerWorkspaceMessage(tenderId, body),
      );
      if (!next) return false;
      setProjection(next);
      return true;
    },
    [projection?.selected_tender?.tender_id, run],
  );

  const retryIntake = useCallback(async () => {
    const tenderId = projection?.selected_tender?.tender_id;
    if (!tenderId) return;
    const succeeded = await run(async () => {
      await retryManagerIntake(tenderId);
      return true;
    });
    if (succeeded) await load();
  }, [load, projection?.selected_tender?.tender_id, run]);

  const rebindIntakeProvider = useCallback(async () => {
    const tenderId = projection?.selected_tender?.tender_id;
    if (!tenderId) return;
    const succeeded = await run(async () => {
      await rebindManagerIntakeProvider(tenderId);
      return true;
    });
    if (succeeded) await load();
  }, [load, projection?.selected_tender?.tender_id, run]);

  const applyRetentionAction = useCallback(async () => {
    const tenderId = projection?.selected_tender?.tender_id;
    const rationale = retentionRationale.trim();
    if (!tenderId || !retentionAction || !rationale) return;
    const result = await run(() =>
      retentionAction === "archive"
        ? archiveTender(tenderId, rationale)
        : restoreArchivedTender(tenderId, rationale),
    );
    if (!result) return;
    setRetentionAction(null);
    setRetentionRationale("");
    if (retentionAction === "archive") await load();
    else await selectTender(tenderId);
  }, [
    load,
    projection?.selected_tender?.tender_id,
    retentionAction,
    retentionRationale,
    run,
    selectTender,
  ]);

  const selected = projection?.selected_tender ?? null;
  const activeTenders =
    projection?.catalogue.filter((tender) => tender.state !== "archived") ?? [];
  const archivedTenders =
    projection?.catalogue.filter((tender) => tender.state === "archived") ?? [];
  const activeCounts = useMemo(() => {
    if (!projection) return 0;
    return (
      projection.team.active_agent_runs +
      projection.team.waiting_tasks +
      projection.team.needs_engineer
    );
  }, [projection]);

  if (!projection && !error) {
    return (
      <div className="workspace-loading" aria-live="polite">
        <LoaderCircle size={22} aria-hidden="true" />
        Opening your Tender workspace…
      </div>
    );
  }

  return (
    <div
      className={`manager-workspace${sidebarOpen ? " is-sidebar-open" : " is-sidebar-closed"}`}
    >
      <header className="manager-workspace__bar">
        <button
          className="manager-workspace__icon-button"
          type="button"
          aria-label={sidebarOpen ? "Hide Tenders" : "Show Tenders"}
          aria-expanded={sidebarOpen}
          onClick={() => setSidebarOpen((open) => !open)}
        >
          {sidebarOpen ? <ChevronLeft size={19} /> : <Menu size={19} />}
        </button>
        <span className="manager-workspace__brand">Quantix</span>
      </header>

      <aside className="manager-workspace__sidebar" aria-label="Tenders">
        <div className="manager-workspace__sidebar-heading">
          <span>Tenders</span>
          {selected ? (
            <button
              type="button"
              aria-label="Start another Tender"
              disabled={busy}
              onClick={() => void startTender("directory")}
            >
              <Plus size={17} aria-hidden="true" />
            </button>
          ) : null}
        </div>
        <nav aria-label="Tenders">
          {activeTenders.map((tender) => (
            <TenderButton
              key={tender.tender_id}
              tender={tender}
              selected={selected?.tender_id === tender.tender_id}
              busy={busy}
              onSelect={() => void selectTender(tender.tender_id)}
            />
          ))}
        </nav>
        <div className="manager-workspace__sidebar-footer">
          <button
            type="button"
            aria-current={retentionOpen ? "page" : undefined}
            onClick={() => {
              setRetentionOpen(true);
              setSettingsOpen(false);
              setTeamOpen(false);
              if (window.matchMedia("(max-width: 819px)").matches) {
                setSidebarOpen(false);
              }
            }}
          >
            <Archive size={17} aria-hidden="true" />
            Archived &amp; Trash
            {archivedTenders.length ? (
              <span>{archivedTenders.length}</span>
            ) : null}
          </button>
          <button
            type="button"
            aria-current={settingsOpen ? "page" : undefined}
            onClick={() => {
              setSettingsOpen(true);
              setRetentionOpen(false);
              setTeamOpen(false);
              if (window.matchMedia("(max-width: 819px)").matches) {
                setSidebarOpen(false);
              }
            }}
          >
            <Settings size={17} aria-hidden="true" />
            Settings
          </button>
        </div>
      </aside>

      <div className="manager-workspace__main">
        {error ? (
          <div className="manager-workspace__error" role="alert">
            <CircleAlert size={18} aria-hidden="true" />
            <span>{error}</span>
            <button type="button" onClick={() => void load()}>
              Try again
            </button>
          </div>
        ) : null}

        {settingsOpen ? (
          <ApplicationSettings
            aiAvailable={aiAvailable}
            onAiAvailabilityChange={setAiAvailable}
            onPreferencesChange={setPreferences}
          />
        ) : retentionOpen ? (
          <ArchivedTenders
            tenders={archivedTenders}
            busy={busy}
            onSelect={(tenderId) => void selectTender(tenderId)}
          />
        ) : selected && projection ? (
          <>
            <div className="manager-workspace__heading">
              <div>
                <span>{phaseLabel[selected.phase]}</span>
                <h1>{selected.name}</h1>
              </div>
              <div className="manager-workspace__heading-actions">
                {selected.state === "active" ? (
                  <details className="manager-workspace__tender-menu">
                    <summary aria-label={`Manage ${selected.name}`}>
                      <MoreHorizontal size={19} aria-hidden="true" />
                    </summary>
                    <div>
                      {selected.can_archive ? (
                        <button
                          type="button"
                          onClick={(event) => {
                            event.currentTarget
                              .closest("details")
                              ?.removeAttribute("open");
                            setRetentionAction("archive");
                          }}
                        >
                          <Archive size={16} aria-hidden="true" /> Archive
                        </button>
                      ) : (
                        <p>
                          Archive becomes available after decline or final
                          approval, when protected work is finished.
                        </p>
                      )}
                    </div>
                  </details>
                ) : null}
                <div className="manager-workspace__team">
                  <button
                    type="button"
                    aria-expanded={teamOpen}
                    onClick={() => setTeamOpen((open) => !open)}
                  >
                    <Users size={18} aria-hidden="true" />
                    Team
                    {activeCounts > 0 ? <span>{activeCounts}</span> : null}
                  </button>
                  {teamOpen ? (
                    <div className="manager-workspace__team-card">
                      <button
                        type="button"
                        aria-label="Close team summary"
                        onClick={() => setTeamOpen(false)}
                      >
                        <X size={16} />
                      </button>
                      <strong>Team activity</strong>
                      <span>{projection.team.active_agent_runs} working</span>
                      <span>{projection.team.waiting_tasks} waiting</span>
                      <span>{projection.team.needs_engineer} need you</span>
                    </div>
                  ) : null}
                </div>
              </div>
            </div>
            {selected.state === "archived" ? (
              <div className="manager-workspace__archived" role="status">
                <Archive size={18} aria-hidden="true" />
                <div>
                  <strong>Archived · read-only</strong>
                  <span>
                    Manager, Work, and Files remain available without changing
                    this Tender.
                  </span>
                </div>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => setRetentionAction("restore")}
                >
                  <Undo2 size={16} aria-hidden="true" /> Restore
                </button>
              </div>
            ) : null}
            <nav
              className="manager-workspace__tabs"
              aria-label="Tender workspace"
            >
              {(
                [
                  ["manager", "Manager", MessageSquare],
                  ["work", "Work", ListChecks],
                  ["files", "Files", Folder],
                ] as const
              ).map(([destination, label, Icon]) => (
                <button
                  key={destination}
                  type="button"
                  aria-current={view === destination ? "page" : undefined}
                  onClick={() => setView(destination)}
                >
                  <Icon size={17} aria-hidden="true" />
                  {label}
                </button>
              ))}
            </nav>
            <main className="manager-workspace__content">
              {view === "manager" ? (
                <ManagerView
                  key={selected.tender_id}
                  projection={projection}
                  aiAvailable={aiAvailable}
                  busy={busy}
                  onImport={importPackage}
                  onRetry={retryIntake}
                  onRebindProvider={rebindIntakeProvider}
                  onSend={sendMessage}
                  onOpenSettings={() => setSettingsOpen(true)}
                  onOpenAction={() =>
                    setView(
                      projection.current_action.kind.includes("package") ||
                        projection.current_action.kind === "review_intake"
                        ? "files"
                        : "work",
                    )
                  }
                  readOnly={selected.state === "archived"}
                />
              ) : null}
              {view === "work" ? <WorkView projection={projection} /> : null}
              {view === "files" ? (
                <FilesView
                  projection={projection}
                  busy={busy}
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
              aiAvailable={aiAvailable}
              busy={busy}
              onStart={startTender}
            />
          </main>
        ) : null}
      </div>

      {retentionAction && selected ? (
        <div className="retention-dialog__backdrop">
          <section
            className="retention-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="retention-dialog-title"
          >
            <div>
              <h2 id="retention-dialog-title">
                {retentionAction === "archive"
                  ? `Archive ${selected.name}?`
                  : `Restore ${selected.name}?`}
              </h2>
              <p>
                {retentionAction === "archive"
                  ? "This keeps the complete Tender Store and history in place, but makes the Tender read-only until you restore it."
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
                disabled={busy}
                onChange={(event) => setRetentionRationale(event.target.value)}
              />
            </label>
            <div className="retention-dialog__actions">
              <button
                type="button"
                disabled={busy}
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
                disabled={busy || !retentionRationale.trim()}
                onClick={() => void applyRetentionAction()}
              >
                {busy
                  ? "Recording decision…"
                  : retentionAction === "archive"
                    ? "Archive Tender"
                    : "Restore Tender"}
              </button>
            </div>
          </section>
        </div>
      ) : null}

      {busy ? <div className="manager-workspace__progress" /> : null}
      {!sidebarOpen ? (
        <button
          className="manager-workspace__reopen"
          type="button"
          onClick={() => setSidebarOpen(true)}
        >
          <ChevronRight size={16} aria-hidden="true" />
          Tenders
        </button>
      ) : null}
    </div>
  );
}
