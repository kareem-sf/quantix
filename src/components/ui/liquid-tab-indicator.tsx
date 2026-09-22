/**
 * A pill behind the active tab that slides to the newly selected tab. Place it
 * inside the tab list; it finds the active trigger by `data-active`.
 *
 * It used to render through liquid-gooey, whose drawn copy stopped following
 * the measured position once tabs stayed mounted, so the pill stuck on an
 * earlier tab. A plain transformed element always matches the selection.
 */
import { useLayoutEffect, useRef, useState } from "react";

type Box = { left: number; width: number; top: number; height: number };

export function LiquidTabIndicator({ value }: { value: string }) {
  // The tab list forwards its ref late, so find the list from our own node.
  const anchorRef = useRef<HTMLSpanElement>(null);
  const [box, setBox] = useState<Box | null>(null);

  useLayoutEffect(() => {
    const list = anchorRef.current?.parentElement;
    if (!list) return;
    const measure = () => {
      const tab = list.querySelector<HTMLElement>("[data-active]");
      if (!tab || !tab.offsetWidth) return setBox(null);
      setBox({
        left: tab.offsetLeft,
        width: tab.offsetWidth,
        top: tab.offsetTop + 6,
        height: tab.offsetHeight - 12,
      });
    };
    measure();
    // The tab library can mark the active trigger after this effect runs.
    const mutations = new MutationObserver(measure);
    mutations.observe(list, {
      subtree: true,
      attributes: true,
      attributeFilter: ["data-active"],
    });
    const resize =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(measure);
    resize?.observe(list);
    return () => {
      mutations.disconnect();
      resize?.disconnect();
    };
  }, [value]);

  return (
    <span ref={anchorRef} aria-hidden="true" className="contents">
      {box ? (
        <span
          data-tab-indicator=""
          className="pointer-events-none absolute -z-10 rounded-md bg-muted transition-[transform,width] duration-300 ease-(--expo-out) motion-reduce:transition-none"
          style={{
            top: box.top,
            left: 0,
            width: box.width,
            height: box.height,
            transform: `translateX(${box.left}px)`,
          }}
        />
      ) : null}
    </span>
  );
}
