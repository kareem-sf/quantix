import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, FileText, History, LoaderCircle } from "lucide-react";
import { errorText, tenderPath, useApi, type Schema } from "../../api";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { SourceSelection } from "../Sources";

type OfficeMessage = Schema<"OfficeMessage">;
type OfficeMessagePage = Schema<"OfficeMessagePage">;
type OfficeArtifactReference = Schema<"OfficeArtifactReference">;

export type OfficeMessagesProps = {
  tenderId: string;
  page: OfficeMessagePage;
  onSource: (source: SourceSelection) => void;
  onOpenResult?: (resultId: string) => void;
  onOpenOutput?: (outputId: string) => void;
  staffId?: string;
  assignmentId?: string;
  compact?: boolean;
};

const HIGHLIGHT_KINDS = new Set<OfficeMessage["kind"]>([
  "instruction",
  "question",
  "reply",
  "finding",
  "handoff",
]);

/**
 * Displays the retained office exchange. Filtering changes presentation only;
 * the service remains the source of truth for the paged conversation.
 */
export function OfficeMessages({
  tenderId,
  page,
  onSource,
  onOpenResult,
  onOpenOutput,
  staffId,
  assignmentId,
  compact = false,
}: OfficeMessagesProps) {
  const api = useApi();
  const [messages, setMessages] = useState<OfficeMessage[]>(page.items ?? []);
  const [nextCursor, setNextCursor] = useState<string | null>(
    page.next_cursor ?? null,
  );
  const [view, setView] = useState<"highlights" | "all">("highlights");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [acknowledged, setAcknowledged] = useState<Record<string, boolean>>({});
  const scopeKey = `${tenderId}\u0000${staffId ?? ""}\u0000${assignmentId ?? ""}`;
  const scopeKeyRef = useRef(scopeKey);
  const generationRef = useRef(0);
  const requestRef = useRef<AbortController | null>(null);

  useEffect(() => {
    if (scopeKeyRef.current !== scopeKey) {
      generationRef.current += 1;
      requestRef.current?.abort();
      requestRef.current = null;
      scopeKeyRef.current = scopeKey;
      setMessages(page.items ?? []);
      setNextCursor(page.next_cursor ?? null);
      setLoading(false);
    } else {
      // Snapshot refreshes add current messages to the retained older history;
      // they must not collapse the user's expanded All exchanges view.
      setMessages((current) => mergeMessages(current, page.items ?? []));
      setNextCursor((current) => current ?? page.next_cursor ?? null);
    }
    setError(null);
  }, [page, scopeKey]);

  useEffect(
    () => () => {
      generationRef.current += 1;
      requestRef.current?.abort();
      requestRef.current = null;
    },
    [],
  );

  const loadEarlier = useCallback(async () => {
    if (!nextCursor || loading) return;
    setLoading(true);
    setError(null);
    const generation = generationRef.current;
    const controller = new AbortController();
    requestRef.current?.abort();
    requestRef.current = controller;
    const query = new URLSearchParams({
      limit: "50",
      cursor: nextCursor,
    });
    if (staffId) query.set("staff_id", staffId);
    if (assignmentId) query.set("assignment_id", assignmentId);
    try {
      const next = await api.get<OfficeMessagePage>(
        `${tenderPath(tenderId)}/office/messages?${query.toString()}`,
        controller.signal,
      );
      if (controller.signal.aborted || generation !== generationRef.current)
        return;
      setMessages((current) => mergeMessages(next.items ?? [], current));
      setNextCursor(next.next_cursor ?? null);
    } catch (failure) {
      if (controller.signal.aborted || generation !== generationRef.current)
        return;
      setError(failure);
    } finally {
      if (requestRef.current === controller) {
        requestRef.current = null;
        setLoading(false);
      }
    }
  }, [api, assignmentId, loading, nextCursor, staffId, tenderId]);

  const markReceived = useCallback(
    async (messageId: string) => {
      setError(null);
      try {
        await api.post(
          `${tenderPath(tenderId)}/office/messages/${encodeURIComponent(messageId)}/acknowledge`,
          {
            message_id: messageId,
            recipient_assignment_id: assignmentId ?? null,
            extent: "received",
            idempotency_key: `ack-${messageId}`,
          },
        );
        setAcknowledged((current) => ({ ...current, [messageId]: true }));
      } catch (failure) {
        setError(failure);
      }
    },
    [api, assignmentId, tenderId],
  );

  const highlights = useMemo(
    () => messages.filter((message) => HIGHLIGHT_KINDS.has(message.kind)),
    [messages],
  );
  const notes = useMemo(
    () => messages.filter((message) => message.kind === "note"),
    [messages],
  );
  const visibleMessages = view === "all" ? messages : highlights;

  return (
    <section
      className={cn(
        "flex min-w-0 flex-col gap-3",
        compact ? "" : "rounded-xl border bg-card p-4",
      )}
      aria-label="Office conversation"
    >
      <div className="flex min-w-0 flex-col gap-2">
        <div className="flex min-w-0 flex-col gap-0.5">
          <h2 className="text-sm font-semibold tracking-tight">
            {compact ? "Recent exchanges" : "Office conversation"}
          </h2>
          <p className="text-xs text-muted-foreground">
            {view === "all"
              ? "All retained exchanges from this Tender."
              : "Important instructions, questions, findings and handoffs."}
          </p>
        </div>
        <div
          className="flex w-fit rounded-lg bg-muted p-0.5"
          role="group"
          aria-label="Conversation view"
        >
          {(
            [
              ["highlights", "Highlights"],
              ["all", "All exchanges"],
            ] as const
          ).map(([value, label]) => (
            <button
              key={value}
              type="button"
              className={cn(
                "rounded-md px-2.5 py-1 text-xs transition-colors",
                view === value
                  ? "bg-background text-foreground shadow-xs"
                  : "text-muted-foreground hover:text-foreground",
              )}
              aria-pressed={view === value}
              onClick={() => setView(value)}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      {view === "highlights" && notes.length ? (
        <details className="rounded-lg border bg-muted/40">
          <summary className="flex cursor-pointer items-center justify-between gap-2 px-3 py-2 text-xs text-muted-foreground">
            <span className="flex items-center gap-1.5">
              <History className="size-3.5" aria-hidden="true" />
              {notes.length} routine {notes.length === 1 ? "note" : "notes"}
            </span>
            <ChevronDown className="size-3.5" aria-hidden="true" />
          </summary>
          <div className="flex flex-col gap-2 p-3 pt-0">
            {notes.map((message) => (
              <OfficeMessageRow
                key={message.id}
                message={message}
                onSource={onSource}
                onOpenResult={onOpenResult}
                onOpenOutput={onOpenOutput}
                received={acknowledged[message.id]}
                onReceived={() => void markReceived(message.id)}
              />
            ))}
          </div>
        </details>
      ) : null}

      <div className="flex min-w-0 flex-col gap-2">
        {visibleMessages.length ? (
          visibleMessages.map((message) => (
            <OfficeMessageRow
              key={message.id}
              message={message}
              onSource={onSource}
              onOpenResult={onOpenResult}
              onOpenOutput={onOpenOutput}
              received={acknowledged[message.id]}
              onReceived={() => void markReceived(message.id)}
            />
          ))
        ) : (
          <p className="py-2 text-sm text-muted-foreground">
            {view === "all"
              ? "No retained exchanges are available for this view."
              : "No important exchanges have been recorded yet."}
          </p>
        )}
      </div>

      {error ? (
        <div
          className="flex flex-wrap items-center gap-2 text-sm text-destructive"
          role="alert"
        >
          <span>{errorText(error)}</span>
          <Button
            type="button"
            variant="link"
            size="sm"
            className="h-auto p-0"
            onClick={() => void loadEarlier()}
          >
            Try again
          </Button>
        </div>
      ) : null}
      {nextCursor ? (
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="w-fit"
          disabled={loading}
          onClick={() => void loadEarlier()}
        >
          {loading ? (
            <LoaderCircle data-icon="inline-start" className="animate-spin" />
          ) : (
            <History data-icon="inline-start" />
          )}
          {loading ? "Loading earlier exchanges…" : "Load earlier exchanges"}
        </Button>
      ) : null}
    </section>
  );
}

function OfficeMessageRow({
  message,
  onSource,
  onOpenResult,
  onOpenOutput,
  received,
  onReceived,
}: {
  message: OfficeMessage;
  onSource: (source: SourceSelection) => void;
  onOpenResult?: (resultId: string) => void;
  onOpenOutput?: (outputId: string) => void;
  received?: boolean;
  onReceived?: () => void;
}) {
  const recipients = message.recipients.length
    ? message.recipients.map((recipient) => recipient.display_name).join(", ")
    : "Office";
  const isNote = message.kind === "note";
  return (
    <article
      className={cn(
        "flex min-w-0 flex-col gap-1.5 rounded-lg border p-3",
        isNote ? "bg-muted/30" : "bg-card",
        message.kind === "question" && "border-amber-500/40",
        message.kind === "finding" && "border-primary/40",
      )}
    >
      <div className="flex min-w-0 flex-wrap items-baseline justify-between gap-x-2">
        <div className="flex min-w-0 items-baseline gap-2">
          <strong className="truncate text-sm font-medium">
            {message.sender.display_name}
          </strong>
          <span className="shrink-0 text-xs text-muted-foreground">
            {message.sender.kind === "manager"
              ? "Tender Manager · AI"
              : "AI staff"}
          </span>
        </div>
        <time
          className="shrink-0 text-xs text-muted-foreground"
          dateTime={message.created_at}
        >
          {formatTimestamp(message.created_at)}
        </time>
      </div>
      <p className="text-xs text-muted-foreground">
        {message.sender.title} · v{message.sender.version} · To {recipients}
        {message.assignment_id ? ` · Assignment ${message.assignment_id}` : ""}
      </p>
      <p className="text-sm leading-relaxed" dir="auto">
        {message.text}
      </p>
      {message.required_response ? (
        <p className="text-sm text-amber-700 dark:text-amber-400">
          Needs a reply: {message.required_response}
        </p>
      ) : null}
      {message.reply_to ? (
        <p className="text-xs text-muted-foreground">
          Reply to retained exchange {message.reply_to}
        </p>
      ) : null}
      {message.artifact_refs?.length ? (
        <div
          className="flex flex-wrap gap-1.5"
          aria-label="Referenced records"
        >
          {message.artifact_refs.map((reference) => (
            <ArtifactReference
              key={`${reference.kind}:${reference.id}`}
              reference={reference}
              onSource={onSource}
              onOpenResult={onOpenResult}
              onOpenOutput={onOpenOutput}
            />
          ))}
        </div>
      ) : null}
      {onReceived ? (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="w-fit"
          disabled={received || message.delivery_state === "acknowledged"}
          onClick={onReceived}
        >
          {received || message.delivery_state === "acknowledged"
            ? "Received"
            : "Mark received"}
        </Button>
      ) : null}
    </article>
  );
}

function ArtifactReference({
  reference,
  onSource,
  onOpenResult,
  onOpenOutput,
}: {
  reference: OfficeArtifactReference;
  onSource: (source: SourceSelection) => void;
  onOpenResult?: (resultId: string) => void;
  onOpenOutput?: (outputId: string) => void;
}) {
  const chip =
    "inline-flex max-w-full items-center gap-1.5 rounded-full border bg-background px-2.5 py-1 text-xs";

  if (reference.kind === "source") {
    return (
      <button
        type="button"
        className={cn(chip, "transition-colors hover:bg-accent")}
        onClick={() =>
          onSource({
            sourceId: reference.id,
            artifactId: reference.artifact_id ?? undefined,
            version: reference.artifact_version ?? undefined,
            contentHash: reference.content_hash ?? undefined,
            page: pageNumber(reference.locator),
          })
        }
      >
        <FileText className="size-3.5 shrink-0" aria-hidden="true" />
        <span className="truncate" dir="auto">
          {reference.filename ?? "Open cited source"}
          {reference.locator ? ` · ${reference.locator}` : ""}
        </span>
      </button>
    );
  }

  if (reference.kind === "staff_result") {
    return onOpenResult ? (
      <button
        type="button"
        className={cn(chip, "transition-colors hover:bg-accent")}
        onClick={() => onOpenResult(reference.id)}
      >
        <FileText className="size-3.5 shrink-0" aria-hidden="true" />
        <span className="truncate">Open staff draft {reference.id}</span>
      </button>
    ) : (
      <span className={cn(chip, "text-muted-foreground")}>
        Staff draft {reference.id}
      </span>
    );
  }

  return onOpenOutput ? (
    <button
      type="button"
      className={cn(chip, "transition-colors hover:bg-accent")}
      onClick={() => onOpenOutput(reference.id)}
    >
      <FileText className="size-3.5 shrink-0" aria-hidden="true" />
      <span className="truncate" dir="auto">
        Open output {reference.filename ?? reference.id}
      </span>
    </button>
  ) : (
    <span className={cn(chip, "text-muted-foreground")} dir="auto">
      Output {reference.filename ?? reference.id}
    </span>
  );
}

function mergeMessages(older: OfficeMessage[], current: OfficeMessage[]) {
  const byId = new Map<string, OfficeMessage>();
  for (const message of [...older, ...current]) byId.set(message.id, message);
  return [...byId.values()].sort((left, right) =>
    left.created_at.localeCompare(right.created_at),
  );
}

function formatTimestamp(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

function pageNumber(locator: string | null | undefined) {
  if (!locator) return undefined;
  const match = /(?:page|p\.)\s*(\d+)/iu.exec(locator);
  return match ? Number(match[1]) : undefined;
}
