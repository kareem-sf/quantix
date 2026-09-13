import { useState } from "react";
import {
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { ErrorNotice, Loading, Modal } from "../components/common";
import { FieldError } from "../components/FieldError";
import { createDraftScope, useFormDraft } from "./useFormDraft";
import {
  ProgrammeForm,
  emptyProgramme,
  programmeValue,
  programmeDraft,
} from "./ProgrammeForm";
import { ProgrammeSuggestions } from "./ProgrammeSuggestions";
import {
  ClientBoqForm,
  emptyClientBoq,
  clientBoqValue,
  type ClientBoqDraft,
} from "./ClientBoqForm";

export const documentTitles: Record<Schema<"OutputRequest">["kind"], string> = {
  boq_xlsx: "Create BOQ workbook",
  analysis_docx: "Create analysis document",
  technical_docx: "Create technical document",
  registers_xlsx: "Create tender registers",
  comparison_xlsx: "Create quotation comparison",
  programme_xlsx: "Create construction programme",
  client_boq: "Create client BOQ copy",
};
export function OutputForm({
  tenderId,
  kind,
  onClose,
  onCreated,
  onTask,
}: {
  tenderId: string;
  kind: Schema<"OutputRequest">["kind"];
  onClose: () => void;
  onCreated?: (output: Schema<"OutputRecord">) => void;
  onTask?: (id: string) => void;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const emptyClient = emptyClientBoq();
  const draft = useFormDraft(
    createDraftScope("submission", tenderId, `output-${kind}`, 1),
    {
      taskId: "",
      programme: emptyProgramme(),
      clientBoq: {
        artifactId: emptyClient.artifactId,
        currency: emptyClient.currency,
        taxBasis: emptyClient.taxBasis,
        mappings: emptyClient.mappings,
      },
      rationale: "",
    },
    ["taskId", "programme", "clientBoq", "rationale"],
  );
  const { taskId, programme, rationale } = draft.value;
  const setTaskId = (value: string) => {
    draft.setField("taskId", value);
    setConfirmed(false);
  };
  const setProgramme = (value: ReturnType<typeof emptyProgramme>) => {
    draft.setField("programme", value);
    setConfirmed(false);
  };
  const setRationale = (value: string) => draft.setField("rationale", value);
  const [mappingReviewed, setMappingReviewed] = useState(false),
    [quantityApproved, setQuantityApproved] = useState(false);
  const clientBoq: ClientBoqDraft = {
    ...draft.value.clientBoq,
    mappingReviewed,
    quantityApproved,
  };
  const setClientBoq = (value: ClientBoqDraft) => {
    setMappingReviewed(value.mappingReviewed);
    setQuantityApproved(value.quantityApproved);
    draft.setField("clientBoq", {
      artifactId: value.artifactId,
      currency: value.currency,
      taxBasis: value.taxBasis,
      mappings: value.mappings,
    });
  };
  const [clientReady, setClientReady] = useState(false),
    [confirmed, setConfirmed] = useState(false),
    [pending, setPending] = useState(false),
    [error, setError] = useState<unknown>(null);
  const programmeInput =
    kind === "programme_xlsx" ? programmeValue(programme) : null;
  const clientInput = kind === "client_boq" ? clientBoqValue(clientBoq) : null;
  const valid =
    !!rationale.trim() &&
    confirmed &&
    (kind !== "technical_docx" || !!taskId) &&
    (kind !== "programme_xlsx" || !!programmeInput) &&
    (kind !== "client_boq" || (!!clientInput && clientReady));
  return (
    <Modal title={documentTitles[kind]} onClose={onClose}>
      <form
        className="output-form"
        onSubmit={async (event) => {
          event.preventDefault();
          if (!valid || pending) return;
          setPending(true);
          setError(null);
          const acceptedRevision = draft.revision;
          try {
            const output = await api.post<Schema<"OutputRecord">>(
              `${tenderPath(tenderId)}/outputs`,
              {
                kind,
                engineer_confirmed: true,
                rationale: rationale.trim(),
                ...(kind === "technical_docx" ? { task_id: taskId } : {}),
                ...(programmeInput ? { programme: programmeInput } : {}),
                ...(clientInput ? { client_boq: clientInput } : {}),
              } satisfies Schema<"OutputRequest">,
            );
            draft.markAccepted(acceptedRevision);
            await refresh();
            if (onCreated) onCreated(output);
            else onClose();
          } catch (failure) {
            setError(failure);
          } finally {
            setPending(false);
          }
        }}
      >
        <p className="muted">
          Create a draft review copy. Open issues remain visible. Creating this
          document does not approve a final export.
        </p>
        <fieldset disabled={pending} className="plain-fieldset">
          {kind === "technical_docx" ? (
            <CompletedTasks
              tenderId={tenderId}
              value={taskId}
              onChange={setTaskId}
              onTask={onTask}
              error={error}
            />
          ) : null}
          {kind === "programme_xlsx" ? (
            <>
              <ProgrammeSuggestions
                tenderId={tenderId}
                onSelect={(proposal) => {
                  setProgramme(programmeDraft(proposal));
                  setConfirmed(false);
                }}
              />
              <ProgrammeForm
                tenderId={tenderId}
                value={programme}
                onChange={setProgramme}
                error={error}
              />
            </>
          ) : null}
          {kind === "client_boq" ? (
            <ClientBoqForm
              error={error}
              tenderId={tenderId}
              value={clientBoq}
              onChange={(value) => {
                setClientBoq(value);
                setConfirmed(false);
              }}
              onReadyChange={setClientReady}
            />
          ) : null}
          <label>
            Document review note
            <textarea
              required
              rows={3}
              maxLength={4000}
              value={rationale}
              onChange={(event) => setRationale(event.target.value)}
            />
            <FieldError error={error} name="rationale" />
          </label>
          <label className="checkbox-label">
            <input
              required
              type="checkbox"
              checked={confirmed}
              onChange={(event) => setConfirmed(event.target.checked)}
            />
            I reviewed these inputs and approve creating a draft.
          </label>
        </fieldset>
        <ErrorNotice error={error || draft.error} />
        <div className="form-actions">
          <button
            type="button"
            className="button"
            disabled={pending}
            onClick={onClose}
          >
            Cancel
          </button>
          <button className="button primary" disabled={pending || !valid}>
            {pending ? "Creating…" : "Create draft"}
          </button>
        </div>
      </form>
    </Modal>
  );
}
function CompletedTasks({
  tenderId,
  value,
  onChange,
  onTask,
  error,
}: {
  tenderId: string;
  value: string;
  onChange: (value: string) => void;
  onTask?: (id: string) => void;
  error?: unknown;
}) {
  const tasks = useResource<Schema<"Task">[]>(`${tenderPath(tenderId)}/tasks`),
    completed = tasks.data?.filter((task) => task.status === "completed") ?? [];
  return (
    <>
      <ErrorNotice error={tasks.error} />
      {tasks.isPending ? (
        <Loading>Loading completed specialist work…</Loading>
      ) : null}
      <label>
        Completed specialist task
        <select
          required
          value={value}
          disabled={tasks.isPending}
          onChange={(event) => onChange(event.target.value)}
        >
          <option value="">Choose completed work</option>
          {completed.map((task) => (
            <option key={task.id} value={task.id}>
              {task.title}
            </option>
          ))}
        </select>
        <FieldError error={error} name="task_id" />
      </label>
      {value && onTask ? (
        <button
          type="button"
          className="text-button"
          onClick={() => onTask(value)}
        >
          Open selected task
        </button>
      ) : null}
      {tasks.data && !completed.length ? (
        <p className="field-help">
          Complete and review specialist work before creating its technical
          document.
        </p>
      ) : null}
    </>
  );
}
