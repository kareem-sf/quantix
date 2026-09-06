import { useState } from "react";
import { Paperclip, Send } from "lucide-react";
import { ErrorNotice } from "../components/ui";
import { readDraft, saveDraft } from "./drafts";

export function Composer({
  onSend,
  onImport,
  busy = false,
  initialDraft = "",
  tenderId,
}: {
  onSend: (content: string) => Promise<void>;
  onImport: () => void;
  busy?: boolean;
  initialDraft?: string;
  tenderId?: string;
}) {
  const [stored] = useState(() => readDraft(tenderId, initialDraft));
  const [draft, setDraft] = useState(stored.text);
  const [storageError, setStorageError] = useState<Error | null>(stored.error);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [sent, setSent] = useState(false);
  async function send() {
    if (!draft.trim() || pending || busy) return;
    setPending(true);
    setError(null);
    setSent(false);
    try {
      await onSend(draft.trim());
      setSent(true);
      setStorageError(saveDraft(tenderId, ""));
    } catch (failure) {
      setError(failure);
    } finally {
      setPending(false);
    }
  }
  return (
    <div className="composer-area">
      <ErrorNotice error={error} />
      <ErrorNotice error={storageError} />
      {sent ? (
        <p role="status" className="composer-help">
          Instruction submitted. This copy stays here while the manager works.
        </p>
      ) : (
        <p className="composer-help">
          You stay in control of scope and approvals.
        </p>
      )}
      <form
        className="composer"
        onSubmit={(event) => {
          event.preventDefault();
          void send();
        }}
      >
        <textarea
          aria-label="Message to Tender Manager"
          placeholder="Ask the manager or add an instruction…"
          value={draft}
          maxLength={20000}
          onChange={(event) => {
            setDraft(event.target.value);
            setStorageError(saveDraft(tenderId, event.target.value));
            setSent(false);
          }}
          onKeyDown={(event) => {
            if (
              event.key === "Enter" &&
              !event.shiftKey &&
              !event.nativeEvent.isComposing
            ) {
              event.preventDefault();
              void send();
            }
          }}
        />
        <div className="composer-controls">
          <button
            type="button"
            className="icon-button"
            aria-label="Import tender package"
            onClick={onImport}
          >
            <Paperclip size={23} />
          </button>
          <button
            type="submit"
            className="send-button"
            aria-label="Send instruction"
            disabled={!draft.trim() || pending || busy}
          >
            <Send size={22} />
          </button>
        </div>
      </form>
    </div>
  );
}
