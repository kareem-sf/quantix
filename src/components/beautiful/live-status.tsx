import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";

/* Chevron wavefront over a 3×3 grid: each cell's delay grows with its column
   and its distance from the middle row, so a "›" sweeps left to right. */
const DELAYS = Array.from({ length: 9 }, (_, index) => {
  const row = Math.floor(index / 3);
  const column = index % 3;
  return (column + Math.abs(row - 1)) * 90;
});

export function formatElapsed(ms: number) {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60)
    return `${minutes}m ${String(seconds % 60).padStart(2, "0")}s`;
  return `${Math.floor(minutes / 60)}h ${String(minutes % 60).padStart(2, "0")}m`;
}

/** Ticks once a second from a real start time; stops when `since` is unset. */
export function useElapsed(since?: string | number | null, running = true) {
  const start = since == null ? null : new Date(since).getTime();
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (start == null || !running) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [start, running]);
  return start == null || Number.isNaN(start) ? null : now - start;
}

export function PixelLoader({ className }: { className?: string }) {
  return (
    <span
      aria-hidden
      className={cn(
        "grid shrink-0 grid-cols-[repeat(3,4px)] gap-[1.5px]",
        className,
      )}
    >
      {DELAYS.map((delay, index) => (
        <span
          key={index}
          className="size-1 rounded-[1px] bg-foreground opacity-15 motion-reduce:animate-none"
          style={{
            animation: `bui-pixel-on 650ms ease-in-out ${delay}ms infinite`,
          }}
        />
      ))}
    </span>
  );
}

/**
 * The live line while work is running: loader, a label describing what is
 * really happening, and time elapsed since it started. Adapted from Beautiful
 * UI's Loading State; the label and start time always come from real events.
 */
export function LiveStatus({
  label,
  since,
  className,
}: {
  label: string;
  since?: string | number | null;
  className?: string;
}) {
  const elapsed = useElapsed(since);
  return (
    <div
      role="status"
      className={cn("flex w-fit items-center gap-2.5", className)}
    >
      <PixelLoader />
      <span className="bui-shimmer text-sm font-medium">{label}</span>
      {elapsed != null ? (
        <span className="font-mono text-xs text-muted-foreground tabular-nums">
          {formatElapsed(elapsed)}
        </span>
      ) : null}
    </div>
  );
}
