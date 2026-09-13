import { readFile } from "node:fs/promises";
import path from "node:path";
import { quantixPaths } from "./paths.mjs";

async function request(connection, route, options = {}) {
  return fetch(`${connection.base_url}${route}`, {
    ...options,
    headers: { Authorization: `Bearer ${connection.token}` },
    signal: AbortSignal.timeout(3000),
  });
}

async function activeWork(connection) {
  const response = await request(connection, "/tenders");
  if (!response.ok)
    throw new Error(
      "Quantix could not read current work before updating. Keep the existing workspace open and try again.",
    );
  const tenders = await response.json();
  if (!Array.isArray(tenders))
    throw new Error("Quantix could not identify current work before updating.");
  for (const tender of tenders) {
    if (typeof tender.id !== "string")
      throw new Error(
        "The existing workspace returned an incomplete Tender record.",
      );
    const runsResponse = await request(
      connection,
      `/tenders/${encodeURIComponent(tender.id)}/runs`,
    );
    if (!runsResponse.ok)
      throw new Error(
        "Quantix could not confirm whether work is running. Stop existing work before updating.",
      );
    const runs = await runsResponse.json();
    if (!Array.isArray(runs))
      throw new Error("Quantix could not confirm the current work state.");
    if (runs.some((run) => ["queued", "running"].includes(run.status)))
      return true;
  }
  const setupResponse = await request(connection, "/ai/setup/accounts");
  if (!setupResponse.ok)
    throw new Error(
      "Quantix could not check existing AI setup. Finish setup or choose Quit in the existing workspace before reopening.",
    );
  const accounts = await setupResponse.json();
  if (!Array.isArray(accounts))
    throw new Error(
      "Quantix could not confirm existing AI setup. Choose Quit in the existing workspace before reopening.",
    );
  return accounts.some((account) => account.active === true);
}

export async function reuseRunningWorkspace(runtime = quantixPaths().runtime) {
  let connection;
  try {
    connection = JSON.parse(
      await readFile(path.join(runtime, "connection.json"), "utf8"),
    );
  } catch (error) {
    if (error?.code === "ENOENT") return false;
    throw new Error(
      "Quantix's local connection record could not be read. Close its existing session before reopening.",
    );
  }
  let target;
  try {
    target = new URL(connection.base_url);
  } catch {
    throw new Error("Quantix's local connection address is invalid.");
  }
  if (
    target.protocol !== "http:" ||
    target.hostname !== "127.0.0.1" ||
    target.pathname !== "/api" ||
    target.username ||
    target.password ||
    target.search ||
    target.hash ||
    typeof connection.token !== "string" ||
    connection.token.length < 32
  ) {
    throw new Error(
      "Quantix's local connection is invalid. Reopen the application from its launcher.",
    );
  }
  let response;
  try {
    response = await request(connection, "/health");
  } catch (error) {
    if (error?.name === "TimeoutError")
      throw new Error(
        "The existing workspace is not responding. Close it before reopening Quantix.",
      );
    return false;
  }
  if (!response.ok)
    throw new Error(
      "The existing workspace could not be opened with this local session. Close it before reopening Quantix.",
    );
  const health = await response.json();
  if (health.reset_pending === true) return true;
  if (
    Number.isInteger(health.ai_setup_revision) &&
    health.ai_setup_revision >= 7 &&
    health.workspace_revision === 2 &&
    health.office_revision === 2
  )
    return true;
  if (await activeWork(connection)) {
    console.warn(
      "Quantix is keeping your running work open. Stop it in Work, then choose Quit and reopen Quantix to finish the workspace update.",
    );
    return true;
  }
  const stopped = await request(connection, "/shutdown", { method: "POST" });
  if (!stopped.ok)
    throw new Error(
      "The older local service could not close. Stop it before reopening Quantix.",
    );
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 250));
    try {
      await request(connection, "/health");
    } catch (error) {
      if (error?.name !== "TimeoutError") return false;
    }
  }
  throw new Error(
    "The older local service is still closing. Wait a moment, then reopen Quantix.",
  );
}
