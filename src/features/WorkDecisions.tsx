import { useEffect, useRef, useState } from "react";
import {
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { Citations, type SourceSelection } from "./Sources";
import { ErrorNotice, Loading, Modal, Status } from "../components/common";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { createDraftScope, useFormDraft } from "./useFormDraft";

export function WorkDecisions({
  tenderId,
  onSource,
  focusedId,
}: {
  tenderId: string;
  onSource: (source: SourceSelection) => void;
  focusedId?: string;
}) {
  const findings = useResource<Schema<"Finding">[]>(
    `${tenderPath(tenderId)}/findings`,
    true,
  );
  return (
    <section className="flex flex-col gap-3 rounded-xl border bg-card p-4">
      <div className="flex flex-col gap-0.5">
        <h2 className="text-sm font-semibold tracking-tight">
          Decisions and findings
        </h2>
        <p className="text-xs text-muted-foreground">
          Review each source-scoped question and record the engineer’s decision.
        </p>
      </div>
      <ErrorNotice error={findings.error} />
      {findings.isPending ? <Loading>Loading findings…</Loading> : null}
      {findings.data?.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No findings have been recorded for this Tender.
        </p>
      ) : null}
      {findings.data?.map((finding) => (
        <FindingRow
          key={finding.id}
          finding={finding}
          tenderId={tenderId}
          onSource={onSource}
          focused={focusedId === finding.id}
        />
      ))}
    </section>
  );
}

export function FindingRow({
  finding,
  tenderId,
  onSource,
  focused = false,
}: {
  finding: Schema<"Finding">;
  tenderId: string;
  onSource: (source: SourceSelection) => void;
  focused?: boolean;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const rowRef = useRef<HTMLElement>(null);
  const [decision, setDecision] = useState<
    Schema<"DecisionRequest">["decision"] | null
  >(null);
  const [error, setError] = useState<unknown>(null);
  useEffect(() => {
    if (focused && rowRef.current) {
      rowRef.current.scrollIntoView?.({ block: "center" });
      rowRef.current.focus();
    }
  }, [focused]);
  return (
    <article
      ref={rowRef}
      tabIndex={focused ? -1 : undefined}
      className={cn(
        "flex flex-col gap-2 rounded-lg border bg-card p-3 outline-none",
        focused && "ring-2 ring-ring",
      )}
      id={`finding-${finding.id}`}
    >
      <div className="flex flex-wrap items-start justify-between gap-2">
        <strong className="min-w-0 text-sm font-medium" dir="auto">
          {finding.title}
        </strong>
        <Status value={finding.state} />
      </div>
      <p className="text-sm text-muted-foreground" dir="auto">
        {finding.detail}
      </p>
      <p className="text-xs text-muted-foreground">
        Source-scoped decision · {finding.source_ids.length} source
        {finding.source_ids.length === 1 ? "" : "s"}
      </p>
      {finding.is_stale ? (
        <p className="text-xs text-amber-700 dark:text-amber-400">
          A source changed. Review this finding against the current revision.
        </p>
      ) : null}
      <details className="text-sm">
        <summary className="w-fit cursor-pointer text-xs text-muted-foreground">
          View source basis
        </summary>
        <Citations
          ids={finding.source_ids}
          tenderId={tenderId}
          onOpen={onSource}
        />
      </details>
      <div className="flex flex-wrap items-center gap-1.5">
        {finding.state === "proposed" ? (
          <>
            <Button type="button" size="sm" onClick={() => setDecision("accept")}>
              Accept
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => setDecision("reject")}
            >
              Reject
            </Button>
          </>
        ) : finding.state === "accepted" ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => setDecision("resolve")}
          >
            Mark resolved
          </Button>
        ) : null}
      </div>
      <ErrorNotice error={error} />
      {decision ? (
        <DecisionDialog
          tenderId={tenderId}
          finding={finding}
          decision={decision}
          onClose={() => setDecision(null)}
          onSubmit={async (rationale) => {
            setError(null);
            await api.post(
              `${tenderPath(tenderId)}/findings/${finding.id}/decision`,
              { decision, rationale } satisfies Schema<"DecisionRequest">,
            );
            await refresh();
            setDecision(null);
          }}
        />
      ) : null}
    </article>
  );
}

function DecisionDialog({
  tenderId,
  finding,
  decision,
  onClose,
  onSubmit,
}: {
  tenderId: string;
  finding: Schema<"Finding">;
  decision: Schema<"DecisionRequest">["decision"];
  onClose: () => void;
  onSubmit: (rationale: string) => Promise<void>;
}) {
  const draft = useFormDraft(
    createDraftScope("work", tenderId, `finding-${finding.id}-${decision}`, 1),
    { rationale: "" },
    ["rationale"],
  );
  const rationale = draft.value.rationale;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const title =
    decision === "accept"
      ? "Accept finding"
      : decision === "reject"
        ? "Reject finding"
        : "Resolve finding";
  return (
    <Modal title={title} onClose={onClose}>
      <form
        onSubmit={async (event) => {
          event.preventDefault();
          if (!rationale.trim() || busy) return;
          setBusy(true);
          setError(null);
          const revision = draft.revision;
          try {
            await onSubmit(rationale.trim());
            draft.markAccepted(revision);
          } catch (failure) {
            setError(failure);
          } finally {
            setBusy(false);
          }
        }}
      >
        <p className="muted">{finding.title}</p>
        <label>
          Decision note
          <textarea
            required
            rows={4}
            maxLength={4000}
            value={rationale}
            onChange={(event) =>
              draft.setField("rationale", event.target.value)
            }
          />
        </label>
        <ErrorNotice error={error || draft.error} />
        <div className="form-actions">
          <button type="button" className="button" onClick={onClose}>
            Cancel
          </button>
          <button
            className="button primary"
            disabled={busy || !rationale.trim()}
          >
            {busy ? "Recording…" : "Record decision"}
          </button>
        </div>
      </form>
    </Modal>
  );
}
