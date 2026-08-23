# Agentic Tender Intake Product Acceptance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Present automatic repair and provider waiting as a clean chronological Tender conversation, then prove the complete intake flow deterministically and with one opted-in Juhayna live run.

**Architecture:** Rust owns Manager state, retry availability, and user-safe copy. React renders chronological messages/actions without provider controls. Acceptance exercises the same public Manager workflow from registered evidence to published Tender Records.

**Tech Stack:** React 19, TypeScript, Vitest, Testing Library, Rust runtime fixtures, existing Product Acceptance commands, Tauri development runtime.

**Spec:** docs/superpowers/specs/2026-08-23-agentic-intake-reliability-design.md

## Global Constraints

- The Engineer workspace shows Tender activity, not providers, models, schemas, tools, or validation codes.
- The prompt composer stays at the bottom.
- Host projections own action availability.
- Automatic repair appears chronologically, never as a duplicate permanent Current Focus card.
- Deterministic verification must be green before live-provider work.
- Live Juhayna acceptance uses the connected account and does not relaunch the running Tauri dev app.
- Do not run a production build.

## File Structure

- Modify src-tauri/src/tender_store/workspace.rs for chronological repair/cooldown projection and actions.
- Modify src-tauri/src/tender_store/manager_intake.rs for user-safe labels, summaries, and run references.
- Modify src/ManagerWorkspace.tsx for chronological activity and bottom composer.
- Modify src/ManagerWorkspace.css for minimalist message rhythm and layout.
- Modify src/ManagerWorkspace.test.tsx for renderer and projection-refresh tests.
- Modify src-tauri/tests/manager_workspace.rs for Rust projection tests.
- Modify src-tauri/tests/tender_records.rs for deterministic end-to-end acceptance.
- Modify src-tauri/src/acceptance.rs only if the existing live command lacks the required Manager assertion.

---

### Task 1: Host-owned chronological repair and cooldown messages

**Files:**
- Modify: src-tauri/src/tender_store/workspace.rs
- Modify: src-tauri/src/tender_store/manager_intake.rs
- Test: src-tauri/tests/manager_workspace.rs

**Interfaces:**
- Consumes: Manager stage, blocking run, repair lineage, retry deadline, and outcome.
- Produces: user-safe WorkspaceMessage chronology and WorkspaceCurrentAction.

- [ ] **Step 1: Write Rust projection tests**

Cover these messages:

~~~rust
assert_eq!(
    repair_message.body,
    "Quantix is reviewing the extracted Tender data."
);
assert_eq!(
    cooldown_message.body,
    "Quantix is waiting for AI capacity before continuing the Tender review."
);
assert_eq!(
    failed_message.body,
    "Quantix could not validate the extracted Tender data after review."
);
~~~

Assert repair and future cooldown have no Engineer action; exhausted recovery has one retry action; each message references the relevant Agent Run internally.

- [ ] **Step 2: Run and verify RED**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --test manager_workspace workspace_projects_agentic_repair_chronologically --features runtime-fixture
~~~

Expected: FAIL because repair/cooldown are generic blockers/current focus.

- [ ] **Step 3: Implement host projection**

Project one chronological message for extraction start, candidate review, automatic repair, provider cooldown, repair success, or terminal validation failure. Use the existing Agent Run workspace reference. During cooldown return ObserveIntake; after expiry return the normal provider action; after exhausted automatic attempts return one retry action.

- [ ] **Step 4: Run and verify GREEN**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --test manager_workspace workspace_projects_agentic_repair_chronologically --features runtime-fixture
~~~

Expected: PASS.

- [ ] **Step 5: Commit**

~~~powershell
git add src-tauri/src/tender_store/workspace.rs src-tauri/src/tender_store/manager_intake.rs src-tauri/tests/manager_workspace.rs
git commit -m "feat: project agentic intake activity"
~~~

---

### Task 2: Minimalist renderer behavior

**Files:**
- Modify: src/ManagerWorkspace.tsx
- Modify: src/ManagerWorkspace.css
- Test: src/ManagerWorkspace.test.tsx

**Interfaces:**
- Consumes: host-owned messages/current action.
- Produces: chronological Tender conversation, suggestion actions, and bottom composer.

- [ ] **Step 1: Write renderer tests**

Assert repair activity appears once in message order, no fixed duplicate Current Focus panel renders, provider/model/reasoning controls are absent, cooldown has no retry button, an expired projection exposes one action, and composer is the last layout region.

- [ ] **Step 2: Run and verify RED**

~~~powershell
npx vitest run src/ManagerWorkspace.test.tsx -t "renders agentic intake repair as chronological tender activity"
~~~

Expected: FAIL on the current mixed focus/activity UI.

- [ ] **Step 3: Simplify workspace rendering**

Render host messages in one ordered feed with quiet timestamps and optional Tender references. Render host-supplied next-step actions as compact suggestions below the latest relevant message. Remove permanent Current Focus and normal-workspace provider controls. Map waiting intake to Waiting, not Ready.

- [ ] **Step 4: Anchor and simplify the composer**

Use a full-height grid with minmax(0, 1fr) and auto rows. Keep scrolling in the first row and composer in the second. Composer contains prompt, context affordance, and send action. Preserve keyboard submission and accessible labels.

- [ ] **Step 5: Run and verify GREEN**

~~~powershell
npx vitest run src/ManagerWorkspace.test.tsx
~~~

Expected: PASS.

- [ ] **Step 6: Commit**

~~~powershell
git add src/ManagerWorkspace.tsx src/ManagerWorkspace.css src/ManagerWorkspace.test.tsx
git commit -m "feat: simplify agentic manager conversation"
~~~

---

### Task 3: Deterministic complete intake acceptance

**Files:**
- Modify: src-tauri/tests/tender_records.rs
- Modify: src-tauri/tests/manager_workspace.rs
- Modify: src-tauri/src/acceptance.rs

**Interfaces:**
- Consumes: registered parsed evidence, fixture sequences, Manager workflow, and workspace projection.
- Produces: one regression proving extraction, repair, review, outcome, and Engineer next step.

- [ ] **Step 1: Write complete acceptance test**

Register a deterministic multi-document tender, run Manager intake with invalid extraction then valid repair, and assert:

~~~rust
assert_eq!(extraction_runs.len(), 2);
assert_eq!(
    extraction_runs[1].retry_of_run_id.as_deref(),
    Some(extraction_runs[0].run_id.as_str())
);
assert!(!records.items.is_empty());
assert!(intake.extraction_run_count > 0);
assert!(matches!(
    intake.stage,
    ManagerIntakeStage::WaitingForEngineer | ManagerIntakeStage::BidDecisionReady
));
assert!(workspace.messages.iter().any(|message| message.body.contains("Tender")));
~~~

Also assert zero generic INVALIDCOMMAND diagnostics and no duplicated evidence across receipts.

- [ ] **Step 2: Run and verify RED**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --test tender_records manager_intake_repairs_and_reaches_tender_records_end_to_end --features runtime-fixture
~~~

Expected: FAIL until the product projection is complete.

- [ ] **Step 3: Complete only the missing acceptance seam**

If the public deterministic acceptance result cannot inspect final Manager projection, extend it with the existing ManagerWorkspaceProjection. Do not add a test-only product path. Keep provider scripting behind runtime-fixture.

- [ ] **Step 4: Run and verify GREEN**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --test tender_records manager_intake_repairs_and_reaches_tender_records_end_to_end --features runtime-fixture
npm test
~~~

Expected: PASS.

- [ ] **Step 5: Commit**

~~~powershell
git add src-tauri/tests/tender_records.rs src-tauri/tests/manager_workspace.rs src-tauri/src/acceptance.rs
git commit -m "test: cover repaired manager intake end to end"
~~~

---

### Task 4: Full repository verification

**Files:**
- Modify only files required by formatter or Rust-owned binding generation.

**Interfaces:**
- Consumes: all three implementation stages.
- Produces: a green repository development gate.

- [ ] **Step 1: Regenerate and test**

~~~powershell
npm test
~~~

Expected: exit 0 with renderer/Rust tests and declarations current.

- [ ] **Step 2: Run static checks**

~~~powershell
npm run check
npm run format:check
~~~

Expected: both exit 0.

- [ ] **Step 3: Run development verification**

~~~powershell
npm run verify
~~~

Expected: exit 0. Do not run a production build.

- [ ] **Step 4: Inspect final diff**

~~~powershell
git status --short
git diff --check
~~~

If npm test regenerated declarations, stage only those declarations and commit:

~~~powershell
git add src/bindings
git commit -m "chore: refresh agentic intake bindings"
~~~

---

### Task 5: Opted-in live Juhayna acceptance

**Files:**
- Create: tmp/juhayna-agentic-intake-command.json only if the acceptance runner requires a command file; do not commit it.
- Modify production files only after a deterministic failing test reproduces any live defect.

**Interfaces:**
- Consumes: connected ChatGPT subscription, existing Juhayna desktop project, and npm run acceptance:live.
- Produces: a recorded live acceptance result with published Tender Records or one precise typed failure artifact.

- [ ] **Step 1: Confirm running app without relaunching**

Use Computer Use with the existing Tauri development window. Confirm Juhayna is open, ChatGPT is connected, GPT-5.3 Codex Spark uses low reasoning, and no provider turn is active.

- [ ] **Step 2: Run normal Engineer flow**

Trigger/resume Tender intake from the workspace. Observe chronological extraction, automatic repair if required, review, and the next Tender decision. Do not expose developer controls.

- [ ] **Step 3: Verify persisted outcomes read-only**

Open the Juhayna SQLite database in read-only mode. Require at least one completed extraction batch, one Tender Record/version, truthful run lineage, no active run, and no future cooldown deadline.

- [ ] **Step 4: Record the acceptance outcome**

Create the command file with apply_patch if required, then run:

~~~powershell
npm run acceptance:live -- C:\Users\kareem\.quantix tmp\juhayna-agentic-intake-command.json
~~~

Expected: accepted. If it fails, stop live retries, preserve the typed artifact, reproduce it deterministically, add the failing test, fix it, rerun npm run verify, and then perform one new live attempt.

- [ ] **Step 5: Report**

Report published Tender Record count, repair attempts, usage, batch count, and rate-limit wait. Never include Tender source text, OAuth tokens, or raw provider payloads.
