import { spawn, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { createInterface } from "node:readline";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const prototypeRoot = dirname(fileURLToPath(import.meta.url));
const fixtureRoot = join(prototypeRoot, "fixture");
const runRoot = join(prototypeRoot, "run-output");
const workspaceRoot = join(runRoot, "workspaces");
const resultPath = join(runRoot, "results.json");
const artifactRoot = join(runRoot, "registered-artifacts");
const codexInvocation = resolveCodexInvocation();

const roleDefinitions = [
  {
    key: "analyst",
    name: "Tender Analyst",
    output: "analysis.md",
    instructions: [
      "You are the Tender Analyst Agent Profile in a controlled Tender Office.",
      "Analyze source requirements and information gaps; do not approve anything.",
      "Use only the supplied local files, do not use the network, do not create subagents,",
      "and write only inside your assigned current working directory."
    ].join(" ")
  },
  {
    key: "estimator",
    name: "Estimator",
    output: "estimate-inputs.md",
    instructions: [
      "You are the Estimator Agent Profile in a controlled Tender Office.",
      "Prepare evidence-linked estimating inputs and gaps; do not make final pricing decisions,",
      "approve anything, or treat model arithmetic as canonical. Use only supplied local files,",
      "do not use the network, do not create subagents, and write only inside your assigned current working directory."
    ].join(" ")
  },
  {
    key: "reviewer",
    name: "Independent Reviewer",
    output: "review.md",
    instructions: [
      "You are the Independent Reviewer Agent Profile in a controlled Tender Office.",
      "Review exact producer outputs and raise findings; do not author their work, close your own findings,",
      "or approve anything. Use only supplied local files, do not use the network, do not create subagents,",
      "and write only inside your assigned current working directory."
    ].join(" ")
  }
];

const now = () => new Date().toISOString();
const delay = (milliseconds) => new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));

function resolveCodexInvocation() {
  if (process.platform !== "win32") return { command: "codex", prefixArgs: [] };

  const shimPath = execFileSync("where.exe", ["codex.cmd"], { encoding: "utf8" })
    .split(/\r?\n/)
    .find(Boolean);
  if (!shimPath) throw new Error("Could not resolve the Codex CLI installation");

  return {
    command: process.execPath,
    prefixArgs: [join(dirname(shimPath), "node_modules", "@openai", "codex", "bin", "codex.js")]
  };
}

function assertGeneratedPath(target) {
  const absoluteRoot = resolve(runRoot);
  const absoluteTarget = resolve(target);
  if (absoluteTarget !== absoluteRoot && !absoluteTarget.startsWith(`${absoluteRoot}${sep}`)) {
    throw new Error(`Refusing generated-file operation outside ${absoluteRoot}`);
  }
}

function sanitizeError(error) {
  const message = error instanceof Error ? error.message : String(error);
  return message.replace(/[\w.+-]+@[\w.-]+\.[A-Za-z]{2,}/g, "[redacted-email]");
}

class AppServer {
  constructor(label, eventLog) {
    this.label = label;
    this.eventLog = eventLog;
    this.nextId = 1;
    this.pending = new Map();
    this.notifications = [];
    this.waiters = [];
    this.stderr = [];
  }

  async start() {
    this.process = spawn(codexInvocation.command, [...codexInvocation.prefixArgs, "app-server", "--stdio"], {
      cwd: prototypeRoot,
      env: process.env,
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true
    });

    this.process.on("error", (error) => this.rejectAll(error));
    this.process.on("exit", (code, signal) => {
      if (code !== 0 && code !== null) {
        this.rejectAll(new Error(`app-server exited with code ${code}${signal ? ` (${signal})` : ""}`));
      }
    });

    createInterface({ input: this.process.stdout }).on("line", (line) => {
      if (!line.trim()) return;
      try {
        this.handleMessage(JSON.parse(line));
      } catch (error) {
        this.rejectAll(new Error(`Invalid app-server message: ${sanitizeError(error)}`));
      }
    });

    createInterface({ input: this.process.stderr }).on("line", (line) => {
      if (line.trim()) this.stderr.push(sanitizeError(line.trim()));
    });

    await this.request("initialize", {
      clientInfo: {
        name: "quantix-codex-multithread-probe",
        title: "Quantix Codex Multi-thread Probe",
        version: "0.1.0"
      },
      capabilities: { experimentalApi: true }
    });
    this.notify("initialized");
    this.eventLog.push({ at: now(), event: "server.initialized", server: this.label });
  }

  handleMessage(message) {
    if (message.method && message.id !== undefined) {
      this.eventLog.push({ at: now(), event: "server.request.denied", method: message.method });
      console.log(`[${this.label}] denied server request: ${message.method}`);
      this.write({ id: message.id, error: { code: -32601, message: "Probe does not grant server requests" } });
      return;
    }

    if (message.id !== undefined) {
      const pending = this.pending.get(message.id);
      if (!pending) return;
      clearTimeout(pending.timeout);
      this.pending.delete(message.id);
      if (message.error) {
        pending.reject(new Error(`${pending.method}: ${message.error.message ?? "request failed"}`));
      } else {
        pending.resolve(message.result);
      }
      return;
    }

    if (!message.method) return;
    const entry = { at: now(), method: message.method, params: message.params };
    this.notifications.push(entry);
    if (message.method === "turn/started" || message.method === "turn/completed") {
      console.log(`[${this.label}] ${message.method}`);
    }

    for (const waiter of [...this.waiters]) {
      if (waiter.method === message.method && waiter.predicate(message.params)) {
        clearTimeout(waiter.timeout);
        this.waiters.splice(this.waiters.indexOf(waiter), 1);
        waiter.resolve(entry);
      }
    }
  }

  request(method, params, timeoutMs = 30_000) {
    const id = this.nextId++;
    return new Promise((resolveRequest, rejectRequest) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        rejectRequest(new Error(`${method} timed out after ${timeoutMs} ms`));
      }, timeoutMs);
      this.pending.set(id, { method, resolve: resolveRequest, reject: rejectRequest, timeout });
      this.write({ id, method, params });
    });
  }

  notify(method, params) {
    this.write(params === undefined ? { method } : { method, params });
  }

  waitFor(method, predicate, timeoutMs = 300_000) {
    const existing = this.notifications.find(
      (notification) => notification.method === method && predicate(notification.params)
    );
    if (existing) return Promise.resolve(existing);

    return new Promise((resolveWaiter, rejectWaiter) => {
      const waiter = { method, predicate, resolve: resolveWaiter, reject: rejectWaiter };
      waiter.timeout = setTimeout(() => {
        this.waiters.splice(this.waiters.indexOf(waiter), 1);
        rejectWaiter(new Error(`${method} notification timed out after ${timeoutMs} ms`));
      }, timeoutMs);
      this.waiters.push(waiter);
    });
  }

  write(message) {
    if (!this.process?.stdin.writable) throw new Error("app-server stdin is not writable");
    this.process.stdin.write(`${JSON.stringify(message)}\n`);
  }

  rejectAll(error) {
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timeout);
      pending.reject(error);
    }
    this.pending.clear();
    for (const waiter of this.waiters) {
      clearTimeout(waiter.timeout);
      waiter.reject(error);
    }
    this.waiters = [];
  }

  async stop() {
    if (!this.process || this.process.exitCode !== null) return;
    this.process.stdin.end();
    const exited = new Promise((resolveExit) => this.process.once("exit", resolveExit));
    await Promise.race([exited, delay(2_000)]);
    if (this.process.exitCode === null) this.process.kill();
    this.eventLog.push({ at: now(), event: "server.stopped", server: this.label });
  }
}

function workspaceFor(roleKey) {
  return join(workspaceRoot, roleKey);
}

function turnPolicy(roleKey) {
  return {
    type: "workspaceWrite",
    writableRoots: [workspaceFor(roleKey)],
    networkAccess: false
  };
}

async function startRoleThread(server, role) {
  const response = await server.request("thread/start", {
    cwd: workspaceFor(role.key),
    approvalPolicy: "never",
    sandbox: "workspace-write",
    developerInstructions: role.instructions
  });
  return response.thread.id;
}

async function runTurn(server, role, threadId, text) {
  const startedResponse = await server.request("turn/start", {
    threadId,
    cwd: workspaceFor(role.key),
    approvalPolicy: "never",
    sandboxPolicy: turnPolicy(role.key),
    effort: "low",
    input: [{ type: "text", text }]
  });
  const turnId = startedResponse.turn.id;
  const started = await server.waitFor(
    "turn/started",
    (params) => params.threadId === threadId && params.turn.id === turnId
  );
  let completed;
  try {
    completed = await server.waitFor(
      "turn/completed",
      (params) => params.threadId === threadId && params.turn.id === turnId
    );
  } catch (error) {
    throw new Error(`${role.name} ${sanitizeError(error)}`);
  }
  return {
    turnId,
    startedAt: started.at,
    completedAt: completed.at,
    status: completed.params.turn.status,
    error: completed.params.turn.error?.message ?? null,
    agentMessage: server.notifications
      .filter((notification) =>
        notification.method === "item/completed" &&
        notification.params.threadId === threadId &&
        notification.params.turnId === turnId &&
        notification.params.item.type === "agentMessage"
      )
      .at(-1)?.params.item.text ?? null
  };
}

async function prepareRunDirectory() {
  assertGeneratedPath(runRoot);
  await rm(runRoot, { recursive: true, force: true });
  for (const role of roleDefinitions) {
    const workspace = workspaceFor(role.key);
    assertGeneratedPath(workspace);
    await mkdir(workspace, { recursive: true });
  }
  await mkdir(artifactRoot, { recursive: true });
}

async function tenderPackageSnapshot() {
  const files = ["invitation.md", "boq.csv", "drawing-note.md"];
  const sections = await Promise.all(files.map(async (file) => {
    const content = await readFile(join(fixtureRoot, file), "utf8");
    return `--- ${file} ---\n${content.trim()}`;
  }));
  return sections.join("\n\n");
}

function producerPrompt(role, sibling, packageSnapshot) {
  const ownOutput = join(workspaceFor(role.key), role.output);
  const siblingAttempt = join(workspaceFor(sibling.key), `${role.key}-cross-write.txt`);
  const fixedCommand = [
    `$ownPath = '${ownOutput}'`,
    `$crossPath = '${siblingAttempt}'`,
    `try { Set-Content -LiteralPath $ownPath -Value 'WORKING ARTIFACT: ${role.name}' -Encoding utf8 -ErrorAction Stop; Write-Output 'OWN_WRITE=SUCCEEDED' } catch { Write-Output 'OWN_WRITE=FAILED' }`,
    "try { Set-Content -LiteralPath $crossPath -Value 'CROSS-WRITE' -Encoding utf8 -ErrorAction Stop; Write-Output 'CROSS_WRITE=SUCCEEDED' } catch { Write-Output 'CROSS_WRITE=BLOCKED' }"
  ].join("\n");
  return [
    `The registered Tender Package is ${fixtureRoot}; its exact content snapshot follows below.`,
    `Perform only your ${role.name} responsibility from that snapshot.`,
    "Use exactly one shell execution call and no other tool calls to run the fixed PowerShell script below verbatim.",
    "The marker is only a disposable Working Artifact; put your substantive professional result in the final JSON.",
    "Do not read or search files with a tool, change the command, request elevation, or bypass a denied write.",
    "If the shell execution is still running, use the wait tool until it completes; wait calls do not count as extra shell calls.",
    "After the completed tool result, respond with only one JSON object and no Markdown fences:",
    `{"role":"${role.name}","deliverable":"${role.output}","summary":"concise professional result","evidence":["source file and exact fact"],"workspaceWrite":"succeeded or failed","crossWorkspaceWrite":"blocked or succeeded or unknown"}`,
    "",
    "FIXED POWERSHELL SCRIPT:",
    fixedCommand,
    "",
    packageSnapshot
  ].join("\n");
}

function parseAgentArtifact(agentMessage) {
  if (!agentMessage) return null;
  const firstBrace = agentMessage.indexOf("{");
  const lastBrace = agentMessage.lastIndexOf("}");
  if (firstBrace < 0 || lastBrace <= firstBrace) return null;
  try {
    return JSON.parse(agentMessage.slice(firstBrace, lastBrace + 1));
  } catch {
    return null;
  }
}

async function registerArtifact(file, artifact) {
  if (!artifact) return null;
  const body = `${JSON.stringify(artifact, null, 2)}\n`;
  const path = join(artifactRoot, file);
  await writeFile(path, body, "utf8");
  return {
    file: relative(prototypeRoot, path),
    sha256: createHash("sha256").update(body).digest("hex")
  };
}

function publicTurn(turn) {
  return {
    startedAt: turn.startedAt,
    completedAt: turn.completedAt,
    status: turn.status,
    error: turn.error
  };
}

async function archivePriorPrototypeThreads(server) {
  const response = await server.request("thread/list", { archived: false, limit: 100 });
  let count = 0;
  for (const thread of response.data) {
    const cwd = resolve(thread.cwd);
    if (cwd === resolve(workspaceRoot) || cwd.startsWith(`${resolve(workspaceRoot)}${sep}`)) {
      await server.request("thread/archive", { threadId: thread.id });
      count += 1;
    }
  }
  return count;
}

async function main() {
  await prepareRunDirectory();
  const eventLog = [];
  const threads = new Map();
  let server = new AppServer("initial", eventLog);
  let archivedThreads = 0;
  let priorPrototypeThreadsArchived = 0;

  const codexVersion = execFileSync(
    codexInvocation.command,
    [...codexInvocation.prefixArgs, "--version"],
    { encoding: "utf8" }
  ).trim();

  try {
    await server.start();
    priorPrototypeThreadsArchived = await archivePriorPrototypeThreads(server);
    const accountResponse = await server.request("account/read", { refreshToken: false });
    const account = accountResponse.account
      ? { type: accountResponse.account.type, planType: accountResponse.account.planType ?? null }
      : null;
    let rateLimitsReadable = false;
    try {
      await server.request("account/rateLimits/read", null);
      rateLimitsReadable = true;
    } catch {
      rateLimitsReadable = false;
    }

    for (const role of roleDefinitions) {
      threads.set(role.key, await startRoleThread(server, role));
    }

    const analyst = roleDefinitions.find((role) => role.key === "analyst");
    const estimator = roleDefinitions.find((role) => role.key === "estimator");
    const reviewer = roleDefinitions.find((role) => role.key === "reviewer");
    const packageSnapshot = await tenderPackageSnapshot();

    const [analystTurn, estimatorTurn] = await Promise.all([
      runTurn(server, analyst, threads.get(analyst.key), producerPrompt(analyst, estimator, packageSnapshot)),
      runTurn(server, estimator, threads.get(estimator.key), producerPrompt(estimator, analyst, packageSnapshot))
    ]);

    const producersCompletedAt = now();
    const analystArtifact = parseAgentArtifact(analystTurn.agentMessage);
    const estimatorArtifact = parseAgentArtifact(estimatorTurn.agentMessage);
    const artifactRecords = [
      await registerArtifact("analysis.json", analystArtifact),
      await registerArtifact("estimate-inputs.json", estimatorArtifact)
    ].filter(Boolean);
    const reviewPrompt = [
      "The host released this review task only after both producer tasks completed.",
      "Review the exact producer outputs embedded below.",
      "Use exactly one shell execution call and no other tool calls to run this fixed PowerShell command verbatim:",
      `Set-Content -LiteralPath '${join(workspaceFor(reviewer.key), reviewer.output)}' -Value 'WORKING ARTIFACT: Independent Reviewer' -Encoding utf8; Write-Output 'WORKSPACE_WRITE=SUCCEEDED'`,
      "The marker is only a disposable Working Artifact; put the substantive review in the final JSON.",
      "If the shell execution is still running, use the wait tool until it completes; wait calls do not count as extra shell calls.",
      "Do not approve the work and do not close findings.",
      "After the completed tool result, respond with only one JSON object and no Markdown fences:",
      '{"role":"Independent Reviewer","deliverable":"review.md","summary":"concise review conclusion","findings":["open finding"],"workspaceWrite":"succeeded or failed"}',
      "",
      `--- ${analyst.output} ---\n${JSON.stringify(analystArtifact)}`,
      "",
      `--- ${estimator.output} ---\n${JSON.stringify(estimatorArtifact)}`
    ].join("\n");
    const reviewerTurn = await runTurn(server, reviewer, threads.get(reviewer.key), reviewPrompt);
    const reviewerArtifact = parseAgentArtifact(reviewerTurn.agentMessage);
    const reviewerRecord = await registerArtifact("review.json", reviewerArtifact);
    if (reviewerRecord) artifactRecords.push(reviewerRecord);

    const interruptStarted = await server.request("turn/start", {
      threadId: threads.get(analyst.key),
      cwd: workspaceFor(analyst.key),
      approvalPolicy: "never",
      sandboxPolicy: turnPolicy(analyst.key),
      effort: "low",
      input: [{
        type: "text",
        text: [
          "Run `Start-Sleep -Seconds 30` in PowerShell.",
          "Only after it finishes, respond with PAUSE-FINISHED."
        ].join("\n")
      }]
    });
    const interruptedTurnId = interruptStarted.turn.id;
    await server.waitFor(
      "turn/started",
      (params) => params.threadId === threads.get(analyst.key) && params.turn.id === interruptedTurnId
    );
    await delay(2_000);
    const interruptedCompletion = server.waitFor(
      "turn/completed",
      (params) => params.threadId === threads.get(analyst.key) && params.turn.id === interruptedTurnId
    );
    await server.request("turn/interrupt", {
      threadId: threads.get(analyst.key),
      turnId: interruptedTurnId
    });
    const interrupted = await interruptedCompletion;

    await server.stop();
    server = new AppServer("resumed", eventLog);
    await server.start();
    await server.request("thread/resume", { threadId: threads.get(analyst.key) });
    const resumeTurn = await runTurn(
      server,
      analyst,
      threads.get(analyst.key),
      [
        "Without being told your role again, determine your persistent Tender Office role and prior deliverable from this thread's context.",
        "Make exactly one shell execution call and no other tool calls.",
        `In that call, test whether ${join(workspaceFor(analyst.key), analyst.output)} is still readable after the server restart. Do not recreate it.`,
        "If the shell execution is still running, use the wait tool until it completes.",
        "After the completed tool result, respond with only one JSON object and no Markdown fences:",
        '{"role":"your persistent role","priorDeliverable":"the prior filename","contextRemembered":true or false,"workspaceFileReadable":true or false}'
      ].join("\n")
    );

    const resumeArtifact = parseAgentArtifact(resumeTurn.agentMessage);
    const resumeRecord = await registerArtifact("resumed-analyst.json", resumeArtifact);
    if (resumeRecord) artifactRecords.push(resumeRecord);
    const producerIntervalsOverlap =
      Math.max(Date.parse(analystTurn.startedAt), Date.parse(estimatorTurn.startedAt)) <
      Math.min(Date.parse(analystTurn.completedAt), Date.parse(estimatorTurn.completedAt));
    const producerArtifactsRegistered =
      analystArtifact?.role === analyst.name && analystArtifact?.deliverable === analyst.output &&
      estimatorArtifact?.role === estimator.name && estimatorArtifact?.deliverable === estimator.output;
    const roleWorkspaceWritesSucceeded =
      analystArtifact?.workspaceWrite?.toLowerCase() === "succeeded" &&
      estimatorArtifact?.workspaceWrite?.toLowerCase() === "succeeded" &&
      reviewerArtifact?.workspaceWrite?.toLowerCase() === "succeeded";
    const isolationHeld =
      analystArtifact?.crossWorkspaceWrite?.toLowerCase() === "blocked" &&
      estimatorArtifact?.crossWorkspaceWrite?.toLowerCase() === "blocked";
    const reviewerStartedAfterProducers = Date.parse(reviewerTurn.startedAt) >= Date.parse(producersCompletedAt);
    const reviewerArtifactRegistered =
      reviewerArtifact?.role === reviewer.name && reviewerArtifact?.deliverable === reviewer.output;
    const resumePreservedRole =
      resumeArtifact?.role === analyst.name &&
      resumeArtifact?.priorDeliverable === analyst.output &&
      resumeArtifact?.contextRemembered === true &&
      resumeArtifact?.workspaceFileReadable === true;

    const criteria = {
      chatGptAuthenticated: account?.type === "chatgpt",
      threePersistentRoleThreadsCreated: threads.size === 3,
      producerTurnsCompleted: analystTurn.status === "completed" && estimatorTurn.status === "completed",
      producerIntervalsOverlap,
      producerArtifactsRegistered,
      roleWorkspaceWritesSucceeded,
      crossWorkspaceWritesBlocked: isolationHeld,
      reviewerHostGated: reviewerStartedAfterProducers && reviewerTurn.status === "completed" && reviewerArtifactRegistered,
      activeTurnInterrupted: interrupted.params.turn.status === "interrupted",
      threadResumedAfterServerRestart: resumeTurn.status === "completed" && resumePreservedRole,
      temporaryThreadsArchived: false
    };

    for (const threadId of threads.values()) {
      await server.request("thread/archive", { threadId });
      archivedThreads += 1;
    }
    criteria.temporaryThreadsArchived = archivedThreads === threads.size;

    const result = {
      question: "Can one engineer-authenticated Codex session run a minimally controlled multi-role Tender Office?",
      measuredAt: now(),
      environment: {
        platform: process.platform,
        codexVersion,
        account,
        rateLimitsReadable
      },
      topology: {
        appServerProcessesUsedSequentially: 2,
        simultaneousAppServerProcesses: 1,
        priorPrototypeThreadsArchived,
        roleThreads: roleDefinitions.map((role) => role.name),
        sharedTenderPackage: relative(prototypeRoot, fixtureRoot),
        registeredArtifacts: artifactRecords
      },
      turns: {
        analyst: publicTurn(analystTurn),
        estimator: publicTurn(estimatorTurn),
        reviewer: publicTurn(reviewerTurn),
        interrupted: {
          status: interrupted.params.turn.status,
          completedAt: interrupted.at
        },
        resumedAnalyst: publicTurn(resumeTurn)
      },
      criteria,
      verdict: Object.values(criteria).every(Boolean) ? "PASS" : "FAIL",
      scope: "Local private-v0 evidence only; not a production-support, concurrency-SLA, or resale-entitlement claim.",
      events: eventLog
    };

    await writeFile(resultPath, `${JSON.stringify(result, null, 2)}\n`, "utf8");
    console.log(JSON.stringify({ resultPath, verdict: result.verdict, criteria }, null, 2));
    if (result.verdict !== "PASS") process.exitCode = 2;
  } catch (error) {
    for (const threadId of threads.values()) {
      try {
        await server.request("thread/archive", { threadId });
        archivedThreads += 1;
      } catch {
        // The sanitized failure record reports any incomplete cleanup.
      }
    }
    const notificationCounts = Object.fromEntries(
      [...new Set(server.notifications.map((notification) => notification.method))]
        .sort()
        .map((method) => [method, server.notifications.filter((notification) => notification.method === method).length])
    );
    const failure = {
      measuredAt: now(),
      codexVersion,
      error: sanitizeError(error),
      archivedThreads,
      createdRoleThreadCount: threads.size,
      priorPrototypeThreadsArchived,
      notificationCounts,
      serverEvents: eventLog,
      stderrTail: server.stderr.slice(-10),
      scope: "Harness failure; no decision verdict can be drawn."
    };
    await mkdir(runRoot, { recursive: true });
    await writeFile(resultPath, `${JSON.stringify(failure, null, 2)}\n`, "utf8");
    throw error;
  } finally {
    await server.stop().catch(() => {});
  }
}

main().catch((error) => {
  console.error(sanitizeError(error));
  process.exitCode = 1;
});
