import type { ApplicationSettingsView } from "./bindings/ApplicationSettingsView";

const CHATGPT_DATA_DESTINATION = "ChatGPT subscription";

function reasoningKey(reasoning: unknown): string {
  return JSON.stringify(reasoning);
}

async function accountFingerprint(
  connectionId: string,
  provider: string,
  accountLabel: string,
): Promise<string> {
  const identity = `${connectionId}\0${provider}\0${accountLabel}`;
  const digest = await globalThis.crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(identity),
  );
  return Array.from(new Uint8Array(digest), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

export async function exactApplicationAiSelectionIsReady(
  settings: ApplicationSettingsView,
): Promise<boolean> {
  const selection = settings.ai_execution_selection;
  const approval = settings.ai_execution_approval;
  if (!selection || !approval) {
    return false;
  }
  const connection = settings.provider_connections.find(
    (candidate) =>
      candidate.connection_id === selection.connection_id &&
      candidate.provider === selection.provider &&
      candidate.status === "ready" &&
      candidate.account_label !== null,
  );
  if (
    !connection ||
    connection.catalogue_fetched_at !== selection.catalogue_fetched_at ||
    connection.adapter_version !== selection.adapter_version
  ) {
    return false;
  }
  const model = connection.models.find(
    (candidate) => candidate.model_id === selection.model_id,
  );
  const selectionReasoning = reasoningKey(selection.reasoning);
  if (
    !model?.reasoning_options.some(
      (option) => reasoningKey(option.selection) === selectionReasoning,
    ) ||
    connection.account_label === null
  ) {
    return false;
  }
  return (
    approval.connection_id === selection.connection_id &&
    approval.provider === selection.provider &&
    approval.model_id === selection.model_id &&
    reasoningKey(approval.reasoning) === selectionReasoning &&
    approval.data_destination === CHATGPT_DATA_DESTINATION &&
    approval.account_fingerprint ===
      (await accountFingerprint(
        connection.connection_id,
        connection.provider,
        connection.account_label,
      ))
  );
}
