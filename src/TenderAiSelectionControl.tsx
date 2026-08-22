import type { AiExecutionSelection } from "./bindings/AiExecutionSelection";
import { QuantixSelect, type QuantixSelectOption } from "./ui";

interface TenderAiSelectionControlProps {
  selection: AiExecutionSelection | null;
  providerOptions: readonly QuantixSelectOption[];
  modelOptions: readonly QuantixSelectOption[];
  reasoningOptions: readonly QuantixSelectOption[];
  modelDisabled: boolean;
  reasoningDisabled: boolean;
  busy: boolean;
  onProviderChange: (connectionId: string) => void;
  onModelChange: (modelId: string) => void;
  onReasoningChange: (reasoning: string) => void;
}

export function TenderAiSelectionControl({
  selection,
  providerOptions,
  modelOptions,
  reasoningOptions,
  modelDisabled,
  reasoningDisabled,
  busy,
  onProviderChange,
  onModelChange,
  onReasoningChange,
}: TenderAiSelectionControlProps) {
  return (
    <div
      className="manager-composer__ai-controls"
      role="group"
      aria-label="Tender AI selection"
    >
      <QuantixSelect
        aria-label="Tender AI provider"
        label="Provider"
        value={selection?.connection_id ?? "local_only"}
        options={providerOptions}
        disabled={busy}
        onChange={onProviderChange}
      />
      <QuantixSelect
        aria-label="Tender AI model"
        label="Model"
        value={selection?.model_id ?? ""}
        options={modelOptions}
        disabled={modelDisabled}
        onChange={onModelChange}
      />
      <QuantixSelect
        aria-label="Tender AI reasoning"
        label="Reasoning"
        value={selection ? JSON.stringify(selection.reasoning) : ""}
        options={reasoningOptions}
        disabled={reasoningDisabled}
        onChange={onReasoningChange}
      />
    </div>
  );
}
