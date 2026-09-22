import { spawn, type ChildProcess } from "node:child_process";
import { randomBytes } from "node:crypto";
import { createServer } from "node:net";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import type { Plugin } from "vite";
import { defineConfig } from "vitest/config";
import { root, servicePython } from "./scripts/python.mjs";

// In development the Vite server starts the local service and forwards /api to it with the launch token,
// so the browser preview and the Tauri window reach the service the same way and never hold the token.
export default defineConfig(async ({ command }) => {
  const dev = command === "serve" && !process.env.VITEST;
  const port = dev ? await freePort() : 0;
  const token = randomBytes(32).toString("hex");

  return {
    root: "ui",
    plugins: [react(), tailwindcss(), ...(dev ? [service(port, token)] : [])],
    server: {
      port: 1420,
      strictPort: true,
      proxy: {
        "/api": {
          target: `http://127.0.0.1:${port}`,
          rewrite: (path: string) => path.replace(/^\/api/, ""),
          headers: { Authorization: `Bearer ${token}` },
        },
      },
    },
    build: { outDir: "../dist", emptyOutDir: true },
    test: {
      root: ".",
      environment: "jsdom",
      include: ["ui/src/**/*.test.{ts,tsx}"],
      setupFiles: ["ui/src/test-setup.ts"],
    },
  };
});

function service(port: number, token: string): Plugin {
  let child: ChildProcess | undefined;
  return {
    name: "quantix-service",
    configureServer(server) {
      child = spawn(servicePython(), ["-m", "quantix", "--port", String(port)], {
        cwd: root,
        env: { ...process.env, QUANTIX_TOKEN: token },
        stdio: "inherit",
      });
      child.on("exit", (code) => {
        if (code) server.config.logger.error(`Quantix service stopped with code ${code}.`);
      });
      const stop = () => child?.kill();
      server.httpServer?.on("close", stop);
      process.on("exit", stop);
    },
  };
}

function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const probe = createServer();
    probe.once("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const address = probe.address();
      probe.close(() => (typeof address === "object" && address ? resolve(address.port) : reject()));
    });
  });
}
