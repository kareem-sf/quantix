import type { Schema } from "../api";

type NativeChoice = Schema<"ReviewedNativeTools">;
type RouteOption = Schema<"DelegationRouteOption">;
export type NativeChoices = Record<string, NativeChoice>;

export function nativeSelectionErrors(
  choices: NativeChoices,
  routes: RouteOption[],
): string[] {
  const errors: string[] = [];
  for (const route of routes) {
    if (
      (route.route.native_tools ?? []).some(
        (tool) => !choices[route.id]?.native_tools?.includes(tool),
      )
    )
      errors.push(
        `Review the requested provider tools for ${route.account_name}.`,
      );
  }
  for (const choice of Object.values(choices)) {
    if (
      choice.native_tools?.includes("web_fetch") &&
      !choice.web_fetch_domains?.length
    )
      errors.push(
        "Enter the public domains that provider page reading may access.",
      );
    if (choice.native_tools?.includes("code_execution")) {
      if (!choice.code_execution_price)
        errors.push(
          "Record the provider's documented code session price in AI accounts.",
        );
      if (
        !choice.max_charge_usd ||
        (choice.code_execution_price &&
          choice.max_charge_usd <
            choice.code_execution_price.per_session_usd *
              choice.max_calls_per_request)
      )
        errors.push(
          "Set a hosted-code allowance that covers the reviewed call limit.",
        );
    }
  }
  return [...new Set(errors)];
}

function recordedPrice(
  route: RouteOption,
): NativeChoice["code_execution_price"] {
  const value = route.model.pricing;
  if (!value || typeof value !== "object") return null;
  const price = value as Record<string, unknown>;
  return typeof price.code_execution_per_session === "number" &&
    typeof price.code_execution_source === "string" &&
    typeof price.code_execution_as_of === "string"
    ? {
        per_session_usd: price.code_execution_per_session,
        source: price.code_execution_source,
        as_of: price.code_execution_as_of,
      }
    : null;
}

export function NativeToolsReview({
  routes,
  artifacts,
  allowedArtifactIds,
  value,
  disabled,
  onChange,
}: {
  routes: RouteOption[];
  artifacts: Schema<"DelegationArtifactOption">[];
  allowedArtifactIds: string[];
  value: NativeChoices;
  disabled: boolean;
  onChange: (value: NativeChoices) => void;
}) {
  const applicable = routes.filter(
    (route) =>
      route.route.native_tools?.length || value[route.id]?.native_tools?.length,
  );
  if (!applicable.length) return null;
  return (
    <section
      className="delegation-list-section"
      aria-label="Provider tools and uploads"
    >
      <h3>Provider tools and uploads</h3>
      <p className="field-help">
        Review the features requested in each AI route. Hosted code receives
        complete selected originals.
      </p>
      {applicable.map((route) => {
        const current: NativeChoice = value[route.id] ?? {
          native_tools: [],
          web_fetch_domains: [],
          uploaded_artifact_ids: [],
          max_calls_per_request: route.route.max_native_tool_calls ?? 3,
          max_charge_usd: null,
          code_execution_price: null,
        };
        const update = (change: Partial<NativeChoice>) =>
          onChange({ ...value, [route.id]: { ...current, ...change } });
        return (
          <fieldset
            key={route.id}
            disabled={disabled}
            className="space-y-3 rounded-lg border p-3"
          >
            <legend>
              {route.account_name} · {route.route.model_id}
            </legend>
            {(route.route.native_tools ?? []).map((tool) => (
              <label className="checkbox-label" key={tool}>
                <input
                  type="checkbox"
                  checked={current.native_tools?.includes(tool) ?? false}
                  onChange={(event) => {
                    const selected = new Set(current.native_tools ?? []);
                    if (event.target.checked) selected.add(tool);
                    else selected.delete(tool);
                    update({
                      native_tools: [...selected],
                      ...(tool === "code_execution"
                        ? {
                            code_execution_price: event.target.checked
                              ? recordedPrice(route)
                              : null,
                            max_charge_usd: null,
                            uploaded_artifact_ids: [],
                          }
                        : {}),
                    });
                  }}
                />
                {tool === "web_fetch"
                  ? "Allow provider page reading"
                  : tool === "code_execution"
                    ? "Allow hosted code execution"
                    : "Allow provider web search"}
              </label>
            ))}
            <label>
              Maximum provider tool calls per request
              <input
                type="number"
                min={1}
                max={20}
                value={current.max_calls_per_request}
                onChange={(event) =>
                  update({ max_calls_per_request: Number(event.target.value) })
                }
              />
            </label>
            {current.native_tools?.includes("web_fetch") ? (
              <label>
                Public domains, one per line
                <textarea
                  value={(current.web_fetch_domains ?? []).join("\n")}
                  onChange={(event) =>
                    update({
                      web_fetch_domains: event.target.value
                        .split(/[\n,]/)
                        .map((line) => line.trim())
                        .filter(Boolean),
                    })
                  }
                  placeholder="supplier.example.com"
                />
              </label>
            ) : null}
            {current.native_tools?.includes("code_execution") ? (
              <>
                {current.code_execution_price ? (
                  <p className="field-help">
                    Recorded session price: USD{" "}
                    {current.code_execution_price.per_session_usd} ·{" "}
                    {current.code_execution_price.as_of}
                  </p>
                ) : (
                  <p className="ai-warning">
                    Add the documented hosted-code price to this model in AI
                    accounts before approval.
                  </p>
                )}
                <label>
                  Hosted-code allowance for this work (USD)
                  <input
                    type="number"
                    min={0.01}
                    step="0.01"
                    value={current.max_charge_usd ?? ""}
                    onChange={(event) =>
                      update({
                        max_charge_usd: event.target.value
                          ? Number(event.target.value)
                          : null,
                      })
                    }
                  />
                </label>
                <p className="field-help">
                  This controls new requests using the recorded rate.
                  Unconfirmed charges remain reserved for review.
                </p>
                <p className="text-sm">
                  Original files allowed for upload ·{" "}
                  {current.uploaded_artifact_ids?.length ?? 0} selected
                </p>
                {!current.uploaded_artifact_ids?.length ? (
                  <p className="field-help">
                    No original files will be uploaded. Code may work with
                    supplied values.
                  </p>
                ) : null}
                {artifacts
                  .filter((artifact) =>
                    allowedArtifactIds.includes(artifact.artifact_id),
                  )
                  .map((artifact) => (
                    <label
                      className="checkbox-label"
                      key={artifact.artifact_id}
                    >
                      <input
                        type="checkbox"
                        checked={
                          current.uploaded_artifact_ids?.includes(
                            artifact.artifact_id,
                          ) ?? false
                        }
                        onChange={(event) => {
                          const selected = new Set(
                            current.uploaded_artifact_ids ?? [],
                          );
                          if (event.target.checked)
                            selected.add(artifact.artifact_id);
                          else selected.delete(artifact.artifact_id);
                          update({ uploaded_artifact_ids: [...selected] });
                        }}
                      />
                      {artifact.display_name}
                    </label>
                  ))}
                <p className="field-help">
                  Use the source picker below to find other original files.
                </p>
              </>
            ) : null}
          </fieldset>
        );
      })}
      {nativeSelectionErrors(value, routes).map((message) => (
        <p className="ai-warning" key={message}>
          {message}
        </p>
      ))}
    </section>
  );
}
