import { vi } from "vitest";

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
