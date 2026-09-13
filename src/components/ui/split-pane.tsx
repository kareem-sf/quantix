/**
 * A two-pane split with a divider the engineer can drag or move with the
 * keyboard. The chosen width is remembered per storage key.
 *
 * Written here rather than taken from react-resizable-panels: that library
 * treats a band around every panel edge as a drag target, which swallows the
 * first click on controls near the divider.
 */
import {
  useCallback,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
} from "react";
import { cn } from "@/lib/utils";

export type SplitPaneProps = {
  start: ReactNode;
  end: ReactNode;
  /** Remembers the divider position across visits. */
  storageKey?: string;
  /** Width of the first pane, as a percentage of the split. */
  defaultPercent?: number;
  minPercent?: number;
  maxPercent?: number;
  label?: string;
  className?: string;
  /** Keep pane contents mounted while changing the workspace layout. */
  startHidden?: boolean;
  endHidden?: boolean;
};

const STEP = 2;

export function SplitPane({
  start,
  end,
  storageKey,
  defaultPercent = 58,
  minPercent = 34,
  maxPercent = 78,
  label = "Resize the panes",
  className,
  startHidden = false,
  endHidden = false,
}: SplitPaneProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [percent, setPercent] = useState(() =>
    Math.min(
      maxPercent,
      Math.max(minPercent, readPercent(storageKey) ?? defaultPercent),
    ),
  );
  const percentRef = useRef(percent);
  const dragging = useRef(false);

  const apply = useCallback(
    (next: number) => {
      const clamped = Math.min(maxPercent, Math.max(minPercent, next));
      percentRef.current = clamped;
      setPercent(clamped);
    },
    [maxPercent, minPercent],
  );

  function move(event: PointerEvent<HTMLDivElement>) {
    if (!dragging.current) return;
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect?.width) return;
    const offset = event.clientX - rect.left;
    const ratio =
      directionOf(containerRef.current) === "rtl"
        ? (rect.width - offset) / rect.width
        : offset / rect.width;
    apply(ratio * 100);
  }

  function stop(event: PointerEvent<HTMLDivElement>) {
    if (!dragging.current) return;
    dragging.current = false;
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
    writePercent(storageKey, percentRef.current);
  }

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const flip = directionOf(containerRef.current) === "rtl" ? -1 : 1;
    const keys: Record<string, number> = {
      ArrowLeft: -STEP * flip,
      ArrowRight: STEP * flip,
    };
    if (event.key in keys) {
      event.preventDefault();
      apply(percentRef.current + keys[event.key]);
      writePercent(storageKey, percentRef.current);
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      apply(event.key === "Home" ? minPercent : maxPercent);
      writePercent(storageKey, percentRef.current);
    }
  }

  return (
    <div
      ref={containerRef}
      className={cn("flex min-h-0 min-w-0 flex-1", className)}
    >
      <div
        className="flex min-h-0 min-w-0 flex-col"
        style={{
          display: startHidden ? "none" : undefined,
          flexBasis: endHidden ? "100%" : `${percent}%`,
          flexGrow: endHidden ? 1 : 0,
          flexShrink: 1,
        }}
      >
        {start}
      </div>
      <div
        role="separator"
        style={{ display: startHidden || endHidden ? "none" : undefined }}
        aria-orientation="vertical"
        aria-label={label}
        aria-valuenow={Math.round(percent)}
        aria-valuemin={minPercent}
        aria-valuemax={maxPercent}
        tabIndex={0}
        className="group relative z-10 w-px shrink-0 cursor-col-resize bg-border outline-none after:absolute after:inset-y-0 after:-start-1 after:-end-1 after:content-[''] hover:bg-primary/40 focus-visible:bg-primary/60"
        onPointerDown={(event) => {
          dragging.current = true;
          event.currentTarget.setPointerCapture(event.pointerId);
        }}
        onPointerMove={move}
        onPointerUp={stop}
        onPointerCancel={stop}
        onKeyDown={onKeyDown}
      />
      <div
        className="flex min-h-0 min-w-0 flex-1 flex-col"
        style={{ display: endHidden ? "none" : undefined }}
      >
        {end}
      </div>
    </div>
  );
}

function directionOf(element: HTMLElement | null) {
  if (!element || typeof window.getComputedStyle !== "function") return "ltr";
  return window.getComputedStyle(element).direction === "rtl" ? "rtl" : "ltr";
}

function readPercent(key?: string) {
  if (!key) return null;
  try {
    const saved = Number(window.localStorage.getItem(key));
    return Number.isFinite(saved) && saved > 0 ? saved : null;
  } catch {
    // Storage can be unavailable in a locked-down WebView; the default applies.
    return null;
  }
}

function writePercent(key: string | undefined, value: number) {
  if (!key) return;
  try {
    window.localStorage.setItem(key, String(Math.round(value * 100) / 100));
  } catch {
    // The chosen width still applies for this session.
  }
}
