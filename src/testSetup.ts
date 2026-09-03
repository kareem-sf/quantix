// @ts-expect-error Node builtin types are intentionally absent from the renderer.
import { webcrypto } from "node:crypto";
import { configure } from "@testing-library/react";
import { vi } from "vitest";

// Testing Library waits one second by default. The workspace re-renders behind a
// 2.5 second refresh, so on a slower machine a query can time out while the render
// it is waiting for is still on its way: the suite passes on a developer machine and
// fails on the build runner, on a different test each time. A longer ceiling costs
// nothing when a query resolves quickly and removes that whole class of false
// failure; a genuinely missing element still fails, just later.
configure({ asyncUtilTimeout: 5_000 });

if (!globalThis.crypto?.subtle) {
  Object.defineProperty(globalThis, "crypto", {
    configurable: true,
    value: webcrypto,
  });
}

const testAppWindow = vi.hoisted(() => ({
  close: vi.fn(() => Promise.resolve()),
  isMaximized: vi.fn(() => Promise.resolve(false)),
  minimize: vi.fn(() => Promise.resolve()),
  onResized: vi.fn(() => Promise.resolve(vi.fn())),
  toggleMaximize: vi.fn(() => Promise.resolve()),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => {
    // @ts-expect-error process is a Node.js global shared by the Vitest pool.
    return process.__QUANTIX_TEST_APP_WINDOW__ as typeof testAppWindow;
  },
}));

// @ts-expect-error process is a Node.js global shared by the Vitest pool.
Object.defineProperty(process, "__QUANTIX_TEST_APP_WINDOW__", {
  configurable: true,
  value: testAppWindow,
});

Object.defineProperty(window, "matchMedia", {
  configurable: true,
  writable: true,
  value: (query: string): MediaQueryList => ({
    matches: query === "(min-width: 820px)",
    media: query,
    onchange: null,
    addListener: () => undefined,
    removeListener: () => undefined,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    dispatchEvent: () => false,
  }),
});
