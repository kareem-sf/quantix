import {
  ArrowLeft,
  Bell,
  Check,
  ClipboardCopy,
  Database,
  ExternalLink,
  Info,
  LoaderCircle,
  LogOut,
  Pencil,
  Plus,
  RefreshCw,
  Trash2,
  Settings2,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { m } from "motion/react";
import { useCallback, useEffect, useRef, useState } from "react";

import { applyGeneralApplicationPreferences } from "./applicationPreferences";
import { enableAttentionNotifications } from "./applicationNotifications";
import { QuantixMark } from "./QuantixMark";
import { DiagnosticsTimeline } from "./DiagnosticsTimeline";
import type { ApplicationSettingsView } from "./bindings/ApplicationSettingsView";
import type { GeneralApplicationPreferences } from "./bindings/GeneralApplicationPreferences";
import type { AiProviderRoute } from "./bindings/AiProviderRoute";
import type { AiProviderSettingsView } from "./bindings/AiProviderSettingsView";
import type { AiProviderView } from "./bindings/AiProviderView";
import type { ProviderConnectionView } from "./bindings/ProviderConnectionView";
import type { ProviderReasoningSelection } from "./bindings/ProviderReasoningSelection";
import type { RuntimeReadiness } from "./bindings/RuntimeReadiness";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import type { QuantixDoctorFinding } from "./bindings/QuantixDoctorFinding";
import type { QuantixDoctorReport } from "./bindings/QuantixDoctorReport";
import { exactApplicationAiSelectionIsReady } from "./applicationAiSelectionReadiness";
import { QuantixSelect } from "./ui/QuantixSelect";
import { QuantixSwitch } from "./ui/QuantixSwitch";
import { QuantixDialog } from "./ui/QuantixDialog";
import "./ui/quantix-ui.css";
import {
  cancelChatGptLogin,
  checkQuantixUpdate,
  confirmAiExecutionSelection,
  disconnectChatGpt,
  inspectAiProviders,
  inspectQuantixDoctor,
  probeAiProvider,
  inspectRuntimeReadiness,
  removeAiProvider,
  saveAiProvider,
  setActiveAiProvider,
  openChatGptDeviceLoginPage,
  repairQuantixDoctor,
  refreshApplicationSettings,
  startChatGptDeviceLogin,
  startChatGptLogin,
  updateAiExecutionSelection,
  updateGeneralApplicationPreferences,
  validateQuantixUpdateRestart,
} from "./quantixHost";

interface ApplicationSettingsProps {
  onAiAvailabilityChange: (available: boolean) => void;
  onSettingsChange?: (settings: ApplicationSettingsView) => void;
  onPreferencesChange?: (preferences: GeneralApplicationPreferences) => void;
  initialSection?: "general" | "ai" | "about";
  selectedTenderId?: string | null;
  onClose: () => void;
}

type ApplicationSettingsSection =
  "general" | "ai" | "data" | "updates" | "about";

const SETTINGS_NAVIGATION = [
  { id: "general", label: "General", icon: Settings2 },
  { id: "ai", label: "AI Providers", icon: Sparkles },
  { id: "data", label: "Data & Storage", icon: Database },
  { id: "updates", label: "Updates", icon: RefreshCw },
  { id: "about", label: "About & Diagnostics", icon: Info },
] as const satisfies readonly {
  id: ApplicationSettingsSection;
  label: string;
  icon: typeof Settings2;
}[];

interface ProviderDraft {
  id: string;
  display_name: string;
  route: AiProviderRoute;
  base_url: string;
  model_id: string;
  api_key: string;
  /** Set when editing, so the key field can mean "leave the stored one alone". */
  existingId: string | null;
}

/**
 * Starting points for the common endpoints. They only prefill the form — anything
 * reachable over an OpenAI- or Anthropic-compatible API can be entered by hand.
 */
const PROVIDER_PRESETS: {
  key: string;
  label: string;
  draft: Omit<ProviderDraft, "api_key" | "existingId">;
}[] = [
  {
    key: "openrouter",
    label: "OpenRouter",
    draft: {
      id: "OPENROUTER",
      display_name: "OpenRouter",
      route: "openai_compatible",
      base_url: "https://openrouter.ai/api/v1",
      model_id: "anthropic/claude-sonnet-4.5",
    },
  },
  {
    key: "groq",
    label: "Groq",
    draft: {
      id: "GROQ",
      display_name: "Groq",
      route: "openai_compatible",
      base_url: "https://api.groq.com/openai/v1",
      model_id: "llama-3.3-70b-versatile",
    },
  },
  {
    key: "openai",
    label: "OpenAI",
    draft: {
      id: "OPENAI",
      display_name: "OpenAI",
      route: "openai",
      base_url: "",
      model_id: "gpt-5.3",
    },
  },
  {
    key: "anthropic",
    label: "Anthropic",
    draft: {
      id: "ANTHROPIC",
      display_name: "Anthropic",
      route: "anthropic",
      base_url: "",
      model_id: "claude-sonnet-4-5",
    },
  },
  {
    key: "google",
    label: "Google Gemini",
    draft: {
      id: "GOOGLE",
      display_name: "Google Gemini",
      route: "google",
      base_url: "",
      model_id: "gemini-3-pro",
    },
  },
  {
    key: "custom",
    label: "Other (OpenAI-compatible)",
    draft: {
      id: "",
      display_name: "",
      route: "openai_compatible",
      base_url: "",
      model_id: "",
    },
  },
];

const PROVIDER_ROUTE_LABELS: Record<AiProviderRoute, string> = {
  openai: "OpenAI",
  openai_compatible: "OpenAI-compatible",
  anthropic: "Anthropic",
  anthropic_compatible: "Anthropic-compatible",
  google: "Google",
  xai: "xAI",
};

const PROVIDER_ROUTE_ORDER: AiProviderRoute[] = [
  "openai_compatible",
  "anthropic_compatible",
  "openai",
  "anthropic",
  "google",
  "xai",
];

function routeRequiresBaseUrl(route: AiProviderRoute): boolean {
  return route === "openai_compatible" || route === "anthropic_compatible";
}

function emptyDraft(): ProviderDraft {
  return {
    id: "",
    display_name: "",
    route: "openai_compatible",
    base_url: "",
    model_id: "",
    api_key: "",
    existingId: null,
  };
}

function draftFrom(connection: AiProviderView): ProviderDraft {
  return {
    id: connection.id,
    display_name: connection.display_name,
    route: connection.route,
    base_url: connection.base_url ?? "",
    model_id: connection.model_id,
    api_key: "",
    existingId: connection.id,
  };
}

function settingsError(reason: unknown): string {
  if (typeof reason === "object" && reason !== null && "code" in reason) {
    if (reason.code === "local_document_tools_required") {
      return "Prepare document tools before continuing.";
    }
    if (reason.code === "ai_provider_required") {
      return "Connect ChatGPT before using AI for Tender work.";
    }
    if (reason.code === "oauth_already_running") {
      return "A ChatGPT sign-in is already running. Finish it in your browser or cancel it first.";
    }
    if (reason.code === "runtime_required") {
      return "Connect ChatGPT before using AI for Tender work.";
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
  if (selection.kind === "provider_default") return "ChatGPT default";
  return selection.value.replace(/_/g, " ");
}

function connectionStatus(connection: ProviderConnectionView): string {
  if (connection.status === "ready") return "Connected";
  if (connection.status === "authentication_required") return "Not connected";
  return "Needs attention";
}

const CHATGPT_DATA_DISCLOSURE =
  "Tender content is sent to OpenAI through your connected ChatGPT account. Usage and limits belong to that account.";

interface DeviceLoginDetails {
  userCode: string;
}

interface AiSelectionReadinessResult {
  settings: ApplicationSettingsView;
  ready: boolean;
}

function reasonHasCode(reason: unknown, code: string): boolean {
  return (
    typeof reason === "object" &&
    reason !== null &&
    "code" in reason &&
    reason.code === code
  );
}

function waitForLoginOwnerRelease(): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, 150));
}

function catalogueProvenance(connection: ProviderConnectionView): string {
  const catalogue = connection.catalogue_fetched_at;
  if (!catalogue) return "No current ChatGPT catalogue is available.";
  return `Built-in catalogue version: ${catalogue}`;
}

export function ApplicationSettings({
  onAiAvailabilityChange,
  onSettingsChange,
  onPreferencesChange,
  initialSection = "general",
  selectedTenderId = null,
  onClose,
}: ApplicationSettingsProps) {
  const [settings, setSettings] = useState<ApplicationSettingsView | null>(
    null,
  );
  const [busy, setBusy] = useState(false);
  const [refreshError, setRefreshError] = useState<string | null>(null);
  const [generalActionError, setGeneralActionError] = useState<string | null>(
    null,
  );
  const [deviceLoginError, setDeviceLoginError] = useState<string | null>(null);
  const [deviceOpenerError, setDeviceOpenerError] = useState<string | null>(
    null,
  );
  const [deviceClipboardError, setDeviceClipboardError] = useState<
    string | null
  >(null);
  const [browserLoginStarted, setBrowserLoginStarted] = useState(false);
  const [deviceLogin, setDeviceLogin] = useState<DeviceLoginDetails | null>(
    null,
  );
  const [deviceCodeCopied, setDeviceCodeCopied] = useState(false);
  const [aiSelectionReadiness, setAiSelectionReadiness] =
    useState<AiSelectionReadinessResult | null>(null);
  const [preferenceBusy, setPreferenceBusy] = useState(false);
  const [runtime, setRuntime] = useState<RuntimeReadiness | null>(null);
  const [update, setUpdate] = useState<UpdateStatus | null>(null);
  const [factsBusy, setFactsBusy] = useState(false);
  const [doctor, setDoctor] = useState<QuantixDoctorReport | null>(null);
  const [repairAllOpen, setRepairAllOpen] = useState(false);
  const [providers, setProviders] = useState<AiProviderSettingsView | null>(
    null,
  );
  const [providerBusy, setProviderBusy] = useState(false);
  const [providerError, setProviderError] = useState<string | null>(null);
  const [providerDraft, setProviderDraft] = useState<ProviderDraft | null>(
    null,
  );
  const [probing, setProbing] = useState<string | null>(null);
  const [probeResult, setProbeResult] = useState<{
    id: string;
    reached: boolean;
    summary: string;
  } | null>(null);
  const mountedRef = useRef(false);
  const ownedLoginRef = useRef<"browser" | "device" | null>(null);
  const settingsRequestVersionRef = useRef(0);
  const loginPollInFlightRef = useRef(false);
  const settingsMutationInFlightRef = useRef(false);
  const deviceOpenerAttemptVersionRef = useRef(0);
  const deviceClipboardAttemptVersionRef = useRef(0);
  const [activeSection, setActiveSection] =
    useState<ApplicationSettingsSection>(initialSection);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      settingsRequestVersionRef.current += 1;
      deviceOpenerAttemptVersionRef.current += 1;
      deviceClipboardAttemptVersionRef.current += 1;
      if (ownedLoginRef.current) {
        ownedLoginRef.current = null;
        void cancelChatGptLogin();
      }
    };
  }, []);

  useEffect(() => {
    setActiveSection(initialSection);
  }, [initialSection]);
  const acceptSettings = useCallback(
    (view: ApplicationSettingsView) => {
      if (!mountedRef.current) return view;
      if (
        view.chatgpt?.state === "connected" ||
        view.chatgpt?.login_phase === "completed" ||
        view.chatgpt?.login_phase === "failed" ||
        view.chatgpt?.login_phase === "cancelled"
      ) {
        ownedLoginRef.current = null;
        deviceOpenerAttemptVersionRef.current += 1;
        deviceClipboardAttemptVersionRef.current += 1;
        if (view.chatgpt?.login_phase !== "failed") {
          setDeviceLoginError(null);
        }
        setDeviceOpenerError(null);
        setDeviceClipboardError(null);
      }
      setRefreshError(null);
      setSettings(view);
      applyGeneralApplicationPreferences(view.general_preferences);
      onPreferencesChange?.(view.general_preferences);
      return view;
    },
    [onPreferencesChange],
  );

  const acceptAuthoritativeSettings = useCallback(
    (view: ApplicationSettingsView) => {
      settingsRequestVersionRef.current += 1;
      setGeneralActionError(null);
      return acceptSettings(view);
    },
    [acceptSettings],
  );

  const refreshLatestSettings = useCallback(async () => {
    const requestVersion = ++settingsRequestVersionRef.current;
    try {
      const view = await refreshApplicationSettings();
      if (
        !mountedRef.current ||
        requestVersion !== settingsRequestVersionRef.current
      ) {
        return null;
      }
      return acceptSettings(view);
    } catch (reason) {
      if (
        mountedRef.current &&
        requestVersion === settingsRequestVersionRef.current
      ) {
        setRefreshError(settingsError(reason));
      }
      return null;
    }
  }, [acceptSettings]);

  const load = useCallback(async () => {
    setBusy(true);
    setRefreshError(null);
    await refreshLatestSettings();
    if (mountedRef.current) setBusy(false);
  }, [refreshLatestSettings]);

  useEffect(() => {
    void load();
  }, [load]);

  const loadProviders = useCallback(async () => {
    try {
      const view = await inspectAiProviders();
      if (mountedRef.current) setProviders(view);
    } catch (reason) {
      if (mountedRef.current) setProviderError(settingsError(reason));
    }
  }, []);

  useEffect(() => {
    void loadProviders();
  }, [loadProviders]);

  // Every provider mutation returns the whole settings view, so one helper keeps the
  // busy flag, the error banner and the refreshed list consistent across all of them.
  const runProviderAction = useCallback(
    async (action: () => Promise<AiProviderSettingsView>) => {
      setProviderBusy(true);
      setProviderError(null);
      try {
        const view = await action();
        if (mountedRef.current) setProviders(view);
        return true;
      } catch (reason) {
        if (mountedRef.current) setProviderError(settingsError(reason));
        return false;
      } finally {
        if (mountedRef.current) setProviderBusy(false);
      }
    },
    [],
  );

  const chatgpt = settings?.chatgpt ?? null;
  const chatgptConnected = chatgpt?.state === "connected";
  const chatgptLoginPhase = chatgpt?.login_phase ?? "idle";
  const chatgptBrowserPending =
    !chatgptConnected &&
    (chatgptLoginPhase === "awaiting_browser" || browserLoginStarted);
  const chatgptDevicePending =
    !chatgptConnected &&
    (chatgptLoginPhase === "awaiting_device" ||
      deviceLogin !== null ||
      deviceLoginError !== null);
  const orphanedDeviceLogin =
    chatgptLoginPhase === "awaiting_device" &&
    deviceLogin === null &&
    ownedLoginRef.current !== "device";
  useEffect(() => {
    if (
      (!chatgptBrowserPending && !chatgptDevicePending) ||
      chatgptLoginPhase === "completed" ||
      chatgptLoginPhase === "failed" ||
      chatgptLoginPhase === "cancelled"
    ) {
      return;
    }
    const timer = window.setInterval(() => {
      if (loginPollInFlightRef.current || settingsMutationInFlightRef.current) {
        return;
      }
      loginPollInFlightRef.current = true;
      void refreshLatestSettings().finally(() => {
        loginPollInFlightRef.current = false;
      });
    }, 1_200);
    return () => window.clearInterval(timer);
  }, [
    chatgptBrowserPending,
    chatgptDevicePending,
    chatgptLoginPhase,
    refreshLatestSettings,
  ]);

  useEffect(() => {
    if (
      chatgptConnected ||
      chatgptLoginPhase === "completed" ||
      chatgptLoginPhase === "failed" ||
      chatgptLoginPhase === "cancelled"
    ) {
      setBrowserLoginStarted(false);
      setDeviceLogin(null);
      setDeviceCodeCopied(false);
    }
  }, [chatgptConnected, chatgptLoginPhase]);

  const persistedSelection = settings?.ai_execution_selection;
  const connection = settings?.provider_connections.find(
    (candidate) => candidate.connection_id === "codex_chatgpt",
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
  const selectionIsReady = Boolean(
    settings &&
    aiSelectionReadiness?.settings === settings &&
    aiSelectionReadiness.ready,
  );

  useEffect(() => {
    if (!settings) return;
    let current = true;
    void exactApplicationAiSelectionIsReady(settings)
      .then((ready) => {
        if (current) setAiSelectionReadiness({ settings, ready });
      })
      .catch(() => {
        if (current) setAiSelectionReadiness({ settings, ready: false });
      });
    return () => {
      current = false;
    };
  }, [settings]);

  useEffect(() => {
    if (!settings) return;
    onAiAvailabilityChange(selectionIsReady);
    onSettingsChange?.(settings);
  }, [onAiAvailabilityChange, onSettingsChange, selectionIsReady, settings]);

  const savePreferences = useCallback(
    async (preferences: GeneralApplicationPreferences) => {
      if (!settings || preferenceBusy) return;
      const previous = settings;
      const optimistic = { ...settings, general_preferences: preferences };
      setPreferenceBusy(true);
      setSettings(optimistic);
      applyGeneralApplicationPreferences(preferences);
      onPreferencesChange?.(preferences);
      setGeneralActionError(null);
      settingsMutationInFlightRef.current = true;
      try {
        acceptAuthoritativeSettings(
          await updateGeneralApplicationPreferences({ preferences }),
        );
      } catch (reason) {
        setSettings(previous);
        applyGeneralApplicationPreferences(previous.general_preferences);
        onPreferencesChange?.(previous.general_preferences);
        setGeneralActionError(settingsError(reason));
      } finally {
        settingsMutationInFlightRef.current = false;
        setPreferenceBusy(false);
      }
    },
    [
      acceptAuthoritativeSettings,
      onPreferencesChange,
      preferenceBusy,
      settings,
    ],
  );

  const setAttentionNotifications = useCallback(
    async (enabled: boolean) => {
      if (!preferences) return;
      if (enabled) {
        try {
          if (!(await enableAttentionNotifications())) {
            setGeneralActionError(
              "Operating-system notifications remain off because permission was not granted.",
            );
            return;
          }
        } catch {
          setGeneralActionError(
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
      setGeneralActionError(null);
      settingsMutationInFlightRef.current = true;
      try {
        acceptAuthoritativeSettings(
          await updateAiExecutionSelection({
            connection_id: connection.connection_id,
            model_id: modelId,
            reasoning,
          }),
        );
      } catch (reason) {
        setGeneralActionError(settingsError(reason));
      } finally {
        settingsMutationInFlightRef.current = false;
        setBusy(false);
      }
    },
    [acceptAuthoritativeSettings, connection],
  );

  const runSettingsAction = useCallback(
    async (operation: () => Promise<ApplicationSettingsView>) => {
      setBusy(true);
      setGeneralActionError(null);
      settingsMutationInFlightRef.current = true;
      try {
        acceptAuthoritativeSettings(await operation());
        return true;
      } catch (reason) {
        setGeneralActionError(settingsError(reason));
        return false;
      } finally {
        settingsMutationInFlightRef.current = false;
        setBusy(false);
      }
    },
    [acceptAuthoritativeSettings],
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
    setGeneralActionError(null);
    setDeviceLoginError(null);
    setDeviceOpenerError(null);
    setDeviceClipboardError(null);
    deviceOpenerAttemptVersionRef.current += 1;
    deviceClipboardAttemptVersionRef.current += 1;
    setBrowserLoginStarted(false);
    setDeviceLogin(null);
    setDeviceCodeCopied(false);
    ownedLoginRef.current = "browser";
    settingsMutationInFlightRef.current = true;
    try {
      const result = await startChatGptLogin();
      if (!mountedRef.current) {
        ownedLoginRef.current = null;
        await cancelChatGptLogin();
        return;
      }
      if (result.status === "connected") {
        ownedLoginRef.current = null;
      }
      setBrowserLoginStarted(result.status === "awaiting_browser");
      await refreshLatestSettings();
    } catch (reason) {
      if (!mountedRef.current) {
        ownedLoginRef.current = null;
        void cancelChatGptLogin();
        return;
      }
      ownedLoginRef.current = null;
      setGeneralActionError(settingsError(reason));
    } finally {
      settingsMutationInFlightRef.current = false;
      if (mountedRef.current) setBusy(false);
    }
  }, [refreshLatestSettings]);

  const connectChatGptOnAnotherDevice = useCallback(
    async (waitForBrowserCancellation = false) => {
      setBusy(true);
      setGeneralActionError(null);
      setDeviceLoginError(null);
      setDeviceOpenerError(null);
      setDeviceClipboardError(null);
      deviceOpenerAttemptVersionRef.current += 1;
      deviceClipboardAttemptVersionRef.current += 1;
      setBrowserLoginStarted(false);
      setDeviceLogin(null);
      setDeviceCodeCopied(false);
      ownedLoginRef.current = "device";
      settingsMutationInFlightRef.current = true;
      try {
        let result: Awaited<ReturnType<typeof startChatGptDeviceLogin>>;
        for (let attempt = 0; ; attempt += 1) {
          if (!mountedRef.current) {
            ownedLoginRef.current = null;
            await cancelChatGptLogin();
            return;
          }
          try {
            result = await startChatGptDeviceLogin();
            break;
          } catch (reason) {
            if (
              !waitForBrowserCancellation ||
              !reasonHasCode(reason, "oauth_already_running") ||
              attempt >= 9
            ) {
              throw reason;
            }
            await waitForLoginOwnerRelease();
          }
        }
        if (!mountedRef.current) {
          ownedLoginRef.current = null;
          await cancelChatGptLogin();
          return;
        }
        setDeviceLogin({ userCode: result.user_code });
        const openerAttemptVersion = ++deviceOpenerAttemptVersionRef.current;
        let devicePageOpenFailure: string | null = null;
        try {
          await openChatGptDeviceLoginPage();
          if (
            mountedRef.current &&
            openerAttemptVersion === deviceOpenerAttemptVersionRef.current
          ) {
            setDeviceOpenerError(null);
          }
        } catch {
          devicePageOpenFailure =
            "Quantix could not open the OpenAI sign-in page. Your one-time code is still ready. Use the button below, or open the OpenAI sign-in page yourself.";
        }
        if (!(await refreshLatestSettings()) && mountedRef.current) {
          if (devicePageOpenFailure === null) {
            setRefreshError(
              "Sign-in started. Quantix will keep checking while you use the one-time code.",
            );
          }
        }
        if (
          devicePageOpenFailure !== null &&
          mountedRef.current &&
          openerAttemptVersion === deviceOpenerAttemptVersionRef.current
        ) {
          setDeviceOpenerError(devicePageOpenFailure);
        }
      } catch {
        if (!mountedRef.current) {
          ownedLoginRef.current = null;
          void cancelChatGptLogin();
          return;
        }
        ownedLoginRef.current = null;
        setDeviceLogin(null);
        setDeviceLoginError(
          "Sign in on another device is unavailable right now. You can still connect ChatGPT in your browser.",
        );
      } finally {
        settingsMutationInFlightRef.current = false;
        if (mountedRef.current) setBusy(false);
      }
    },
    [refreshLatestSettings],
  );

  const copyDeviceCode = useCallback(async () => {
    if (!deviceLogin) return;
    const attemptVersion = ++deviceClipboardAttemptVersionRef.current;
    setDeviceCodeCopied(false);
    try {
      await navigator.clipboard.writeText(deviceLogin.userCode);
      if (
        !mountedRef.current ||
        attemptVersion !== deviceClipboardAttemptVersionRef.current
      ) {
        return;
      }
      setDeviceCodeCopied(true);
      setDeviceClipboardError(null);
    } catch {
      if (
        !mountedRef.current ||
        attemptVersion !== deviceClipboardAttemptVersionRef.current
      ) {
        return;
      }
      setDeviceClipboardError(
        "Quantix could not copy the code. Select it and copy it manually.",
      );
    }
  }, [deviceLogin]);

  const openDeviceSignInPage = useCallback(async () => {
    if (!deviceLogin) return;
    const attemptVersion = ++deviceOpenerAttemptVersionRef.current;
    try {
      await openChatGptDeviceLoginPage();
      if (
        !mountedRef.current ||
        attemptVersion !== deviceOpenerAttemptVersionRef.current
      ) {
        return;
      }
      setDeviceOpenerError(null);
    } catch {
      if (
        !mountedRef.current ||
        attemptVersion !== deviceOpenerAttemptVersionRef.current
      ) {
        return;
      }
      setDeviceOpenerError(
        "Quantix could not open the OpenAI sign-in page. Your one-time code is still ready. Try again, or open the OpenAI sign-in page yourself.",
      );
    }
  }, [deviceLogin]);

  const cancelChatGpt = useCallback(async () => {
    setBusy(true);
    setGeneralActionError(null);
    setDeviceLoginError(null);
    deviceOpenerAttemptVersionRef.current += 1;
    deviceClipboardAttemptVersionRef.current += 1;
    settingsMutationInFlightRef.current = true;
    try {
      await cancelChatGptLogin();
      setDeviceOpenerError(null);
      setDeviceClipboardError(null);
      ownedLoginRef.current = null;
      setBrowserLoginStarted(false);
      setDeviceLogin(null);
      setDeviceCodeCopied(false);
      await refreshLatestSettings();
      return true;
    } catch (reason) {
      if (mountedRef.current) setDeviceLoginError(settingsError(reason));
      return false;
    } finally {
      settingsMutationInFlightRef.current = false;
      if (mountedRef.current) setBusy(false);
    }
  }, [refreshLatestSettings]);

  const disconnectChatGptAccount = async () => {
    if (await runSettingsAction(disconnectChatGpt)) {
      setDeviceLogin(null);
      setDeviceCodeCopied(false);
      setBrowserLoginStarted(false);
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
            setGeneralActionError(
              "The signed update source is unavailable. No local repair can fix an external outage.",
            ),
          );
        setDoctor(await inspectQuantixDoctor(selectedTenderId));
        return;
      }
      setFactsBusy(true);
      setGeneralActionError(null);
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
        setGeneralActionError(settingsError(reason));
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
    setGeneralActionError(null);
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
      await refreshLatestSettings();
    } catch (reason) {
      setGeneralActionError(settingsError(reason));
    } finally {
      setFactsBusy(false);
    }
  }, [doctor, refreshLatestSettings]);

  const checkForUpdate = useCallback(async () => {
    setFactsBusy(true);
    setGeneralActionError(null);
    try {
      setUpdate(await checkQuantixUpdate());
    } catch {
      setGeneralActionError(
        "Quantix could not reach the signed update source. Try again later.",
      );
    } finally {
      setFactsBusy(false);
    }
  }, []);

  const activeModel = settings?.provider_connections
    .find(
      (candidate) =>
        candidate.connection_id === persistedSelection?.connection_id,
    )
    ?.models.find((model) => model.model_id === persistedSelection?.model_id);
  const globalError = generalActionError ?? refreshError;

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
        {globalError ? (
          <p className="application-settings__error" role="alert">
            {globalError}
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
                <h2 id="ai-settings">AI Providers</h2>
                <p>
                  Connect the AI accounts Quantix may use for Tender work.
                  Sign-in always completes in your browser, with the provider.
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

            {connection ? (
              <>
                <div className="application-settings__chatgpt-card">
                  <div className="application-settings__chatgpt-status">
                    <div>
                      <span className="application-settings__chatgpt-mark">
                        <Sparkles size={17} aria-hidden="true" />
                      </span>
                      <span>
                        <strong>ChatGPT</strong>
                        <small>
                          {chatgptConnected
                            ? (connection.account_label ??
                              chatgpt?.account_id ??
                              "Connected account")
                            : "Your ChatGPT account for Tender work"}
                        </small>
                      </span>
                    </div>
                    <span data-status={connection.status}>
                      {connectionStatus(connection)}
                    </span>
                  </div>

                  {chatgptConnected && chatgpt ? (
                    <>
                      <div className="application-settings__connected-account">
                        <div>
                          <Check size={16} aria-hidden="true" />
                          <span>
                            <strong>Connected</strong>
                            {(connection.account_plan ?? chatgpt.plan_type) ? (
                              <small>
                                {(
                                  connection.account_plan ?? chatgpt.plan_type
                                )?.replace(/_/g, " ")}
                                {" plan"}
                              </small>
                            ) : null}
                          </span>
                        </div>
                        <button
                          type="button"
                          className="application-settings__logout"
                          disabled={busy}
                          onClick={() => void disconnectChatGptAccount()}
                        >
                          <LogOut size={15} /> Disconnect
                        </button>
                      </div>
                      {connection.status !== "ready" ? (
                        <div
                          className="application-settings__connection-attention"
                          role="status"
                        >
                          <strong>ChatGPT needs attention</strong>
                          <p>{connection.status_summary}</p>
                          <small>
                            Select Refresh above. If ChatGPT is still
                            unavailable, disconnect it and connect again.
                          </small>
                        </div>
                      ) : null}
                    </>
                  ) : chatgptBrowserPending ? (
                    <div
                      className="application-settings__sign-in-state"
                      aria-live="polite"
                    >
                      <p>
                        <LoaderCircle className="is-spinning" size={16} />
                        Finish signing in in your browser. Quantix will connect
                        automatically.
                      </p>
                      <div className="application-settings__login-actions">
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => void cancelChatGpt()}
                        >
                          Cancel
                        </button>
                      </div>
                      <details className="application-settings__trouble">
                        <summary>Having trouble signing in?</summary>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => {
                            void cancelChatGpt().then((cancelled) => {
                              if (cancelled) {
                                return connectChatGptOnAnotherDevice(true);
                              }
                            });
                          }}
                        >
                          Sign in on another device
                        </button>
                      </details>
                    </div>
                  ) : chatgptDevicePending ? (
                    <div className="application-settings__device-sign-in">
                      {deviceLoginError ? (
                        <p
                          className="application-settings__error"
                          role="alert"
                          aria-label="Device sign-in problem"
                        >
                          {deviceLoginError}
                        </p>
                      ) : null}
                      <p>
                        {orphanedDeviceLogin
                          ? "The previous one-time code is no longer available. Get a new code to continue."
                          : deviceLogin
                            ? "Enter this one-time code on the OpenAI page, then return to Quantix."
                            : "Sign in on another device to continue."}
                      </p>
                      {deviceLogin ? (
                        <>
                          <output
                            className="application-settings__device-code"
                            aria-label="One-time code"
                          >
                            {deviceLogin.userCode}
                          </output>
                          <div className="application-settings__device-actions">
                            <button
                              type="button"
                              disabled={busy}
                              onClick={() => void copyDeviceCode()}
                            >
                              {deviceCodeCopied ? (
                                <Check size={15} />
                              ) : (
                                <ClipboardCopy size={15} />
                              )}
                              {deviceCodeCopied ? "Copied" : "Copy code"}
                            </button>
                            <button
                              type="button"
                              disabled={busy}
                              onClick={() => void openDeviceSignInPage()}
                            >
                              <ExternalLink size={15} /> Open OpenAI sign-in
                              page
                            </button>
                          </div>
                          {deviceClipboardError ? (
                            <p
                              className="application-settings__error"
                              role="alert"
                              aria-label="Copy code problem"
                            >
                              {deviceClipboardError}
                            </p>
                          ) : null}
                          {deviceOpenerError ? (
                            <p
                              className="application-settings__error"
                              role="alert"
                              aria-label="OpenAI page problem"
                            >
                              {deviceOpenerError}
                            </p>
                          ) : null}
                        </>
                      ) : orphanedDeviceLogin ? (
                        <div className="application-settings__device-actions">
                          <button
                            type="button"
                            disabled={busy}
                            onClick={() => {
                              void cancelChatGpt().then((cancelled) => {
                                if (cancelled) {
                                  return connectChatGptOnAnotherDevice(true);
                                }
                              });
                            }}
                          >
                            Get a new one-time code
                          </button>
                        </div>
                      ) : deviceLoginError ? (
                        <div className="application-settings__device-actions">
                          <button
                            type="button"
                            className="application-settings__primary"
                            disabled={busy}
                            onClick={() => void connectChatGpt()}
                          >
                            Connect ChatGPT
                          </button>
                          <button
                            type="button"
                            disabled={busy}
                            onClick={() => void connectChatGptOnAnotherDevice()}
                          >
                            Try device sign-in again
                          </button>
                        </div>
                      ) : (
                        <small>Waiting for sign-in to finish.</small>
                      )}
                      {deviceLogin && !orphanedDeviceLogin ? (
                        <div
                          className="application-settings__device-waiting"
                          role="status"
                        >
                          <LoaderCircle className="is-spinning" size={15} />
                          Waiting for you to finish signing in
                        </div>
                      ) : null}
                      {deviceLogin || orphanedDeviceLogin ? (
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => void cancelChatGpt()}
                        >
                          Cancel
                        </button>
                      ) : null}
                    </div>
                  ) : chatgptLoginPhase === "failed" ? (
                    <div className="application-settings__sign-in-state">
                      <p role="alert">
                        ChatGPT sign-in did not finish. Try the browser again or
                        sign in on another device.
                      </p>
                      <div className="application-settings__login-actions">
                        <button
                          type="button"
                          className="application-settings__primary"
                          disabled={busy}
                          onClick={() => void connectChatGpt()}
                        >
                          Connect ChatGPT
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => void connectChatGptOnAnotherDevice()}
                        >
                          Sign in on another device
                        </button>
                      </div>
                    </div>
                  ) : (
                    <div className="application-settings__sign-in-state">
                      {chatgptLoginPhase === "cancelled" ? (
                        <p>
                          ChatGPT sign-in was cancelled. Try again when ready.
                        </p>
                      ) : (
                        <p>
                          Connect once. Quantix will remember this account on
                          this computer.
                        </p>
                      )}
                      <div className="application-settings__login-actions">
                        <button
                          type="button"
                          className="application-settings__primary"
                          disabled={busy}
                          onClick={() => void connectChatGpt()}
                        >
                          Connect ChatGPT
                        </button>
                      </div>
                      <details className="application-settings__trouble">
                        <summary>Having trouble signing in?</summary>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => void connectChatGptOnAnotherDevice()}
                        >
                          Sign in on another device
                        </button>
                      </details>
                    </div>
                  )}
                </div>

                <div className="application-settings__active-choice">
                  <Sparkles size={17} aria-hidden="true" />
                  <div>
                    <small>Default prepared for newly created Tenders</small>
                    <strong>
                      {persistedSelection
                        ? `ChatGPT · ${activeModel?.display_name ?? persistedSelection.model_id} · ${reasoningName(persistedSelection.reasoning)}`
                        : "Connect ChatGPT to prepare a recommended model"}
                    </strong>
                    {persistedSelection && !selectionIsReady ? (
                      <>
                        <small>{CHATGPT_DATA_DISCLOSURE}</small>
                        <button
                          type="button"
                          disabled={busy || !selectedReasoning}
                          onClick={() => void confirmSelection()}
                        >
                          Use ChatGPT
                        </button>
                      </>
                    ) : persistedSelection ? (
                      <small>ChatGPT is ready for new Tenders.</small>
                    ) : null}
                  </div>
                </div>

                <details className="application-settings__advanced">
                  <summary>Advanced model settings</summary>
                  <div className="application-settings__advanced-content">
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
                                  label:
                                    "Recommended model is prepared after connection",
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
                            )?.selection ??
                            model?.reasoning_options[0]?.selection;
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
                          The saved model is no longer in the current ChatGPT
                          catalogue.
                          {recommendedModel
                            ? ` Choose ${recommendedModel.display_name}${recommendedReasoning ? ` with ${reasoningName(recommendedReasoning.selection)}` : ""} to continue.`
                            : " Reconnect ChatGPT to load its models."}
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
                          busy ||
                          connection.status !== "ready" ||
                          !selectedModel
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
                          ...(selectedModel?.reasoning_options.map(
                            (option) => ({
                              value: reasoningKey(option.selection),
                              label:
                                option.selection.kind === "provider_default"
                                  ? "ChatGPT default"
                                  : option.label,
                              description: option.description ?? undefined,
                            }),
                          ) ?? []),
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
                          Choose a current reasoning option to continue.
                        </small>
                      ) : null}
                    </div>

                    <div className="application-settings__disclosure">
                      <ShieldCheck size={16} aria-hidden="true" />
                      <p>{CHATGPT_DATA_DISCLOSURE}</p>
                    </div>

                    <div className="application-settings__provenance">
                      <strong>Catalogue provenance</strong>
                      <span>{catalogueProvenance(connection)}</span>
                      <small>
                        Model changes become the default for future Tenders.
                        Existing Tenders keep their saved selection.
                      </small>
                    </div>
                  </div>
                </details>
              </>
            ) : busy ? (
              <div className="application-settings__loading" aria-live="polite">
                <LoaderCircle className="is-spinning" size={18} /> Loading
                ChatGPT settings…
              </div>
            ) : (
              <p className="application-settings__empty">
                ChatGPT settings are unavailable. Refresh to try again.
              </p>
            )}

            <div className="application-settings__providers">
              <div className="application-settings__providers-heading">
                <div>
                  <h3>Model providers</h3>
                  <p>
                    Use your own API key with any OpenAI- or
                    Anthropic-compatible endpoint.
                  </p>
                </div>
                <button
                  type="button"
                  disabled={providerBusy || providerDraft !== null}
                  onClick={() => {
                    setProviderError(null);
                    setProviderDraft(emptyDraft());
                  }}
                >
                  <Plus size={16} aria-hidden="true" />
                  Add provider
                </button>
              </div>

              {providerError ? (
                <p className="application-settings__error" role="alert">
                  {providerError}
                </p>
              ) : null}

              {providers && providers.connections.length > 0 ? (
                <ul className="application-settings__provider-list">
                  {providers.connections.map((connection) => (
                    <li key={connection.id}>
                      <div>
                        <span>
                          <strong>{connection.display_name}</strong>
                          {connection.is_active ? <em>Default</em> : null}
                        </span>
                        <small>
                          {PROVIDER_ROUTE_LABELS[connection.route]}
                          {" \u00b7 "}
                          {connection.model_id}
                          {connection.base_url
                            ? " \u00b7 " + connection.base_url
                            : ""}
                        </small>
                        {connection.has_api_key ? null : (
                          <small className="application-settings__unavailable">
                            No API key stored. Edit this provider to add one.
                          </small>
                        )}
                        {probeResult && probeResult.id === connection.id ? (
                          <small
                            className={
                              probeResult.reached
                                ? undefined
                                : "application-settings__unavailable"
                            }
                            role="status"
                          >
                            {probeResult.summary}
                          </small>
                        ) : null}
                      </div>
                      <div className="application-settings__provider-actions">
                        <button
                          type="button"
                          disabled={
                            providerBusy ||
                            probing !== null ||
                            !connection.has_api_key
                          }
                          onClick={() => {
                            setProbing(connection.id);
                            setProbeResult(null);
                            void probeAiProvider(connection.id)
                              .then((result) => {
                                if (!mountedRef.current) return;
                                setProbeResult({
                                  id: connection.id,
                                  ...result,
                                });
                              })
                              .catch((reason: unknown) => {
                                if (!mountedRef.current) return;
                                setProbeResult({
                                  id: connection.id,
                                  reached: false,
                                  summary: settingsError(reason),
                                });
                              })
                              .finally(() => {
                                if (mountedRef.current) setProbing(null);
                              });
                          }}
                        >
                          {probing === connection.id ? (
                            <LoaderCircle className="is-spinning" size={15} />
                          ) : null}
                          Test
                        </button>
                        {connection.is_active ? null : (
                          <button
                            type="button"
                            disabled={providerBusy}
                            onClick={() => {
                              void runProviderAction(() =>
                                setActiveAiProvider(connection.id),
                              );
                            }}
                          >
                            Make default
                          </button>
                        )}
                        <button
                          type="button"
                          disabled={providerBusy}
                          aria-label={"Edit " + connection.display_name}
                          onClick={() => {
                            setProviderError(null);
                            setProviderDraft(draftFrom(connection));
                          }}
                        >
                          <Pencil size={15} aria-hidden="true" />
                        </button>
                        <button
                          type="button"
                          disabled={providerBusy}
                          aria-label={"Remove " + connection.display_name}
                          onClick={() => {
                            void runProviderAction(() =>
                              removeAiProvider(connection.id),
                            );
                          }}
                        >
                          <Trash2 size={15} aria-hidden="true" />
                        </button>
                      </div>
                    </li>
                  ))}
                </ul>
              ) : providerDraft === null ? (
                <p className="application-settings__empty">
                  No model providers yet. Add one to use your own API key.
                </p>
              ) : null}

              {providerDraft ? (
                <form
                  className="application-settings__provider-form"
                  onSubmit={(event) => {
                    event.preventDefault();
                    const draft = providerDraft;
                    void runProviderAction(() =>
                      saveAiProvider({
                        id: draft.id,
                        display_name: draft.display_name,
                        route: draft.route,
                        base_url: draft.base_url.trim()
                          ? draft.base_url.trim()
                          : null,
                        model_id: draft.model_id,
                        // Blank while editing means keep the stored key, so the
                        // interface never has to hold a secret in order to save.
                        api_key:
                          draft.api_key.trim() === "" && draft.existingId
                            ? null
                            : draft.api_key,
                      }),
                    ).then((saved) => {
                      if (saved) setProviderDraft(null);
                    });
                  }}
                >
                  {providerDraft.existingId ? null : (
                    <div className="application-settings__provider-field">
                      <QuantixSelect
                        aria-label="Start from"
                        label="Start from"
                        value=""
                        onChange={(value) => {
                          const preset = PROVIDER_PRESETS.find(
                            (entry) => entry.key === value,
                          );
                          if (!preset) return;
                          setProviderDraft({
                            ...preset.draft,
                            api_key: "",
                            existingId: null,
                          });
                        }}
                        options={[
                          { value: "", label: "Choose a provider" },
                          ...PROVIDER_PRESETS.map((entry) => ({
                            value: entry.key,
                            label: entry.label,
                          })),
                        ]}
                      />
                    </div>
                  )}

                  <label>
                    <span>Name</span>
                    <input
                      type="text"
                      required
                      value={providerDraft.display_name}
                      onChange={(event) =>
                        setProviderDraft({
                          ...providerDraft,
                          display_name: event.target.value,
                        })
                      }
                    />
                  </label>

                  <div className="application-settings__provider-field">
                    <label>
                      <span>Id</span>
                      <input
                        type="text"
                        required
                        readOnly={providerDraft.existingId !== null}
                        aria-describedby="provider-id-hint"
                        value={providerDraft.id}
                        onChange={(event) =>
                          setProviderDraft({
                            ...providerDraft,
                            id: event.target.value,
                          })
                        }
                      />
                    </label>
                    <small id="provider-id-hint">
                      Letters, digits and underscores. Used as the entry name in
                      the .env file.
                    </small>
                  </div>

                  <div className="application-settings__provider-field">
                    <QuantixSelect
                      aria-label="API style"
                      label="API style"
                      value={providerDraft.route}
                      onChange={(value) =>
                        setProviderDraft({
                          ...providerDraft,
                          route: value as AiProviderRoute,
                        })
                      }
                      options={PROVIDER_ROUTE_ORDER.map((route) => ({
                        value: route,
                        label: PROVIDER_ROUTE_LABELS[route],
                      }))}
                    />
                  </div>

                  <label>
                    <span>
                      {routeRequiresBaseUrl(providerDraft.route)
                        ? "Base URL"
                        : "Base URL (optional)"}
                    </span>
                    <input
                      type="url"
                      required={routeRequiresBaseUrl(providerDraft.route)}
                      placeholder="https://openrouter.ai/api/v1"
                      value={providerDraft.base_url}
                      onChange={(event) =>
                        setProviderDraft({
                          ...providerDraft,
                          base_url: event.target.value,
                        })
                      }
                    />
                  </label>

                  <label>
                    <span>Model id</span>
                    <input
                      type="text"
                      required
                      value={providerDraft.model_id}
                      onChange={(event) =>
                        setProviderDraft({
                          ...providerDraft,
                          model_id: event.target.value,
                        })
                      }
                    />
                  </label>

                  <label>
                    <span>API key</span>
                    <input
                      type="password"
                      autoComplete="off"
                      required={providerDraft.existingId === null}
                      placeholder={
                        providerDraft.existingId
                          ? "Leave blank to keep the stored key"
                          : ""
                      }
                      value={providerDraft.api_key}
                      onChange={(event) =>
                        setProviderDraft({
                          ...providerDraft,
                          api_key: event.target.value,
                        })
                      }
                    />
                  </label>

                  <div className="application-settings__provider-form-actions">
                    <button type="submit" disabled={providerBusy}>
                      {providerBusy ? (
                        <LoaderCircle className="is-spinning" size={16} />
                      ) : null}
                      Save provider
                    </button>
                    <button
                      type="button"
                      disabled={providerBusy}
                      onClick={() => {
                        setProviderDraft(null);
                        setProviderError(null);
                      }}
                    >
                      Cancel
                    </button>
                  </div>
                </form>
              ) : null}

              {providers ? (
                <div className="application-settings__disclosure">
                  <ShieldCheck size={16} aria-hidden="true" />
                  <p>
                    API keys are stored as plain text in{" "}
                    <code>{providers.file_path}</code>. Anything that can read
                    that file can use your keys, so keep it as private as the
                    rest of your Quantix data.
                  </p>
                </div>
              ) : null}
            </div>
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
                                    "refresh_ai_provider"
                                  ? "Refresh ChatGPT"
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
              refresh ChatGPT status, and retry the redacted diagnostics writer
              and retention checks. It will not change your ChatGPT account,
              alter Tender data, install an application update, or make recovery
              decisions.
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
            application-wide. AI Providers supplies the default for new Tenders
            only; every existing Tender and Agent Run keeps its exact selection.
          </p>
        </m.div>
      </div>
    </main>
  );
}
