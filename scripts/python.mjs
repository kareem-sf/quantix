// Run the service's virtual-environment Python with the given arguments, from the repository root.
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

export const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

export function servicePython() {
  const venv = path.join(root, "service", ".venv");
  const python = process.platform === "win32" ? path.join(venv, "Scripts", "python.exe") : path.join(venv, "bin", "python");
  if (!existsSync(python)) {
    throw new Error(`No service environment at ${venv}. See README.md to create it.`);
  }
  return python;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const result = spawnSync(servicePython(), process.argv.slice(2), { cwd: root, stdio: "inherit" });
  process.exit(result.status ?? 1);
}
