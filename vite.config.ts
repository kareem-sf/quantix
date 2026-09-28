import { spawn, type ChildProcess } from "node:child_process";
import { randomBytes } from "node:crypto";
import { createReadStream, existsSync, readdirSync, readFileSync } from "node:fs";
import { createServer } from "node:net";
import path from "node:path";
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
    plugins: [react(), tailwindcss(), pdfjsAssets(), ...(dev ? [service(port, token)] : [])],
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
      testTimeout: 20000, // a busy machine or CI runner must not fail a correct test
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

// PDF.js fetches its character maps, standard fonts, colour profiles and image decoders by address while it reads a
// PDF: they are served from the package under /pdfjs in development and copied there in a build.
function pdfjsAssets(): Plugin {
  const source = path.join(root, "node_modules", "pdfjs-dist");
  const folders = ["cmaps", "standard_fonts", "iccs", "wasm"];
  return {
    name: "pdfjs-assets",
    configureServer(server) {
      server.middlewares.use("/pdfjs", (request, response, next) => {
        const [folder, file, ...rest] = decodeURIComponent((request.url ?? "").split("?")[0]).split("/").filter(Boolean);
        if (!folders.includes(folder) || !file || rest.length || file.includes("..")) return next();
        const full = path.join(source, folder, file);
        if (!existsSync(full)) return next();
        response.setHeader("Content-Type", file.endsWith(".wasm") ? "application/wasm" : "application/octet-stream");
        createReadStream(full).pipe(response);
      });
    },
    generateBundle() {
      for (const folder of folders) {
        for (const file of readdirSync(path.join(source, folder))) {
          const fileName = `pdfjs/${folder}/${file}`;
          this.emitFile({ type: "asset", fileName, source: readFileSync(path.join(source, folder, file)) });
        }
      }
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
