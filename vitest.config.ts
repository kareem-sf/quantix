import react from "@vitejs/plugin-react";
import { configDefaults, defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()],
  define: {
    __QUANTIX_WINDOWS_TITLEBAR__: true,
  },
  test: {
    clearMocks: true,
    environment: "jsdom",
    exclude: [...configDefaults.exclude, ".worktrees/**"],
    fileParallelism: false,
    pool: "forks",
    setupFiles: ["./src/testSetup.ts"],
    // Must exceed the Testing Library asyncUtilTimeout set in testSetup.ts, or a failing
    // query burns the whole test budget and is reported as a timeout, hiding what it
    // actually could not find.
    testTimeout: 20_000,
  },
});
