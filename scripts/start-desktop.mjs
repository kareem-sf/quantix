import { spawn } from "node:child_process";
import { writeFile, mkdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { prepareAIHost } from "./prepare-ai-host.mjs";
import { reuseRunningWorkspace } from "./running-workspace.mjs";
import { createDiagnostics } from "./diagnostics.mjs";
import { applyProcessStorage, quantixPaths } from "./paths.mjs";
import { resetPreflight } from "./reset-preflight.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
await resetPreflight();
const storage = applyProcessStorage(quantixPaths());
const diagnostics = createDiagnostics("launcher");
const startedAt = Date.now();
const runtime = storage.runtime;
diagnostics.record("startup_started", { phase: "desktop_launcher" });
process.on("uncaughtExceptionMonitor", (error) => {
  diagnostics.recordError("uncaught_exception", { phase: "desktop_launcher" }, error);
});
let unhandledRejectionEscalated = false;
process.on("unhandledRejection", (reason) => {
  diagnostics.recordError("unhandled_rejection", { phase: "desktop_launcher" }, reason);
  if (unhandledRejectionEscalated) return;
  unhandledRejectionEscalated = true;
  process.exitCode = 1;
  queueMicrotask(() => {
    throw reason instanceof Error ? reason : new Error("Unhandled promise rejection.");
  });
});
process.on("exit", (code) => {
  diagnostics.record("process_exit", {
    phase: "desktop_launcher",
    outcome: code === 0 ? "success" : "failed",
    exit_code: code,
    duration_ms: Math.max(0, Date.now() - startedAt),
  });
  diagnostics.close();
});
try {
  await mkdir(runtime, { recursive: true });
  await mkdir(storage.tmp, { recursive: true });
  await mkdir(storage.cache, { recursive: true });
} catch (error) {
  diagnostics.recordError("startup_failed", { phase: "runtime_directory" }, error);
  throw error;
}
try {
  await prepareAIHost();
  diagnostics.record("ai_host_prepared", { phase: "prepare_ai_host", outcome: "success" });
} catch (error) {
  diagnostics.recordError("startup_failed", { phase: "prepare_ai_host" }, error);
  throw error;
}
const args = [
  path.join(root, "node_modules/@tauri-apps/cli/tauri.js"),
  "dev",
  "--no-watch",
];
let reuseWorkspace;
try {
  reuseWorkspace = await reuseRunningWorkspace(runtime);
} catch (error) {
  diagnostics.recordError("startup_failed", { phase: "workspace_reuse" }, error);
  throw error;
}
if (reuseWorkspace) {
  const config = storage.tauriRunningConfig;
  try {
    await writeFile(config, JSON.stringify({ build: { beforeDevCommand: "" } }));
  } catch (error) {
    diagnostics.recordError("startup_failed", { phase: "tauri_config" }, error);
    throw error;
  }
  args.push("--config", config);
}
let child;
const tauriStartedAt = Date.now();
try {
  child = spawn(process.execPath, args, {
    cwd: root,
    stdio: "inherit",
    windowsHide: true,
  });
  diagnostics.record("process_spawned", { phase: "tauri_spawn", process: "tauri", outcome: "success", child_process_id: child.pid });
} catch (error) {
  diagnostics.recordError("process_spawn_failed", { phase: "tauri_spawn", process: "tauri" }, error);
  throw error;
}
child.on("error", (error) => {
  diagnostics.recordError("process_error", { phase: "tauri", process: "tauri" }, error);
  console.error(error.message);
  process.exitCode = 1;
});
child.on("exit", (code, signal) => {
  diagnostics.record("process_exit", {
    phase: "tauri",
    process: "tauri",
    outcome: code === 0 ? "success" : "failed",
    duration_ms: Math.max(0, Date.now() - tauriStartedAt),
    ...(Number.isInteger(code) && code >= 0 ? { exit_code: code } : {}),
    ...(signal ? { signal_name: signal } : {}),
  });
  process.exitCode = code ?? 1;
});
