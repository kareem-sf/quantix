import {
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { cn } from "@/lib/utils";

/**
 * One highlight that glides to whichever row the pointer or keyboard focus is
 * on, instead of each row painting its own hover background. Rows opt in with
 * a `data-menu-row` attribute (or a custom selector). Adapted from Beautiful UI.
 */
export function GlideMenu({
  children,
  className,
  highlightClassName,
  rowSelector = "[data-menu-row]",
}: {
  children: ReactNode;
  className?: string;
  highlightClassName?: string;
  rowSelector?: string;
}) {
  const root = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState<{ top: number; height: number } | null>(null);
  const [visible, setVisible] = useState(false);

  const moveTo = useCallback(
    (target: EventTarget | null) => {
      if (!(target instanceof Element) || !root.current) return;
      const row = target.closest<HTMLElement>(rowSelector);
      if (!row || !root.current.contains(row)) return;
      setBox({ top: row.offsetTop, height: row.offsetHeight });
      setVisible(true);
    },
    [rowSelector],
  );

  // Keep the highlight aligned when rows change height (text wraps, a row
  // expands) while it is showing.
  useLayoutEffect(() => {
    const node = root.current;
    if (!node || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      const active = node.querySelector<HTMLElement>(
        `${rowSelector}:hover, ${rowSelector}:focus-visible`,
      );
      if (active)
        setBox({ top: active.offsetTop, height: active.offsetHeight });
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [rowSelector]);

  return (
    <div
      ref={root}
      className={cn("relative", className)}
      onPointerOver={(event) => moveTo(event.target)}
      onFocus={(event) => moveTo(event.target)}
      onPointerLeave={() => setVisible(false)}
      onBlur={(event) => {
        if (!root.current?.contains(event.relatedTarget as Node | null))
          setVisible(false);
      }}
    >
      <span
        aria-hidden
        data-glide-highlight
        className={cn(
          "pointer-events-none absolute inset-x-0 rounded-md bg-muted transition-[top,height,opacity] duration-200 ease-(--bui-ease) motion-reduce:transition-none",
          highlightClassName,
        )}
        style={{
          top: box?.top ?? 0,
          height: box?.height ?? 0,
          opacity: visible && box ? 1 : 0,
        }}
      />
      {children}
    </div>
  );
}
