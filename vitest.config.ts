import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("./", import.meta.url));

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.join(root, "src"),
    },
  },
  test: {
    maxWorkers: 2,
    testTimeout: 15000,
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/testSetup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
