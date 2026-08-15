import { LoaderCircle, RefreshCw, Settings2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import type { ApplicationSettingsView } from "./bindings/ApplicationSettingsView";
import type { ProviderReasoningSelection } from "./bindings/ProviderReasoningSelection";
import {
  refreshApplicationSettings,
  updateAiExecutionSelection,
} from "./quantixHost";

interface ApplicationSettingsProps {
  aiAvailable: boolean;
}

function settingsError(reason: unknown): string {
  if (typeof reason === "object" && reason !== null && "code" in reason) {
    if (reason.code === "runtime_required") {
      return "The AI runtime is unavailable. Your last valid selection is preserved.";
    }
    if (reason.code === "invalid_command") {
      return "That model or reasoning option is no longer offered by the provider.";
    }
  }
  return "Quantix could not load AI settings.";
}

function reasoningKey(selection: ProviderReasoningSelection): string {
  return JSON.stringify(selection);
}

export function ApplicationSettings({ aiAvailable }: ApplicationSettingsProps) {
  const [settings, setSettings] = useState<ApplicationSettingsView | null>(
    null,
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      setSettings(await refreshApplicationSettings());
    } catch (reason) {
      setError(settingsError(reason));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [aiAvailable, load]);

  const connection = settings?.provider_connections.find(
    (candidate) => candidate.connection_id === "codex_chatgpt",
  );
  const persistedSelection = settings?.ai_execution_selection;
  const selectedModel = persistedSelection
    ? connection?.models.find(
        (model) => model.model_id === persistedSelection.model_id,
      )
    : connection?.models.find((model) => model.is_default);
  const persistedReasoningKey = persistedSelection
    ? reasoningKey(persistedSelection.reasoning)
    : null;
  const selectedReasoning = persistedReasoningKey
    ? selectedModel?.reasoning_options.find(
        (option) => reasoningKey(option.selection) === persistedReasoningKey,
      )
    : selectedModel?.reasoning_options.find((option) => option.is_default);
  const modelUnavailable = Boolean(persistedSelection && !selectedModel);
  const reasoningUnavailable = Boolean(
    persistedSelection && selectedModel && !selectedReasoning,
  );

  const save = useCallback(
    async (modelId: string, reasoning: ProviderReasoningSelection) => {
      if (!connection || !aiAvailable) return;
      setBusy(true);
      setError(null);
      try {
        setSettings(
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
    [aiAvailable, connection],
  );

  return (
    <main className="application-settings">
      <div className="application-settings__heading">
        <span aria-hidden="true">
          <Settings2 size={20} />
        </span>
        <div>
          <h1>Settings</h1>
          <p>Application-wide AI preferences for future Agent Runs.</p>
        </div>
      </div>

      <section
        className="application-settings__section"
        aria-labelledby="ai-settings"
      >
        <div className="application-settings__section-heading">
          <div>
            <h2 id="ai-settings">AI provider</h2>
            <p>
              Quantix reads models and reasoning choices from the live provider.
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

        {error ? (
          <p className="application-settings__error" role="alert">
            {error}
          </p>
        ) : null}

        {connection ? (
          <div className="application-settings__provider">
            <div className="application-settings__provider-status">
              <div>
                <strong>{connection.display_name}</strong>
                <span>
                  {connection.account_label ?? "No account details available"}
                </span>
              </div>
              <span data-status={connection.status}>
                {connection.status.replace(/_/g, " ")}
              </span>
            </div>
            <p>{connection.status_summary}</p>

            <label>
              <span>Model</span>
              <select
                value={
                  selectedModel?.model_id ?? persistedSelection?.model_id ?? ""
                }
                disabled={busy || !aiAvailable || connection.status !== "ready"}
                onChange={(event) => {
                  const model = connection.models.find(
                    (candidate) => candidate.model_id === event.target.value,
                  );
                  const reasoning =
                    model?.reasoning_options.find((option) => option.is_default)
                      ?.selection ?? model?.reasoning_options[0]?.selection;
                  if (model && reasoning) void save(model.model_id, reasoning);
                }}
              >
                {modelUnavailable && persistedSelection ? (
                  <option value={persistedSelection.model_id} disabled>
                    Unavailable — {persistedSelection.model_id}
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
                  The saved model is no longer in the provider's live catalog.
                  Choose an available model before Quantix starts another run.
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
                    void save(
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
                  The saved reasoning option is no longer available. Choose a
                  current option before Quantix starts another run.
                </small>
              ) : null}
            </label>

            {persistedSelection ? (
              <p className="application-settings__provenance">
                {modelUnavailable ||
                reasoningUnavailable ||
                connection.status !== "ready" ? (
                  "Saved selection is unavailable and blocked for new runs."
                ) : (
                  <>
                    Applies to new runs · catalog{" "}
                    {new Date(
                      persistedSelection.catalogue_fetched_at,
                    ).toLocaleString()}{" "}
                    · adapter {persistedSelection.adapter_version}
                  </>
                )}
              </p>
            ) : null}
          </div>
        ) : busy ? (
          <div className="application-settings__loading" aria-live="polite">
            <LoaderCircle className="is-spinning" size={18} /> Loading live
            models…
          </div>
        ) : (
          <p className="application-settings__empty">
            No provider connection has been discovered yet.
          </p>
        )}
      </section>
    </main>
  );
}
