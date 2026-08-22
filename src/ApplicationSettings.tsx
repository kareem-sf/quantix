import {
  ArrowLeft,
  Bell,
  Bot,
  Database,
  Info,
  LoaderCircle,
  LogOut,
  RefreshCw,
  Settings2,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { m } from "motion/react";
import { useCallback, useEffect, useState } from "react";

import { applyGeneralApplicationPreferences } from "./applicationPreferences";
import { enableAttentionNotifications } from "./applicationNotifications";
import { QuantixMark } from "./QuantixMark";
import { DiagnosticsTimeline } from "./DiagnosticsTimeline";
import type { ApplicationSettingsView } from "./bindings/ApplicationSettingsView";
import type { ChatGptPortHolders } from "./bindings/ChatGptPortHolders";
import type { GeneralApplicationPreferences } from "./bindings/GeneralApplicationPreferences";
import type { ProviderConnectionView } from "./bindings/ProviderConnectionView";
import type { ProviderReasoningSelection } from "./bindings/ProviderReasoningSelection";
import type { RuntimeReadiness } from "./bindings/RuntimeReadiness";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import type { QuantixDoctorFinding } from "./bindings/QuantixDoctorFinding";
import type { QuantixDoctorReport } from "./bindings/QuantixDoctorReport";
import { QuantixSelect } from "./ui/QuantixSelect";
import { QuantixSwitch } from "./ui/QuantixSwitch";
import { QuantixDialog } from "./ui/QuantixDialog";
import "./ui/quantix-ui.css";
import {
  cancelChatGptLogin,
  checkQuantixUpdate,
  confirmAiExecutionSelection,
  connectAnthropic,
  connectGemini,
  disconnectAiProvider,
  disconnectChatGpt,
  inspectQuantixDoctor,
  inspectRuntimeReadiness,
  repairQuantixDoctor,
  refreshApplicationSettings,
  startChatGptLogin,
  updateAiExecutionSelection,
  updateGeneralApplicationPreferences,
  validateQuantixUpdateRestart,
} from "./quantixHost";

interface ApplicationSettingsProps {
  aiAvailable: boolean;
  onAiAvailabilityChange: (available: boolean) => void;
  onPreferencesChange?: (preferences: GeneralApplicationPreferences) => void;
  initialSection?: "general" | "ai" | "about";
  selectedTenderId?: string | null;
  onClose: () => void;
}

type ApplicationSettingsSection =
  "general" | "ai" | "data" | "updates" | "about";

const SETTINGS_NAVIGATION = [
  { id: "general", label: "General", icon: Settings2 },
  { id: "ai", label: "AI & Models", icon: Sparkles },
  { id: "data", label: "Data & Storage", icon: Database },
  { id: "updates", label: "Updates", icon: RefreshCw },
  { id: "about", label: "About & Diagnostics", icon: Info },
] as const satisfies readonly {
  id: ApplicationSettingsSection;
  label: string;
  icon: typeof Settings2;
}[];

function settingsError(reason: unknown): string {
  if (typeof reason === "object" && reason !== null && "code" in reason) {
    if (reason.code === "local_document_tools_required") {
      return "Prepare document tools before continuing.";
    }
    if (reason.code === "ai_provider_required") {
      return "Waiting for AI Provider. Reconnect the selected provider or choose another ready connection.";
    }
    if (reason.code === "oauth_port_blocked") {
      const holders =
        "port_holders" in reason
          ? ((reason.port_holders ?? null) as ChatGptPortHolders | null)
          : null;
      const holdingProcesses = [
        holders?.port_1455 != null
          ? `port 1455 — PID ${holders.port_1455}`
          : null,
        holders?.port_1457 != null
          ? `port 1457 — PID ${holders.port_1457}`
          : null,
      ].filter((detail): detail is string => detail !== null);
      if (holdingProcesses.length > 0) {
        return `Ports needed for ChatGPT sign-in are busy. Close these programs and try again: ${holdingProcesses.join("; ")}.`;
      }
      return "Ports needed for ChatGPT sign-in are busy. Close the programs using them and try again.";
    }
    if (reason.code === "oauth_already_running") {
      return "A ChatGPT sign-in is already running. Finish it in your browser or cancel it first.";
    }
    if (reason.code === "runtime_required") {
      return "Waiting for AI Provider. Reconnect the selected provider or choose another ready connection.";
    }
    if (reason.code === "invalid_command") {
      return "Finish current AI work, then try this change again.";
    }
    if (reason.code === "store_unavailable") {
      return "Quantix could not complete this local action. Nothing was changed.";
    }
  }
  return "Quantix could not load Settings. Your saved choices are unchanged.";
}

function reasoningKey(selection: ProviderReasoningSelection): string {
  return JSON.stringify(selection);
}

function reasoningName(selection: ProviderReasoningSelection): string {
  if (selection.kind === "provider_default") return "Provider default";
  return selection.value.replace(/_/g, " ");
}

function connectionStatus(connection: ProviderConnectionView): string {
  return connection.status === "ready" ? "Ready" : "Waiting for AI Provider";
}

function providerDisclosure(connection: ProviderConnectionView): string {
  switch (connection.provider) {
    case "codex":
      return "Tender content is sent to OpenAI through your connected ChatGPT account. Usage and limits belong to that account.";
    case "anthropic":
      return "Tender content is sent to the Anthropic API. Usage is billed to the Anthropic account that owns this key.";
    case "gemini":
      return "Tender content is sent to the Google Gemini API. Usage and limits belong to the Google project or account that owns this key.";
  }
}

function exactSelectionIsReady(view: ApplicationSettingsView): boolean {
  const selection = view.ai_execution_selection;
  if (!selection) return false;
  const approval = view.ai_execution_approval;
  if (
    !approval ||
    approval.connection_id !== selection.connection_id ||
    approval.model_id !== selection.model_id ||
    reasoningKey(approval.reasoning) !== reasoningKey(selection.reasoning)
  ) {
    return false;
  }
  const connection = view.provider_connections.find(
    (candidate) =>
      candidate.connection_id === selection.connection_id &&
      candidate.status === "ready",
  );
  const model = connection?.models.find(
    (candidate) => candidate.model_id === selection.model_id,
  );
  return Boolean(
    model?.reasoning_options.some(
      (option) =>
        JSON.stringify(option.selection) ===
        JSON.stringify(selection.reasoning),
    ),
  );
}

export function ApplicationSettings({
  onAiAvailabilityChange,
  onPreferencesChange,
  initialSection = "general",
  selectedTenderId = null,
  onClose,
}: ApplicationSettingsProps) {
  const [settings, setSettings] = useState<ApplicationSettingsView | null>(
    null,
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [connectionId, setConnectionId] = useState<string | null>(null);
  const [anthropicKey, setAnthropicKey] = useState("");
  const [geminiKey, setGeminiKey] = useState("");
  const [preferenceBusy, setPreferenceBusy] = useState(false);
  const [runtime, setRuntime] = useState<RuntimeReadiness | null>(null);
  const [update, setUpdate] = useState<UpdateStatus | null>(null);
  const [factsBusy, setFactsBusy] = useState(false);
  const [doctor, setDoctor] = useState<QuantixDoctorReport | null>(null);
  const [repairAllOpen, setRepairAllOpen] = useState(false);
  const [activeSection, setActiveSection] =
    useState<ApplicationSettingsSection>(initialSection);

  useEffect(() => {
    setActiveSection(initialSection);
  }, [initialSection]);
  const [chatgptAwaiting, setChatgptAwaiting] = useState(false);

  const acceptSettings = useCallback(
    (view: ApplicationSettingsView) => {
      setSettings(view);
      applyGeneralApplicationPreferences(view.general_preferences);
      onPreferencesChange?.(view.general_preferences);
      onAiAvailabilityChange(exactSelectionIsReady(view));
      return view;
    },
    [onAiAvailabilityChange, onPreferencesChange],
  );

  const load = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      acceptSettings(await refreshApplicationSettings());
    } catch (reason) {
      setError(settingsError(reason));
    } finally {
      setBusy(false);
    }
  }, [acceptSettings]);

  useEffect(() => {
    void load();
  }, [load]);

  const chatgpt = settings?.chatgpt ?? null;
  const chatgptConnected = chatgpt?.state === "connected";
  const chatgptLoginPhase = chatgpt?.login_phase ?? "idle";
  const chatgptSignInPending =
    chatgptAwaiting &&
    !chatgptConnected &&
    chatgptLoginPhase === "awaiting_browser";
  useEffect(() => {
    if (!chatgptSignInPending) return;
    const timer = window.setInterval(() => {
      void refreshApplicationSettings()
        .then(acceptSettings)
        .catch((reason) => setError(settingsError(reason)));
    }, 1_200);
    return () => window.clearInterval(timer);
  }, [acceptSettings, chatgptSignInPending]);

  useEffect(() => {
    if (
      chatgptAwaiting &&
      (chatgptConnected ||
        chatgptLoginPhase === "completed" ||
        chatgptLoginPhase === "failed" ||
        chatgptLoginPhase === "cancelled")
    ) {
      setChatgptAwaiting(false);
    }
  }, [chatgptAwaiting, chatgptConnected, chatgptLoginPhase]);

  const persistedSelection = settings?.ai_execution_selection;
  const connection = settings?.provider_connections.find(
    (candidate) =>
      candidate.connection_id ===
      (connectionId ?? persistedSelection?.connection_id ?? "codex_chatgpt"),
  );
  const connectionSelection =
    persistedSelection?.connection_id === connection?.connection_id
      ? persistedSelection
      : null;
  const selectedModel = connectionSelection
    ? connection?.models.find(
        (model) => model.model_id === connectionSelection.model_id,
      )
    : undefined;
  const persistedReasoningKey = connectionSelection
    ? reasoningKey(connectionSelection.reasoning)
    : null;
  const selectedReasoning = persistedReasoningKey
    ? selectedModel?.reasoning_options.find(
        (option) => reasoningKey(option.selection) === persistedReasoningKey,
      )
    : selectedModel?.reasoning_options.find((option) => option.is_default);
  const modelUnavailable = Boolean(connectionSelection && !selectedModel);
  const reasoningUnavailable = Boolean(
    connectionSelection && selectedModel && !selectedReasoning,
  );
  const recommendedModel =
    connection?.models.find((model) => model.is_default) ??
    connection?.models[0];
  const recommendedReasoning =
    recommendedModel?.reasoning_options.find((option) => option.is_default) ??
    recommendedModel?.reasoning_options[0];
  const preferences = settings?.general_preferences;

  const savePreferences = useCallback(
    async (preferences: GeneralApplicationPreferences) => {
      if (!settings || preferenceBusy) return;
      const previous = settings;
      const optimistic = { ...settings, general_preferences: preferences };
      setPreferenceBusy(true);
      setSettings(optimistic);
      applyGeneralApplicationPreferences(preferences);
      onPreferencesChange?.(preferences);
      setError(null);
      try {
        acceptSettings(
          await updateGeneralApplicationPreferences({ preferences }),
        );
      } catch (reason) {
        setSettings(previous);
        applyGeneralApplicationPreferences(previous.general_preferences);
        onPreferencesChange?.(previous.general_preferences);
        setError(settingsError(reason));
      } finally {
        setPreferenceBusy(false);
      }
    },
    [acceptSettings, onPreferencesChange, preferenceBusy, settings],
  );

  const setAttentionNotifications = useCallback(
    async (enabled: boolean) => {
      if (!preferences) return;
      if (enabled) {
        try {
          if (!(await enableAttentionNotifications())) {
            setError(
              "Operating-system notifications remain off because permission was not granted.",
            );
            return;
          }
        } catch {
          setError(
            "Operating-system notifications are unavailable on this installation.",
          );
          return;
        }
      }
      await savePreferences({
        ...preferences,
        notify_when_attention_needed: enabled,
      });
    },
    [preferences, savePreferences],
  );

  const saveSelection = useCallback(
    async (modelId: string, reasoning: ProviderReasoningSelection) => {
      if (!connection || connection.status !== "ready") return;
      setBusy(true);
      setError(null);
      try {
        acceptSettings(
          await updateAiExecutionSelection({
            connection_id: connection.connection_id,
            model_id: modelId,
            reasoning,
          }),
        );
      } catch (reason) {
        setError(settingsError(reason));
      } finally {
        setBusy(false);
      }
    },
    [acceptSettings, connection],
  );

  const runSettingsAction = useCallback(
    async (operation: () => Promise<ApplicationSettingsView>) => {
      setBusy(true);
      setError(null);
      try {
        acceptSettings(await operation());
        return true;
      } catch (reason) {
        setError(settingsError(reason));
        return false;
      } finally {
        setBusy(false);
      }
    },
    [acceptSettings],
  );

  const confirmSelection = useCallback(async () => {
    if (!connection || !connectionSelection || !selectedReasoning) return;
    await runSettingsAction(() =>
      confirmAiExecutionSelection({
        connection_id: connection.connection_id,
        model_id: connectionSelection.model_id,
        reasoning: selectedReasoning.selection,
      }),
    );
  }, [connection, connectionSelection, runSettingsAction, selectedReasoning]);

  const connectChatGpt = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const result = await startChatGptLogin();
      setChatgptAwaiting(result.status === "awaiting_browser");
      acceptSettings(await refreshApplicationSettings());
    } catch (reason) {
      setError(settingsError(reason));
    } finally {
      setBusy(false);
    }
  }, [acceptSettings]);

  const cancelChatGpt = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      await cancelChatGptLogin();
      setChatgptAwaiting(false);
      acceptSettings(await refreshApplicationSettings());
    } catch (reason) {
      setError(settingsError(reason));
    } finally {
      setBusy(false);
    }
  }, [acceptSettings]);

  const disconnectChatGptAccount = async () => {
    if (await runSettingsAction(disconnectChatGpt)) {
      setChatgptAwaiting(false);
    }
  };

  const disconnectConnection = (connection_id: string) =>
    runSettingsAction(() => disconnectAiProvider({ connection_id }));

  const saveAnthropicKey = async () => {
    const apiKey = anthropicKey.trim();
    if (!apiKey) return;
    if (await runSettingsAction(() => connectAnthropic({ api_key: apiKey }))) {
      setAnthropicKey("");
      setConnectionId("anthropic_byok");
    }
  };

  const saveGeminiKey = async () => {
    const apiKey = geminiKey.trim();
    if (!apiKey) return;
    if (await runSettingsAction(() => connectGemini({ api_key: apiKey }))) {
      setGeminiKey("");
      setConnectionId("gemini_byok");
    }
  };

  const loadHostFacts = useCallback(async () => {
    setFactsBusy(true);
    const [runtimeResult, updateResult, doctorResult] =
      await Promise.allSettled([
        inspectRuntimeReadiness(),
        validateQuantixUpdateRestart(),
        inspectQuantixDoctor(selectedTenderId),
      ]);
    if (runtimeResult.status === "fulfilled") setRuntime(runtimeResult.value);
    if (updateResult.status === "fulfilled") setUpdate(updateResult.value);
    if (doctorResult.status === "fulfilled") setDoctor(doctorResult.value);
    setFactsBusy(false);
  }, [selectedTenderId]);

  const repairDoctorFinding = useCallback(
    async (finding: QuantixDoctorFinding) => {
      if (!doctor || !finding.repair_action) return;
      if (finding.repair_action === "rebind_tender_ai_selection") {
        setActiveSection("ai");
        return;
      }
      if (finding.repair_action === "retry_update_inspection") {
        await checkQuantixUpdate()
          .then(setUpdate)
          .catch(() =>
            setError(
              "The signed update source is unavailable. No local repair can fix an external outage.",
            ),
          );
        setDoctor(await inspectQuantixDoctor(selectedTenderId));
        return;
      }
      setFactsBusy(true);
      setError(null);
      try {
        const target =
          finding.repair_action === "inspect_tender_integrity"
            ? "tender"
            : "application";
        setDoctor(
          await repairQuantixDoctor({
            report_revision: doctor.revision,
            code: finding.code,
            action: finding.repair_action,
            target,
            tender_id: target === "tender" ? selectedTenderId : null,
          }),
        );
        setRuntime(await inspectRuntimeReadiness());
      } catch (reason) {
        setError(settingsError(reason));
      } finally {
        setFactsBusy(false);
      }
    },
    [doctor, selectedTenderId],
  );

  const repairAllSafeIssues = useCallback(async () => {
    if (!doctor) return;
    setRepairAllOpen(false);
    setFactsBusy(true);
    setError(null);
    try {
      let current = doctor;
      for (const finding of current.findings.filter((candidate) =>
        [
          "prepare_document_tools",
          "retry_document_tools",
          "refresh_ai_provider",
          "retry_diagnostics",
        ].includes(candidate.repair_action ?? ""),
      )) {
        if (!finding.repair_action) continue;
        current = await repairQuantixDoctor({
          report_revision: current.revision,
          code: finding.code,
          action: finding.repair_action,
          target: "application",
          tender_id: null,
        });
      }
      setDoctor(current);
      setRuntime(await inspectRuntimeReadiness());
      acceptSettings(await refreshApplicationSettings());
    } catch (reason) {
      setError(settingsError(reason));
    } finally {
      setFactsBusy(false);
    }
  }, [acceptSettings, doctor]);

  const checkForUpdate = useCallback(async () => {
    setFactsBusy(true);
    try {
      setUpdate(await checkQuantixUpdate());
    } catch {
      setError(
        "Quantix could not reach the signed update source. Try again later.",
      );
    } finally {
      setFactsBusy(false);
    }
  }, []);

  const canConnect = connection?.status === "authentication_required";
  const canDisconnect =
    connection?.status === "ready" ||
    connection?.status === "subscription_required";
  const activeModel = settings?.provider_connections
    .find(
      (candidate) =>
        candidate.connection_id === persistedSelection?.connection_id,
    )
    ?.models.find((model) => model.model_id === persistedSelection?.model_id);

  return (
    <main className="application-settings" data-active-section={activeSection}>
      <aside className="application-settings__navigation">
        <button
          className="application-settings__back"
          type="button"
          onClick={onClose}
        >
          <ArrowLeft size={16} aria-hidden="true" />
          Back to workspace
        </button>
        <div className="application-settings__heading">
          <span aria-hidden="true">
            <Settings2 size={20} />
          </span>
          <div>
            <h1>Settings</h1>
            <p>Application-wide choices.</p>
          </div>
        </div>
        <nav aria-label="Settings sections">
          {SETTINGS_NAVIGATION.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              type="button"
              aria-current={activeSection === id ? "page" : undefined}
              onClick={() => setActiveSection(id)}
            >
              <Icon size={18} aria-hidden="true" />
              {label}
              {activeSection === id ? (
                <m.span
                  className="application-settings__active-indicator"
                  layoutId="application-settings-active-section"
                  aria-hidden="true"
                />
              ) : null}
            </button>
          ))}
        </nav>
      </aside>

      <div className="application-settings__content">
        {error ? (
          <p className="application-settings__error" role="alert">
            {error}
          </p>
        ) : null}

        <m.div
          key={activeSection}
          className="application-settings__view"
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.22, ease: [0.2, 0, 0, 1] }}
        >
          <section
            className="application-settings__section"
            aria-labelledby="general-settings"
            hidden={activeSection !== "general"}
          >
            <div className="application-settings__section-heading">
              <div>
                <h2 id="general-settings">General</h2>
                <p>Appearance and attention preferences apply immediately.</p>
              </div>
            </div>
            {preferences ? (
              <div className="application-settings__preference-card">
                <QuantixSelect
                  label="Appearance"
                  value={preferences.appearance}
                  disabled={preferenceBusy}
                  options={[
                    { value: "system", label: "Use system setting" },
                    { value: "light", label: "Light" },
                    { value: "dark", label: "Dark" },
                  ]}
                  onChange={(appearance) =>
                    void savePreferences({
                      ...preferences,
                      appearance: appearance as "system" | "light" | "dark",
                    })
                  }
                />
                <QuantixSwitch
                  checked={preferences.reduced_motion}
                  disabled={preferenceBusy}
                  label="Reduce motion"
                  summary="Stops decorative movement and loading rotation."
                  onChange={(reduced_motion) =>
                    void savePreferences({ ...preferences, reduced_motion })
                  }
                />
                <QuantixSwitch
                  checked={preferences.larger_text}
                  disabled={preferenceBusy}
                  label="Larger text"
                  summary="Increases the application text size without changing content."
                  onChange={(larger_text) =>
                    void savePreferences({ ...preferences, larger_text })
                  }
                />
                <QuantixSwitch
                  checked={preferences.notify_when_attention_needed}
                  disabled={preferenceBusy}
                  label="Notify when I am needed"
                  summary="Allows Quantix to alert you when work needs an Engineer decision."
                  onChange={(enabled) =>
                    void setAttentionNotifications(enabled)
                  }
                />
              </div>
            ) : null}
          </section>

          <section
            className="application-settings__section"
            aria-labelledby="ai-settings"
            hidden={activeSection !== "ai"}
          >
            <div className="application-settings__section-heading">
              <div>
                <h2 id="ai-settings">AI &amp; Models</h2>
                <p>
                  Live provider choices. Changes apply only to future Agent
                  Runs.
                </p>
              </div>
              <button type="button" disabled={busy} onClick={() => void load()}>
                {busy ? (
                  <LoaderCircle className="is-spinning" size={16} />
                ) : (
                  <RefreshCw size={16} />
                )}
                Refresh
              </button>
            </div>

            {settings ? (
              <>
                <div className="application-settings__active-choice">
                  <Sparkles size={17} aria-hidden="true" />
                  <div>
                    <small>Default copied into newly created Tenders</small>
                    <strong>
                      {persistedSelection
                        ? `${settings.provider_connections.find((item) => item.connection_id === persistedSelection.connection_id)?.display_name ?? persistedSelection.connection_id} · ${activeModel?.display_name ?? persistedSelection.model_id} · ${reasoningName(persistedSelection.reasoning)}`
                        : "Waiting for AI Provider"}
                    </strong>
                    {persistedSelection && !exactSelectionIsReady(settings) ? (
                      <>
                        <small>
                          {connection
                            ? providerDisclosure(connection)
                            : "Choose a ready provider before sending Tender content."}
                        </small>
                        <button
                          type="button"
                          disabled={busy || !selectedReasoning}
                          onClick={() => void confirmSelection()}
                        >
                          Use this AI
                        </button>
                      </>
                    ) : null}
                  </div>
                </div>

                <div
                  className="application-settings__connections"
                  aria-label="Provider connections"
                >
                  {settings.provider_connections.map((candidate) => (
                    <button
                      key={candidate.connection_id}
                      type="button"
                      aria-pressed={
                        candidate.connection_id === connection?.connection_id
                      }
                      onClick={() => setConnectionId(candidate.connection_id)}
                    >
                      <span>
                        <Bot size={16} aria-hidden="true" />
                        <strong>{candidate.display_name}</strong>
                      </span>
                      <small data-status={candidate.status}>
                        {connectionStatus(candidate)}
                      </small>
                    </button>
                  ))}
                </div>
              </>
            ) : null}

            {connection ? (
              <div className="application-settings__provider">
                <div className="application-settings__provider-status">
                  <div>
                    <strong>{connection.display_name}</strong>
                    <span>
                      {connection.account_label ??
                        (canDisconnect ? "Connected account" : "Not connected")}
                      {connection.account_plan
                        ? ` · ${connection.account_plan.replace(/_/g, " ")} plan`
                        : ""}
                    </span>
                  </div>
                  <span data-status={connection.status}>
                    {connectionStatus(connection)}
                  </span>
                </div>
                <p>{connection.status_summary}</p>

                {connection.provider === "codex" ? (
                  <div
                    className="application-settings__login"
                    data-status={chatgpt?.state ?? "absent"}
                  >
                    {chatgptConnected && chatgpt ? (
                      <>
                        <strong>{chatgpt.account_id}</strong>
                        <span>
                          {chatgpt.plan_type
                            ? `${chatgpt.plan_type.replace(/_/g, " ")} plan · `
                            : ""}
                          Expires{" "}
                          {chatgpt.expires_at_ms != null
                            ? new Date(
                                Number(chatgpt.expires_at_ms),
                              ).toLocaleString()
                            : "unknown"}
                        </span>
                        <div className="application-settings__login-actions">
                          <button
                            type="button"
                            className="application-settings__logout"
                            disabled={busy}
                            onClick={() => void disconnectChatGptAccount()}
                          >
                            <LogOut size={15} /> Disconnect
                          </button>
                        </div>
                      </>
                    ) : chatgptSignInPending ? (
                      <>
                        <strong>
                          <LoaderCircle className="is-spinning" size={15} />{" "}
                          Finish signing in through your browser.
                        </strong>
                        <div className="application-settings__login-actions">
                          <button
                            type="button"
                            disabled={busy}
                            onClick={() => void cancelChatGpt()}
                          >
                            Cancel
                          </button>
                        </div>
                      </>
                    ) : chatgptLoginPhase === "failed" ? (
                      <>
                        <p role="alert">
                          ChatGPT sign-in did not finish. Check your browser and
                          try connecting again.
                        </p>
                        <div className="application-settings__login-actions">
                          <button
                            type="button"
                            disabled={busy}
                            onClick={() => void connectChatGpt()}
                          >
                            Connect ChatGPT
                          </button>
                        </div>
                      </>
                    ) : chatgptLoginPhase === "cancelled" ? (
                      <>
                        <p>
                          ChatGPT sign-in was cancelled. Connect again when
                          ready.
                        </p>
                        <div className="application-settings__login-actions">
                          <button
                            type="button"
                            disabled={busy}
                            onClick={() => void connectChatGpt()}
                          >
                            Connect ChatGPT
                          </button>
                        </div>
                      </>
                    ) : (
                      <div className="application-settings__login-actions">
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => void connectChatGpt()}
                        >
                          Connect ChatGPT
                        </button>
                      </div>
                    )}
                  </div>
                ) : connection.provider === "anthropic" && canConnect ? (
                  <form
                    className="application-settings__login"
                    onSubmit={(event) => {
                      event.preventDefault();
                      void saveAnthropicKey();
                    }}
                  >
                    <label>
                      <span>Anthropic API key</span>
                      <input
                        type="password"
                        aria-label="Anthropic API key"
                        value={anthropicKey}
                        autoComplete="off"
                        disabled={busy}
                        onChange={(event) =>
                          setAnthropicKey(event.target.value)
                        }
                      />
                      <small>
                        Stored only in your system credential vault.
                      </small>
                    </label>
                    <button
                      type="submit"
                      disabled={busy || !anthropicKey.trim()}
                    >
                      Connect Anthropic
                    </button>
                  </form>
                ) : connection.provider === "gemini" && canConnect ? (
                  <form
                    className="application-settings__login"
                    onSubmit={(event) => {
                      event.preventDefault();
                      void saveGeminiKey();
                    }}
                  >
                    <label>
                      <span>Gemini API key</span>
                      <input
                        type="password"
                        aria-label="Gemini API key"
                        value={geminiKey}
                        autoComplete="off"
                        disabled={busy}
                        onChange={(event) => setGeminiKey(event.target.value)}
                      />
                      <small>
                        Stored only in your system credential vault.
                      </small>
                    </label>
                    <button type="submit" disabled={busy || !geminiKey.trim()}>
                      Connect Gemini
                    </button>
                  </form>
                ) : canDisconnect ? (
                  <div className="application-settings__login-actions">
                    <button
                      type="button"
                      className="application-settings__logout"
                      disabled={busy}
                      onClick={() =>
                        void disconnectConnection(connection.connection_id)
                      }
                    >
                      <LogOut size={15} /> Remove local key
                    </button>
                  </div>
                ) : null}

                <div className="application-settings__field-group">
                  <QuantixSelect
                    aria-label="Model"
                    label="Model"
                    value={
                      selectedModel?.model_id ??
                      connectionSelection?.model_id ??
                      ""
                    }
                    disabled={busy || connection.status !== "ready"}
                    options={[
                      ...(!connectionSelection
                        ? [
                            {
                              value: "",
                              label: "Choose a live model",
                              disabled: true,
                            },
                          ]
                        : []),
                      ...(modelUnavailable && connectionSelection
                        ? [
                            {
                              value: connectionSelection.model_id,
                              label: `Unavailable — ${connectionSelection.model_id}`,
                              disabled: true,
                            },
                          ]
                        : []),
                      ...connection.models.map((model) => ({
                        value: model.model_id,
                        label: model.display_name,
                        description: model.description ?? undefined,
                      })),
                    ]}
                    onChange={(modelId) => {
                      const model = connection.models.find(
                        (candidate) => candidate.model_id === modelId,
                      );
                      const reasoning =
                        model?.reasoning_options.find(
                          (option) => option.is_default,
                        )?.selection ?? model?.reasoning_options[0]?.selection;
                      if (model && reasoning) {
                        void saveSelection(model.model_id, reasoning);
                      }
                    }}
                  />
                  {selectedModel?.description ? (
                    <small>{selectedModel.description}</small>
                  ) : null}
                  {modelUnavailable ? (
                    <small className="application-settings__unavailable">
                      Waiting for AI Provider. The saved model is no longer in
                      the live catalog.
                      {recommendedModel
                        ? ` Choose ${recommendedModel.display_name}${recommendedReasoning ? ` with ${recommendedReasoning.label}` : ""} to continue.`
                        : " Reconnect this provider to load its models."}
                    </small>
                  ) : null}
                </div>

                <div className="application-settings__field-group">
                  <QuantixSelect
                    aria-label="Reasoning"
                    label="Reasoning"
                    value={
                      selectedReasoning
                        ? reasoningKey(selectedReasoning.selection)
                        : (persistedReasoningKey ?? "")
                    }
                    disabled={
                      busy || connection.status !== "ready" || !selectedModel
                    }
                    options={[
                      ...(reasoningUnavailable && persistedReasoningKey
                        ? [
                            {
                              value: persistedReasoningKey,
                              label: "Unavailable saved reasoning",
                              disabled: true,
                            },
                          ]
                        : []),
                      ...(selectedModel?.reasoning_options.map((option) => ({
                        value: reasoningKey(option.selection),
                        label: option.label,
                        description: option.description ?? undefined,
                      })) ?? []),
                    ]}
                    onChange={(reasoningValue) => {
                      if (selectedModel) {
                        void saveSelection(
                          selectedModel.model_id,
                          JSON.parse(
                            reasoningValue,
                          ) as ProviderReasoningSelection,
                        );
                      }
                    }}
                  />
                  {reasoningUnavailable ? (
                    <small className="application-settings__unavailable">
                      Waiting for AI Provider. Choose a current reasoning option
                      to continue.
                    </small>
                  ) : null}
                </div>

                <div className="application-settings__disclosure">
                  <ShieldCheck size={16} aria-hidden="true" />
                  <p>{providerDisclosure(connection)}</p>
                </div>

                <p className="application-settings__provenance">
                  {connection.catalogue_fetched_at
                    ? `Live catalog refreshed ${new Date(connection.catalogue_fetched_at).toLocaleString()} · adapter ${connection.adapter_version}`
                    : "No live catalog is available for this connection."}
                  {connectionSelection &&
                  !modelUnavailable &&
                  !reasoningUnavailable &&
                  connection.status === "ready"
                    ? " · Seeds new Tenders only."
                    : ""}
                </p>
              </div>
            ) : busy ? (
              <div className="application-settings__loading" aria-live="polite">
                <LoaderCircle className="is-spinning" size={18} /> Loading live
                models…
              </div>
            ) : (
              <p className="application-settings__empty">
                Waiting for AI Provider. Refresh connections to continue.
              </p>
            )}
          </section>

          <details
            className="application-settings__details"
            hidden={activeSection !== "data"}
            open={activeSection === "data"}
          >
            <summary>
              <span>
                <Database size={18} aria-hidden="true" />
                <span>
                  <strong>Data &amp; Storage</strong>
                  <small>Where Quantix keeps your work</small>
                </span>
              </span>
            </summary>
            {settings ? (
              <div className="application-settings__facts">
                <div>
                  <span>Application Home</span>
                  <code>{settings.storage.application_home}</code>
                </div>
                <p>
                  Tender data stays in Application Home.
                  {settings.storage.tender_backups_are_preserved
                    ? " Quantix-managed backups are preserved."
                    : " No backup-preservation policy is active."}
                  {settings.storage.trash_requires_explicit_purge
                    ? " Trash is never purged without an explicit Engineer decision."
                    : " Review the current Trash policy before deleting work."}
                </p>
              </div>
            ) : null}
          </details>

          <details
            className="application-settings__details"
            hidden={activeSection !== "updates"}
            open={activeSection === "updates"}
            onToggle={(event) => {
              if (event.currentTarget.open && !update) void loadHostFacts();
            }}
          >
            <summary>
              <span>
                <RefreshCw size={18} aria-hidden="true" />
                <span>
                  <strong>Updates</strong>
                  <small>Signed application update status</small>
                </span>
              </span>
            </summary>
            <div className="application-settings__facts">
              <p aria-live="polite">
                {update
                  ? `Status: ${update.state.replace(/_/g, " ")}.`
                  : "Open this section to validate the current update state."}
              </p>
              <button
                type="button"
                disabled={factsBusy}
                onClick={() => void checkForUpdate()}
              >
                <RefreshCw size={15} />
                {factsBusy ? "Checking…" : "Check for update"}
              </button>
            </div>
          </details>

          <details
            className="application-settings__details"
            hidden={activeSection !== "about"}
            open={activeSection === "about"}
            onToggle={(event) => {
              if (event.currentTarget.open && !runtime) void loadHostFacts();
            }}
          >
            <summary>
              <span>
                <span>
                  <strong>About &amp; Diagnostics</strong>
                  <small>Version and local runtime details</small>
                </span>
              </span>
            </summary>
            {settings ? (
              <>
                <div className="application-settings__about-brand">
                  <QuantixMark className="application-settings__about-mark" />
                  <span>
                    <strong>Quantix</strong>
                    <small>Tender operating system</small>
                  </span>
                </div>
                <dl className="application-settings__diagnostics">
                  <div>
                    <dt>Quantix version</dt>
                    <dd>{settings.diagnostics.quantix_version}</dd>
                  </div>
                  <div>
                    <dt>Document tools</dt>
                    <dd>
                      {doctor
                        ? doctor.healthy
                          ? "Ready"
                          : "Needs attention"
                        : "Inspecting…"}
                    </dd>
                  </div>
                  <div>
                    <dt>Installation data</dt>
                    <dd>
                      schema {settings.diagnostics.installation_schema_version}
                    </dd>
                  </div>
                  <div>
                    <dt>Tender data</dt>
                    <dd>schema {settings.diagnostics.tender_schema_version}</dd>
                  </div>
                </dl>
                <section
                  className="application-settings__doctor"
                  aria-labelledby="doctor-report-title"
                >
                  <div className="application-settings__doctor-heading">
                    <div>
                      <strong id="doctor-report-title">Quantix Doctor</strong>
                      <small>
                        Setup, document tools, AI, signed updates, and selected
                        Tender integrity
                      </small>
                    </div>
                    <span data-state={doctor?.healthy ? "ready" : "attention"}>
                      {doctor
                        ? doctor.healthy
                          ? "Healthy"
                          : "Needs attention"
                        : "Inspecting…"}
                    </span>
                  </div>
                  {doctor?.findings.length ? (
                    <ul>
                      {doctor.findings.map((finding) => (
                        <li key={finding.code} data-severity={finding.severity}>
                          <div>
                            <code>{finding.code}</code>
                            <strong>{finding.title}</strong>
                          </div>
                          <dl>
                            <div>
                              <dt>Cause</dt>
                              <dd>{finding.cause}</dd>
                            </div>
                            <div>
                              <dt>Affected capability</dt>
                              <dd>{finding.affected_capability}</dd>
                            </div>
                            <div>
                              <dt>Impact</dt>
                              <dd>{finding.impact}</dd>
                            </div>
                            <div>
                              <dt>Safe remediation</dt>
                              <dd>{finding.safe_remediation}</dd>
                            </div>
                          </dl>
                          {finding.repair_action ? (
                            <button
                              type="button"
                              disabled={
                                factsBusy ||
                                ((finding.repair_action ===
                                  "inspect_tender_integrity" ||
                                  finding.repair_action ===
                                    "rebind_tender_ai_selection") &&
                                  !selectedTenderId)
                              }
                              onClick={() => void repairDoctorFinding(finding)}
                            >
                              {finding.repair_action ===
                              "rebind_tender_ai_selection"
                                ? "Choose Tender AI"
                                : finding.repair_action ===
                                    "inspect_tender_integrity"
                                  ? "Inspect Tender recovery"
                                  : finding.repair_action.replace(/_/g, " ")}
                            </button>
                          ) : null}
                        </li>
                      ))}
                    </ul>
                  ) : doctor ? (
                    <p>All Doctor checks passed.</p>
                  ) : (
                    <p
                      className="application-settings__doctor-status"
                      role="status"
                    >
                      Quantix is refreshing the Doctor report.
                    </p>
                  )}
                  {doctor?.findings.some((finding) =>
                    [
                      "prepare_document_tools",
                      "retry_document_tools",
                      "refresh_ai_provider",
                      "retry_diagnostics",
                    ].includes(finding.repair_action ?? ""),
                  ) ? (
                    <button
                      type="button"
                      disabled={factsBusy}
                      onClick={() => setRepairAllOpen(true)}
                    >
                      Repair all safe issues
                    </button>
                  ) : null}
                </section>
                <DiagnosticsTimeline
                  open={activeSection === "about"}
                  selectedTenderId={selectedTenderId}
                />
              </>
            ) : null}
          </details>

          <QuantixDialog
            isOpen={repairAllOpen}
            title="Repair all safe issues?"
            onOpenChange={setRepairAllOpen}
          >
            <p>
              Quantix will only prepare or repair managed document tools,
              refresh provider status, and retry the redacted diagnostics writer
              and retention checks. It will not change providers, alter Tender
              data, install an application update, or make recovery decisions.
            </p>
            <div className="application-settings__doctor-dialog-actions">
              <button type="button" onClick={() => setRepairAllOpen(false)}>
                Cancel
              </button>
              <button type="button" onClick={() => void repairAllSafeIssues()}>
                Repair safe issues
              </button>
            </div>
          </QuantixDialog>

          <p className="application-settings__notification-note">
            <Bell size={15} aria-hidden="true" /> General preferences are
            application-wide. AI &amp; Models supplies the default for new
            Tenders only; every existing Tender and Agent Run keeps its exact
            selection.
          </p>
        </m.div>
      </div>
    </main>
  );
}
