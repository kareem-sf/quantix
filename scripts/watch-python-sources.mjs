/** Watch bundled Python sources by content, never by filesystem metadata alone. */
import { createHash } from "node:crypto";
import { watch } from "node:fs";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";

const ignoredDirectories = new Set([
  "__pycache__",
  ".git",
  ".venv",
  "node_modules",
  ".quantix",
]);

async function snapshot(directory) {
  const hashes = new Map();
  async function visit(folder, relative = "") {
    let entries;
    try {
      entries = await readdir(folder, { withFileTypes: true });
    } catch (error) {
      if (error.code === "ENOENT") return;
      throw error;
    }
    for (const entry of entries) {
      if (entry.isSymbolicLink()) continue;
      const name = relative ? `${relative}/${entry.name}` : entry.name;
      const target = path.join(folder, entry.name);
      if (entry.isDirectory()) {
        if (!ignoredDirectories.has(entry.name)) await visit(target, name);
      } else if (entry.isFile() && entry.name.endsWith(".py")) {
        try {
          hashes.set(
            name,
            createHash("sha256")
              .update(await readFile(target))
              .digest("hex"),
          );
        } catch (error) {
          if (error.code !== "ENOENT") throw error;
        }
      }
    }
  }
  await visit(directory);
  return hashes;
}

export async function watchPythonSources(
  directory,
  { onChange, onError = () => {}, debounceMs = 250 },
) {
  if (
    typeof onChange !== "function" ||
    !Number.isFinite(debounceMs) ||
    debounceMs < 1
  )
    throw new TypeError(
      "Source watching requires a change callback and positive debounce interval.",
    );
  let previous = await snapshot(directory);
  let closed = false,
    timer,
    scanning,
    dirty = false;

  function schedule() {
    if (closed) return;
    dirty = true;
    clearTimeout(timer);
    timer = setTimeout(() => void check(), debounceMs);
  }

  async function check() {
    timer = undefined;
    if (closed || scanning) return;
    dirty = false;
    scanning = (async () => {
      try {
        const current = await snapshot(directory);
        // A newer save landed during hashing; wait for its quiet window and
        // compare the complete final snapshot rather than an intermediate edit.
        if (closed || dirty) return;
        const changed = [...new Set([...previous.keys(), ...current.keys()])]
          .filter((name) => previous.get(name) !== current.get(name))
          .sort();
        previous = current;
        if (changed.length) await onChange(changed);
      } catch (error) {
        if (!closed) onError(error);
      }
    })();
    try {
      await scanning;
    } finally {
      scanning = undefined;
      if (dirty && !closed) schedule();
    }
  }

  const watcher = watch(directory, { recursive: true }, (event, filename) => {
    const name = filename ? String(filename).replaceAll("\\", "/") : "";
    if (name.split("/").some((part) => ignoredDirectories.has(part))) return;
    // Renamed directories may add/delete many modules. A null filename also
    // requires a full source snapshot; non-Python file changes do not.
    if (name && event !== "rename" && !name.endsWith(".py")) return;
    schedule();
  });
  watcher.on("error", (error) => {
    if (!closed) onError(error);
  });
  // Close the gap between the first snapshot and attaching the native watcher.
  schedule();
  return {
    async close() {
      closed = true;
      dirty = false;
      clearTimeout(timer);
      watcher.close();
      await scanning;
    },
  };
}
