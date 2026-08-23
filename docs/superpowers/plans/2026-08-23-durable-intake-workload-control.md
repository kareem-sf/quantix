# Durable Tender Intake Workload Control Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep provider requests within a deterministic size budget and make rate-limit recovery durable, bounded, automatic, and impossible to bypass.

**Architecture:** Manager intake persists an immutable byte-budgeted extraction plan before provider work begins. Direct HTTP rate-limit metadata flows into a transactional Manager cooldown with host-owned admission checks and a Tokio resumption scheduler.

**Tech Stack:** Rust 2021, serde_json_canonicalizer, rusqlite, reqwest, tokio, existing Manager projections and diagnostics.

**Spec:** docs/superpowers/specs/2026-08-23-agentic-intake-reliability-design.md

## Global Constraints

- Use serialized-byte budgets; do not add a tokenizer or claim exact token guarantees.
- Persist batch plans and retry deadlines before relying on them.
- Limit one intake operation to three automatic rate-limit retries.
- Enforce cooldown in host APIs; renderer state is never authoritative.
- Keep each persistence transition and audit event in one immediate transaction.
- Use existing Tokio and SQLite time facilities; add no retry dependency.
- Regenerate bindings with npm test and do not run a production build.

## File Structure

- Modify src-tauri/src/tender_store.rs for plan/cooldown schema and triggers.
- Modify src-tauri/src/tender_store/manager_intake.rs for planning, cooldown, admission, and projection.
- Modify src-tauri/src/tender_store/tender_records.rs for request-size estimation.
- Modify src-tauri/src/agent_backend/turn_executor.rs for structured Retry-After propagation.
- Modify src-tauri/src/agent_runtime.rs for failure metadata, scheduling, request guard, and Manager integration.
- Modify src-tauri/src/agent_backend/client.rs for HTTP-boundary tests.
- Test src-tauri/tests/tender_records.rs, src-tauri/tests/manager_workspace.rs, and src-tauri/tests/agent_runtime.rs.

---

### Task 1: Preserve structured Retry-After metadata

**Files:**
- Modify: src-tauri/src/agent_runtime.rs
- Modify: src-tauri/src/agent_backend/turn_executor.rs
- Test: src-tauri/src/agent_backend/turn_executor.rs

**Interfaces:**
- Consumes: BackendError::RateLimited { retry_after_ms }.
- Produces: ProviderFailure.retry_after_milliseconds.

- [ ] **Step 1: Extend existing 429 tests**

Assert integer seconds, normalized HTTP date, and missing Retry-After survive through ProviderFailure as Some(1500), Some(2000), and None.

- [ ] **Step 2: Run tests and verify RED**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib rate_limit_failure --features runtime-fixture
~~~

Expected: FAIL because ProviderFailure has no structured retry-delay field.

- [ ] **Step 3: Add the optional field and builder**

Add:

~~~rust
pub retry_after_milliseconds: Option<u64>,
~~~

Keep ProviderFailure::new defaulting it to None, then add:

~~~rust
pub(crate) fn with_retry_after_milliseconds(mut self, value: Option<u64>) -> Self {
    self.retry_after_milliseconds = value;
    self
}
~~~

Use it only in rate_limit_failure. Never parse timing from redacted_detail.

- [ ] **Step 4: Run tests and verify GREEN**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib rate_limit_failure --features runtime-fixture
~~~

Expected: PASS.

- [ ] **Step 5: Commit**

~~~powershell
git add src-tauri/src/agent_runtime.rs src-tauri/src/agent_backend/turn_executor.rs
git commit -m "fix: preserve provider retry timing"
~~~

---

### Task 2: Persist and enforce Manager cooldown

**Files:**
- Modify: src-tauri/src/tender_store.rs
- Modify: src-tauri/src/tender_store/manager_intake.rs
- Modify: src-tauri/src/agent_runtime.rs
- Test: src-tauri/tests/manager_workspace.rs
- Test: src-tauri/tests/agent_runtime.rs

**Interfaces:**
- Consumes: a completed rate-limited AgentRunInspection.
- Produces: transactional wait_manager_intake_for_provider, durable deadline/count, and host admission gates.

- [ ] **Step 1: Write atomic cooldown and bypass tests**

Assert a rate-limited run stores its blocking run ID, future deadline, attempt count, and audit event atomically. Set a fixed future deadline and assert direct rebind creates zero runs; set a fixed past deadline and assert one call is admitted.

- [ ] **Step 2: Run tests and verify RED**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --test manager_workspace rate_limited_manager_wait_is_persisted_atomically --features runtime-fixture
cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime rebind_cannot_bypass_future_cooldown --features runtime-fixture
~~~

Expected: FAIL because Manager wait has no durable cooldown.

- [ ] **Step 3: Add cooldown schema/status fields**

Add blocking_agent_run_id, retry_not_before_epoch_seconds, and provider_retry_attempt_count to manager_intake_runs. Require that a future deadline has waiting_for_provider stage and a blocking run. Add the first two fields to ManagerIntakeStatus; keep attempt count internal.

- [ ] **Step 4: Implement transactional delay calculation**

Change the store API to:

~~~rust
pub(crate) fn wait_manager_intake_for_provider(
    &mut self,
    source_run: Option<&AgentRunInspection>,
) -> Result<(), TenderCommandError>;
~~~

Within one immediate transaction, derive a deadline from the maximum future usage reset, then structured Retry-After, then fallback delays [60, 120, 240]. Store stage, run link, deadline, count, and audit. A fourth rate-limit result stores no automatic deadline and waits for Engineer retry.

- [ ] **Step 5: Add host admission checks**

Check SQLite current epoch inside begin_manager_intake_processing and bind_manager_intake_provider_selection. A future deadline returns waiting state without task/run creation. A legitimate resume clears deadline/blocking run in the same transaction that moves back to processing.

- [ ] **Step 6: Run tests and verify GREEN**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --test manager_workspace rate_limited_manager_wait_is_persisted_atomically --features runtime-fixture
cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime rebind_cannot_bypass_future_cooldown --features runtime-fixture
~~~

Expected: PASS.

- [ ] **Step 7: Commit**

~~~powershell
git add src-tauri/src/tender_store.rs src-tauri/src/tender_store/manager_intake.rs src-tauri/src/agent_runtime.rs src-tauri/tests/manager_workspace.rs src-tauri/tests/agent_runtime.rs
git commit -m "feat: persist manager provider cooldown"
~~~

---

### Task 3: Automatically resume after persisted cooldown

**Files:**
- Modify: src-tauri/src/agent_runtime.rs
- Test: src-tauri/tests/agent_runtime.rs

**Interfaces:**
- Consumes: retry_not_before_epoch_seconds from Task 2.
- Produces: one scheduled Manager wake-up per tender and restart reconstruction.

- [ ] **Step 1: Write scheduler/restart tests**

Store a deadline far in the future and assert a bounded 50 ms observation makes no provider call. Store a deadline in the past and assert exactly one call occurs. Schedule the same intake twice and assert one call. Reopen QuantixHost and assert it reconstructs the persisted future wait. This uses existing Tokio timing without the test-util feature.

- [ ] **Step 2: Run and verify RED**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime manager_cooldown_resumes_once_and_survives_restart --features runtime-fixture
~~~

Expected: FAIL because waiting intake currently requires manual resumption.

- [ ] **Step 3: Add the keyed scheduler**

Reuse the host Manager execution registry to allow one wake-up per tender. Spawn a Tokio task that sleeps until the persisted deadline, reloads Manager status, and calls start_manager_intake_background only if the same intake/deadline is still current. Cancellation or changed state exits without mutation.

- [ ] **Step 4: Reconstruct schedules on startup**

Change resume_manager_intakes so future cooldowns schedule one wake-up and expired cooldowns start immediately. Do not poll or busy-loop.

- [ ] **Step 5: Run and verify GREEN**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime manager_cooldown_resumes_once_and_survives_restart --features runtime-fixture
~~~

Expected: PASS.

- [ ] **Step 6: Commit**

~~~powershell
git add src-tauri/src/agent_runtime.rs src-tauri/tests/agent_runtime.rs
git commit -m "feat: resume intake after provider cooldown"
~~~

---

### Task 4: Persist immutable byte-budgeted extraction plans

**Files:**
- Modify: src-tauri/src/tender_store.rs
- Modify: src-tauri/src/tender_store/manager_intake.rs
- Modify: src-tauri/src/tender_store/tender_records.rs
- Test: src-tauri/tests/tender_records.rs

**Interfaces:**
- Consumes: ordered parsed evidence and authority references.
- Produces: estimate_record_extraction_request_bytes, immutable plan rows, and remaining batches.

- [ ] **Step 1: Write pure boundary and restart tests**

Cover exact fit, one-byte overflow, stable ordering, stable fingerprints, one oversized item, reopen/resume, and completed-batch subtraction. Assert no evidence is omitted or duplicated.

- [ ] **Step 2: Run and verify RED**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --test tender_records byte_batch_plan_is_deterministic_at_boundaries --features runtime-fixture
cargo test --manifest-path src-tauri/Cargo.toml --test tender_records byte_batch_plan_is_persisted_and_resumed --features runtime-fixture
~~~

Expected: FAIL because batching uses chunks(256) and no plan is persisted.

- [ ] **Step 3: Add the immutable plan table**

Create manager_intake_extraction_plan_batches with intake ID, positive ordinal, unique 64-character fingerprint, canonical inputs JSON, positive estimated bytes, estimator version, and timestamp. Add immutable update/delete triggers.

- [ ] **Step 4: Implement the deterministic estimator**

Add:

~~~rust
pub(crate) fn estimate_record_extraction_request_bytes(
    evidence: &[TenderEvidenceReference],
    authorities: &[TenderRecordAuthority],
) -> Result<u64, TenderCommandError>;
~~~

Build the same task schema/data-view shape as preparation and add named fixed overhead plus output headroom. Use checked arithmetic. Return a typed oversize intake error for one item that cannot fit.

- [ ] **Step 5: Replace count chunking**

On first call, greedily append evidence while the estimate fits, persist all plan rows and extraction stage in one immediate transaction, and commit. Later calls load the plan and subtract fingerprints in manager_intake_extraction_batches. Delete MAX_RECORD_EVIDENCE_INPUTS as the batching policy; retain only an absolute safety maximum if input validation requires it.

- [ ] **Step 6: Run and verify GREEN**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --test tender_records byte_batch_plan_is_deterministic_at_boundaries --features runtime-fixture
cargo test --manifest-path src-tauri/Cargo.toml --test tender_records byte_batch_plan_is_persisted_and_resumed --features runtime-fixture
~~~

Expected: PASS.

- [ ] **Step 7: Commit**

~~~powershell
git add src-tauri/src/tender_store.rs src-tauri/src/tender_store/manager_intake.rs src-tauri/src/tender_store/tender_records.rs src-tauri/tests/tender_records.rs
git commit -m "feat: persist byte budgeted intake batches"
~~~

---

### Task 5: Guard the actual provider request

**Files:**
- Modify: src-tauri/src/agent_backend/client.rs
- Modify: src-tauri/src/agent_runtime.rs
- Test: src-tauri/src/agent_backend/client.rs
- Test: src-tauri/tests/agent_runtime.rs

**Interfaces:**
- Consumes: final serialized direct ChatGPT request body.
- Produces: a pre-network hard budget failure and diagnostic request size.

- [ ] **Step 1: Write under/over-budget tests**

Assert a request exactly at the hard cap is sent. Assert one byte above makes zero HTTP calls and returns a non-retryable local budget failure.

- [ ] **Step 2: Run and verify RED**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib request_body_respects_hard_intake_budget --features runtime-fixture
~~~

Expected: FAIL because request size is unchecked.

- [ ] **Step 3: Add the final serialized-body guard**

After build_request_body and before send, serialize once, compare byte length with the named cap, and reuse those bytes for submission. Attach byte count to deep diagnostics without logging the body.

- [ ] **Step 4: Run stage verification**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib request_body_respects_hard_intake_budget --features runtime-fixture
npm test
npm run check
npm run format:check
~~~

Expected: all commands exit 0.

- [ ] **Step 5: Commit**

~~~powershell
git add src-tauri/src/agent_backend/client.rs src-tauri/src/agent_runtime.rs src-tauri/tests/agent_runtime.rs src/bindings
git commit -m "fix: enforce intake provider request budget"
~~~
