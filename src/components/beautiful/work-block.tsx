import { useId, useState, type ReactNode } from "react";
import { ChevronDown, Sparkle } from "lucide-react";
import { cn } from "@/lib/utils";
import { RichText } from "../rich-text";
import { formatElapsed } from "./live-status";

function useDisclosure(
  open: boolean | undefined,
  defaultOpen: boolean,
  onOpenChange?: (open: boolean) => void,
) {
  const [inner, setInner] = useState(defaultOpen);
  const value = open ?? inner;
  return [
    value,
    () => {
      setInner(!value);
      onOpenChange?.(!value);
    },
  ] as const;
}

/**
 * One turn of work: a heading (the AI's own note, or a summary built from what
 * it did), an optional thinking trace, and the facts underneath. Adapted from
 * Beautiful UI's Thinking trace; nothing here advances on a timer, so the
 * heading shimmers only while `working` is true.
 */
export function WorkBlock({
  heading,
  headingDir,
  working = false,
  failed = false,
  durationMs,
  open,
  defaultOpen = false,
  onOpenChange,
  children,
  className,
}: {
  heading: ReactNode;
  /** "rtl" when the heading is mostly Arabic. */
  headingDir?: "rtl" | "ltr";
  working?: boolean;
  failed?: boolean;
  durationMs?: number | null;
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  children?: ReactNode;
  className?: string;
}) {
  const [expanded, toggle] = useDisclosure(open, defaultOpen, onOpenChange);
  const bodyId = useId();
  const hasBody = Boolean(children);
  return (
    <div className={cn("flex min-w-0 flex-col", className)}>
      <button
        type="button"
        aria-expanded={hasBody ? expanded : undefined}
        aria-controls={hasBody ? bodyId : undefined}
        disabled={!hasBody}
        onClick={toggle}
        className="group/block -mx-1.5 flex min-w-0 items-start gap-2 rounded-md px-1.5 py-1 text-start transition-colors duration-100 enabled:hover:bg-muted/70"
      >
        <ChevronDown
          aria-hidden
          className={cn(
            "mt-0.5 size-4 shrink-0 text-muted-foreground transition-transform duration-300",
            !expanded && "-rotate-90",
            !hasBody && "invisible",
          )}
        />
        <span
          dir={headingDir}
          className={cn(
            "min-w-0 flex-1 text-sm leading-relaxed whitespace-pre-line wrap-anywhere",
            working ? "text-foreground" : "text-foreground/85",
            failed && "text-destructive",
            // A long note stays short until its step is opened.
            !expanded && "line-clamp-3",
          )}
        >
          {heading}
        </span>
        {durationMs != null ? (
          <span className="mt-0.5 shrink-0 font-mono text-xs text-muted-foreground tabular-nums">
            {formatElapsed(durationMs)}
          </span>
        ) : null}
      </button>
      {hasBody ? (
        <div id={bodyId} className="bui-collapse" data-open={expanded}>
          <div>
            <div className="relative ms-[7px] mt-0.5 mb-1 border-s ps-4">
              {children}
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

/**
 * The AI's raw reasoning, folded away by default: the label shimmers while it
 * thinks and reads "Thought for Ns" after. Opening it shows the full text.
 */
export function ThinkingTrace({
  text,
  streaming = false,
  durationMs,
  className,
}: {
  text: string;
  streaming?: boolean;
  durationMs?: number | null;
  className?: string;
}) {
  const [expanded, setExpanded] = useState(false);
  const bodyId = useId();
  const seconds =
    durationMs != null ? Math.max(1, Math.round(durationMs / 1000)) : null;
  return (
    <div className={cn("flex min-w-0 flex-col py-0.5", className)}>
      <button
        type="button"
        aria-expanded={expanded}
        aria-controls={bodyId}
        onClick={() => setExpanded(!expanded)}
        className="-mx-1.5 flex w-fit items-center gap-1.5 rounded-md px-1.5 py-0.5 text-xs transition-colors duration-100 hover:bg-muted/70"
      >
        <Sparkle
          aria-hidden
          className={cn(
            "size-3.5",
            streaming ? "text-foreground" : "text-muted-foreground",
          )}
        />
        {streaming ? (
          <span className="bui-shimmer font-medium">Thinking</span>
        ) : (
          <span className="bui-fade-in font-medium text-muted-foreground">
            {seconds != null
              ? `Thought for ${seconds} ${seconds === 1 ? "second" : "seconds"}`
              : "Thinking"}
          </span>
        )}
        <ChevronDown
          aria-hidden
          className={cn(
            "size-3.5 text-muted-foreground transition-transform duration-300",
            expanded && "rotate-180",
          )}
        />
      </button>
      <div id={bodyId} className="bui-collapse" data-open={expanded}>
        <div>
          <div className="mt-0.5 max-h-72 overflow-y-auto border-s ps-3 text-xs text-muted-foreground wrap-anywhere">
            <RichText text={text} className="text-xs" />
            {streaming ? (
              <span
                aria-hidden
                className="ms-0.5 inline-block h-3 w-0.5 translate-y-0.5 rounded-full bg-foreground motion-safe:animate-pulse"
              />
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}
