import { act } from "@testing-library/react";
import { vi } from "vitest";

/** jsdom lays nothing out and has no ResizeObserver: a stand-in for it, and a way to give an element the size a
 * window would, telling whatever watches it. */
export function layout() {
  const watching = new Set<{ element: Element; callback: ResizeObserverCallback; observer: ResizeObserver }>();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(private callback: ResizeObserverCallback) {}
      observe(element: Element) {
        watching.add({ element, callback: this.callback, observer: this as unknown as ResizeObserver });
      }
      unobserve() {}
      disconnect() {
        for (const watch of watching) if (watch.observer === (this as unknown)) watching.delete(watch);
      }
    },
  );
  return {
    resize(element: Element, width: number, height: number) {
      Object.defineProperties(element, {
        clientWidth: { value: width, configurable: true },
        clientHeight: { value: height, configurable: true },
      });
      const entry = { target: element, contentRect: { width, height } } as unknown as ResizeObserverEntry;
      act(() => {
        for (const watch of [...watching]) if (watch.element === element) watch.callback([entry], watch.observer);
      });
    },
  };
}
