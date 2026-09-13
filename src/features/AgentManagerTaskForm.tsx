import { useState, type FormEvent } from "react";
import { tenderPath, useApi } from "../api";
import { ErrorNotice } from "../components/common";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select";
import { Textarea } from "@/components/ui/textarea";
import {
  stableKey,
  type TaskMode,
  type TenderSummary,
} from "./agentLibraryTypes";

export function AgentManagerTaskForm({
  mode,
  tenders,
  tenderError,
  onRetryTenders,
  onComplete,
}: {
  mode: TaskMode;
  tenders: TenderSummary[];
  tenderError: unknown;
  onRetryTenders: () => Promise<void>;
  onComplete: (message: string) => void;
}) {
  const api = useApi();
  const [tenderId, setTenderId] = useState("");
  const [brief, setBrief] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [idempotencyKey] = useState(() =>
    stableKey(
      mode.kind === "generate" ? "definition-generate" : "definition-reuse",
    ),
  );
  const label =
    mode.kind === "generate" ? "Professional brief" : "First work brief";

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!tenderId || pending) return;
    setPending(true);
    setError(null);
    const content =
      mode.kind === "generate"
        ? [
            "Create and save one reusable professional definition from this brief.",
            "Use save_agent_definition with a complete provider-independent profile and generation settings.",
            "Do not create Tender staff, assign work or add permission while preparing this definition.",
            `Professional brief: ${brief}`,
          ].join("\n")
        : [
            `Reuse professional definition ${mode.definition.id}, version ${mode.definition.version}, exactly.`,
            "Read that exact version and use create_staff_from_definition to create planned Tender staff.",
            "Do not grant execution, select an account or start the assignment.",
            `First work brief: ${brief}`,
          ].join("\n");
    try {
      await api.post(`${tenderPath(tenderId)}/messages`, {
        content,
        action: "review_documents",
        idempotency_key: idempotencyKey,
      });
      onComplete(
        mode.kind === "generate"
          ? "The Tender Manager is creating the professional definition."
          : `The Tender Manager is adding ${mode.definition.profile.display_name} to the Tender.`,
      );
    } catch (failure) {
      setError(failure);
    } finally {
      setPending(false);
    }
  }

  return (
    <form className="flex flex-col gap-4" onSubmit={submit}>
      <p className="text-sm text-muted-foreground">
        This uses the selected Tender&apos;s real Manager task and AI account.
        The saved definition remains available across Tenders.
      </p>
      <ErrorNotice error={tenderError} />
      {tenderError ? (
        <Button
          type="button"
          variant="outline"
          className="w-fit"
          onClick={() => void onRetryTenders()}
        >
          Retry loading Tenders
        </Button>
      ) : null}
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="agent-task-tender">Tender</FieldLabel>
          <NativeSelect
            id="agent-task-tender"
            required
            className="w-full"
            value={tenderId}
            onChange={(event) => setTenderId(event.target.value)}
          >
            <NativeSelectOption value="">Choose a Tender</NativeSelectOption>
            {tenders.map((tender) => (
              <NativeSelectOption key={tender.id} value={tender.id}>
                {tender.name}
              </NativeSelectOption>
            ))}
          </NativeSelect>
          {tenders.length === 0 && !tenderError ? (
            <FieldDescription>
              Create or open a Tender before starting this Manager task.
            </FieldDescription>
          ) : null}
        </Field>
        <Field>
          <FieldLabel htmlFor="agent-task-brief">{label}</FieldLabel>
          <Textarea
            id="agent-task-brief"
            required
            maxLength={4000}
            value={brief}
            onChange={(event) => setBrief(event.target.value)}
          />
          <FieldDescription>
            {mode.kind === "generate"
              ? "Describe the professional discipline, judgement, working style and expected outputs."
              : "Describe the first bounded piece of work. The Manager will preserve the selected definition version."}
          </FieldDescription>
        </Field>
      </FieldGroup>
      <ErrorNotice error={error} />
      <div className="flex justify-end">
        <Button type="submit" disabled={pending || tenders.length === 0}>
          {pending ? "Starting Manager task…" : "Ask Tender Manager"}
        </Button>
      </div>
    </form>
  );
}
