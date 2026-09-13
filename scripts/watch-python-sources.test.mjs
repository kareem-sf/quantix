import assert from "node:assert/strict";
import {
  mkdir,
  mkdtemp,
  readFile,
  rename,
  rm,
  utimes,
  writeFile,
} from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { watchPythonSources } from "./watch-python-sources.mjs";

const pause = (milliseconds) =>
  new Promise((resolve) => setTimeout(resolve, milliseconds));
async function until(condition) {
  const deadline = Date.now() + 4000;
  while (!condition()) {
    if (Date.now() > deadline)
      assert.fail("Timed out waiting for the real filesystem watcher.");
    await pause(20);
  }
}

async function fixture(t) {
  const parent = path.resolve(
    os.homedir(),
    ".quantix/cache/development/watcher-tests",
  );
  await mkdir(parent, { recursive: true });
  const directory = await mkdtemp(path.join(parent, "sources-"));
  const file = path.join(directory, "module.py");
  await writeFile(file, "value = 1\n");
  const calls = [],
    errors = [];
  const watcher = await watchPythonSources(directory, {
    debounceMs: 40,
    onChange: (files) => calls.push(files),
    onError: (error) => errors.push(error),
  });
  t.after(async () => {
    await watcher.close();
    assert.equal(path.dirname(path.resolve(directory)), parent);
    await rm(directory, { recursive: true, force: true });
    assert.deepEqual(errors, []);
  });
  return { directory, file, calls, watcher };
}

test("metadata touches and same-byte writes do not reload Python", async (t) => {
  const { directory, file, calls } = await fixture(t);
  const original = await readFile(file);
  await utimes(file, new Date(), new Date());
  await writeFile(file, original);
  const replacement = path.join(directory, "editor-save.tmp");
  await writeFile(replacement, original);
  await rename(replacement, file);
  await writeFile(
    path.join(directory, "runtime-data.json"),
    '{"changed":true}',
  );
  await mkdir(path.join(directory, "__pycache__"));
  await writeFile(
    path.join(directory, "__pycache__", "ignored.py"),
    "ignored = True\n",
  );
  await pause(250);
  assert.deepEqual(calls, []);
});

test("changed bytes debounce into one source change callback", async (t) => {
  const { file, calls } = await fixture(t);
  await writeFile(file, "value = 2\n");
  await writeFile(file, "value = 3\n");
  await until(() => calls.length === 1);
  assert.deepEqual(calls, [["module.py"]]);
  await writeFile(file, "value = 3\n");
  await utimes(file, new Date(), new Date());
  await pause(250);
  assert.equal(calls.length, 1);
});

test("new and deleted Python files are detected, including new directories", async (t) => {
  const { directory, calls } = await fixture(t);
  const nested = path.join(directory, "new_package");
  await mkdir(nested);
  const file = path.join(nested, "added.py");
  await writeFile(file, "added = True\n");
  await until(() => calls.length === 1);
  assert.deepEqual(calls[0], ["new_package/added.py"]);
  await rm(file);
  await until(() => calls.length === 2);
  assert.deepEqual(calls[1], ["new_package/added.py"]);
});

test("close clears queued work and the native watcher", async (t) => {
  const { file, calls, watcher } = await fixture(t);
  await writeFile(file, "value = 2\n");
  await watcher.close();
  await watcher.close();
  await writeFile(file, "value = 3\n");
  await pause(250);
  assert.deepEqual(calls, []);
});
