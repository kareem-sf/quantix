import type { Schema } from "../api";
import type { GenerationSettings } from "../components/GenerationControls";

export type Personality = Schema<"Personality">;
export type ProfessionalProfile = Schema<"StaffProfileDraft">;
export type AgentDefinition = Schema<"AgentDefinitionRecord">;
export type AgentDefinitionExport = Schema<"AgentDefinitionExport">;
export type TenderSummary = Pick<Schema<"Tender">, "id" | "name">;

export type FormMode =
  { kind: "create" } | { kind: "edit"; definition: AgentDefinition };

export type TaskMode =
  { kind: "generate" } | { kind: "reuse"; definition: AgentDefinition };

export const DEFAULT_GENERATION_SETTINGS: GenerationSettings = {
  temperature: null,
  top_p: null,
  reasoning: null,
  max_output_tokens: 8192,
  output_mode: "auto",
  native_tools: [],
  max_search_calls: 3,
  max_native_tool_calls: 3,
};

export function stableKey(prefix: string) {
  return `${prefix}-${
    typeof crypto !== "undefined" && "randomUUID" in crypto
      ? crypto.randomUUID()
      : Math.random().toString(36).slice(2)
  }`;
}

export function safeFilename(value: string) {
  return (
    value
      .trim()
      .replace(/[^A-Za-z0-9._-]+/gu, "-")
      .replace(/^-+|-+$/gu, "") || "professional-definition"
  );
}
