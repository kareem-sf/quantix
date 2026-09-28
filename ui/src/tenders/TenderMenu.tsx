import { IconArchive, IconCheck, IconDots, IconTrash } from "@tabler/icons-react";
import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { useDeleteTender, useSetArchived, useSetOutcome } from "./queries";

type Outcome = "open" | "submitted" | "won" | "lost";
const OUTCOMES: [Outcome, string][] = [
  ["open", "Open"],
  ["submitted", "Submitted"],
  ["won", "Won"],
  ["lost", "Lost"],
];

/** What the engineer does to the tender as a whole: record how it went, put it away, or delete it. Later tenders use
 * won and lost prices as benchmarks, so archiving keeps everything; deleting asks first. */
export function TenderMenu(props: { tenderId: string; name: string; outcome: Outcome; archived: boolean }) {
  const [open, setOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const setOutcome = useSetOutcome(props.tenderId);
  const setArchived = useSetArchived(props.tenderId);
  const remove = useDeleteTender(props.tenderId);
  const navigate = useNavigate();
  const error = setOutcome.error ?? setArchived.error ?? remove.error;

  useEffect(() => {
    if (!open) return;
    const close = () => {
      setOpen(false);
      setDeleting(false);
    };
    const onPointer = (e: MouseEvent) => !box.current?.contains(e.target as Node) && close();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    document.addEventListener("mousedown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const item = "flex w-full items-center gap-2.5 rounded-md px-2.5 py-1.5 text-left hover:bg-selected";
  return (
    <div ref={box} className="relative">
      <button
        aria-label="Tender actions"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="flex size-8 items-center justify-center rounded-lg text-ink-3 hover:bg-selected hover:text-ink"
      >
        <IconDots className="size-4" stroke={1.75} />
      </button>
      {open && (
        <div
          role="menu"
          aria-label="Tender actions"
          className="absolute top-full right-0 z-30 mt-1 flex w-72 flex-col rounded-lg border border-line-strong bg-white p-1 shadow-[0_12px_32px_rgb(0_0_0/0.12)]"
        >
          {deleting ? (
            <div className="flex flex-col gap-2.5 p-2">
              <span className="leading-normal">
                Delete <span className="font-medium">{props.name}</span> and everything Quantix keeps for it:
                documents, BOQ, rates, the team and its conversations. Your original files and built packages stay.
                Archiving keeps it instead.
              </span>
              <span className="flex gap-2">
                <button
                  onClick={() => remove.mutate(undefined, { onSuccess: () => navigate("/") })}
                  disabled={remove.isPending}
                  className="h-8 rounded-lg bg-attention px-3 text-white"
                >
                  Delete tender
                </button>
                <button onClick={() => setDeleting(false)} className="h-8 rounded-lg border border-line-strong px-3">
                  Keep it
                </button>
              </span>
            </div>
          ) : (
            <>
              <span className="px-2.5 pt-1.5 pb-1 text-xs text-ink-3">How it went</span>
              {OUTCOMES.map(([value, label]) => (
                <button
                  key={value}
                  role="menuitemradio"
                  aria-checked={props.outcome === value}
                  onClick={() => setOutcome.mutate(value, { onSuccess: () => setOpen(false) })}
                  className={item}
                >
                  <span className="flex size-4 items-center justify-center">
                    {props.outcome === value && <IconCheck className="size-4" stroke={2} />}
                  </span>
                  {label}
                </button>
              ))}
              <span className="my-1 border-t border-line" />
              <button
                role="menuitem"
                onClick={() => setArchived.mutate(!props.archived, { onSuccess: () => setOpen(false) })}
                className={item}
              >
                <IconArchive className="size-4 text-ink-3" stroke={1.75} />
                {props.archived ? "Restore from the archive" : "Archive tender"}
              </button>
              <button role="menuitem" onClick={() => setDeleting(true)} className={`${item} text-attention`}>
                <IconTrash className="size-4" stroke={1.75} />
                Delete this tender
              </button>
            </>
          )}
          {error && <span className="px-2.5 py-1.5 text-attention">{error.message}</span>}
        </div>
      )}
    </div>
  );
}
