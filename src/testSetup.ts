import "@testing-library/jest-dom/vitest";
import { cleanup, configure } from "@testing-library/react";
import { afterEach } from "vitest";

// Large source and staff histories can render slowly in jsdom on desktop machines.
configure({ asyncUtilTimeout: 5000 });

// jsdom lacks browser APIs that shadcn/Base UI components use for responsive
// layout and popup positioning.
if (typeof window.matchMedia !== "function") {
  window.matchMedia = (query: string) =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
      addListener: () => undefined,
      removeListener: () => undefined,
      dispatchEvent: () => false,
    }) as MediaQueryList;
}
if (typeof globalThis.ResizeObserver === "undefined") {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
}
if (typeof Element.prototype.getAnimations !== "function") {
  Element.prototype.getAnimations = () => [];
}

afterEach(() => {
  cleanup();
  window.localStorage.clear();
});
