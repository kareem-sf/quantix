import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, readFile, rm } from "node:fs/promises";
import path from "node:path";
import { resetPreflight } from "./reset-preflight.mjs";

async function fixture(t, phase = "ready") {
  const parent = path.resolve("src-tauri/target");
  await mkdir(parent, { recursive: true });
  const root = await mkdtemp(path.join(parent, "reset-script-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const saved = {
    format: 1,
    reset_id: "0123456789abcdef0123456789abcdef",
    home: root,
    phase,
    credentials_cleared: !["credential_error", "cleaning_credentials"].includes(
      phase,
    ),
    fingerprint: "0123456789abcdef".repeat(4),
    confirmed_at: "2026-09-09T12:00:00Z",
    detail: "Confirmed reset.",
  };
  await writeFile(path.join(root, "pending-reset.json"), JSON.stringify(saved));
  return { root, saved };
}

test("confirmed reset preflight completes before launch and receives no delete path", async (t) => {
  const { root } = await fixture(t);
  const calls = [];
  await resetPreflight({
    storage: { root },
    platform: "win32",
    runNative: async (...args) => {
      calls.push(args);
      await rm(path.join(root, "pending-reset.json"));
    },
  });
  assert.deepEqual(calls, [[]]);
});

test("failed and credential recovery retain the journal for explicit retry", async (t) => {
  for (const phase of ["failed", "credential_error", "cleaning_credentials"]) {
    const { root, saved } = await fixture(t, phase);
    await resetPreflight({
      storage: { root },
      platform: "win32",
      runNative: async () => {
        throw new Error("Unexpected cleanup");
      },
    });
    assert.deepEqual(
      JSON.parse(await readFile(path.join(root, "pending-reset.json"), "utf8")),
      saved,
    );
  }
});

test("unsafe or unsupported pending reset fails closed without native cleanup", async (t) => {
  const { root, saved } = await fixture(t);
  const runNative = async () => {
    throw new Error("Unexpected cleanup");
  };
  await assert.rejects(
    resetPreflight({ storage: { root }, platform: "linux", runNative }),
    /Windows/,
  );
  await writeFile(
    path.join(root, "pending-reset.json"),
    JSON.stringify({ ...saved, home: path.dirname(root) }),
  );
  await assert.rejects(
    resetPreflight({ storage: { root }, platform: "win32", runNative }),
    /verify/,
  );
});

test("native cleanup failure can return only into durable failed recovery", async (t) => {
  const { root, saved } = await fixture(t);
  await resetPreflight({
    storage: { root },
    platform: "win32",
    runNative: async () => {
      await writeFile(
        path.join(root, "pending-reset.json"),
        JSON.stringify({ ...saved, phase: "failed" }),
      );
    },
  });
  assert.equal(
    JSON.parse(await readFile(path.join(root, "pending-reset.json"), "utf8"))
      .phase,
    "failed",
  );
  await writeFile(path.join(root, "pending-reset.json"), JSON.stringify(saved));
  await assert.rejects(
    resetPreflight({
      storage: { root },
      platform: "win32",
      runNative: async () => {},
    }),
    /finish/,
  );
});
