import { act } from "@testing-library/react";
import { vi } from "vitest";

/** A window this many pixels wide: its media queries answer as a browser's would, and a resize tells their listeners. */
export function windowOf(width: number) {
  const listeners: (() => void)[] = [];
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: width <= Number(/max-width: (\d+)px/.exec(query)![1]),
    addEventListener: (_: string, listener: () => void) => listeners.push(listener),
    removeEventListener: (_: string, listener: () => void) => listeners.splice(listeners.indexOf(listener), 1),
  }));
  return {
    listeners,
    resize(to: number) {
      width = to;
      act(() => new Set(listeners).forEach((listener) => listener()));
    },
  };
}
