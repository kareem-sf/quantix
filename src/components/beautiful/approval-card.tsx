import { useId, useState, type ReactNode } from "react";
import { Check } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export type ApprovalOption = {
  id: string;
  /** One-line summary shown in the alternatives list. */
  summary: string;
  body: ReactNode;
  actionLabel: string;
};

/**
 * Something the team proposes that needs the engineer's OK, such as a plan or
 * a plan change. Other real options, when there are any, sit in a drawer; the
 * engineer can promote one before approving. Adapted from Beautiful UI's
 * Recommendation Card, without its confidence meter.
 */
export function ApprovalCard({
  title,
  options,
  onApprove,
  secondary,
  approved = false,
  approving = false,
  meta,
  className,
}: {
  title: string;
  options: ApprovalOption[];
  onApprove: (option: ApprovalOption) => void;
  /** A second action, e.g. "Ask for changes". */
  secondary?: { label: string; onClick: () => void };
  approved?: boolean;
  approving?: boolean;
  /** Left side of the footer, e.g. "Proposed 3:47 PM". */
  meta?: ReactNode;
  className?: string;
}) {
  const [selected, setSelected] = useState(0);
  const [open, setOpen] = useState(false);
  const drawerId = useId();
  if (!options.length) return null;
  const active = options[Math.min(selected, options.length - 1)];
  const others = options
    .map((option, index) => ({ option, index }))
    .filter(({ index }) => index !== selected);

  return (
    <section
      aria-label={title}
      className={cn(
        "bui-fade-up w-full overflow-hidden rounded-lg bg-card ring-1 ring-border",
        className,
      )}
    >
      <div className="px-4 pt-3.5 pb-3">
        <h3 className="text-sm font-medium">{title}</h3>
        <div
          key={active.id}
          className="bui-fade-in mt-1.5 text-sm leading-relaxed text-foreground/80"
        >
          {active.body}
        </div>
      </div>
      {others.length ? (
        <div id={drawerId} className="bui-collapse" data-open={open}>
          <div>
            <div className="border-t px-2 py-2">
              <p className="px-1.5 pb-1 text-xs font-medium text-muted-foreground">
                Other options
              </p>
              {others.map(({ option, index }) => (
                <button
                  key={option.id}
                  type="button"
                  onClick={() => setSelected(index)}
                  className="flex w-full items-center rounded-md px-1.5 py-1.5 text-start text-sm transition-colors duration-100 hover:bg-muted"
                >
                  <span className="min-w-0 flex-1 truncate">
                    {option.summary}
                  </span>
                </button>
              ))}
            </div>
          </div>
        </div>
      ) : null}
      <div className="flex flex-wrap items-center justify-between gap-2 border-t bg-muted/40 px-3 py-2">
        <span className="text-xs text-muted-foreground">{meta}</span>
        <span className="flex items-center gap-1.5">
          {others.length ? (
            <Button
              variant="ghost"
              size="sm"
              aria-expanded={open}
              aria-controls={drawerId}
              onClick={() => setOpen(!open)}
            >
              Alternatives
            </Button>
          ) : null}
          {secondary && !approved ? (
            <Button variant="secondary" size="sm" onClick={secondary.onClick}>
              {secondary.label}
            </Button>
          ) : null}
          {approved ? (
            <span className="bui-pop-in inline-flex items-center gap-1.5 rounded-full bg-(--success-tint) py-1 ps-1 pe-2.5 text-xs font-medium text-(--success)">
              <span className="flex size-4 items-center justify-center rounded-full bg-(--success) text-white">
                <Check aria-hidden className="size-2.5" strokeWidth={3} />
              </span>
              Approved
            </span>
          ) : (
            <Button
              size="sm"
              disabled={approving}
              onClick={() => onApprove(active)}
            >
              {approving ? "Approving…" : active.actionLabel}
            </Button>
          )}
        </span>
      </div>
    </section>
  );
}
