import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Check, MessageSquare, X } from "lucide-react";
import { tenderPath, useApi, useRefresh, type Schema } from "../../api";
import { ErrorNotice, Loading } from "../../components/common";
import { Button } from "@/components/ui/button";
import { MetaParts } from "@/components/meta-line";
import { plainText, RichText } from "@/components/rich-text";
import { rtlDir } from "@/lib/text-direction";
import { cn } from "@/lib/utils";
import { Citations, type SourceSelection } from "../Sources";
import { APPROVAL_NAMES, decideApproval } from "./ApprovalList";

type Detail = Schema<"ApprovalDetail">;

// Decisions that can be changed after a slip, while nothing depends on them yet.
const CHANGEABLE = new Set(["finding", "boq_row"]);

/**
 * One item the Tender Manager needs your OK on, in full: what it says, the key
 * facts, the passages it rests on, and the decision.
 */
export function ApprovalDetail({
  tenderId,
  recordKey,
  onSource,
  onAskChanges,
}: {
  tenderId: string;
  /** "kind:id", as linked from the chat. */
  recordKey: string;
  onSource: (source: SourceSelection) => void;
  onAskChanges?: (text: string) => void;
}) {
  const api = useApi();
  const refresh = useRefresh();
  const [kind, ...rest] = recordKey.split(":");
  const id = rest.join(":");
  const path = `${tenderPath(tenderId)}/approvals/${encodeURIComponent(kind)}/${encodeURIComponent(id)}`;
  const detail = useQuery({
    queryKey: [path],
    queryFn: ({ signal }) => api.get<Detail>(path, signal),
    retry: false,
  });
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<unknown>(null);

  if (detail.isPending) return <Loading>Loading…</Loading>;
  if (!detail.data) return <ErrorNotice error={detail.error} />;
  const item = detail.data;
  const facts = item.facts ?? [];
  const sources = item.source_ids ?? [];
  const name = APPROVAL_NAMES[item.kind]?.[0] ?? "item";

  async function decide(decision: "accept" | "reject") {
    setSaving(true);
    setError(null);
    try {
      await decideApproval(api, tenderId, item, decision);
      await detail.refetch();
      await refresh();
    } catch (failure) {
      setError(failure);
    } finally {
      setSaving(false);
    }
  }

  return (
    <article aria-label={plainText(item.title)} className="flex flex-col gap-4">
      <header className="flex flex-col gap-1.5">
        <p className="text-xs text-muted-foreground">
          <MetaParts
            parts={[name.charAt(0).toUpperCase() + name.slice(1), ...facts]}
          />
        </p>
        <h3
          dir={rtlDir(item.title)}
          className="text-base leading-snug font-medium wrap-break-word"
        >
          {plainText(item.title)}
        </h3>
        <p
          className={cn(
            "flex flex-wrap items-center gap-x-2 text-xs",
            item.state === "waiting" && "text-(--warning-ink)",
            item.state === "accepted" && "text-(--success)",
            (item.state === "rejected" || item.state === "closed") &&
              "text-muted-foreground",
          )}
        >
          {item.state === "waiting"
            ? "Waiting for your OK"
            : item.state === "accepted"
              ? "Accepted"
              : item.state === "rejected"
                ? "Rejected"
                : "Out of date: the documents behind it changed"}
          {CHANGEABLE.has(item.kind) &&
          (item.state === "accepted" || item.state === "rejected") ? (
            <button
              type="button"
              disabled={saving}
              onClick={() =>
                void decide(item.state === "accepted" ? "reject" : "accept")
              }
              className="text-muted-foreground underline underline-offset-2 hover:text-foreground disabled:opacity-50"
            >
              {item.state === "accepted" ? "Reject instead" : "Accept instead"}
            </button>
          ) : null}
        </p>
      </header>

      {item.state === "waiting" ? (
        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            size="sm"
            disabled={saving}
            onClick={() => void decide("accept")}
          >
            <Check data-icon="inline-start" />
            Accept
          </Button>
          {item.can_reject ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={saving}
              onClick={() => void decide("reject")}
            >
              <X data-icon="inline-start" />
              Reject
            </Button>
          ) : null}
          {onAskChanges ? (
            <Button
              type="button"
              size="sm"
              variant="ghost"
              onClick={() =>
                onAskChanges(`About "${plainText(item.title, 80)}": `)
              }
            >
              <MessageSquare data-icon="inline-start" />
              Ask for changes
            </Button>
          ) : null}
        </div>
      ) : null}
      <ErrorNotice error={error} />

      {item.detail ? (
        <RichText
          text={item.detail}
          className="text-sm"
          onSource={(sourceId) => onSource({ sourceId })}
        />
      ) : null}

      {sources.length ? (
        <section aria-label="Sources" className="flex flex-col gap-2">
          <h4 className="text-xs font-medium text-muted-foreground">
            From{" "}
            {sources.length === 1 ? "1 passage" : `${sources.length} passages`}
          </h4>
          <Citations ids={sources} tenderId={tenderId} onOpen={onSource} />
        </section>
      ) : null}
    </article>
  );
}
