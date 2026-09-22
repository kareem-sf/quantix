import { useState } from "react";
import { ArrowUp, PencilLine } from "lucide-react";
import type { Schema } from "../../api";
import { ErrorNotice } from "../../components/common";
import { Button } from "@/components/ui/button";
import { plainText } from "@/components/rich-text";
import { rtlDir } from "@/lib/text-direction";
import { cn } from "@/lib/utils";

const OTHER = "__other__";

/**
 * The Tender Manager's question with its suggested answers. The recommended
 * answer is picked already, so one press of Send answers it; the engineer can
 * pick another or write their own. The answer is sent as the engineer's reply.
 */
export function ChoiceQuestion({
  question,
  answer,
  onAnswer,
}: {
  question: Schema<"MessageQuestion">;
  /** The engineer's reply after this question, once answered. */
  answer?: string;
  /** Absent when the question can no longer be answered here. */
  onAnswer?: (text: string) => Promise<unknown>;
}) {
  const choices = question.choices ?? [];
  const recommended = Math.max(
    0,
    choices.findIndex((choice) => choice.recommended),
  );
  const [picked, setPicked] = useState<string>(String(recommended));
  const [other, setOther] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const answered = answer !== undefined;
  const open = !answered && Boolean(onAnswer);
  const chosenIndex = answered
    ? choices.findIndex((choice) => choice.label.trim() === answer.trim())
    : picked === OTHER
      ? -1
      : Number(picked);
  const text =
    picked === OTHER ? other.trim() : (choices[Number(picked)]?.label ?? "");

  async function send() {
    if (!onAnswer || !text) return;
    setSending(true);
    setError(null);
    try {
      await onAnswer(text);
    } catch (failure) {
      setError(failure);
    } finally {
      setSending(false);
    }
  }

  return (
    <section
      aria-label="Question for you"
      className="bui-fade-up w-full max-w-xl overflow-hidden rounded-xl bg-card ring-1 ring-border"
    >
      <h3
        dir={rtlDir(question.text)}
        className="px-4 pt-3.5 pb-2 text-sm font-medium wrap-anywhere"
      >
        {plainText(question.text)}
      </h3>
      <div
        role="radiogroup"
        aria-label={plainText(question.text)}
        className="flex flex-col px-2 pb-2"
      >
        {choices.map((choice, index) => {
          const on = answered
            ? index === chosenIndex
            : picked === String(index);
          return (
            <button
              key={`${index}:${choice.label}`}
              type="button"
              role="radio"
              aria-checked={on}
              disabled={!open}
              onClick={() => setPicked(String(index))}
              className={cn(
                "flex w-full items-start gap-2.5 rounded-lg px-2 py-2 text-start transition-colors",
                open && "hover:bg-muted/70",
                on && "bg-muted/60",
                answered && !on && "opacity-50",
              )}
            >
              <span
                aria-hidden
                className={cn(
                  "mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-full",
                  on ? "bg-foreground" : "ring-[1.5px] ring-input ring-inset",
                )}
              >
                {on ? (
                  <span className="size-1.5 rounded-full bg-background" />
                ) : null}
              </span>
              <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
                  <span
                    dir={rtlDir(choice.label)}
                    className="text-sm font-medium wrap-anywhere"
                  >
                    {plainText(choice.label)}
                  </span>
                  {choice.recommended ? (
                    <span className="rounded-full bg-(--success-tint) px-1.5 py-px text-[11px] font-medium text-(--success)">
                      Recommended
                    </span>
                  ) : null}
                </span>
                {choice.detail ? (
                  <span
                    dir={rtlDir(choice.detail)}
                    className="text-xs leading-relaxed text-muted-foreground wrap-anywhere"
                  >
                    {plainText(choice.detail)}
                  </span>
                ) : null}
              </span>
            </button>
          );
        })}
        {open ? (
          <label
            className={cn(
              "flex w-full items-start gap-2.5 rounded-lg px-2 py-1.5",
              picked === OTHER && "bg-muted/60",
            )}
          >
            <PencilLine
              aria-hidden
              className="mt-0.5 size-4 shrink-0 text-muted-foreground"
            />
            <textarea
              rows={1}
              dir="auto"
              value={other}
              onFocus={() => setPicked(OTHER)}
              onChange={(event) => {
                setPicked(OTHER);
                setOther(event.target.value);
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.shiftKey) {
                  event.preventDefault();
                  if (text) void send();
                }
              }}
              placeholder="Write another answer…"
              aria-label="Write another answer"
              className="field-sizing-content max-h-40 min-w-0 flex-1 resize-none bg-transparent text-sm outline-none placeholder:text-muted-foreground"
            />
          </label>
        ) : null}
        {answered && chosenIndex < 0 ? (
          <p
            dir={rtlDir(answer)}
            className="px-2 py-1.5 text-sm text-muted-foreground wrap-anywhere"
          >
            You answered: {answer}
          </p>
        ) : null}
      </div>
      {open ? (
        <div className="flex items-center justify-end gap-2 border-t bg-muted/30 px-3 py-2">
          <ErrorNotice error={error} />
          <Button
            type="button"
            size="sm"
            disabled={!text || sending}
            onClick={() => void send()}
          >
            <ArrowUp data-icon="inline-start" />
            {sending ? "Sending…" : "Send"}
          </Button>
        </div>
      ) : null}
    </section>
  );
}
