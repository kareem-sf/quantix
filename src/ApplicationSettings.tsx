import {
  ArrowLeft,
  Bell,
  Bot,
  Copy,
  Database,
  ExternalLink,
  Info,
  LoaderCircle,
  LogOut,
  RefreshCw,
  Settings2,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { applyGeneralApplicationPreferences } from "./applicationPreferences";
import { enableAttentionNotifications } from "./applicationNotifications";
import type { ApplicationSettingsView } from "./bindings/ApplicationSettingsView";
import type { GeneralApplicationPreferences } from "./bindings/GeneralApplicationPreferences";
import type { ProviderConnectionView } from "./bindings/ProviderConnectionView";
import type { ProviderReasoningSelection } from "./bindings/ProviderReasoningSelection";
import type { RuntimeReadiness } from "./bindings/RuntimeReadiness";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import {
  cancelProviderLogin,
  checkQuantixUpdate,
  connectAnthropic,
  connectGemini,
  disconnectAiProvider,
  inspectRuntimeReadiness,
  logoutProvider,
  openProviderLogin,
  refreshApplicationSettings,
  startProviderLogin,
  updateAiExecutionSelection,
  updateGeneralApplicationPreferences,
  validateQuantixUpdateRestart,
} from "./quantixHost";

interface ApplicationSettingsProps {
  aiAvailable: boolean;
  onAiAvailabilityChange: (available: boolean) => void;
  onPreferencesChange?: (preferences: GeneralApplicationPreferences) => void;
  onClose: () => void;
}

function settingsError(reason: unknown): string {
  if (typeof reason === "object" && reason !== null && "code" in reason) {
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
      return "Tender content is sent to OpenAI through your managed Codex session. Usage and limits belong to the connected OpenAI account.";
    case "anthropic":
      return "Tender content is sent to the Anthropic API. Usage is billed to the Anthropic account that owns this key.";
    case "gemini":
      return "Tender content is sent to the Google Gemini API. Usage and limits belong to the Google project or account that owns this key.";
  }
}

function PreferenceSwitch({
  checked,
  disabled,
  label,
  summary,
  onChange,
}: {
  checked: boolean;
  disabled: boolean;
  label: string;
  summary: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="application-settings__switch">
      <span>
        <strong>{label}</strong>
        <small>{summary}</small>
      </span>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
      />
    </label>
  );
}

export function ApplicationSettings({
  aiAvailable,
  onAiAvailabilityChange,
  onPreferencesChange,
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

  const acceptSettings = useCallback(
    (view: ApplicationSettingsView) => {
      setSettings(view);
      applyGeneralApplicationPreferences(view.general_preferences);
      onPreferencesChange?.(view.general_preferences);
      onAiAvailabilityChange(
        view.provider_connections.some(
          (connection) => connection.status === "ready",
        ),
      );
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
  }, [aiAvailable, load]);

  const login = settings?.active_provider_login;
  useEffect(() => {
    const completedCataloguePending =
      login?.status === "completed" &&
      !settings?.provider_connections.some(
        (connection) => connection.status === "ready",
      );
    if (
      !login ||
      (!completedCataloguePending &&
        !["awaiting_user", "cancelling"].includes(login.status))
    ) {
      return;
    }
    const timer = window.setInterval(() => {
      void refreshApplicationSettings()
        .then(acceptSettings)
        .catch((reason) => setError(settingsError(reason)));
    }, 1_200);
    return () => window.clearInterval(timer);
  }, [acceptSettings, login, settings?.provider_connections]);

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
      if (!connection || !aiAvailable) return;
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
    [acceptSettings, aiAvailable, connection],
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

  const beginLogin = (method: "browser" | "device_code") =>
    runSettingsAction(() => startProviderLogin({ method }));
  const cancelLogin = () =>
    login
      ? runSettingsAction(() =>
          cancelProviderLogin({ login_id: login.login_id }),
        )
      : Promise.resolve();
  const disconnect = () => runSettingsAction(logoutProvider);
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

  const openLogin = useCallback(async () => {
    if (!login) return;
    setError(null);
    try {
      await openProviderLogin({ login_id: login.login_id });
    } catch (reason) {
      setError(settingsError(reason));
    }
  }, [login]);

  const loadHostFacts = useCallback(async () => {
    setFactsBusy(true);
    const [runtimeResult, updateResult] = await Promise.allSettled([
      inspectRuntimeReadiness(),
      validateQuantixUpdateRestart(),
    ]);
    if (runtimeResult.status === "fulfilled") setRuntime(runtimeResult.value);
    if (updateResult.status === "fulfilled") setUpdate(updateResult.value);
    setFactsBusy(false);
  }, []);

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
    <main className="application-settings">
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
          <p>Simple application-wide choices. Tender work stays unchanged.</p>
        </div>
      </div>

      {error ? (
        <p className="application-settings__error" role="alert">
          {error}
        </p>
      ) : null}

      <section
        className="application-settings__section"
        aria-labelledby="general-settings"
      >
        <div className="application-settings__section-heading">
          <div>
            <h2 id="general-settings">General</h2>
            <p>Appearance and attention preferences apply immediately.</p>
          </div>
        </div>
        {preferences ? (
          <div className="application-settings__preference-card">
            <label>
              <span>Appearance</span>
              <select
                value={preferences.appearance}
                disabled={preferenceBusy}
                onChange={(event) =>
                  void savePreferences({
                    ...preferences,
                    appearance: event.target.value as
                      "system" | "light" | "dark",
                  })
                }
              >
                <option value="system">Use system setting</option>
                <option value="light">Light</option>
                <option value="dark">Dark</option>
              </select>
            </label>
            <PreferenceSwitch
              checked={preferences.reduced_motion}
              disabled={preferenceBusy}
              label="Reduce motion"
              summary="Stops decorative movement and loading rotation."
              onChange={(reduced_motion) =>
                void savePreferences({ ...preferences, reduced_motion })
              }
            />
            <PreferenceSwitch
              checked={preferences.high_contrast}
              disabled={preferenceBusy}
              label="Higher contrast"
              summary="Strengthens text, borders, and focus indicators."
              onChange={(high_contrast) =>
                void savePreferences({ ...preferences, high_contrast })
              }
            />
            <PreferenceSwitch
              checked={preferences.larger_text}
              disabled={preferenceBusy}
              label="Larger text"
              summary="Increases the application text size without changing content."
              onChange={(larger_text) =>
                void savePreferences({ ...preferences, larger_text })
              }
            />
            <PreferenceSwitch
              checked={preferences.notify_when_attention_needed}
              disabled={preferenceBusy}
              label="Notify when I am needed"
              summary="Allows Quantix to alert you when work needs an Engineer decision."
              onChange={(enabled) => void setAttentionNotifications(enabled)}
            />
          </div>
        ) : null}
      </section>

      <section
        className="application-settings__section"
        aria-labelledby="ai-settings"
      >
        <div className="application-settings__section-heading">
          <div>
            <h2 id="ai-settings">AI &amp; Models</h2>
            <p>
              Live provider choices. Changes apply only to future Agent Runs.
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
                <small>Default for future Agent Runs</small>
                <strong>
                  {persistedSelection
                    ? `${settings.provider_connections.find((item) => item.connection_id === persistedSelection.connection_id)?.display_name ?? persistedSelection.connection_id} · ${activeModel?.display_name ?? persistedSelection.model_id} · ${reasoningName(persistedSelection.reasoning)}`
                    : "Waiting for AI Provider"}
                </strong>
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

            {connection.provider === "codex" && login ? (
              <div
                className="application-settings__login"
                data-status={login.status}
              >
                <strong>{login.status_summary}</strong>
                {login.status === "awaiting_user" ? (
                  <>
                    {login.user_code ? (
                      <div className="application-settings__device-code">
                        <code>{login.user_code}</code>
                        <button
                          type="button"
                          onClick={() =>
                            void navigator.clipboard.writeText(
                              login.user_code ?? "",
                            )
                          }
                        >
                          <Copy size={15} /> Copy code
                        </button>
                      </div>
                    ) : null}
                    <div className="application-settings__login-actions">
                      <button
                        type="button"
                        disabled={busy}
                        onClick={() => void openLogin()}
                      >
                        <ExternalLink size={15} /> Continue in browser
                      </button>
                      <button
                        type="button"
                        disabled={busy}
                        onClick={() => void cancelLogin()}
                      >
                        Cancel
                      </button>
                    </div>
                  </>
                ) : null}
                {["cancelled", "failed"].includes(login.status) &&
                canConnect ? (
                  <div className="application-settings__login-actions">
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void beginLogin("browser")}
                    >
                      Try browser login
                    </button>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void beginLogin("device_code")}
                    >
                      Use device code
                    </button>
                  </div>
                ) : null}
                {login.status === "completed" && canDisconnect ? (
                  <div className="application-settings__login-actions">
                    <button
                      type="button"
                      className="application-settings__logout"
                      disabled={busy}
                      onClick={() => void disconnect()}
                    >
                      <LogOut size={15} /> Disconnect
                    </button>
                  </div>
                ) : null}
              </div>
            ) : connection.provider === "codex" ? (
              <div className="application-settings__login-actions">
                {canConnect ? (
                  <>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void beginLogin("browser")}
                    >
                      Connect in browser
                    </button>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void beginLogin("device_code")}
                    >
                      Use device code
                    </button>
                  </>
                ) : null}
                {canDisconnect ? (
                  <button
                    type="button"
                    className="application-settings__logout"
                    disabled={busy}
                    onClick={() => void disconnect()}
                  >
                    <LogOut size={15} /> Disconnect
                  </button>
                ) : null}
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
                    value={anthropicKey}
                    autoComplete="off"
                    disabled={busy}
                    onChange={(event) => setAnthropicKey(event.target.value)}
                  />
                  <small>Stored only in your system credential vault.</small>
                </label>
                <button type="submit" disabled={busy || !anthropicKey.trim()}>
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
                    value={geminiKey}
                    autoComplete="off"
                    disabled={busy}
                    onChange={(event) => setGeminiKey(event.target.value)}
                  />
                  <small>Stored only in your system credential vault.</small>
                </label>
                <button type="submit" disabled={busy || !geminiKey.trim()}>
                  Connect Gemini
                </button>
              </form>
            ) : connection.provider !== "codex" && canDisconnect ? (
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

            <label>
              <span>Model</span>
              <select
                value={
                  selectedModel?.model_id ?? connectionSelection?.model_id ?? ""
                }
                disabled={busy || !aiAvailable || connection.status !== "ready"}
                onChange={(event) => {
                  const model = connection.models.find(
                    (candidate) => candidate.model_id === event.target.value,
                  );
                  const reasoning =
                    model?.reasoning_options.find((option) => option.is_default)
                      ?.selection ?? model?.reasoning_options[0]?.selection;
                  if (model && reasoning) {
                    void saveSelection(model.model_id, reasoning);
                  }
                }}
              >
                {!connectionSelection ? (
                  <option value="" disabled>
                    Choose a live model
                  </option>
                ) : null}
                {modelUnavailable && connectionSelection ? (
                  <option value={connectionSelection.model_id} disabled>
                    Unavailable — {connectionSelection.model_id}
                  </option>
                ) : null}
                {connection.models.map((model) => (
                  <option key={model.model_id} value={model.model_id}>
                    {model.display_name}
                  </option>
                ))}
              </select>
              {selectedModel?.description ? (
                <small>{selectedModel.description}</small>
              ) : null}
              {modelUnavailable ? (
                <small className="application-settings__unavailable">
                  Waiting for AI Provider. The saved model is no longer in the
                  live catalog.
                  {recommendedModel
                    ? ` Choose ${recommendedModel.display_name}${recommendedReasoning ? ` with ${recommendedReasoning.label}` : ""} to continue.`
                    : " Reconnect this provider to load its models."}
                </small>
              ) : null}
            </label>

            <label>
              <span>Reasoning</span>
              <select
                value={
                  selectedReasoning
                    ? reasoningKey(selectedReasoning.selection)
                    : (persistedReasoningKey ?? "")
                }
                disabled={
                  busy ||
                  !aiAvailable ||
                  connection.status !== "ready" ||
                  !selectedModel
                }
                onChange={(event) => {
                  if (selectedModel) {
                    void saveSelection(
                      selectedModel.model_id,
                      JSON.parse(
                        event.target.value,
                      ) as ProviderReasoningSelection,
                    );
                  }
                }}
              >
                {reasoningUnavailable && persistedReasoningKey ? (
                  <option value={persistedReasoningKey} disabled>
                    Unavailable saved reasoning
                  </option>
                ) : null}
                {selectedModel?.reasoning_options.map((option) => (
                  <option
                    key={reasoningKey(option.selection)}
                    value={reasoningKey(option.selection)}
                  >
                    {option.label}
                  </option>
                ))}
              </select>
              {reasoningUnavailable ? (
                <small className="application-settings__unavailable">
                  Waiting for AI Provider. Choose a current reasoning option to
                  continue.
                </small>
              ) : null}
            </label>

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
                ? " · Applies only to future Agent Runs."
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

      <details className="application-settings__details">
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
        onToggle={(event) => {
          if (event.currentTarget.open && !runtime) void loadHostFacts();
        }}
      >
        <summary>
          <span>
            <Info size={18} aria-hidden="true" />
            <span>
              <strong>About &amp; Diagnostics</strong>
              <small>Version and local runtime details</small>
            </span>
          </span>
        </summary>
        {settings ? (
          <dl className="application-settings__diagnostics">
            <div>
              <dt>Quantix</dt>
              <dd>{settings.diagnostics.quantix_version}</dd>
            </div>
            <div>
              <dt>AI runtime</dt>
              <dd>{runtime?.state.replace(/_/g, " ") ?? "Inspecting…"}</dd>
            </div>
            <div>
              <dt>Installation data</dt>
              <dd>schema {settings.diagnostics.installation_schema_version}</dd>
            </div>
            <div>
              <dt>Tender data</dt>
              <dd>schema {settings.diagnostics.tender_schema_version}</dd>
            </div>
          </dl>
        ) : null}
      </details>

      <p className="application-settings__notification-note">
        <Bell size={15} aria-hidden="true" /> Preferences are application-wide;
        provider and model changes never alter an Agent Run already in progress.
      </p>
    </main>
  );
}
