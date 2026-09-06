import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const executable = fileURLToPath(
  new URL(
    process.platform === "win32"
      ? "../backend/.venv/Scripts/python.exe"
      : "../backend/.venv/bin/python",
    import.meta.url,
  ),
);
const child = spawn(executable, process.argv.slice(2), { stdio: "inherit" });
child.on("error", (error) => {
  console.error(
    `The Quantix Python environment could not start: ${error.message}`,
  );
  process.exitCode = 1;
});
child.on("exit", (code) => {
  process.exitCode = code ?? 1;
});
