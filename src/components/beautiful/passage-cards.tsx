import { AlignLeft, ArrowUpRight } from "lucide-react";
import { cn } from "@/lib/utils";
import { rtlDir } from "@/lib/text-direction";

export type Passage = {
  id: string;
  /** What the passage is about, or its heading in the document. */
  title: string;
  /** Location, e.g. "Page 4". */
  location?: string;
  text: string;
  fileName: string;
  /** File type badge, e.g. "PDF". */
  fileType?: string;
};

const BADGE_TONES: Record<string, string> = {
  PDF: "bg-destructive",
  XLSX: "bg-(--success)",
  XLS: "bg-(--success)",
  CSV: "bg-(--success)",
  DOCX: "bg-sky-600",
  DWG: "bg-foreground/70",
};

/**
 * Passages from the tender documents, each with its file and location, and a
 * link that opens the file at that place. Adapted from Beautiful UI's Context
 * Cards.
 */
export function PassageCards({
  passages,
  heading,
  onOpen,
  className,
}: {
  passages: Passage[];
  heading?: string;
  onOpen?: (passage: Passage) => void;
  className?: string;
}) {
  if (!passages.length) return null;
  return (
    <div className={cn("flex w-full flex-col gap-2", className)}>
      {heading ? (
        <div className="flex items-center gap-2 px-0.5">
          <span className="text-sm font-medium">{heading}</span>
          <span className="inline-flex h-5 items-center rounded-md bg-muted px-1.5 text-xs font-medium text-muted-foreground tabular-nums">
            {passages.length}
          </span>
        </div>
      ) : null}
      {passages.map((passage, index) => {
        const type = passage.fileType?.toUpperCase();
        return (
          <article
            key={passage.id}
            className="bui-fade-up overflow-hidden rounded-lg bg-card ring-1 ring-border"
            style={{ animationDelay: `${Math.min(index, 6) * 60}ms` }}
          >
            <div className="flex items-center gap-2.5 border-b px-3 py-2">
              <span className="flex min-w-0 items-center gap-1.5 text-sm font-medium">
                <AlignLeft aria-hidden className="size-3 shrink-0" />
                <span className="truncate">{passage.title}</span>
              </span>
              {passage.location ? (
                <span className="ms-auto shrink-0 text-xs text-muted-foreground">
                  {passage.location}
                </span>
              ) : null}
            </div>
            <p
              dir={rtlDir(passage.text)}
              className="bidi-text px-3 pt-2 pb-1 text-sm leading-relaxed text-foreground/85 wrap-anywhere"
            >
              {passage.text}
            </p>
            <div className="px-3 pb-3">
              <button
                type="button"
                disabled={!onOpen}
                onClick={() => onOpen?.(passage)}
                className="inline-flex h-6 max-w-full items-center gap-1.5 rounded-full bg-muted px-2 text-xs font-medium text-foreground/80 ring-1 ring-border/70 transition-colors duration-150 enabled:hover:bg-accent"
              >
                {type ? (
                  <span
                    className={cn(
                      "flex h-3.5 min-w-3.5 items-center justify-center rounded-[4px] px-0.5 text-[7px] font-bold text-white",
                      BADGE_TONES[type] ?? "bg-muted-foreground",
                    )}
                  >
                    {type}
                  </span>
                ) : null}
                <span className="truncate">{passage.fileName}</span>
                {onOpen ? (
                  <ArrowUpRight aria-hidden className="size-2.5 shrink-0" />
                ) : null}
              </button>
            </div>
          </article>
        );
      })}
    </div>
  );
}
