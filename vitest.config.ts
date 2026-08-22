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
    pool: "vmThreads",
    setupFiles: ["./src/testSetup.ts"],
  },
});
