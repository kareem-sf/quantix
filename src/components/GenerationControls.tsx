import { useContext, useRef, useState } from "react";
import { ApiContext, errorText, type Schema } from "../api";

export type GenerationSettings = Schema<"GenerationSettings">;
type Preview = Schema<"GenerationPreview">;
type Props = {
  value: GenerationSettings;
  onChange: (value: GenerationSettings) => void;
  connectionId?: string;
  modelId?: string;
  reasoningLevels?: string[];
  disabled?: boolean;
  /** Provider-hosted web search lives on the route, not in generation settings. */
  webSearch?: boolean;
  onWebSearch?: (enabled: boolean) => void;
};

export function GenerationControls({
  value,
  onChange,
  connectionId,
  modelId,
  reasoningLevels = [],
  disabled = false,
  webSearch,
  onWebSearch,
}: Props) {
  const api = useContext(ApiContext);
  const [result, setResult] = useState<{
    key: string;
    preview?: Preview;
    error?: string;
  }>();
  const [pending, setPending] = useState(false);
  const attempt = useRef(0);
  const key = JSON.stringify([connectionId, modelId, value]);
  const current = result?.key === key ? result : undefined;
  const update = (changes: Partial<GenerationSettings>) =>
    onChange({ ...value, ...changes });

  async function checkSettings() {
    if (!api || !connectionId || !modelId) return;
    const id = ++attempt.current;
    setPending(true);
    try {
      const preview = await api.post<Preview>(
        `/ai/connections/${encodeURIComponent(connectionId)}/generation-preview`,
        { model_id: modelId, settings: value },
      );
      if (id === attempt.current) setResult({ key, preview });
    } catch (error) {
      if (id === attempt.current) setResult({ key, error: errorText(error) });
    } finally {
      if (id === attempt.current) setPending(false);
    }
  }

  return (
    <fieldset disabled={disabled} className="space-y-3">
      <legend className="text-sm font-medium">How this colleague works</legend>
      <p className="text-sm text-muted-foreground">
        Leave a setting blank to use the AI's default. Work permissions and
        spending are reviewed for each Tender.
      </p>
      <details className="space-y-3 rounded-lg border p-3">
        <summary className="cursor-pointer text-sm font-medium">
          More options
        </summary>
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="grid gap-1 text-sm">
            Thinking
            <select
              value={value.reasoning ?? ""}
              onChange={(event) =>
                update({ reasoning: event.target.value || null })
              }
            >
              <option value="">AI default</option>
              {[
                ...new Set([
                  ...reasoningLevels,
                  ...(value.reasoning ? [value.reasoning] : []),
                ]),
              ].map((level) => (
                <option key={level} value={level}>
                  {level}
                </option>
              ))}
            </select>
          </label>
          <label className="grid gap-1 text-sm">
            Output token limit
            <input
              type="number"
              min={128}
              max={200000}
              step={1}
              value={value.max_output_tokens ?? 8192}
              onChange={(event) => {
                if (Number.isFinite(event.target.valueAsNumber))
                  update({ max_output_tokens: event.target.valueAsNumber });
              }}
            />
          </label>
          <label className="grid gap-1 text-sm">
            Temperature
            <input
              type="number"
              min={0}
              max={2}
              step={0.1}
              placeholder="AI default"
              value={value.temperature ?? ""}
              onChange={(event) =>
                update({
                  temperature:
                    event.target.value === ""
                      ? null
                      : event.target.valueAsNumber,
                })
              }
            />
          </label>
          <label className="grid gap-1 text-sm">
            Sampling probability (top_p)
            <input
              type="number"
              min={0}
              max={1}
              step={0.05}
              placeholder="AI default"
              value={value.top_p ?? ""}
              onChange={(event) =>
                update({
                  top_p:
                    event.target.value === ""
                      ? null
                      : event.target.valueAsNumber,
                })
              }
            />
          </label>
          <label className="grid gap-1 text-sm">
            Structured response method
            <select
              value={value.output_mode ?? "auto"}
              onChange={(event) =>
                update({
                  output_mode: event.target
                    .value as GenerationSettings["output_mode"],
                })
              }
            >
              <option value="auto">Current adapter default</option>
              <option value="tool">Structured tool response</option>
              <option value="native">Provider JSON Schema</option>
              <option value="prompted">JSON instructions and validation</option>
            </select>
          </label>
          <label className="grid gap-1 text-sm">
            Search calls per request
            <input
              type="number"
              min={1}
              max={20}
              step={1}
              value={value.max_search_calls ?? 3}
              onChange={(event) => {
                if (Number.isFinite(event.target.valueAsNumber))
                  update({ max_search_calls: event.target.valueAsNumber });
              }}
            />
          </label>
        </div>
        {onWebSearch ? (
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={webSearch ?? false}
              onChange={(event) => onWebSearch(event.target.checked)}
            />
            Provider web search
          </label>
        ) : null}
        {connectionId && modelId ? (
          <button
            type="button"
            disabled={!api || pending}
            onClick={checkSettings}
          >
            {pending ? "Checking settings…" : "Check settings"}
          </button>
        ) : (
          <p className="text-sm text-muted-foreground">
            Choose an account and model to check these settings before work
            starts.
          </p>
        )}
        {current?.error && <p role="alert">{current.error}</p>}
        {current?.preview && (
          <div aria-live="polite" className="space-y-2 text-sm">
            {current.preview.blockers.map((message) => (
              <p key={message} role="alert">
                {message}
              </p>
            ))}
            {current.preview.effective && (
              <p>
                Supported settings can be applied. Check the limitations below;
                Tender permission and spending checks still apply.
              </p>
            )}
            <ul className="space-y-1">
              {current.preview.capabilities.map((capability) => (
                <li key={capability.id}>
                  <strong>{capability.id.replaceAll("_", " ")}</strong>:{" "}
                  {capability.runtime_supported
                    ? capability.support
                    : "unavailable"}
                  . {capability.detail}
                </li>
              ))}
            </ul>
          </div>
        )}
      </details>
    </fieldset>
  );
}
