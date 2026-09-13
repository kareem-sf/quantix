import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

// Node's experimental global storage conflicts with jsdom's per-window storage.
// Pass the flag through the environment so Vitest workers inherit it too.
const child = spawn(
  process.execPath,
  [
    fileURLToPath(
      new URL("../node_modules/vitest/vitest.mjs", import.meta.url),
    ),
    "run",
    ...process.argv.slice(2),
  ],
  {
    stdio: "inherit",
    env: {
      ...process.env,
      NODE_OPTIONS: [process.env.NODE_OPTIONS, "--no-experimental-webstorage"]
        .filter(Boolean)
        .join(" "),
    },
  },
);
child.on("error", (error) => {
  console.error(`UI tests could not start: ${error.message}`);
  process.exitCode = 1;
});
child.on("exit", (code) => {
  process.exitCode = code ?? 1;
});
