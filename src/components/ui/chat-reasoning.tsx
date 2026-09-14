import { useEffect, useState, type ReactNode } from "react";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import { cn } from "@/lib/utils";

/**
 * The agent's reasoning as a collapsible thread. It stays open while the agent
 * is still reasoning and folds away once it is done, unless the reader opened
 * or closed it themselves.
 */
export default function ChatReasoning<Part>({
  parts,
  reasoning,
  renderPart,
  reasoningLabel = "Reasoning…",
  doneLabel = "Done reasoning",
  className,
}: {
  parts: Part[];
  /** True while the agent is still reasoning. */
  reasoning: boolean;
  renderPart: (part: Part, index: number) => ReactNode;
  reasoningLabel?: string;
  doneLabel?: string;
  className?: string;
}) {
  const [value, setValue] = useState<string[]>(reasoning ? ["reasoning"] : []);
  const [touched, setTouched] = useState(false);

  useEffect(() => {
    if (!touched) setValue(reasoning ? ["reasoning"] : []);
  }, [reasoning, touched]);

  if (!parts.length) return null;
  return (
    <Accordion
      value={value}
      onValueChange={(next) => {
        setTouched(true);
        setValue(next as string[]);
      }}
      className={cn("w-full", className)}
    >
      <AccordionItem value="reasoning" className="border-b-0">
        <AccordionTrigger className="py-1.5 text-sm font-normal text-muted-foreground hover:no-underline hover:text-foreground">
          <span className={cn(reasoning && "animate-pulse")}>
            {reasoning ? reasoningLabel : doneLabel}
          </span>
        </AccordionTrigger>
        <AccordionContent className="pb-1">
          <ol className="flex flex-col">
            {parts.map((part, index) => (
              <li key={index} className="flex gap-2 ps-1">
                <div
                  aria-hidden
                  className="flex flex-col items-center gap-1 pt-2 -mb-1"
                >
                  <span className="size-1.5 rounded-full bg-muted-foreground/50" />
                  <span
                    className={cn(
                      "w-0.5 min-h-0 flex-1 rounded-full bg-border",
                      index === parts.length - 1 &&
                        "bg-gradient-to-b from-border to-transparent",
                    )}
                  />
                </div>
                <div className="min-w-0 flex-1">{renderPart(part, index)}</div>
              </li>
            ))}
          </ol>
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  );
}
