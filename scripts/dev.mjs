import { spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import net from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";

const root = fileURLToPath(new URL("../", import.meta.url));
const runtime = path.join(root, ".quantix-dev");
await mkdir(runtime, { recursive: true });
const token = randomBytes(32).toString("hex");
const port = await new Promise((resolve, reject) => {
  const server = net.createServer();
  server.on("error", reject);
  server.listen(0, "127.0.0.1", () => {
    const selected = server.address().port;
    server.close(() => resolve(selected));
  });
});
const baseUrl = `http://127.0.0.1:${port}/api`;
const home =
  process.env.QUANTIX_HOME ||
  path.join(process.env.LOCALAPPDATA || os.homedir(), "Quantix");
const python = path.join(
  root,
  "backend",
  ".venv",
  process.platform === "win32" ? "Scripts/python.exe" : "bin/python",
);
const backend = spawn(
  python,
  ["-m", "quantix", "--home", home, "--port", String(port)],
  {
    cwd: root,
    stdio: "inherit",
    windowsHide: true,
    env: { ...process.env, QUANTIX_SESSION_TOKEN: token },
  },
);
let vite;
let stopping = false;
let backendExited = false;
backend.on("error", (error) => {
  console.error(error.message);
  process.exitCode = 1;
});
backend.on("exit", () => {
  backendExited = true;
  if (!stopping) void stop(1);
});

async function stop(code = 0) {
  if (stopping) return;
  stopping = true;
  vite?.kill();
  try {
    await fetch(`${baseUrl}/shutdown`, {
      method: "POST",
      headers: { Authorization: `Bearer ${token}` },
      signal: AbortSignal.timeout(2000),
    });
  } catch {
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
      await new Promise((resolve) => {
        killer.once("exit", resolve);
        killer.once("error", resolve);
      });
    } else backend.kill();
  }
  process.exitCode = code;
}
process.on("SIGINT", () => void stop());
process.on("SIGTERM", () => void stop());

let ready = false;
for (let attempt = 0; attempt < 150 && !backendExited; attempt++) {
  try {
    const response = await fetch(`http://127.0.0.1:${port}/healthz`, {
      signal: AbortSignal.timeout(1000),
    });
    if (response.ok) {
      ready = true;
      break;
    }
  } catch {
    /* Wait for the owned service to bind its local port. */
  }
  await new Promise((resolve) => setTimeout(resolve, 200));
}
if (!ready) {
  console.error(
    "Quantix could not open the local workspace. See the service message above.",
  );
  await stop(1);
} else {
  await writeFile(
    path.join(runtime, "connection.json"),
    JSON.stringify({ base_url: baseUrl, token }),
    { mode: 0o600 },
  );
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
  vite.on("error", (error) => {
    console.error(error.message);
    void stop(1);
  });
  vite.on("exit", (code) => {
    if (!stopping) void stop(code ?? 0);
  });
  console.log(
    "Quantix local workspace is ready. Interface: http://127.0.0.1:1420",
  );
}
