import { useState } from "react";
import {
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { ErrorNotice, Loading, Modal } from "../components/ui";
import { ProgrammeForm, emptyProgramme, programmeValue, programmeDraft } from "./ProgrammeForm";
import { ProgrammeSuggestions } from "./ProgrammeSuggestions";
import { ClientBoqForm, emptyClientBoq, clientBoqValue } from "./ClientBoqForm";
import "../styles/deliverables.css";

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
}: {
  tenderId: string;
  kind: Schema<"OutputRequest">["kind"];
  onClose: () => void;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const [taskId, setTaskId] = useState(""),
    [programme, setProgramme] = useState(emptyProgramme),
    [clientBoq, setClientBoq] = useState(emptyClientBoq),
    [clientReady, setClientReady] = useState(false),
    [rationale, setRationale] = useState(""),
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
          try {
            await api.post<Schema<"OutputRecord">>(
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
            await refresh();
            onClose();
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
            />
          ) : null}
          {kind === "programme_xlsx" ? (
            <>
            <ProgrammeSuggestions tenderId={tenderId} onSelect={proposal => {
              setProgramme(programmeDraft(proposal));
              setConfirmed(false);
            }} />
            <ProgrammeForm
              tenderId={tenderId}
              value={programme}
              onChange={setProgramme}
            />
            </>
          ) : null}
          {kind === "client_boq" ? (
            <ClientBoqForm tenderId={tenderId} value={clientBoq} onChange={value => {
              setClientBoq(value);
              setConfirmed(false);
            }} onReadyChange={setClientReady} />
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
        <ErrorNotice error={error} />
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
}: {
  tenderId: string;
  value: string;
  onChange: (value: string) => void;
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
      </label>
      {tasks.data && !completed.length ? (
        <p className="field-help">
          Complete and review specialist work before creating its technical
          document.
        </p>
      ) : null}
    </>
  );
}
