import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { quantixPaths } from "./paths.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const storage = quantixPaths();
const cli = path.join(root, "node_modules", "openapi-typescript", "bin", "cli.js");
const output = path.join(root, "src", "bindings", "api.ts");

await new Promise((resolve, reject) => {
  const child = spawn(process.execPath, [cli, storage.openapiSchema, "-o", output], {
    cwd: root,
    stdio: "inherit",
    windowsHide: true,
  });
  child.once("error", reject);
  child.once("exit", (code, signal) => {
    if (code === 0) {
      resolve();
      return;
    }
    reject(new Error(signal ? `API binding generation stopped by ${signal}.` : `API binding generation failed with exit code ${code ?? "unknown"}.`));
  });
});
