import { useCallback, useRef, useState, type PointerEvent } from "react";
import { createPortal } from "react-dom";
import { useCtrlWheel, useSize, Waiting, ZoomControls, zoomStep } from "./Viewer";

const MARGIN = 32;

/** An image at its own sharpness: fitted to the window (never enlarged) until zoomed with the controls or Ctrl +
 * wheel, and moved by dragging. */
export function ImageView(props: { src: string; name: string; toolbar: HTMLElement | null }) {
  const [scroller, setScroller] = useState<HTMLDivElement | null>(null);
  const [natural, setNatural] = useState<{ width: number; height: number } | null>(null);
  const [chosen, setChosen] = useState<number | null>(null); // null: fitted to the window
  const [failed, setFailed] = useState(false);
  const drag = useRef<{ x: number; y: number; left: number; top: number } | null>(null);
  const room = useSize(scroller);
  const fitted =
    natural && room.width > MARGIN
      ? Math.min((room.width - MARGIN) / natural.width, (room.height - MARGIN) / natural.height, 1)
      : null;
  const scale = chosen ?? fitted;

  const wheel = useCallback(
    (factor: number) => setChosen((s) => Math.min(Math.max((s ?? fitted ?? 1) * factor, 0.05), 8)),
    [fitted],
  );
  useCtrlWheel(scroller, wheel);

  const down = (event: PointerEvent<HTMLDivElement>) => {
    if (!scroller || event.button !== 0) return;
    drag.current = { x: event.clientX, y: event.clientY, left: scroller.scrollLeft, top: scroller.scrollTop };
    event.currentTarget.setPointerCapture?.(event.pointerId);
  };
  const move = (event: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d || !scroller) return;
    scroller.scrollLeft = d.left - (event.clientX - d.x);
    scroller.scrollTop = d.top - (event.clientY - d.y);
  };

  return (
    <>
      {props.toolbar &&
        scale !== null &&
        createPortal(
          <ZoomControls
            scale={scale}
            onOut={() => setChosen(zoomStep(scale, -1))}
            onIn={() => setChosen(zoomStep(scale, 1))}
            onFit={() => setChosen(null)}
          />,
          props.toolbar,
        )}
      <div
        ref={setScroller}
        tabIndex={0}
        onPointerDown={down}
        onPointerMove={move}
        onPointerUp={() => (drag.current = null)}
        className="absolute inset-0 cursor-grab overflow-auto outline-none active:cursor-grabbing"
      >
        <div className="flex min-h-full w-max min-w-full items-center justify-center p-4">
          <img
            src={props.src}
            alt={props.name}
            draggable={false}
            onLoad={(e) => setNatural({ width: e.currentTarget.naturalWidth, height: e.currentTarget.naturalHeight })}
            onError={() => setFailed(true)}
            style={natural && scale !== null ? { width: natural.width * scale, height: natural.height * scale } : { opacity: 0 }}
            className="block max-w-none bg-white shadow-[0_0_0_1px_var(--color-line-strong)]"
          />
        </div>
      </div>
      {failed && (
        <div className="absolute inset-0 flex bg-subtle">
          <Waiting>Quantix couldn’t show this image. Open the original.</Waiting>
        </div>
      )}
    </>
  );
}
