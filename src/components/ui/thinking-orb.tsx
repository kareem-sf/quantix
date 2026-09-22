/**
 * The animated orb that stands in for a spinner while the AI is working. The
 * animation follows what the work is doing: searching documents sweeps a
 * globe, checking figures scrambles, delegating wires a constellation.
 */
import { ThinkingOrb, type OrbState } from "thinking-orbs";
import { cn } from "@/lib/utils";

export type { OrbState };

/** Words in a step title, matched in order, to the animation that fits it. */
const cues: [RegExp, OrbState][] = [
  [/search|find|look|lookup|scan|retriev/i, "searching"],
  [/takeoff|measur|drawing|quantit|shape|sketch/i, "shaping"],
  [/check|verif|calculat|solv|validat|review|estimat|price/i, "solving"],
  [/delegat|assign|team|staff|engineer|connect|hand/i, "connecting"],
  [/compar|cross|link|map|match|merg/i, "weaving"],
  [/writ|draft|compos|prepar|answer|reply|summar|report/i, "composing"],
  [/wait|ask|listen|question|input/i, "listening"],
  [/plan|decid|think|reason|work out/i, "breathing"],
];

/** The orb animation that best describes a step, from its title. */
export function orbStateFor(text?: string): OrbState {
  if (!text) return "working";
  return cues.find(([pattern]) => pattern.test(text))?.[1] ?? "working";
}

export function AiOrb({
  state = "working",
  size = 20,
  paused,
  className,
  label,
}: {
  state?: OrbState;
  /** The two tuned presets: 20 inline, 64 chat-avatar. */
  size?: 20 | 64;
  paused?: boolean;
  className?: string;
  /** Spoken label; the orb is decorative when omitted. */
  label?: string;
}) {
  return (
    <span
      className={cn("inline-flex shrink-0", className)}
      style={{ width: size, height: size }}
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
    >
      <ThinkingOrb state={state} size={size} paused={paused} />
    </span>
  );
}
