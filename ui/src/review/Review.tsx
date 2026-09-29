import { IconAlertTriangle, IconInfoCircle } from "@tabler/icons-react";
import { useState } from "react";
import { Link } from "react-router";
import { firstName, type Staff } from "../office/queries";
import { useFindings, useReopen, type Checked, type Reopenable } from "./queries";

/** Work the staff proposed waits for the Tender Manager's review before it comes to the engineer. */
export const WITH_MANAGER = "With the Manager for review";

/** Who reviewed it and what they checked. */
export function ReviewNote(props: { reviewedBy: string | null; note: string | null; people: Map<string, Staff> }) {
  if (!props.reviewedBy) return null;
  const who = props.people.get(props.reviewedBy);
  return (
    <span className="leading-normal text-ink-2">
      Reviewed by {who ? firstName(who) : "the Manager"}
      {props.note && <span className="text-[#27272A]">: {props.note}</span>}
    </span>
  );
}

/** What Quantix's checks found, each one click from its source. A warning the Manager accepted shows his reason. */
export function Findings({ kind, id, tenderId }: { kind: Checked; id: string; tenderId: string }) {
  const found = useFindings(kind, id).data ?? [];
  if (!found.length) return null;
  return (
    <div className="flex flex-col gap-2 rounded-lg bg-rail px-3 py-2.5">
      <span className="text-xs font-medium text-ink-3">Quantix’s checks</span>
      {found.map((f, n) => {
        const blocker = f.severity === "blocker";
        const Mark = blocker ? IconAlertTriangle : IconInfoCircle;
        return (
          <div key={n} className="flex gap-2 leading-normal">
            <Mark className={`mt-0.5 size-4 shrink-0 ${blocker ? "text-attention" : "text-ink-4"}`} stroke={1.75} />
            <div className="flex min-w-0 flex-col gap-0.5">
              {blocker && <span className="text-xs font-medium text-attention">Must be fixed</span>}
              <span className="text-[#27272A]">{f.message}</span>
              {f.refs.map((r) =>
                r.document_id ? (
                  <Link
                    key={r.label}
                    to={`/tenders/${tenderId}/documents?doc=${r.document_id}&page=${r.page}`}
                    className="self-start text-ink-2 underline decoration-ink-4 underline-offset-4 hover:text-ink hover:decoration-ink"
                  >
                    {r.label}
                  </Link>
                ) : (
                  <span key={r.label} className="text-ink-2">
                    See {r.label}
                  </span>
                ),
              )}
              {f.reason && (
                <span className="text-ink-2">
                  Accepted by {f.accepted_by ?? "the Manager"}: {f.reason}
                </span>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}

/** The one look for approving on every screen, and for the quieter actions beside it. */
export const APPROVE =
  "inline-flex h-7 shrink-0 items-center justify-center gap-1.5 rounded-md bg-ink px-2.5 text-[13px] font-medium whitespace-nowrap text-white hover:bg-ink/85 active:scale-[0.98] disabled:cursor-default disabled:bg-subtle disabled:text-ink-4 disabled:active:scale-100";
export const SECONDARY =
  "inline-flex h-7 shrink-0 items-center justify-center gap-1.5 rounded-md border border-line-strong bg-white px-2.5 text-[13px] whitespace-nowrap text-ink-2 hover:border-ink-4 hover:text-ink active:scale-[0.98] disabled:cursor-default disabled:text-ink-4 disabled:active:scale-100";

/** Send it back with the reason, so whoever did it knows what to put right. */
export function SendBack(props: { onSend: (reason: string) => void; label?: string; placeholder?: string }) {
  const [open, setOpen] = useState(false);
  const [reason, setReason] = useState("");
  const label = props.label ?? "Send back";
  const placeholder = props.placeholder ?? "What to put right";
  if (!open)
    return (
      <button onClick={() => setOpen(true)} className={SECONDARY}>
        {label}
      </button>
    );
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        if (!reason.trim()) return;
        props.onSend(reason);
        setOpen(false);
      }}
      className="flex items-center gap-1.5"
    >
      <input
        aria-label={placeholder}
        autoFocus
        value={reason}
        onChange={(e) => setReason(e.target.value)}
        placeholder={placeholder}
        className="h-7 w-56 rounded-md border border-line-strong px-2 text-[13px] outline-none focus:border-ink"
      />
      <button className="font-medium text-ink">Send</button>
    </form>
  );
}

/** Approved work can still be sent back, with the reason, for the office to do again. */
export function Reopen({ kind, id }: { kind: Reopenable; id: string }) {
  const reopen = useReopen();
  return (
    <span className="flex flex-wrap items-center gap-2">
      <SendBack
        label="Reopen"
        placeholder="Why it needs doing again"
        onSend={(reason) => reopen.mutate({ kind, id, reason })}
      />
      {reopen.isError && <span className="text-attention">{reopen.error.message}</span>}
    </span>
  );
}
