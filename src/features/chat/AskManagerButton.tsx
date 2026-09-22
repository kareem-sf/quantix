import { useState, type ReactNode } from "react";
import { MessageSquare } from "lucide-react";
import { tenderPath, useApi, useRefresh, type Schema } from "../../api";
import { ErrorNotice } from "../../components/common";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/**
 * Hands a job to the Tender Manager from a page, as a message in the chat, and
 * then takes the engineer to the chat to follow it.
 */
export function AskManagerButton({
  tenderId,
  request,
  children,
  onSent,
  variant = "default",
  className,
}: {
  tenderId: string;
  /** The instruction sent to the Manager, in the engineer's words. */
  request: string;
  children: ReactNode;
  onSent?: () => void;
  variant?: "default" | "outline" | "ghost";
  className?: string;
}) {
  const api = useApi();
  const refresh = useRefresh();
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<unknown>(null);

  async function ask() {
    setSending(true);
    setError(null);
    try {
      await api.post<Schema<"MessageSubmission">>(
        `${tenderPath(tenderId)}/messages`,
        {
          content: request,
          idempotency_key: `ask-${tenderId}-${Date.now()}`,
        } satisfies Schema<"MessageRequest">,
      );
      await refresh();
      onSent?.();
    } catch (failure) {
      setError(failure);
    } finally {
      setSending(false);
    }
  }

  return (
    <div className={cn("flex flex-col items-start gap-2", className)}>
      <Button
        type="button"
        size="sm"
        variant={variant}
        disabled={sending}
        onClick={() => void ask()}
      >
        <MessageSquare data-icon="inline-start" />
        {sending ? "Sending…" : children}
      </Button>
      <ErrorNotice error={error} />
    </div>
  );
}
