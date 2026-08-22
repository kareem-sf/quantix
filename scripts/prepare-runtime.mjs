import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  chmodSync,
  copyFileSync,
  existsSync,
  lstatSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const CODEX_VERSION = "0.147.0";
const UV_VERSION = "0.12.2";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const runtimeRoot = path.join(root, "src-tauri", "runtime");
const runtimeBin = path.join(runtimeRoot, "bin");
const developmentRoot = path.join(root, ".dev", "runtime-provisioning");
const extension = process.platform === "win32" ? ".exe" : "";
const codexDestination = path.join(runtimeBin, `codex${extension}`);
const uvDestination = path.join(runtimeBin, `uv${extension}`);
const codexSchema = path.join(
  runtimeRoot,
  "codex_app_server_protocol.schemas.json",
);
const ocrProject = path.join(runtimeRoot, "ocr");
const provenance = path.join(runtimeRoot, "runtime-provenance.json");

mkdirSync(runtimeBin, { recursive: true });
mkdirSync(developmentRoot, { recursive: true });

const codexSource = resolveCodexExecutable();
requireVersion(codexSource, CODEX_VERSION, "@openai/codex");
stageExecutable(codexSource, codexDestination);
requireVersion(codexDestination, CODEX_VERSION, "staged Codex");

const uvSource = await resolveVerifiedUvExecutable();
requireVersion(uvSource, UV_VERSION, "verified uv release artifact");
stageExecutable(uvSource, uvDestination);
requireVersion(uvDestination, UV_VERSION, "staged uv");

verifyCodexSchema();
writeRuntimeProvenance();

function resolveCodexExecutable() {
  const targets = {
    "darwin-arm64": ["@openai/codex-darwin-arm64", "aarch64-apple-darwin"],
    "darwin-x64": ["@openai/codex-darwin-x64", "x86_64-apple-darwin"],
    "linux-arm64": ["@openai/codex-linux-arm64", "aarch64-unknown-linux-musl"],
    "linux-x64": ["@openai/codex-linux-x64", "x86_64-unknown-linux-musl"],
    "win32-arm64": ["@openai/codex-win32-arm64", "aarch64-pc-windows-msvc"],
    "win32-x64": ["@openai/codex-win32-x64", "x86_64-pc-windows-msvc"],
  };
  const target = targets[`${process.platform}-${process.arch}`];
  if (!target) {
    throw new Error(
      `Codex has no approved binary for ${process.platform}-${process.arch}`,
    );
  }
  const [packageName, triple] = target;
  const require = createRequire(import.meta.url);
  const packageJson = require.resolve(`${packageName}/package.json`);
  return path.join(
    path.dirname(packageJson),
    "vendor",
    triple,
    "bin",
    `codex${extension}`,
  );
}

async function resolveVerifiedUvExecutable() {
  const targets = {
    "darwin-arm64": [
      "aarch64-apple-darwin",
      "tar.gz",
      "fa909fea3bc06f460db79017030a221fdbc43ec4478f089cb554d8335c090817",
    ],
    "linux-x64": [
      "x86_64-unknown-linux-gnu",
      "tar.gz",
      "d66e96b5f1ca3b99806eee283a8125d33a0bd669e6e6d9bc4ab7ffda63c41bf4",
    ],
    "win32-x64": [
      "x86_64-pc-windows-msvc",
      "zip",
      "01442d8ce5c7124151a73e697c836d252c6da853c18c73206d3cc4c2378a91d2",
    ],
  };
  const target = targets[`${process.platform}-${process.arch}`];
  if (!target) {
    throw new Error(
      `uv has no approved binary for ${process.platform}-${process.arch}`,
    );
  }
  const [triple, archiveExtension, approvedArchiveSha256] = target;
  const archiveName = `uv-${triple}.${archiveExtension}`;
  const release = `https://github.com/astral-sh/uv/releases/download/${UV_VERSION}`;
  const cache = path.join(developmentRoot, "cache");
  const archive = path.join(cache, archiveName);
  mkdirSync(cache, { recursive: true });
  if (!existsSync(archive) || digestFile(archive) !== approvedArchiveSha256) {
    await download(`${release}/${archiveName}`, archive);
  }
  requireDigest(archive, approvedArchiveSha256, `uv ${archiveName}`);

  const extracted = path.join(developmentRoot, "extracted");
  rmSync(extracted, { recursive: true, force: true });
  mkdirSync(extracted, { recursive: true });
  const result = spawnSync("tar", ["-xf", archive, "-C", extracted], {
    encoding: "utf8",
    windowsHide: true,
  });
  if (result.status !== 0) {
    throw new Error(
      `Could not extract the verified uv artifact: ${result.stderr || "tar failed"}`,
    );
  }
  const nested = path.join(extracted, `uv-${triple}`, `uv${extension}`);
  const flat = path.join(extracted, `uv${extension}`);
  return existsSync(nested) ? nested : flat;
}

async function download(url, destination) {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Could not download ${url}: HTTP ${response.status}`);
  }
  const staging = `${destination}.staging`;
  writeFileSync(staging, Buffer.from(await response.arrayBuffer()));
  renameSync(staging, destination);
}

function verifyCodexSchema() {
  const generatedRoot = path.join(developmentRoot, "codex-schema");
  rmSync(generatedRoot, { recursive: true, force: true });
  mkdirSync(generatedRoot, { recursive: true });
  const result = spawnSync(
    codexDestination,
    ["app-server", "generate-json-schema", "--out", generatedRoot],
    { encoding: "utf8", windowsHide: true },
  );
  if (result.status !== 0) {
    throw new Error(
      `The staged Codex could not export its schema: ${result.stderr || "schema export failed"}`,
    );
  }
  const generated = path.join(
    generatedRoot,
    "codex_app_server_protocol.schemas.json",
  );
  const approvedDigest = digestFile(codexSchema);
  requireDigest(
    generated,
    approvedDigest,
    "Codex schema generated by the staged executable",
  );
}

function writeRuntimeProvenance() {
  const platform =
    { win32: "windows", darwin: "macos" }[process.platform] ?? process.platform;
  const architecture =
    { x64: "x86_64", arm64: "aarch64" }[process.arch] ?? process.arch;
  const manifest = {
    schema_version: 3,
    platform,
    architecture,
    codex: { version: CODEX_VERSION, sha256: digestFile(codexDestination) },
    uv: { version: UV_VERSION, sha256: digestFile(uvDestination) },
    codex_schema_sha256: digestFile(codexSchema),
    ocr_project_files: collectHashedFiles(ocrProject),
  };
  const staging = `${provenance}.staging`;
  const serialized = `${JSON.stringify(manifest, null, 2)}\n`;
  if (
    existsSync(provenance) &&
    readFileSync(provenance, "utf8") === serialized
  ) {
    return;
  }
  writeFileSync(staging, serialized, "utf8");
  rmSync(provenance, { force: true });
  renameSync(staging, provenance);
}

function collectHashedFiles(rootDirectory) {
  const files = [];
  const visit = (directory) => {
    for (const name of readdirSync(directory).sort()) {
      const absolute = path.join(directory, name);
      const metadata = lstatSync(absolute);
      if (metadata.isSymbolicLink()) {
        throw new Error(`Runtime resources cannot contain links: ${absolute}`);
      }
      if (metadata.isDirectory()) {
        visit(absolute);
      } else if (metadata.isFile()) {
        files.push({
          path: path
            .relative(rootDirectory, absolute)
            .split(path.sep)
            .join("/"),
          size_bytes: metadata.size,
          sha256: digestFile(absolute),
        });
      } else {
        throw new Error(`Unsupported runtime resource: ${absolute}`);
      }
    }
  };
  visit(rootDirectory);
  if (files.length === 0) {
    throw new Error(`Runtime resource directory is empty: ${rootDirectory}`);
  }
  return files.sort((left, right) => left.path.localeCompare(right.path));
}

function stageExecutable(source, destination) {
  if (
    existsSync(destination) &&
    digestFile(source) === digestFile(destination)
  ) {
    return;
  }
  const staging = `${destination}.staging`;
  copyFileSync(source, staging);
  if (process.platform !== "win32") {
    chmodSync(staging, 0o755);
  }
  rmSync(destination, { force: true });
  renameSync(staging, destination);
}

function requireVersion(executable, expected, label) {
  const actual = readVersion(executable);
  if (actual !== expected) {
    throw new Error(
      `${label} returned ${actual ?? "no version"}; expected ${expected}`,
    );
  }
}

function readVersion(executable) {
  if (!existsSync(executable)) {
    return null;
  }
  const result = spawnSync(executable, ["--version"], {
    encoding: "utf8",
    windowsHide: true,
  });
  if (result.status !== 0) {
    return null;
  }
  return result.stdout.match(/\b\d+\.\d+\.\d+\b/)?.[0] ?? null;
}

function requireDigest(file, expected, label) {
  const actual = digestFile(file);
  if (actual !== expected) {
    throw new Error(`${label} has SHA-256 ${actual}; expected ${expected}`);
  }
}

function digestFile(file) {
  if (!existsSync(file)) {
    throw new Error(`Required runtime file is missing: ${file}`);
  }
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}
