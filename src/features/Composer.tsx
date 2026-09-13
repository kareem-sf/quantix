import { useEffect, useRef, useState, type ReactNode } from "react";
import { ArrowUp, Mic, Paperclip } from "lucide-react";
import type { Schema } from "../api";
import { ErrorNotice } from "../components/common";
import { useResolvedTheme } from "../theme";
import { BorderBeam } from "@/components/ui/border-beam";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupTextarea,
} from "@/components/ui/input-group";
import {
  Popover,
  PopoverContent,
  PopoverDescription,
  PopoverHeader,
  PopoverTitle,
  PopoverTrigger,
} from "@/components/ui/popover";
import { useLoop } from "@/hooks/use-loop";
import { cn } from "@/lib/utils";
import { composerChip } from "./ModelPicker";
import { VoiceInput } from "./VoiceInput";
import { readDraft, saveDraft } from "./drafts";
import {
  createDraftScope,
  readFormDraft,
  removeFormDraft,
  writeFormDraft,
} from "./useFormDraft";

type Attempt = { text: string; idempotencyKey: string };
const attemptFields = ["text", "idempotencyKey"] as const;
const textFields = ["text"] as const;

const placeholders = [
  "Ask the Tender Manager…",
  "Summarise the scope and the key risks…",
  "Find BOQ items the drawings do not cover…",
  "Draft clarification questions for the client…",
  "List what the submission must include…",
];

function newIdempotencyKey() {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `message-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function Composer(props: ComposerProps) {
  return <ComposerDraft key={props.tenderId ?? "unsaved"} {...props} />;
}

type ComposerProps = {
  onSend: (
    content: string,
    idempotencyKey: string,
  ) => Promise<Schema<"MessageSubmission"> | void>;
  onImport: () => void;
  busy?: boolean;
  busyMessage?: string;
  hasPending?: boolean;
  initialDraft?: string;
  tenderId?: string;
  /** The AI chips shown in the prompt box: account and model, then thinking. */
  modelPicker?: ReactNode;
  /** No AI is approved for this Tender yet; sending opens the picker instead. */
  blocked?: boolean;
  onBlocked?: () => void;
};

function ComposerDraft({
  onSend,
  onImport,
  busy = false,
  busyMessage,
  hasPending = false,
  initialDraft = "",
  tenderId,
  modelPicker,
  blocked = false,
  onBlocked,
}: ComposerProps) {
  const [stored] = useState(() => readDraft(tenderId, initialDraft));
  const [draft, setDraft] = useState(stored.text);
  const draftRef = useRef(stored.text);
  const busyRef = useRef(busy);
  const attemptScope = createDraftScope(
    "manager",
    tenderId ?? "unsaved",
    "send-attempt",
    1,
  );
  const textScope = createDraftScope(
    "manager",
    tenderId ?? "unsaved",
    "instruction",
    1,
  );
  const [storedAttempt] = useState(() =>
    tenderId ? readFormDraft<Attempt>(attemptScope, attemptFields).value : null,
  );
  const attemptRef = useRef<Attempt | null>(
    storedAttempt?.text && storedAttempt.idempotencyKey
      ? {
          text: storedAttempt.text,
          idempotencyKey: storedAttempt.idempotencyKey,
        }
      : null,
  );
  const sendingRef = useRef(false);
  busyRef.current = busy;
  const [storageError, setStorageError] = useState<Error | null>(stored.error);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [sent, setSent] = useState<"immediate" | "pending" | null>(null);
  const [focused, setFocused] = useState(false);
  const theme = useResolvedTheme();
  const { key: placeholderKey } = useLoop(3800, !draft && !focused && !busy);

  useEffect(() => {
    setError((current: unknown) =>
      isObsoleteBusyConflict(current) ? null : current,
    );
  }, [busy, busyMessage]);

  async function send() {
    const rawSnapshot = draftRef.current;
    const submitted = rawSnapshot.trim();
    if (!submitted || sending || sendingRef.current || hasPending) return;
    if (blocked) {
      onBlocked?.();
      return;
    }
    const previous = attemptRef.current;
    const idempotencyKey =
      previous?.text === submitted
        ? previous.idempotencyKey
        : newIdempotencyKey();
    attemptRef.current = { text: submitted, idempotencyKey };
    // Persist the retry identity before the request, so a lost response and a
    // route change cannot turn a retry into a second instruction.
    const storedRetry = tenderId
      ? writeFormDraft(attemptScope, attemptRef.current, attemptFields)
      : null;
    const storedText = tenderId
      ? writeFormDraft(textScope, { text: rawSnapshot }, textFields)
      : null;
    setStorageError(storedRetry?.error ?? storedText?.error ?? null);
    sendingRef.current = true;
    setSending(true);
    setError(null);
    setSent(null);
    try {
      const result = await onSend(submitted, idempotencyKey);
      setSent(result?.outcome === "pending" ? "pending" : "immediate");
      // Keep later typing intact while the accepted snapshot is in flight.
      if (draftRef.current === rawSnapshot) {
        setDraft("");
        draftRef.current = "";
        if (tenderId && storedText?.revision) {
          const removed = removeFormDraft(textScope, storedText.revision);
          setStorageError(removed.error);
          // Remove only the old migration entry, never another mounted
          // editor's newer scoped draft.
          try {
            window.localStorage.removeItem(
              `quantix.manager-draft.v1:${tenderId}`,
            );
          } catch {
            /* The scoped storage error is shown above. */
          }
        }
      }
      // A server acknowledgement ends this retry window. Newer text gets a
      // fresh identity even if it remains in the editor.
      attemptRef.current = null;
      if (tenderId && storedRetry?.revision)
        removeFormDraft(attemptScope, storedRetry.revision);
    } catch (failure) {
      if (!(busyRef.current && isObsoleteBusyConflict(failure))) {
        setError(failure);
      }
    } finally {
      setSending(false);
      sendingRef.current = false;
    }
  }

  const disabled = !draft.trim() || sending || hasPending;
  const working = busy || sending;
  const statusText = hasPending
    ? "One instruction is waiting above. Edit or cancel it before adding another."
    : busy
      ? // Work in progress is already obvious from the conversation and from the
        // send button. Repeating it under the prompt box said nothing new.
        (busyMessage ?? null)
      : sent === "pending"
        ? "Instruction saved to wait. It has not been sent to the manager yet."
        : sent === "immediate"
          ? "Instruction accepted. The manager will report the next step here."
          : null;

  return (
    <div className="mx-auto flex w-full max-w-3xl shrink-0 flex-col gap-2 px-4 pb-4">
      <ErrorNotice error={error} />
      <ErrorNotice error={storageError} />
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void send();
        }}
        onFocus={() => setFocused(true)}
        onBlur={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node | null))
            setFocused(false);
        }}
      >
        <BorderBeam
          size="md"
          theme={theme}
          colorVariant={working ? "colorful" : "mono"}
          active={working || focused}
          strength={working ? 0.9 : 0.35}
          borderRadius={20}
          className="block w-full"
        >
          <InputGroup className="h-auto rounded-[20px] border-border/80 bg-card shadow-[0_1px_2px_rgb(0_0_0/0.04),inset_0_0_48px_0_color-mix(in_oklch,var(--foreground)_2%,transparent)] dark:bg-card">
            <InputGroupTextarea
              aria-label="Message to Tender Manager"
              placeholder={placeholders[placeholderKey % placeholders.length]}
              value={draft}
              dir="auto"
              rows={2}
              maxLength={20000}
              className="max-h-60 min-h-16 px-4 pt-3.5 text-sm leading-relaxed"
              onChange={(event) => {
                const value = event.target.value;
                setDraft(value);
                draftRef.current = value;
                setStorageError(saveDraft(tenderId, value));
                setSent(null);
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
            <InputGroupAddon
              align="block-end"
              className="gap-1.5 px-2.5 pb-2.5"
            >
              <InputGroupButton
                size="icon-xs"
                className={cn(composerChip, "size-7")}
                aria-label="Import tender package"
                title="Import tender package"
                onClick={onImport}
              >
                <Paperclip />
              </InputGroupButton>
              <Popover>
                <PopoverTrigger
                  render={
                    <InputGroupButton
                      size="icon-xs"
                      className={cn(composerChip, "size-7")}
                      aria-label="Voice input"
                      title="Voice input"
                    />
                  }
                >
                  <Mic />
                </PopoverTrigger>
                <PopoverContent side="top" align="start" className="w-80">
                  <PopoverHeader>
                    <PopoverTitle>Voice input</PopoverTitle>
                    <PopoverDescription>
                      Speak an instruction into the unsent draft.
                    </PopoverDescription>
                  </PopoverHeader>
                  <VoiceInput
                    tenderId={tenderId}
                    onTranscript={(text) => {
                      if (!text) return;
                      setDraft((current) => (current ? current : text));
                    }}
                  />
                </PopoverContent>
              </Popover>
              {modelPicker}
              <InputGroupButton
                type="submit"
                size="icon-xs"
                variant="default"
                className="ms-auto size-7 rounded-full shadow-sm transition-transform active:scale-95"
                aria-label={busy ? "Send when ready" : "Send instruction"}
                disabled={disabled}
                title={
                  hasPending
                    ? "Edit or cancel the waiting instruction first"
                    : undefined
                }
              >
                <ArrowUp />
              </InputGroupButton>
            </InputGroupAddon>
          </InputGroup>
        </BorderBeam>
      </form>
      {/* Only say something when there is something to say. A standing tip under
          every composer is a permanent line of chrome that tells the engineer
          nothing they do not already know. */}
      {statusText ? (
        <p role="status" className="px-2 text-xs text-muted-foreground">
          {statusText}
        </p>
      ) : blocked ? (
        <p className="px-2 text-xs text-muted-foreground">
          Choose an AI for this Tender to start. Your draft stays here.
        </p>
      ) : null}
    </div>
  );
}

function isObsoleteBusyConflict(failure: unknown) {
  if (!failure || typeof failure !== "object") return false;
  const message = "message" in failure ? failure.message : undefined;
  const status = "status" in failure ? failure.status : undefined;
  return (
    (status === 409 || status === undefined) &&
    typeof message === "string" &&
    /(?:stop(?: the)? current work|already has work in progress|work in progress)/i.test(
      message,
    )
  );
}
