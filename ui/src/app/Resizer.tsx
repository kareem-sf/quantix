import type { PointerEvent } from "react";

/** The edge of the sidebar or the team panel: drag it to change the width, double-click it for the usual width. */
export function Resizer(props: {
  edge: "left" | "right";
  width: number;
  label: string;
  onWidth: (width: number) => void;
  onReset: () => void;
}) {
  const start = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const from = e.clientX;
    const width = props.width;
    const sign = props.edge === "right" ? 1 : -1;
    document.documentElement.dataset.resizing = "";
    const move = (m: globalThis.PointerEvent) => props.onWidth(width + sign * (m.clientX - from));
    const stop = () => {
      delete document.documentElement.dataset.resizing;
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
  };
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={props.label}
      title="Drag to resize, double-click for the usual width"
      onPointerDown={start}
      onDoubleClick={props.onReset}
      className={`group absolute inset-y-0 z-10 w-2 cursor-col-resize ${props.edge === "right" ? "-right-1" : "-left-1"}`}
    >
      <span className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-transparent transition-colors group-hover:bg-ink-4 group-active:bg-ink-3" />
    </div>
  );
}
