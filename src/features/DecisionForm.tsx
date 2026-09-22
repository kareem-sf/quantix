import { useState, type ReactNode } from "react";
import { ErrorNotice, Modal } from "../components/common";
import { draftStorageKey, useFormDraft, type DraftScope } from "./useFormDraft";

/** A decision that needs a short note, e.g. confirming a BOQ source row. */
type DecisionFormProps = {
  title: string;
  action: string;
  description: string;
  onClose: () => void;
  onSubmit: (rationale: string) => Promise<void>;
  children?: ReactNode;
  submitDisabled?: boolean;
  draftScope?: DraftScope;
};
export function DecisionForm(props: DecisionFormProps) {
  return props.draftScope ? (
    <SavedDecisionForm
      key={draftStorageKey(props.draftScope)}
      {...props}
      draftScope={props.draftScope}
    />
  ) : (
    <DecisionFormContent {...props} />
  );
}
function SavedDecisionForm(
  props: DecisionFormProps & { draftScope: DraftScope },
) {
  const draft = useFormDraft(props.draftScope, { rationale: "" }, [
    "rationale",
  ]);
  return <DecisionFormContent {...props} savedDraft={draft} />;
}
function DecisionFormContent({
  title,
  action,
  description,
  onClose,
  onSubmit,
  children,
  submitDisabled = false,
  savedDraft,
}: DecisionFormProps & {
  savedDraft?: ReturnType<typeof useFormDraft<{ rationale: string }>>;
}) {
  const [localRationale, setLocalRationale] = useState("");
  const rationale = savedDraft?.value.rationale ?? localRationale;
  const setRationale = (value: string) =>
    savedDraft
      ? savedDraft.setField("rationale", value)
      : setLocalRationale(value);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>(null);
  return (
    <Modal title={title} onClose={onClose}>
      <form
        onSubmit={async (event) => {
          event.preventDefault();
          if (submitDisabled || pending || !rationale.trim()) return;
          setPending(true);
          setError(null);
          const revision = savedDraft?.revision;
          try {
            await onSubmit(rationale.trim());
            savedDraft?.markAccepted(revision);
          } catch (failure) {
            setError(failure);
          } finally {
            setPending(false);
          }
        }}
      >
        <p className="muted">{description}</p>
        {children}
        <label>
          Decision note
          <textarea
            required
            rows={4}
            maxLength={4000}
            value={rationale}
            onChange={(event) => setRationale(event.target.value)}
          />
        </label>
        <ErrorNotice error={error || savedDraft?.error} />
        <div className="form-actions">
          <button type="button" className="button" onClick={onClose}>
            Cancel
          </button>
          <button
            className="button primary"
            disabled={pending || submitDisabled || !rationale.trim()}
          >
            {pending ? "Saving…" : action}
          </button>
        </div>
      </form>
    </Modal>
  );
}
