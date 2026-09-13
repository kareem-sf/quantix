import os from "node:os";
import path from "node:path";

/**
 * Resolve the normal Quantix application-owned storage tree.
 *
 * This helper is deliberately side-effect free. Callers create only the
 * directories they need after startup has been authorized to begin.
 */
export function quantixPaths() {
  const root = path.resolve(os.homedir(), ".quantix");
  const runtime = path.join(root, "runtime");
  return Object.freeze({
    root,
    runtime,
    connectionFile: path.join(runtime, "connection.json"),
    tauriRunningConfig: path.join(runtime, "tauri-running.json"),
    openapiSchema: path.join(runtime, "openapi.json"),
    logs: path.join(root, "logs"),
    tmp: path.join(runtime, "tmp"),
    webview: path.join(runtime, "webview"),
    cache: path.join(root, "cache"),
    aiComponents: path.join(root, "ai-components"),
    aiRuntimes: path.join(root, "ai-runtimes"),
  });
}

/**
 * Keep temporary and cache writes made by launcher dependencies within the
 * same application root. This must run before spawning dependent processes.
 */
export function applyProcessStorage(paths = quantixPaths()) {
  process.env.TMPDIR = paths.tmp;
  process.env.TMP = paths.tmp;
  process.env.TEMP = paths.tmp;
  process.env.UV_CACHE_DIR = path.join(paths.cache, "uv");
  process.env.npm_config_cache = path.join(paths.cache, "npm");
  return paths;
}
