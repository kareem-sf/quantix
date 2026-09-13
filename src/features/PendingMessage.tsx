import { useEffect, useId, useRef, useState } from "react";
import { Check, Clock3, Edit3, PauseCircle, Send, X } from "lucide-react";
import { tenderPath, useApi, type Schema } from "../api";
import { ErrorNotice } from "../components/common";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel } from "@/components/ui/field";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";

export function PendingMessage({
  tenderId,
  pending,
  onChanged,
  onRepair,
}: {
  tenderId: string;
  pending: Schema<"PendingInstruction">;
  onChanged: () => void | Promise<void>;
  onRepair?: (target: string) => void;
}) {
  const api = useApi();
  const editorId = useId();
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState(pending.content);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const [error, setError] = useState<unknown>(null);
  const path = `${tenderPath(tenderId)}/pending-message`;
  const held = pending.status === "held";
  useEffect(() => {
    if (!editing) setValue(pending.content);
  }, [editing, pending.content, pending.id, pending.revision]);

  async function run(work: () => Promise<void>) {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    setError(null);
    try {
      await work();
    } catch (failure) {
      setError(failure);
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  }

  const edit = () =>
    value.trim()
      ? run(async () => {
          await api.patch<Schema<"PendingInstruction">>(path, {
            content: value.trim(),
            pending_id: pending.id,
            expected_revision: pending.revision,
            action: pending.action ?? null,
          } satisfies Schema<"PendingInstructionEdit">);
          setEditing(false);
          await onChanged();
        })
      : undefined;

  const cancel = () =>
    run(async () => {
      await api.delete<Schema<"MutationReceipt">>(path, {
        pending_id: pending.id,
        expected_revision: pending.revision,
      } satisfies Schema<"PendingInstructionCancel">);
      await onChanged();
    });

  const confirm = () =>
    run(async () => {
      await api.post<Schema<"MessageSubmission">>(`${path}/confirm`, {
        pending_id: pending.id,
        expected_revision: pending.revision,
      } satisfies Schema<"PendingInstructionConfirm">);
      await onChanged();
    });

  const repairable =
    held &&
    ["permission", "model", "budget", "source", "work"].includes(
      pending.hold_reason ?? "",
    );

  return (
    <article
      id="pending-instruction"
      className={cn(
        "flex flex-col gap-3 rounded-xl border bg-card p-4 text-sm shadow-xs",
        held && "border-amber-500/40",
      )}
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="flex items-center gap-2 text-sm font-medium">
          {held ? (
            <PauseCircle className="size-4 text-amber-600 dark:text-amber-400" />
          ) : (
            <Clock3 className="size-4 text-muted-foreground" />
          )}
          {held ? "Held for your confirmation" : "Waiting to be sent"}
        </h3>
        <Badge
          variant="outline"
          className={cn(
            "font-normal",
            held && "border-amber-500/40 text-amber-700 dark:text-amber-400",
          )}
        >
          {held
            ? holdLabel(pending.hold_reason)
            : waitLabel(pending.wait_for_run_ids?.length ?? 0)}
        </Badge>
      </div>

      {editing ? (
        <form
          className="flex flex-col gap-3"
          onSubmit={(event) => {
            event.preventDefault();
            void edit();
          }}
        >
          <Field>
            <FieldLabel htmlFor={editorId}>Instruction</FieldLabel>
            <Textarea
              id={editorId}
              aria-label="Pending instruction"
              dir="auto"
              rows={4}
              maxLength={20000}
              value={value}
              onChange={(event) => setValue(event.target.value)}
              disabled={busy}
            />
          </Field>
          <div className="flex justify-end gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={busy}
              onClick={() => {
                setValue(pending.content);
                setEditing(false);
              }}
            >
              Cancel edit
            </Button>
            <Button type="submit" size="sm" disabled={busy || !value.trim()}>
              {busy ? "Saving…" : "Save instruction"}
            </Button>
          </div>
        </form>
      ) : (
        <p
          className="rounded-lg bg-muted/60 px-3 py-2 whitespace-pre-wrap"
          dir="auto"
        >
          {pending.content}
        </p>
      )}

      {!editing ? (
        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={busy}
            onClick={() => {
              setValue(pending.content);
              setEditing(true);
            }}
          >
            <Edit3 data-icon="inline-start" />
            Edit
          </Button>
          {held ? (
            <Button
              type="button"
              size="sm"
              disabled={busy}
              onClick={() => void confirm()}
            >
              <Send data-icon="inline-start" />
              Confirm and send
            </Button>
          ) : null}
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="text-muted-foreground"
            disabled={busy}
            onClick={() => void cancel()}
          >
            <X data-icon="inline-start" />
            Cancel instruction
          </Button>
        </div>
      ) : null}

      {held ? (
        <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <Check className="size-3.5" />
          This held text has not been sent. Your explicit click above starts it.
        </p>
      ) : null}

      {repairable ? (
        <Button
          type="button"
          variant="link"
          size="sm"
          className="h-auto self-start px-0"
          onClick={() => {
            const target =
              pending.hold_reason === "source"
                ? `/tenders/${encodeURIComponent(tenderId)}/documents`
                : pending.hold_reason === "work"
                  ? `/tenders/${encodeURIComponent(tenderId)}/work?view=run`
                  : pending.hold_reason === "model"
                    ? "/settings?section=accounts"
                    : `/tenders/${encodeURIComponent(tenderId)}/work?view=ai`;
            if (onRepair) onRepair(target);
            else window.location.hash = target;
          }}
        >
          {pending.hold_reason === "source"
            ? "Review sources"
            : pending.hold_reason === "model"
              ? "Check AI model"
              : pending.hold_reason === "budget"
                ? "Review spending"
                : pending.hold_reason === "work"
                  ? "Review current work"
                  : "Review AI permissions"}
        </Button>
      ) : null}
      <ErrorNotice error={error} />
    </article>
  );
}

function waitLabel(count: number) {
  if (!count) return "Queued";
  return `Waiting for ${count} active ${count === 1 ? "run" : "runs"}`;
}

function holdLabel(reason: Schema<"PendingInstruction">["hold_reason"]) {
  switch (reason) {
    case "failed":
      return "Previous work failed";
    case "cancelled":
      return "Previous work was cancelled";
    case "interrupted":
      return "Work was interrupted";
    case "stopped":
      return "Work was stopped";
    case "restored":
      return "Workspace was restored";
    case "permission":
      return "Permission needs review";
    case "model":
      return "AI model needs review";
    case "budget":
      return "Spending needs review";
    case "source":
      return "Source changes need review";
    case "work":
      return "Work needs review";
    default:
      return "Review required";
  }
}
