// Start the interface and a fresh service for the end-to-end tests on the given port, with a data folder of their
// own: never ~/.quantix.
import { spawn } from "node:child_process";
import { cpSync, existsSync, mkdirSync, rmSync } from "node:fs";
import path from "node:path";
import { root } from "../scripts/python.mjs";

const port = process.argv[2];
const home = path.join(root, "e2e", ".home");

rmSync(home, { recursive: true, force: true });
mkdirSync(home, { recursive: true });

// The meaning model the service tests keep, so a run doesn't fetch 135 MB; without it search goes by words.
const model = path.join(root, "service", ".pytest_cache", "d", "quantix-models");
if (existsSync(path.join(model, "model.onnx"))) {
  cpSync(model, path.join(home, "models", "multilingual-e5-small"), { recursive: true });
}

const vite = spawn(process.execPath, [path.join(root, "node_modules", "vite", "bin", "vite.js"), "--port", port], {
  cwd: root,
  env: { ...process.env, QUANTIX_HOME: home },
  stdio: "inherit",
});
vite.on("exit", (code) => process.exit(code ?? 0));
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => vite.kill(signal));
