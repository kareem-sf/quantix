import { IconChevronRight } from "@tabler/icons-react";
import { useState } from "react";
import { Prose } from "./Prose";
import { useTurn, type Turn, type TurnStep } from "./queries";

function duration(turn: Turn): string {
  const end = turn.ended_at ? new Date(turn.ended_at).getTime() : Date.now();
  const seconds = Math.max(1, Math.round((end - new Date(turn.started_at).getTime()) / 1000));
  if (seconds < 60) return `${seconds} s`;
  const minutes = Math.floor(seconds / 60);
  return seconds % 60 ? `${minutes} min ${seconds % 60} s` : `${minutes} min`;
}

/** The folded line, under the person's name: for how long, what they last did, and how it ended, in words. */
function headline(turn: Turn): string {
  const took = duration(turn);
  const did = turn.doing ? ` · ${turn.doing}` : "";
  if (turn.running) return `Working${did}`;
  switch (turn.ended) {
    case "done":
      return `Worked for ${took}${did}`;
    case "out_of_steps":
      return `Worked for ${took} and ran out of steps; carries on by itself`;
    case "tool_failed":
      return `Worked for ${took}; one step kept failing`;
    case "ai_failed":
      return `Stopped after ${took}: ${turn.note ?? "the AI service failed."}`;
    case "stopped":
      return `Stopped by you after ${took}`;
    case "failed":
      return `Stopped after ${took}: Quantix hit a problem of its own`;
    default:
      return "Cut off when Quantix closed";
  }
}

function toolWords(step: TurnStep): string {
  return step.doing ?? (step.tool ?? "").replaceAll("_", " ").replace(/^./, (c) => c.toUpperCase());
}

function pretty(args: string): string {
  try {
    return JSON.stringify(JSON.parse(args), null, 2);
  } catch {
    return args; // still being written
  }
}

function Raw({ label, text, tone = "" }: { label: string; text: string; tone?: string }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-[11px] font-medium text-ink-3">{label}</span>
      <pre
        className={`max-h-60 overflow-auto rounded-md bg-subtle px-2.5 py-1.5 font-mono text-[11.5px] leading-snug whitespace-pre-wrap [overflow-wrap:anywhere] ${tone}`}
      >
        {text}
      </pre>
    </div>
  );
}

function Step({ step, technical, name, running }: { step: TurnStep; technical: boolean; name: string; running: boolean }) {
  if (step.kind === "brief") {
    return technical ? (
      <details className="text-xs">
        <summary className="cursor-pointer text-ink-3 hover:text-ink">What {name} was told at the start</summary>
        <pre className="mt-1 max-h-80 overflow-auto rounded-md bg-subtle px-2.5 py-1.5 font-mono text-[11.5px] leading-snug whitespace-pre-wrap [overflow-wrap:anywhere]">
          {step.text}
        </pre>
      </details>
    ) : null;
  }
  if (step.kind === "thinking") {
    return <Prose text={step.text ?? ""} className="text-[13px] text-ink-3 italic" />;
  }
  if (step.kind === "note") {
    return <Prose text={step.text ?? ""} className="text-[13px] text-ink-2" />;
  }
  const waiting = step.result === null && step.sent_back === null;
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-baseline gap-2 text-[13px]">
        <span className={`size-1.5 shrink-0 translate-y-[-1px] rounded-full ${step.sent_back ? "bg-attention" : "bg-ink-4"}`} />
        <span className="text-ink" dir="auto">
          {toolWords(step)}
        </span>
        {technical && <code className="font-mono text-[11.5px] text-ink-3">{step.tool}</code>}
        {waiting && running && <span className="text-xs text-ink-3">working…</span>}
      </div>
      {step.sent_back && !technical && (
        <p className="pl-3.5 text-[13px] text-attention" dir="auto">
          Quantix sent this back: {step.sent_back}
        </p>
      )}
      {technical && (
        <div className="flex flex-col gap-1.5 pl-3.5">
          {step.args && step.args !== "{}" && <Raw label="Sent" text={pretty(step.args)} />}
          {step.result !== null && step.result !== undefined && <Raw label="Quantix answered" text={step.result} />}
          {step.sent_back && <Raw label="Quantix sent it back" text={step.sent_back} tone="text-attention" />}
        </div>
      )}
    </div>
  );
}

/** One person's turn at work, folded to a line in the chat. Opened, it shows their thinking and notes as they wrote
 * them and each step they took; the technical view adds the tools, what was sent and what Quantix answered. */
export function TurnLog(props: { turn: Turn; name: string; technical: boolean; onTechnical: (on: boolean) => void }) {
  const { turn, name, technical } = props;
  const [open, setOpen] = useState(false);
  const detail = useTurn(turn.id, open, turn.running);
  const trouble = !turn.running && turn.ended !== "done" && turn.ended !== "out_of_steps";
  const log = detail.data?.log ?? [];
  const shown = log.filter((s) => technical || s.kind !== "brief");

  return (
    <div className="flex flex-col gap-2">
      <button
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        className={`flex items-center gap-1.5 self-start text-left text-[13px] ${trouble ? "text-attention" : "text-ink-3"} hover:text-ink`}
      >
        {turn.running && <span className="size-1.5 shrink-0 animate-pulse rounded-full bg-ink-2" />}
        <span dir="auto">{headline(turn)}</span>
        <IconChevronRight className={`size-3.5 shrink-0 transition-transform ${open ? "rotate-90" : ""}`} stroke={1.75} />
      </button>
      {open && (
        <div className="flex max-w-[640px] flex-col gap-3 border-l-2 border-line pl-4">
          <label className="flex items-center gap-2 self-end text-xs text-ink-3">
            <input
              type="checkbox"
              role="switch"
              checked={technical}
              onChange={(e) => props.onTechnical(e.target.checked)}
              className="accent-ink"
            />
            Technical details
          </label>
          {detail.isPending && <p className="text-[13px] text-ink-3">Opening…</p>}
          {!detail.isPending && shown.length === 0 && (
            <p className="text-[13px] text-ink-3">
              {turn.running
                ? "Thinking…"
                : log.length === 0
                  ? "This turn ran before Quantix kept each turn's steps."
                  : "Nothing was written or done in this turn."}
            </p>
          )}
          {shown.map((step, index) => (
            <Step key={index} step={step} technical={technical} name={name} running={turn.running} />
          ))}
          {technical && turn.note && !turn.running && <Raw label={`Ended: ${turn.ended ?? "cut off"}`} text={turn.note} />}
        </div>
      )}
    </div>
  );
}
