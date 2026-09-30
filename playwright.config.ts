import { defineConfig, devices } from "@playwright/test";

// The interface and a fresh service on a port and data folder of their own (e2e/serve.mjs), never reusing a server
// that may already be running, so the tests can't reach the engineer's own Quantix.
const port = 1430;

export default defineConfig({
  testDir: "e2e",
  fullyParallel: false,
  workers: 1, // one service and one database: the tests run in order, each on tenders of its own
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: `http://localhost:${port}`,
    trace: "retain-on-failure",
    ...devices["Desktop Chrome"],
    viewport: { width: 1440, height: 900 },
  },
  projects: [
    { name: "first run", testMatch: "first-run.spec.ts" },
    { name: "flows", testIgnore: "first-run.spec.ts", dependencies: ["first run"] },
  ],
  webServer: {
    command: `node e2e/serve.mjs ${port}`,
    url: `http://localhost:${port}/api/tenders`,
    reuseExistingServer: false,
    timeout: 120_000,
    stdout: "pipe",
  },
});
