import { spawn } from "node:child_process";
import { lstat, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { quantixPaths } from "./paths.mjs";

const project = fileURLToPath(new URL("../", import.meta.url));
const invalid =
  "Quantix cannot verify its pending reset. Keep Quantix closed and contact support to repair the reset record.";

async function pending(root) {
  try {
    const directory = await lstat(root);
    if (!directory.isDirectory() || directory.isSymbolicLink())
      throw new Error(invalid);
    const marker = path.join(root, "pending-reset.json");
    const info = await lstat(marker);
    if (
      !info.isFile() ||
      info.isSymbolicLink() ||
      info.nlink !== 1 ||
      info.size > 8 * 1024 * 1024
    )
      throw new Error(invalid);
    const saved = JSON.parse(await readFile(marker, "utf8"));
    if (
      saved.format !== 1 ||
      saved.home !== root ||
      !/^[0-9a-f]{32}$/.test(saved.reset_id) ||
      !/^[0-9a-f]{64}$/.test(saved.fingerprint) ||
      typeof saved.credentials_cleared !== "boolean" ||
      typeof saved.confirmed_at !== "string" ||
      !saved.confirmed_at ||
      ![
        "cleaning_credentials",
        "credential_error",
        "ready",
        "deleting",
        "failed",
      ].includes(saved.phase) ||
      (["ready", "deleting"].includes(saved.phase) &&
        !saved.credentials_cleared)
    )
      throw new Error(invalid);
    return saved;
  } catch (error) {
    if (error?.code === "ENOENT") return null;
    throw new Error(invalid);
  }
}

async function command(executable, args) {
  await new Promise((resolve, reject) => {
    const child = spawn(executable, args, {
      cwd: project,
      stdio: "inherit",
      windowsHide: true,
    });
    child.once("error", () =>
      reject(
        new Error(
          "Quantix could not start reset recovery. Check the local Rust installation and reopen Quantix.",
        ),
      ),
    );
    child.once("exit", (code) =>
      code === 0
        ? resolve()
        : reject(
            new Error(
              "Quantix could not finish reset recovery. Close its other windows and reopen Quantix.",
            ),
          ),
    );
  });
}

async function runNativePreflight() {
  // A previous desktop binary may predate helper mode. Build the development
  // executable before invoking it; never guess or run an older UI binary.
  // No home temp/cache/log handles are opened by this launcher beforehand.
  const target = path.join(project, "src-tauri", "target");
  await command("cargo", [
    "build",
    "--manifest-path",
    path.join(project, "src-tauri", "Cargo.toml"),
    "--target-dir",
    target,
    "--bin",
    "quantix-desktop",
  ]);
  await command(path.join(target, "debug", "quantix-desktop.exe"), [
    "--quantix-reset-preflight",
  ]);
}

// Dependency inputs let isolated tests exercise launch decisions; no deletion
// implementation or caller-provided deletion path is exposed to the helper.
export async function resetPreflight({
  storage = quantixPaths(),
  platform = process.platform,
  runNative = runNativePreflight,
} = {}) {
  const saved = await pending(storage.root);
  if (!saved) return;
  if (platform !== "win32")
    throw new Error(
      "This pending Quantix reset requires the supported Windows application.",
    );
  if (!["ready", "deleting"].includes(saved.phase)) return;
  await runNative();
  const remaining = await pending(storage.root);
  if (
    remaining &&
    (remaining.reset_id !== saved.reset_id ||
      remaining.phase !== "failed" ||
      !remaining.credentials_cleared)
  ) {
    throw new Error(
      "Quantix could not finish reset recovery. Close its other windows and reopen Quantix.",
    );
  }
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  try {
    await resetPreflight();
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
