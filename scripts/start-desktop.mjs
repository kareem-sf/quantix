import { spawn } from "node:child_process";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const runtime = path.join(root, ".quantix-dev");
await mkdir(runtime, { recursive: true });
const args = [
  path.join(root, "node_modules/@tauri-apps/cli/tauri.js"),
  "dev",
  "--no-watch",
];
try {
  const connection = JSON.parse(
    await readFile(path.join(runtime, "connection.json"), "utf8"),
  );
  const target = new URL(connection.base_url);
  if (target.protocol === "http:" && target.hostname === "127.0.0.1") {
    const response = await fetch(`${connection.base_url}/health`, {
      headers: { Authorization: `Bearer ${connection.token}` },
      signal: AbortSignal.timeout(1500),
    });
    if (response.ok) {
      const config = path.join(runtime, "tauri-running.json");
      await writeFile(
        config,
        JSON.stringify({ build: { beforeDevCommand: "" } }),
      );
      args.push("--config", config);
    }
  }
} catch {
  /* A fresh desktop session starts its own service through Tauri. */
}
const child = spawn(process.execPath, args, {
  cwd: root,
  stdio: "inherit",
  windowsHide: true,
});
child.on("error", (error) => {
  console.error(error.message);
  process.exitCode = 1;
});
child.on("exit", (code) => {
  process.exitCode = code ?? 1;
});
