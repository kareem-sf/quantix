# SDK-First AI Runtime Plan Suite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the approved SDK-first runtime in three independently reviewable slices that each leave Quantix working.

**Architecture:** Slice 1 replaces the broken private ChatGPT route with official Codex app-server managed login. Slice 2 adds six official-SDK/Pydantic provider routes through one disposable worker. Slice 3 adds a Host-owned tools-only MCP client. Each slice first passes a deterministic pre-integration gate in its worktree, then lands on `main`, then runs interactive/browser/live verification through the already-running main dev app before the next slice starts.

**Tech Stack:** Rust/Tauri/SQLite Host, official Codex app-server 0.149.1, Python 3.12.13/Pydantic AI 2.33.0 and pinned official provider SDKs, official rmcp 3.1.4/MCP 2026-07-28.

**Spec:** `docs/superpowers/specs/2026-08-25-sdk-first-ai-runtime-cutover-design.md`

## Global Constraints

- Execute plans in the exact order below; never develop slices concurrently against the same production files.
- Use a new isolated worktree per slice from the then-current `main`.
- Use strict TDD, fresh implementer/reviewer agents, and two-stage spec/code review per task.
- Fast-forward a slice into local `main` only after its deterministic pre-integration gate, independent spec/code review, and the user-approved integration choice. Its post-integration gate then runs on merged `main`; only that second gate completes the slice.
- Keep the user's dev server running throughout; Tauri may rebuild itself after merges but must not be stopped for questions.
- Do not launch a second Tauri server from a worktree. Worktree verification uses deterministic Rust/Python/renderer tests; interactive/browser/live verification waits until the reviewed branch is integrated and the existing main server has rebuilt.
- If post-integration verification exposes a defect, reproduce it with a RED deterministic test in a fresh correction worktree, review and integrate that correction, then repeat the post-integration gate. Never patch unreviewed product code directly on `main`.
- Preserve unrelated user changes and never use destructive git commands.
- Do not run production builds during ordinary development.
- This suite intentionally provides no schema/vault migration. Before each schema-breaking slice reaches the running main app, execute the explicit fresh-state cutover below; never let a newer binary open an older home.

## Fresh-State Schema Cutover

Each slice changes an exact persisted contract without backward compatibility:

| Before integrating | Incompatible change |
| --- | --- |
| Slice 1 | Tender schema 45 → 46 and canonical provider/selection JSON |
| Slice 2 | Installation schema 25 → 26 plus budget/usage/active-selection JSON |
| Slice 3 | Installation schema 26 → 27, vault schema 1 → 2, Tender schema 46 → 47 |

At execution time, the primary agent must stop at each boundary and obtain explicit user approval for the exact phrase `PURGE CREDENTIALS, ARCHIVE DATA, AND RECREATE QUANTIX HOME FOR SLICE N`. Declining blocks that slice; it is never implicit permission. After approval:

1. In the still-running old app, require every Agent Run/update/recovery to be terminal, clear Active AI Configuration, perform the current account disconnect/logout flow (`account/logout` once the managed Codex slice exists), and delete/disconnect every API-key and MCP connection through its Host command. Confirm Settings reports no account, connection, or credential. These credentials are intentionally not recoverable; the Engineer will authenticate again in the fresh home.
2. Task 13 of Slice 1 installs the permanent startup interlock used by all three cutovers. Before merging, set `$sliceNumber` to literal `1`, `2`, or `3`, `$reviewedWorktreePath` to the exact recorded reviewed worktree, and `$reviewedSliceSha` to its reviewed commit; compute the remaining marker values under `C:\QuantixAcceptance\sdk-first-schema-cutovers`; then atomically write non-secret `active-cutover.json` with status `armed`, exact home, old/new revisions, slice, nonce hash, expected old/new schema versions, and exact acknowledgement path. The old child ignores the marker; the merged child must read it before constructing `QuantixHost` or touching Application Home. Keep that worktree intact until status `completed` so the reviewed operator script remains available.

   ```powershell
   $oldSourceSha = (& git rev-parse main).Trim()
   if ($reviewedSliceSha -notmatch '^[0-9a-f]{40}$') { throw 'set reviewedSliceSha to the reviewed slice commit' }
   $expectedInstallationSchema = @(25, 26, 27)[$sliceNumber - 1]
   $expectedVaultSchema = @(1, 1, 2)[$sliceNumber - 1]
   $expectedTenderSchema = @(46, 46, 47)[$sliceNumber - 1]
   $cutoverScript = [IO.Path]::GetFullPath((Join-Path $reviewedWorktreePath 'scripts\run-schema-cutover.ps1'))
   if (-not (Test-Path -LiteralPath $cutoverScript -PathType Leaf)) { throw 'reviewed cutover script is missing' }
   powershell -NoProfile -File $cutoverScript `
       -Phase Arm `
       -SliceNumber $sliceNumber `
       -OldSourceSha $oldSourceSha `
       -NewSourceSha $reviewedSliceSha `
       -ExpectedInstallationSchema $expectedInstallationSchema `
       -ExpectedVaultSchema $expectedVaultSchema `
       -ExpectedTenderSchema $expectedTenderSchema
   if ($LASTEXITCODE -ne 0) { throw 'schema cutover did not arm' }
   ```
3. Integrate the reviewed branch while keeping the `npm run tauri dev` PTY alive. The new child acquires the named schema-cutover mutex, validates the marker, opens no home path, and atomically writes the acknowledgement containing its compiled source revision and nonce hash. Wait for and validate that acknowledgement. This is the atomic exclusion boundary; a process-name check is never used.
4. While the acknowledged new child remains blocked, validate exact non-link targets and remove only the credential-bearing paths excluded by ADR 0018: `%USERPROFILE%\.quantix\codex` after normal logout, `%USERPROFILE%\.quantix\ai-connections.vault` after all connections are deleted, and any obsolete root `auth.json`. Use literal native PowerShell paths; never archive these paths and never use a glob. If validation/removal fails, keep the startup marker armed and the new child maintenance-blocked until the user resolves it or approves a reviewed rollback.
5. Move the remaining credential-free `%USERPROFILE%\.quantix` to the new explicit archive path. Generate a canonical sorted inventory of relative path, byte length, and SHA-256 for every archived file; write it beside the archive, hash it, and verify the archive contains no `auth.json`, `codex` child, vault, key, token, or credential sentinel. Copy nothing from the archive into the new home.
6. Atomically change the marker to status `released` with archive inventory SHA-256. The blocked child verifies the old home is absent, lets normal setup create a restricted fresh home at the exact target schemas, validates readiness, writes `Completed`, atomically moves `active-cutover.json` to its immutable completion-receipt path, releases the mutex, and only then creates `QuantixHost`/renderer state. The next startup sees no active marker. If any check fails, it exits without opening the old/archive home and leaves the active marker for recovery.

   ```powershell
   powershell -NoProfile -File $cutoverScript `
       -Phase Release `
       -SliceNumber $sliceNumber `
       -OldSourceSha $oldSourceSha `
       -NewSourceSha $reviewedSliceSha `
       -ExpectedInstallationSchema $expectedInstallationSchema `
       -ExpectedVaultSchema $expectedVaultSchema `
       -ExpectedTenderSchema $expectedTenderSchema
   if ($LASTEXITCODE -ne 0) {
       throw 'schema cutover remains maintenance-blocked; do not remove the marker or open the old home'
   }
   ```
7. Verify the immutable completion receipt, absence of `active-cutover.json`, new setup/schema versions, runtime readiness, and empty connection/Tender state. Restart once and prove the completed receipt is inert; before the next slice, prove `Arm` accepts the prior receipt but refuses any active marker. Re-import `C:\Users\kareem\Desktop\Test Project` and reconfigure only through the new slice's ordinary Settings flow. Formal acceptance remains in its separately documented sanitized account/home.

This is an interlocked purge/archive/recreate operation, not a migration or credential backup. The parent dev-server PTY is never stopped or sent Ctrl+C.

## Spec Coverage

| Design requirement | Implementing plan/task |
| --- | --- |
| Managed ChatGPT auth, isolated Codex home, protocol policy, qualification, and explicit selection | Codex Tasks 1-10 |
| Removal of private backend/custom OAuth and truthful test-only boundary | Codex Tasks 11-14 |
| Credential/canonical split, immutable `ActiveAiConfiguration`, no defaults/fallbacks | Codex Task 4; General Tasks 2, 8-9, and 11 |
| Six official-SDK/Pydantic routes, strict worker IPC, and Settings lifecycle | General Tasks 0-9 |
| Compatible endpoint guarded transport and reroute policy | General Tasks 5-9 and 11 |
| Pricing, discovery, qualification, usage and monetary limits | General Tasks 0, 2, and 8-13 |
| Host-only tools, persistence-before-continuation, 32-round ceiling, loop/non-progress detection | Codex Task 7; General Tasks 0, 7, and 11; MCP Task 6 |
| SQLite/vault/Codex-home secret separation and immutable execution revisions | Codex Tasks 2, 4, and 8; General Tasks 2 and 8-9; MCP Tasks 2 and 8 |
| No automatic retry after dispatch; terminal/indeterminate cancellation semantics | Codex Task 7; General Tasks 5-7 and 11; MCP Tasks 4 and 6 |
| FastEmbed/sqlite-vec non-regression and no worker retrieval paths | General Task 11 |
| Official tools-only Host MCP client, vault credentials, transports, versioned tool review, and permissions | MCP Tasks 1-10 |
| Dynamic Arabic Agent Profile CRUD and recursive swarms | Explicitly preserved as later Layer 2; no implementation in this suite |
| Provider-native hosted tools and Codex MCP bridge | Explicitly blocked pending separate approved designs |
| Remote MCP OAuth | Explicitly deferred to a separate guarded authorization/token/callback design; Slice 3 supports none, bearer, and named-header auth only |
| Test-only Codex isolation and public-release OS/built-in-tool/terms blockers | Codex Tasks 5 and 11-14 |
| Full deterministic/live acceptance and release blockers | Codex Tasks 11-14; General Tasks 12-13; MCP Tasks 9-10 |

---

### Task 1: Land the Codex Managed-Runtime Cutover

**Plan:** `docs/superpowers/plans/2026-08-25-sdk-first-codex-cutover.md`

**Produces:** Official managed ChatGPT login, explicit Codex model selection, one successful governed Test Project run, and zero production private ChatGPT/custom OAuth code.

- [ ] **Step 1: Create an isolated worktree from current main**

Use `superpowers:using-git-worktrees`; record base SHA, worktree path, and branch.

- [ ] **Step 2: Execute the Codex plan through its pre-integration gate**

Use `superpowers:subagent-driven-development`; complete every deterministic implementation task and its two-stage review, but leave the child plan's explicitly labelled post-integration Test Project/live task pending.

- [ ] **Step 3: Run the exact pre-integration gate**

```powershell
npm run verify
```

- [ ] **Step 4: Integrate the reviewed deterministic slice**

Use `superpowers:finishing-a-development-branch` together with the approved Fresh-State Schema Cutover for Slice 1. Preserve the existing dev-server process and verify the automatic rebuild opens only the fresh schema-46 home.

- [ ] **Step 5: Run the Codex post-integration gate on merged main**

Use the rebuilt main app for managed login, explicit probe/selection, the sanitized Test Project run, and the child plan's formal live/private evidence. The slice is incomplete until this passes.

### Task 2: Land the General Provider Worker

**Plan:** `docs/superpowers/plans/2026-08-25-sdk-first-general-provider-worker.md`

**Consumes:** Task 1's active-selection, managed-Codex, and immutable-run interfaces.

**Produces:** Six official-SDK routes with zero hidden retry/fallback/default behavior and one disposable bounded worker per operation.

- [ ] **Step 1: Start from the merged Task 1 main revision**

Create a fresh isolated worktree and rerun the relevant connection/runtime baseline tests before editing.

- [ ] **Step 2: Execute the worker plan through its pre-integration gate**

Use fresh task implementers and reviewers; reject any attempt to revive the deleted direct provider or reuse Codex credentials. Leave only the child plan's explicitly labelled post-integration live task pending.

- [ ] **Step 3: Run Python, Rust, renderer, and full gates**

```powershell
node scripts/run-ai-worker-tests.mjs
npm run verify
```

- [ ] **Step 4: Integrate the reviewed deterministic slice**

Merge only reviewed product code and deterministic fixtures together with the approved Fresh-State Schema Cutover for Slice 2. Preserve the existing main dev server and verify the rebuild opens only the fresh installation-schema-26 home.

- [ ] **Step 5: Run the general-provider post-integration gate**

Through the rebuilt main app and the exact acceptance Application Home, configure one opted-in direct-key connection, qualify and explicitly activate it, complete Test Project once, then record the child plan's live evidence. Keep credentials and Application Home evidence uncommitted.

### Task 3: Land the Host-Owned MCP Client

**Plan:** `docs/superpowers/plans/2026-08-25-sdk-first-mcp-client.md`

**Consumes:** Tasks 1-2 process supervision, vault, active selection, permission, tool, budget, and audit interfaces.

**Produces:** Tools-only local stdio and guarded remote MCP connections through official rmcp, with no marketplace, server bridge, provider-native MCP, or authority bypass.

- [ ] **Step 1: Start from the merged Task 2 main revision**

Create a fresh isolated worktree and verify current schema/vault/permission baselines.

- [ ] **Step 2: Execute the MCP plan through its pre-integration gate**

Use the hostile server fixtures defined by the MCP transport tasks; no Settings UI is mounted before the permission/execution boundary passes review. Leave the child plan's explicitly labelled post-integration browser/acceptance task pending.

- [ ] **Step 3: Run the full gate**

```powershell
npm run verify
```

- [ ] **Step 4: Integrate the reviewed deterministic slice**

Execute the approved Fresh-State Schema Cutover for Slice 3 while integrating. Preserve the existing main dev server and verify the rebuilt renderer/Host opens only installation schema 27, vault schema 2, and Tender schema 47.

- [ ] **Step 5: Run the MCP post-integration browser/acceptance gate**

Use the rebuilt main app to create reviewed local and remote fixture connections, approve exact discovered tool definitions, exercise governed calls, and capture acceptance evidence.

- [ ] **Step 6: Record remaining later-layer scope**

Record that Codex MCP bridging, provider-native hosted tools, dynamic Arabic Agent Profile CRUD, memory authority, and recursive swarms still require their separately approved designs/plans.

## Suite Completion Gate

The suite is complete only when all three child completion gates are satisfied on merged `main`, the dev app is still running, deterministic verification is green, required opt-in live evidence exists, and no obsolete plan was executed.
