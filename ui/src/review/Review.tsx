import { useState } from "react";
import { firstName, type Staff } from "../office/queries";
import { useReopen, type Reopenable } from "./queries";

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

/** Send it back with the reason, so whoever did it knows what to put right. */
export function SendBack(props: { onSend: (reason: string) => void; label?: string; placeholder?: string }) {
  const [open, setOpen] = useState(false);
  const [reason, setReason] = useState("");
  const label = props.label ?? "Send back";
  const placeholder = props.placeholder ?? "What to put right";
  if (!open)
    return (
      <button onClick={() => setOpen(true)} className="text-ink-2 hover:text-ink">
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
