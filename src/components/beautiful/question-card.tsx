import { useState } from "react";
import { Check, ChevronDown, ChevronUp } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { GlideMenu } from "./glide-menu";

export type Question = {
  id: string;
  text: string;
  /** Who asked, e.g. "Khalid Bin Salem". */
  askedBy?: string;
  type: "single" | "multiple";
  options: string[];
  /** Allow a free-text answer instead of the options. */
  allowOther?: boolean;
};

export type QuestionAnswer = { choices: string[]; other?: string };

/**
 * Questions the team needs answered before it can continue, one at a time.
 * Adapted from Beautiful UI's Approval Card. Answers are only sent when the
 * engineer presses Send; nothing is auto-submitted.
 */
export function QuestionCard({
  questions,
  onSubmit,
  submitting = false,
  className,
}: {
  questions: Question[];
  onSubmit: (answers: Record<string, QuestionAnswer>) => void;
  submitting?: boolean;
  className?: string;
}) {
  const [index, setIndex] = useState(0);
  const [answers, setAnswers] = useState<Record<string, QuestionAnswer>>({});
  if (!questions.length) return null;
  const current = questions[Math.min(index, questions.length - 1)];
  const answer = answers[current.id] ?? { choices: [] };
  const last = index >= questions.length - 1;
  const answered = answer.choices.length > 0 || Boolean(answer.other?.trim());
  const allAnswered = questions.every((question) => {
    const value = answers[question.id];
    return Boolean(value && (value.choices.length || value.other?.trim()));
  });

  const update = (next: QuestionAnswer) =>
    setAnswers((all) => ({ ...all, [current.id]: next }));
  const choose = (option: string) => {
    if (current.type === "single") update({ choices: [option] });
    else
      update({
        ...answer,
        choices: answer.choices.includes(option)
          ? answer.choices.filter((value) => value !== option)
          : [...answer.choices, option],
      });
  };

  return (
    <section
      aria-label="Questions for you"
      className={cn(
        "bui-fade-up w-full overflow-hidden rounded-lg bg-card ring-1 ring-border",
        className,
      )}
    >
      <div key={current.id} className="bui-fade-in px-4 pt-3.5 pb-3">
        {current.askedBy ? (
          <p className="mb-1 text-xs text-muted-foreground">
            {current.askedBy} asks
          </p>
        ) : null}
        <h3 className="text-sm font-medium wrap-anywhere">{current.text}</h3>
        <GlideMenu className="mt-2.5 flex flex-col gap-0.5">
          {current.options.map((option) => {
            const on = answer.choices.includes(option);
            return (
              <button
                key={option}
                type="button"
                data-menu-row
                role={current.type === "single" ? "radio" : "checkbox"}
                aria-checked={on}
                onClick={() => choose(option)}
                className="relative z-10 flex items-center gap-2 rounded-md py-1.5 ps-1.5 pe-2 text-start"
              >
                <span
                  className={cn(
                    "flex size-4 shrink-0 items-center justify-center transition-colors duration-200",
                    current.type === "single"
                      ? "rounded-full"
                      : "rounded-[5px]",
                    on
                      ? "bg-foreground text-background"
                      : "text-transparent ring-[1.5px] ring-input ring-inset",
                  )}
                >
                  {current.type === "single" ? (
                    <span
                      className={cn(
                        "size-1.5 rounded-full bg-background transition-transform duration-200",
                        on ? "scale-100" : "scale-0",
                      )}
                    />
                  ) : (
                    <Check aria-hidden className="size-3" strokeWidth={3} />
                  )}
                </span>
                <span
                  className={cn(
                    "text-sm",
                    on ? "text-foreground" : "text-foreground/80",
                  )}
                >
                  {option}
                </span>
              </button>
            );
          })}
          {current.allowOther !== false ? (
            <label
              data-menu-row
              className="relative z-10 flex items-center rounded-md py-1 ps-1.5 pe-2"
            >
              <input
                value={answer.other ?? ""}
                onChange={(event) =>
                  update({
                    choices: current.type === "single" ? [] : answer.choices,
                    other: event.target.value,
                  })
                }
                placeholder="Something else…"
                aria-label="Other answer"
                className="min-w-0 flex-1 bg-transparent ps-6 text-sm outline-none placeholder:text-muted-foreground"
              />
            </label>
          ) : null}
        </GlideMenu>
      </div>
      <div className="flex items-center justify-between gap-3 border-t bg-muted/40 px-3 py-2">
        <div className="flex items-center gap-1 text-muted-foreground">
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label="Previous question"
            disabled={index === 0}
            onClick={() => setIndex(index - 1)}
          >
            <ChevronUp />
          </Button>
          <span className="text-xs font-medium tabular-nums">
            {index + 1} / {questions.length}
          </span>
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label="Next question"
            disabled={last}
            onClick={() => setIndex(index + 1)}
          >
            <ChevronDown />
          </Button>
        </div>
        {last ? (
          <Button
            size="sm"
            disabled={!allAnswered || submitting}
            onClick={() => onSubmit(answers)}
          >
            {submitting ? "Sending…" : "Send answers"}
          </Button>
        ) : (
          <Button
            size="sm"
            variant="secondary"
            disabled={!answered}
            onClick={() => setIndex(index + 1)}
          >
            Next
          </Button>
        )}
      </div>
    </section>
  );
}
