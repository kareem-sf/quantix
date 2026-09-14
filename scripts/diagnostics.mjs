import { randomBytes } from "node:crypto";
import {
  closeSync,
  existsSync,
  mkdirSync,
  openSync,
  readdirSync,
  renameSync,
  statSync,
  unlinkSync,
  writeSync,
} from "node:fs";
import path from "node:path";
import { quantixPaths } from "./paths.mjs";

const MAX_FILE_BYTES = 5 * 1024 * 1024;
const BACKUP_COUNT = 2;
const RETENTION_DAYS = 14;
const MAX_TOTAL_BYTES = 100 * 1024 * 1024;
const SAFE_NAME = /^[a-z][a-z0-9_-]{0,48}$/;
const SAFE_CODE = /^[A-Z][A-Z0-9_.-]{0,31}$/;
const SAFE_ERROR_TYPES = new Set([
  "Error",
  "TypeError",
  "RangeError",
  "ReferenceError",
  "SyntaxError",
  "URIError",
  "EvalError",
  "AggregateError",
  "SystemError",
  "Unknown",
]);

function sessionIdentifier() {
  return randomBytes(16).toString("hex");
}

function boundedName(value, fallback) {
  const candidate = typeof value === "string" ? value.toLowerCase() : "";
  return SAFE_NAME.test(candidate) ? candidate : fallback;
}

function boundedText(value, pattern, fallback) {
  if (typeof value !== "string" || value.length > 64 || !pattern.test(value)) {
    return fallback;
  }
  return value;
}

function boundedNumber(value, key) {
  const minimum = key === "exit_code" ? -1_000_000 : 0;
  const maximum = 1_000_000_000_000;
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    return undefined;
  }
  return value;
}

function processAlive(pid) {
  if (!Number.isSafeInteger(pid) || pid <= 0) return true;
  if (pid === process.pid) return true;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    // ESRCH means the process is definitely gone. Access denied and every
    // other uncertain result stay protected from retention deletion.
    return error?.code !== "ESRCH";
  }
}

function parseOwnedFile(name) {
  const match =
    /^quantix-[a-z][a-z0-9_-]{0,48}-(\d+)-([a-f0-9]{32})\.jsonl(?:\.(\d+))?$/.exec(
      name,
    );
  if (!match) return undefined;
  return { pid: Number(match[1]), backup: Number(match[3] || 0) };
}

function retainLogs(directory, currentPath) {
  let entries;
  try {
    entries = readdirSync(directory, { withFileTypes: true });
  } catch {
    return;
  }
  const files = [];
  for (const entry of entries) {
    if (!entry.isFile()) continue;
    const owned = parseOwnedFile(entry.name);
    if (
      !owned ||
      entry.name === path.basename(currentPath) ||
      processAlive(owned.pid)
    )
      continue;
    const fullPath = path.join(directory, entry.name);
    try {
      const stat = statSync(fullPath);
      files.push({ path: fullPath, bytes: stat.size, modified: stat.mtimeMs });
    } catch {
      // A concurrent cleanup or rotation is harmless.
    }
  }
  const cutoff = Date.now() - RETENTION_DAYS * 24 * 60 * 60 * 1000;
  files.sort((left, right) => left.modified - right.modified);
  let total = files.reduce((sum, file) => sum + file.bytes, 0);
  for (const file of files) {
    if (file.modified > cutoff && total <= MAX_TOTAL_BYTES) continue;
    try {
      unlinkSync(file.path);
      total -= file.bytes;
    } catch {
      // Retention is best effort and cannot make app startup fail.
    }
  }
}

function safeFields(fields) {
  if (!fields || typeof fields !== "object") return {};
  const result = {};
  const allowedStrings = new Set([
    "phase",
    "outcome",
    "child",
    "process",
    "error_class",
    "error_code",
    "signal_name",
  ]);
  const allowedNumbers = new Set([
    "child_process_id",
    "exit_code",
    "signal",
    "duration_ms",
    "line",
    "column",
  ]);
  for (const [key, value] of Object.entries(fields)) {
    if (allowedStrings.has(key)) {
      const pattern =
        key === "error_code"
          ? SAFE_CODE
          : key === "signal_name"
            ? /^[A-Z][A-Z0-9_]{0,15}$/
            : SAFE_NAME;
      const bounded =
        key === "error_class"
          ? SAFE_ERROR_TYPES.has(value)
            ? value
            : undefined
          : boundedText(value, pattern, undefined);
      if (bounded !== undefined) result[key] = bounded;
    } else if (allowedNumbers.has(key)) {
      const bounded = boundedNumber(value, key);
      if (bounded !== undefined) result[key] = bounded;
    }
  }
  return result;
}

function errorFields(error) {
  const errorClass = SAFE_ERROR_TYPES.has(error?.name) ? error.name : "Unknown";
  const code =
    typeof error?.code === "string" ? error.code.toUpperCase() : undefined;
  return {
    error_class: errorClass,
    ...(code && SAFE_CODE.test(code) ? { error_code: code } : {}),
  };
}

class DiagnosticLogger {
  #component;
  #sessionId;
  #filePath = undefined;
  #handle = undefined;
  #bytes = 0;
  #failed = false;
  #warned = false;

  constructor(component) {
    this.#component = boundedName(component, "launcher");
    try {
      this.#sessionId = sessionIdentifier();
    } catch {
      this.#sessionId = `${Date.now().toString(16)}${process.pid.toString(16)}`
        .padStart(32, "0")
        .slice(-32);
    }
    try {
      const directory = quantixPaths().logs;
      this.#filePath = path.join(
        directory,
        `quantix-${this.#component}-${process.pid}-${this.#sessionId}.jsonl`,
      );
      mkdirSync(directory, { recursive: true, mode: 0o700 });
      this.#handle = openSync(this.#filePath, "a", 0o600);
      this.#bytes = statSync(this.#filePath).size;
      retainLogs(directory, this.#filePath);
    } catch {
      this.#failed = true;
      this.#warnOnce();
    }
  }

  #warnOnce() {
    if (this.#warned) return;
    this.#warned = true;
    try {
      process.stderr.write("Quantix diagnostic logging is unavailable.\n");
    } catch {
      // A broken stderr must not trigger a logging loop.
    }
  }

  #rotate(nextBytes) {
    if (this.#handle === undefined || this.#bytes + nextBytes <= MAX_FILE_BYTES)
      return;
    try {
      closeSync(this.#handle);
      this.#handle = undefined;
      for (let index = BACKUP_COUNT; index >= 1; index -= 1) {
        const source =
          index === 1 ? this.#filePath : `${this.#filePath}.${index - 1}`;
        const destination = `${this.#filePath}.${index}`;
        if (existsSync(destination)) unlinkSync(destination);
        if (existsSync(source)) renameSync(source, destination);
      }
      this.#handle = openSync(this.#filePath, "a", 0o600);
      this.#bytes = statSync(this.#filePath).size;
    } catch {
      this.#failed = true;
      this.#warnOnce();
    }
  }

  record(event, fields = {}) {
    if (this.#failed || this.#handle === undefined) return;
    const safeEvent = boundedName(event, "diagnostic_event");
    const payload = {
      timestamp: new Date().toISOString(),
      level: fields.level === "error" ? "error" : "info",
      component: this.#component,
      event: safeEvent,
      process_id: process.pid,
      session_id: this.#sessionId,
      ...safeFields(fields),
    };
    const line = `${JSON.stringify(payload)}\n`;
    const bytes = Buffer.byteLength(line, "utf8");
    this.#rotate(bytes);
    if (this.#failed || this.#handle === undefined) return;
    try {
      writeSync(this.#handle, line, undefined, "utf8");
      this.#bytes += bytes;
    } catch {
      this.#failed = true;
      this.#warnOnce();
    }
  }

  recordError(event, fields = {}, error) {
    this.record(event, { ...fields, level: "error", ...errorFields(error) });
  }

  close() {
    if (this.#handle === undefined) return;
    try {
      closeSync(this.#handle);
    } catch {
      this.#warnOnce();
    }
    this.#handle = undefined;
  }
}

export function createDiagnostics(component) {
  return new DiagnosticLogger(component);
}
