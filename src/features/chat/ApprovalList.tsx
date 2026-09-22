import { useState } from "react";
import { Check, ChevronRight, X } from "lucide-react";
import { tenderPath, useApi, useRefresh, type Schema } from "../../api";
import { ErrorNotice } from "../../components/common";
import { Button } from "@/components/ui/button";
import { MetaParts } from "@/components/meta-line";
import { plainText } from "@/components/rich-text";
import { rtlDir } from "@/lib/text-direction";
import { cn } from "@/lib/utils";

type Approval = Schema<"MessageApproval">;
type Decision = "accept" | "reject";

export const APPROVAL_NAMES: Record<Approval["kind"], [string, string]> = {
  finding: ["finding", "findings"],
  plan: ["work plan", "work plans"],
  requirement: ["submission requirement", "submission requirements"],
  boq_row: ["BOQ row", "BOQ rows"],
  takeoff: ["drawing quantity", "drawing quantities"],
  quantity: ["quantity", "quantities"],
  rate: ["rate", "rates"],
};

const SHOWN = 3;

function heading(items: Approval[], waiting: number) {
  const kinds = [...new Set(items.map((item) => item.kind))];
  const [one, many] =
    kinds.length === 1 ? APPROVAL_NAMES[kinds[0]] : ["item", "items"];
  if (!waiting) {
    const accepted = items.filter((item) => item.state === "accepted").length;
    const rejected = items.filter((item) => item.state === "rejected").length;
    const parts = [
      accepted ? `${accepted} accepted` : "",
      rejected ? `${rejected} rejected` : "",
    ];
    return `${items.length} ${items.length === 1 ? one : many} · ${parts.filter(Boolean).join(", ") || "decided"}`;
  }
  return `${waiting} ${waiting === 1 ? one : many} ${waiting === 1 ? "needs" : "need"} your OK`;
}

/** Send one chat decision and return the record's new state. */
export async function decideApproval(
  api: ReturnType<typeof useApi>,
  tenderId: string,
  item: Pick<Approval, "kind" | "id">,
  decision: Decision,
) {
  return api.post<Approval>(`${tenderPath(tenderId)}/approvals`, {
    kind: item.kind,
    id: item.id,
    decision,
    note: "",
  } satisfies Schema<"ApprovalDecision">);
}

/**
 * What the Tender Manager's job produced that needs the engineer's OK, right
 * under its reply. Each item opens in full in the side panel; small buttons
 * accept or reject it here, and Accept all decides the rest at once.
 */
export function ApprovalList({
  tenderId,
  approvals,
  onOpen,
  onAskChanges,
}: {
  tenderId: string;
  approvals: Approval[];
  onOpen?: (item: Approval) => void;
  onAskChanges?: (items: Approval[]) => void;
}) {
  const api = useApi();
  const refresh = useRefresh();
  const [decided, setDecided] = useState<Record<string, Approval["state"]>>({});
  const [busy, setBusy] = useState(false);
  const [saving, setSaving] = useState<{ done: number; total: number } | null>(
    null,
  );
  const [all, setAll] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const items = approvals.map((item) => ({
    ...item,
    state: decided[`${item.kind}:${item.id}`] ?? item.state,
  }));
  const waiting = items.filter((item) => item.state === "waiting");
  if (!items.length) return null;
  // What still needs a decision comes first, so a short list never hides it.
  const ordered = [
    ...waiting,
    ...items.filter((item) => item.state !== "waiting"),
  ];
  const shown = all ? items : ordered.slice(0, SHOWN);

  async function decide(targets: Approval[], decision: Decision) {
    setBusy(true);
    setError(null);
    // Show the decision at once; saving a record can take a few seconds.
    const shownState = decision === "accept" ? "accepted" : "rejected";
    const keys = targets.map((item) => `${item.kind}:${item.id}`);
    setDecided((current) => ({
      ...current,
      ...Object.fromEntries(keys.map((key) => [key, shownState])),
    }));
    const done = new Set<string>();
    if (targets.length > 1) setSaving({ done: 0, total: targets.length });
    try {
      for (const item of targets) {
        const key = `${item.kind}:${item.id}`;
        const saved = await decideApproval(api, tenderId, item, decision);
        done.add(key);
        setDecided((current) => ({ ...current, [key]: saved.state }));
        if (targets.length > 1)
          setSaving({ done: done.size, total: targets.length });
      }
    } catch (failure) {
      // Put back what was not saved.
      setDecided((current) =>
        Object.fromEntries(
          Object.entries(current).filter(
            ([key]) => done.has(key) || !keys.includes(key),
          ),
        ),
      );
      setError(failure);
    } finally {
      setBusy(false);
      setSaving(null);
      await refresh();
    }
  }

  return (
    <section
      aria-label="Needs your OK"
      className="bui-fade-up w-full max-w-xl overflow-hidden rounded-xl bg-card ring-1 ring-border"
    >
      <h3 className="px-4 pt-3 pb-2 text-sm font-medium">
        {heading(items, waiting.length)}
        {saving ? (
          <span
            role="status"
            className="ms-2 text-xs font-normal text-muted-foreground"
          >
            Saving {saving.done} of {saving.total}…
          </span>
        ) : null}
      </h3>
      <ul className="flex flex-col">
        {shown.map((item) => (
          <li
            key={`${item.kind}:${item.id}`}
            className="flex items-stretch border-t"
          >
            <button
              type="button"
              disabled={!onOpen}
              onClick={() => onOpen?.(item)}
              aria-label={`Open: ${plainText(item.title)}`}
              className="group/item flex min-w-0 flex-1 items-start gap-2 px-4 py-2.5 text-start transition-colors enabled:hover:bg-muted/50"
            >
              <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span
                  dir={rtlDir(item.title)}
                  className="line-clamp-2 text-sm wrap-anywhere"
                >
                  {plainText(item.title)}
                </span>
                {item.detail ? (
                  <span className="line-clamp-2 text-xs leading-relaxed text-muted-foreground wrap-anywhere">
                    <MetaParts parts={plainText(item.detail).split(" · ")} />
                  </span>
                ) : null}
              </span>
              {onOpen ? (
                <ChevronRight
                  aria-hidden
                  className="mt-0.5 size-4 shrink-0 text-muted-foreground/60 group-hover/item:text-foreground rtl:-scale-x-100"
                />
              ) : null}
            </button>
            <div className="flex shrink-0 items-start gap-0.5 py-2 pe-2.5">
              {item.state === "waiting" ? (
                <>
                  <Button
                    type="button"
                    size="icon-sm"
                    variant="ghost"
                    title="Accept"
                    disabled={busy}
                    aria-label={`Accept: ${item.title}`}
                    className="text-(--success) hover:bg-(--success-tint) hover:text-(--success)"
                    onClick={() => void decide([item], "accept")}
                  >
                    <Check />
                  </Button>
                  {item.can_reject ? (
                    <Button
                      type="button"
                      size="icon-sm"
                      variant="ghost"
                      title="Reject"
                      disabled={busy}
                      aria-label={`Reject: ${item.title}`}
                      className="text-muted-foreground hover:text-destructive"
                      onClick={() => void decide([item], "reject")}
                    >
                      <X />
                    </Button>
                  ) : null}
                </>
              ) : (
                <span
                  className={cn(
                    "px-1 pt-1 text-xs",
                    item.state === "accepted"
                      ? "text-(--success)"
                      : "text-muted-foreground",
                  )}
                >
                  {item.state === "accepted"
                    ? "Accepted"
                    : item.state === "rejected"
                      ? "Rejected"
                      : "Out of date"}
                </span>
              )}
            </div>
          </li>
        ))}
      </ul>
      <ErrorNotice error={error} />
      {waiting.length || items.length > SHOWN ? (
        <div className="flex flex-wrap items-center gap-2 border-t bg-muted/30 px-3 py-2">
          {waiting.length > 1 ? (
            <Button
              type="button"
              size="sm"
              disabled={busy}
              onClick={() => void decide(waiting, "accept")}
            >
              <Check data-icon="inline-start" />
              {busy ? "Saving…" : `Accept all ${waiting.length}`}
            </Button>
          ) : null}
          {items.length > SHOWN ? (
            <Button
              type="button"
              size="sm"
              variant="ghost"
              onClick={() => setAll(!all)}
            >
              {all ? "Show fewer" : `Show all ${items.length}`}
            </Button>
          ) : null}
          {waiting.length && onAskChanges ? (
            <Button
              type="button"
              size="sm"
              variant="ghost"
              onClick={() => onAskChanges(waiting)}
            >
              Ask for changes…
            </Button>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
