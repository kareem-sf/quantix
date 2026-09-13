import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { reuseRunningWorkspace } from "./running-workspace.mjs";

const connection = {
  base_url: "http://127.0.0.1:47831/api",
  token: "synthetic-running-workspace-token-0123456789",
};

async function fixture(t, health) {
  const parent = path.resolve("src-tauri/target");
  await mkdir(parent, { recursive: true });
  const root = await mkdtemp(path.join(parent, "running-workspace-test-"));
  const runtime = path.join(root, "runtime");
  await mkdir(runtime, { recursive: true });
  await writeFile(
    path.join(runtime, "connection.json"),
    JSON.stringify(connection),
  );
  const previousFetch = globalThis.fetch;
  const calls = [];
  globalThis.fetch = async (input, options = {}) => {
    const url = new URL(String(input));
    calls.push(url.pathname);
    if (url.pathname === "/api/health")
      return new Response(JSON.stringify(health));
    throw new Error(`Unexpected reuse request: ${url.pathname}`);
  };
  t.after(async () => {
    globalThis.fetch = previousFetch;
    assert.ok(path.resolve(root).startsWith(parent + path.sep));
    await rm(root, { recursive: true, force: true });
  });
  return { runtime, calls };
}

test("compatible workspace revision is reused without probing or shutting it down", async (t) => {
  const { runtime, calls } = await fixture(t, {
    workspace_revision: 2,
    office_revision: 2,
    ai_setup_revision: 7,
    reset_pending: false,
  });

  assert.equal(await reuseRunningWorkspace(runtime), true);
  assert.deepEqual(calls, ["/api/health"]);
});

test("confirmed reset recovery is kept even when ordinary revisions are absent", async (t) => {
  const { runtime, calls } = await fixture(t, { reset_pending: true });

  assert.equal(await reuseRunningWorkspace(runtime), true);
  assert.deepEqual(calls, ["/api/health"]);
});

test("older workspace with active Tender work is kept without shutdown", async (t) => {
  const { runtime, calls } = await fixture(t, {
    workspace_revision: 0,
    office_revision: 0,
    ai_setup_revision: 6,
    reset_pending: false,
  });
  globalThis.fetch = async (input, options = {}) => {
    const url = new URL(String(input));
    calls.push(url.pathname);
    if (url.pathname === "/api/health") {
      return new Response(
        JSON.stringify({
          workspace_revision: 0,
          office_revision: 0,
          ai_setup_revision: 6,
          reset_pending: false,
        }),
      );
    }
    if (url.pathname === "/api/tenders") {
      return new Response(JSON.stringify([{ id: "synthetic-tender" }]));
    }
    if (url.pathname === "/api/tenders/synthetic-tender/runs") {
      return new Response(JSON.stringify([{ status: "running" }]));
    }
    throw new Error(`Unexpected shutdown request: ${url.pathname}`);
  };

  assert.equal(await reuseRunningWorkspace(runtime), true);
  assert.deepEqual(calls, [
    "/api/health",
    "/api/tenders",
    "/api/tenders/synthetic-tender/runs",
  ]);
});

test("older idle workspace is shut down before a new launch", async (t) => {
  const { runtime, calls } = await fixture(t, {
    workspace_revision: 0,
    office_revision: 0,
    ai_setup_revision: 6,
    reset_pending: false,
  });
  let healthReads = 0;
  globalThis.fetch = async (input, options = {}) => {
    const url = new URL(String(input));
    calls.push(url.pathname);
    if (url.pathname === "/api/health") {
      healthReads += 1;
      if (healthReads > 1) throw new Error("Synthetic service closed");
      return new Response(
        JSON.stringify({
          workspace_revision: 0,
          office_revision: 0,
          ai_setup_revision: 6,
          reset_pending: false,
        }),
      );
    }
    if (["/api/tenders", "/api/ai/setup/accounts"].includes(url.pathname))
      return new Response(JSON.stringify([]));
    if (url.pathname === "/api/shutdown" && options.method === "POST") {
      return new Response("", { status: 200 });
    }
    throw new Error(`Unexpected request: ${url.pathname}`);
  };

  assert.equal(await reuseRunningWorkspace(runtime), false);
  assert.deepEqual(calls, [
    "/api/health",
    "/api/tenders",
    "/api/ai/setup/accounts",
    "/api/shutdown",
    "/api/health",
  ]);
});

test("older workspace keeps an active AI setup operation", async (t) => {
  const { runtime, calls } = await fixture(t, { ai_setup_revision: 6 });
  globalThis.fetch = async (input) => {
    const route = new URL(String(input)).pathname;
    calls.push(route);
    if (route === "/api/health") return Response.json({ ai_setup_revision: 6 });
    if (route === "/api/tenders") return Response.json([]);
    if (route === "/api/ai/setup/accounts")
      return Response.json([{ active: true }]);
    throw new Error("An active setup must not be shut down");
  };
  assert.equal(await reuseRunningWorkspace(runtime), true);
  assert.equal(calls.includes("/api/shutdown"), false);
});
