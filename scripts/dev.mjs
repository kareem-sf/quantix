import { spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { mkdir } from "node:fs/promises";
import net from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { prepareAIHost } from "./prepare-ai-host.mjs";
import { createDiagnostics } from "./diagnostics.mjs";
import { applyProcessStorage, quantixPaths } from "./paths.mjs";
import { resetPreflight } from "./reset-preflight.mjs";
import { watchPythonSources } from "./watch-python-sources.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
await resetPreflight();
const storage = applyProcessStorage(quantixPaths());
const diagnostics = createDiagnostics("launcher");
const startedAt = Date.now();
const runtime = storage.runtime;
diagnostics.record("startup_started", { phase: "launcher" });
process.on("uncaughtExceptionMonitor", (error) => {
  diagnostics.recordError("uncaught_exception", { phase: "launcher" }, error);
});
let unhandledRejectionEscalated = false;
process.on("unhandledRejection", (reason) => {
  diagnostics.recordError("unhandled_rejection", { phase: "launcher" }, reason);
  if (unhandledRejectionEscalated) return;
  unhandledRejectionEscalated = true;
  process.exitCode = 1;
  queueMicrotask(() => {
    throw reason instanceof Error
      ? reason
      : new Error("Unhandled promise rejection.");
  });
});
process.on("exit", (code) => {
  diagnostics.record("process_exit", {
    phase: "launcher",
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
  diagnostics.recordError(
    "startup_failed",
    { phase: "runtime_directory" },
    error,
  );
  throw error;
}
try {
  await prepareAIHost();
  diagnostics.record("ai_host_prepared", {
    phase: "prepare_ai_host",
    outcome: "success",
  });
} catch (error) {
  diagnostics.recordError(
    "startup_failed",
    { phase: "prepare_ai_host" },
    error,
  );
  throw error;
}
const token = randomBytes(32).toString("hex");
let port;
try {
  port = await new Promise((resolve, reject) => {
    const server = net.createServer();
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const selected = server.address().port;
      server.close(() => resolve(selected));
    });
  });
} catch (error) {
  diagnostics.recordError("startup_failed", { phase: "port_bind" }, error);
  throw error;
}
const baseUrl = `http://127.0.0.1:${port}/api`;
const home = storage.root;
const python = path.join(
  root,
  "backend",
  ".venv",
  process.platform === "win32" ? "Scripts/python.exe" : "bin/python",
);
let backend;
let vite;
let stopping = false;
let backendExited = false;
let reloading = false;
let sourceWatcher;

function spawnBackend() {
  const spawnedAt = Date.now();
  const child = spawn(
    python,
    ["-m", "quantix", "--home", home, "--port", String(port)],
    {
      cwd: root,
      stdio: "inherit",
      windowsHide: true,
      env: { ...process.env, QUANTIX_SESSION_TOKEN: token },
    },
  );
  diagnostics.record("process_spawned", {
    phase: "backend_spawn",
    process: "backend",
    outcome: "success",
    child_process_id: child.pid,
  });
  child.on("error", (error) => {
    if (child !== backend) return;
    diagnostics.recordError(
      "process_error",
      { phase: "backend", process: "backend" },
      error,
    );
    console.error(error.message);
    process.exitCode = 1;
  });
  child.on("exit", (code, signal) => {
    if (child !== backend) return;
    backendExited = true;
    diagnostics.record("process_exit", {
      phase: "backend",
      process: "backend",
      outcome: code === 0 ? "success" : "failed",
      duration_ms: Math.max(0, Date.now() - spawnedAt),
      ...(Number.isInteger(code) && code >= 0 ? { exit_code: code } : {}),
      ...(signal ? { signal_name: signal } : {}),
    });
    // A reload owns this exit. Before the workspace is ready an unexpected
    // exit still fails startup loudly; once it is running, the service is
    // recovered instead of taking hot reload down with it.
    if (!stopping && !reloading) {
      if (watching) {
        console.error("The Quantix service stopped unexpectedly.");
        scheduleRecovery("recovery after an unexpected exit");
      } else void stop(1);
    }
  });
  return child;
}

try {
  backend = spawnBackend();
} catch (error) {
  diagnostics.recordError(
    "process_spawn_failed",
    { phase: "backend_spawn", process: "backend" },
    error,
  );
  throw error;
}

async function stop(code = 0) {
  if (stopping) return;
  stopping = true;
  clearTimeout(retryTimer);
  await sourceWatcher?.close();
  diagnostics.record("shutdown_started", {
    phase: "launcher",
    outcome: "requested",
  });
  vite?.kill();
  try {
    await fetch(`${baseUrl}/shutdown`, {
      method: "POST",
      headers: { Authorization: `Bearer ${token}` },
      signal: AbortSignal.timeout(2000),
    });
  } catch (error) {
    diagnostics.recordError(
      "shutdown_request_failed",
      { phase: "service_shutdown" },
      error,
    );
    /* A failed startup may not have an HTTP listener. */
  }
  const until = Date.now() + 17000;
  while (!backendExited && Date.now() < until)
    await new Promise((resolve) => setTimeout(resolve, 100));
  if (!backendExited) {
    if (process.platform === "win32") {
      const killer = spawn(
        "taskkill.exe",
        ["/PID", String(backend.pid), "/T", "/F"],
        { windowsHide: true, stdio: "ignore" },
      );
      diagnostics.record("process_stop_requested", {
        phase: "backend_stop",
        process: "backend",
        child_process_id: backend.pid,
      });
      await new Promise((resolve) => {
        killer.once("exit", resolve);
        killer.once("error", (error) => {
          diagnostics.recordError(
            "process_stop_failed",
            { phase: "backend_stop", process: "backend" },
            error,
          );
          resolve();
        });
      });
    } else backend.kill();
  }
  diagnostics.record("shutdown_finished", {
    phase: "launcher",
    outcome: code === 0 ? "success" : "failed",
    duration_ms: Math.max(0, Date.now() - startedAt),
  });
  process.exitCode = code;
}
async function waitForBackendHealth(attempts = 150) {
  for (let attempt = 0; attempt < attempts && !backendExited; attempt++) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/healthz`, {
        signal: AbortSignal.timeout(1000),
      });
      if (response.ok) return true;
    } catch {
      /* Wait for the owned service to bind its local port. */
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  return false;
}

async function settled(child) {
  while (child.exitCode === null && child.signalCode === null)
    await new Promise((resolve) => setTimeout(resolve, 100));
}

let reloadQueued = false;
// Once the workspace is running, a service that fails to come back is
// recovered and never allowed to stop the launcher. Saves routinely land in
// the middle of a multi-file edit, so one start can fail on a half-written
// module that the very next save repairs. Stopping on that failure used to
// take the backend and the Vite dev server down together, leave the desktop
// window pointing at nothing, and ignore every later save.
let watching = false;
let retryTimer;
let retryDelay = 0;
const RETRY_MIN_MS = 2000;
const RETRY_MAX_MS = 30000;

function scheduleRecovery(reason) {
  if (stopping) return;
  clearTimeout(retryTimer);
  retryDelay = retryDelay
    ? Math.min(retryDelay * 2, RETRY_MAX_MS)
    : RETRY_MIN_MS;
  diagnostics.record("backend_reload", {
    phase: "backend_reload",
    outcome: "retry_scheduled",
    duration_ms: retryDelay,
  });
  console.error(
    `The Quantix service is not running. Retrying in ${Math.round(retryDelay / 1000)}s, or as soon as you save.`,
  );
  retryTimer = setTimeout(() => void reloadBackend(reason), retryDelay);
}

/** Replace the service process in place so edited Python is loaded. */
async function reloadBackend(reason) {
  if (stopping) return;
  if (reloading) {
    reloadQueued = true;
    return;
  }
  clearTimeout(retryTimer);
  reloading = true;
  console.log(`Quantix service reloading (${reason})…`);
  diagnostics.record("backend_reload", {
    phase: "backend_reload",
    outcome: "requested",
  });
  const previous = backend;
  try {
    await fetch(`${baseUrl}/shutdown`, {
      method: "POST",
      headers: { Authorization: `Bearer ${token}` },
      signal: AbortSignal.timeout(2000),
    });
  } catch {
    /* An already-stopped service still reaches the wait below. */
  }
  const until = Date.now() + 17000;
  while (
    previous.exitCode === null &&
    previous.signalCode === null &&
    Date.now() < until
  )
    await new Promise((resolve) => setTimeout(resolve, 100));
  if (previous.exitCode === null && previous.signalCode === null) {
    if (process.platform === "win32")
      spawn("taskkill.exe", ["/PID", String(previous.pid), "/T", "/F"], {
        windowsHide: true,
        stdio: "ignore",
      });
    else previous.kill();
    await settled(previous);
  }
  // The workspace lock is released with the old process. Only then can the
  // replacement bind the same home and port.
  backendExited = false;
  backend = spawnBackend();
  const healthy = await waitForBackendHealth(75);
  reloading = false;
  diagnostics.record("backend_reload", {
    phase: "backend_reload",
    outcome: healthy ? "success" : "failed",
  });
  if (!healthy) {
    if (reloadQueued) {
      // A save arrived while this attempt was starting. It most likely
      // finishes the edit that broke the start, so use it straight away.
      reloadQueued = false;
      console.error(
        "The Quantix service did not start. Retrying with your latest save…",
      );
      await reloadBackend("further changes");
      return;
    }
    scheduleRecovery("retry after a failed reload");
    return;
  }
  retryDelay = 0;
  console.log("Quantix service reloaded.");
  if (reloadQueued) {
    reloadQueued = false;
    await reloadBackend("further changes");
  }
}

async function watchBackend() {
  const directory = path.join(root, "backend", "quantix");
  try {
    sourceWatcher = await watchPythonSources(directory, {
      onChange: (files) => {
        if (stopping) return;
        // Only changed source bytes reset recovery backoff. Metadata touches,
        // cache writes and duplicate save notifications never restart service.
        retryDelay = 0;
        void reloadBackend(
          files.length === 1
            ? files[0]
            : `${files.length} source files changed`,
        );
      },
      onError: (error) =>
        diagnostics.recordError(
          "backend_watch_failed",
          { phase: "backend_watch" },
          error,
        ),
    });
    if (stopping) {
      await sourceWatcher.close();
      return;
    }
    watching = true;
    console.log(
      "Watching backend/quantix — the service reloads when Python source content changes.",
    );
  } catch (error) {
    diagnostics.recordError(
      "backend_watch_failed",
      { phase: "backend_watch" },
      error,
    );
  }
}

process.on("SIGINT", () => {
  diagnostics.record("signal_received", {
    phase: "launcher",
    signal_name: "SIGINT",
  });
  void stop();
});
process.on("SIGTERM", () => {
  diagnostics.record("signal_received", {
    phase: "launcher",
    signal_name: "SIGTERM",
  });
  void stop();
});

const ready = await waitForBackendHealth();
if (!ready) {
  diagnostics.record("startup_failed", {
    phase: "health_check",
    outcome: "timeout",
    duration_ms: 150 * 200,
  });
  console.error(
    "Quantix could not open the local workspace. See the service message above.",
  );
  await stop(1);
} else {
  try {
    // The backend publishes its connection atomically. Do not overwrite it
    // from a second process while the desktop may be reading the record.
    const viteStartedAt = Date.now();
    vite = spawn(
      process.execPath,
      [
        path.join(root, "node_modules/vite/bin/vite.js"),
        "--host",
        "127.0.0.1",
        "--port",
        "1420",
        "--strictPort",
      ],
      {
        cwd: root,
        windowsHide: true,
        stdio: "inherit",
        env: {
          ...process.env,
          VITE_QUANTIX_API_BASE: baseUrl,
          VITE_QUANTIX_API_TOKEN: token,
        },
      },
    );
    diagnostics.record("process_spawned", {
      phase: "vite_spawn",
      process: "vite",
      outcome: "success",
      child_process_id: vite.pid,
    });
    vite.on("error", (error) => {
      diagnostics.recordError(
        "process_error",
        { phase: "vite", process: "vite" },
        error,
      );
      console.error(error.message);
      void stop(1);
    });
    vite.on("exit", (code, signal) => {
      diagnostics.record("process_exit", {
        phase: "vite",
        process: "vite",
        outcome: code === 0 ? "success" : "failed",
        duration_ms: Math.max(0, Date.now() - viteStartedAt),
        ...(Number.isInteger(code) && code >= 0 ? { exit_code: code } : {}),
        ...(signal ? { signal_name: signal } : {}),
      });
      if (!stopping) void stop(code ?? 0);
    });
    diagnostics.record("startup_ready", {
      phase: "launcher",
      outcome: "success",
    });
    console.log(
      "Quantix local workspace is ready. Interface: http://127.0.0.1:1420",
    );
    await watchBackend();
  } catch (error) {
    diagnostics.recordError("startup_failed", { phase: "vite_spawn" }, error);
    await stop(1);
  }
}
