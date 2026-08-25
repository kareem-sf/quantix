# Layer 1D AI Security and Acceptance Implementation Plan

> **Superseded — do not execute.** ADR 0018 and the
> [SDK-first runtime design](../specs/2026-08-25-sdk-first-ai-runtime-cutover-design.md)
> replace its security and acceptance assumptions. A replacement plan will be
> written after the revised design is approved.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove Layer 1 fails closed, leaks no credentials, never falls back, survives worker/storage faults, records durable acceptance evidence, and tells one truthful current product story.

**Architecture:** Deterministic Windows tests attack the vault, renderer/IPC boundary, worker protocols, custom endpoints, process supervision, backups, diagnostics, and revision recovery. Product Acceptance adds a dedicated AI-connections gate. Live provider qualification remains separate and opt-in.

**Tech Stack:** Existing Rust/React test suites, Windows fixture processes, local fake HTTP/SSE servers, Product Acceptance records, GitHub Actions Windows CI.

**Spec:** `docs/superpowers/specs/2026-08-24-layer-1-ai-connection-foundation-design.md`, parent plan `docs/superpowers/plans/2026-08-24-layer-1-ai-connection-foundation.md`, and completed Layer 1A–1C implementation.

## Global Constraints

- Deterministic tests use sentinel credentials and local fixtures only. Never read a developer's real vault, environment keys, Codex session, or provider account.
- Security assertions inspect exact temporary application homes and owned fixture processes. Never recursively inspect/delete broad user paths.
- No test may weaken DPAPI, TLS, redirect policy, worker containment, or secret redaction to make fixtures easier.
- No live provider result is required for deterministic acceptance. Live account/API qualification is explicit, private, and release-stage only.
- Do not run `npm run build:desktop` or any production package build in this plan.
- Update documentation only for Layer 1 capabilities now implemented. Dynamic employees, memory, tools/MCP catalogue, Tool Workshop, recursive swarms, and Improvement Lab remain future layers.

---

### Task 1: Add adversarial vault and secret-boundary tests

**Files:**

- Create: `src-tauri/tests/support/vault_fixture.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/tests/ai_connection_vault.rs`
- Modify: `src-tauri/tests/ai_connection_repository.rs`
- Modify: `src-tauri/tests/quantix_setup.rs`
- Modify: `src-tauri/tests/tender_backup.rs`
- Modify: `src-tauri/src/diagnostics.rs`
- Modify: `src-tauri/tests/manager_workspace.rs`

**Sentinel:** Use one generated 64-character URL-safe ASCII test secret per test and retain the Rust copy in a `Zeroizing<Vec<u8>>`; never put a fixed credential-like literal in repository files. A create/update test necessarily passes a temporary copy through the WebView mock and Tauri IPC request; the assertion is that it is cleared best-effort and never returned, persisted, logged, or exported. JavaScript string zeroization is not claimed.

**Interfaces:**

- Consumes: final vault/repository commands, backups, diagnostics, and fixture application homes.
- Produces: `quantix-vault-fixture` and a reusable encoded-secret leak scanner.

- [ ] **Write the red encoded-secret scan test**

```rust
#[test]
fn credential_forms_are_absent_after_submission() {
    let fixture = SecretBoundaryFixture::new_random_url_safe_secret(64);
    fixture.exercise_connection_lifecycle();
    let leaks = fixture.scan_owned_artifacts(&[
        SecretEncoding::Utf8,
        SecretEncoding::Utf16Le,
        SecretEncoding::JsonEscaped,
        SecretEncoding::UrlEncoded,
        SecretEncoding::Base64,
    ]);
    assert!(leaks.is_empty(), "credential leak locations: {leaks:?}");
}
```

- [ ] **Run it and confirm red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_vault credential_forms_are_absent_after_submission --features runtime-fixture`

Expected: FAIL because the lifecycle scanner/fixture is missing.

- [ ] Register a `quantix-vault-fixture` test binary that performs one named compare-and-swap mutation against an exact passed temporary application home. It prints only success/failure and revision.
- [ ] Add a two-process barrier test that races independent mutations. Assert no lost update, one monotonic revision per success, valid DPAPI ciphertext, and clean lock/temp state.
- [ ] Add power-loss simulations at pre-temp-write, post-write/pre-sync, post-sync/pre-replace, and post-replace checkpoints. Reopen must yield either the complete old or complete new payload, never partial/empty/corrupt-as-empty.
- [ ] Add tests for wrong vault version, random ciphertext, bit flip, truncation, unexpected directory/link/alternate stream, read-only lock, and ReplaceFile sharing contention.
- [ ] Verify vault, lock, and temp/replacement files inherit the private current-user/SYSTEM/Administrators DACL and never become world-readable during publication.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_vault --features runtime-fixture`; expect the new two-process, checkpoint, or leak assertions to fail before hardening production code/fixtures.
- [ ] Exercise create/update/test/activate/disconnect/disable/delete with the sentinel, clear the submitted request/form references, then recursively scan exact owned outputs for raw UTF-8, UTF-16LE, JSON-escaped, URL-encoded, and base64 forms: vault ciphertext, installation DB bytes, Tender DBs, WAL/journal files, logs, diagnostics bundles, backup ZIP contents, portable archives, exports, and renderer response JSON. No form may occur in a scanned artifact or returned projection.
- [ ] Assert disconnect removes only the exact connection credential, leaves other encrypted connections intact, and makes an active reference unavailable without fallback.
- [ ] Assert diagnostics report only stable error category, redacted connection ID, worker/adapter version, and correlation ID—never endpoint query/header values, account tokens, or raw provider bodies.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_vault --features runtime-fixture` and `cargo test --manifest-path src-tauri/Cargo.toml --test tender_backup --features runtime-fixture`; expect success.
- [ ] Commit as `test: harden the ai credential boundary`.

---

### Task 2: Attack worker isolation, redirects, cancellation, and no-fallback behavior

**Files:**

- Modify: `src-tauri/tests/ai_worker_contract.rs`
- Modify: `src-tauri/tests/support/ai_worker_fixture.rs`
- Create: `src-tauri/tests/fixtures/ai_worker/redirect-trap.jsonl`
- Create: `src-tauri/tests/fixtures/ai_worker/output-overflow.jsonl`
- Create: `src-tauri/tests/fixtures/ai_worker/stderr-secret.jsonl`
- Modify: `src-tauri/runtime/ai/tests/test_general_adapter.py`
- Modify: `src-tauri/runtime/ai/tests/test_probe.py`
- Modify: `src-tauri/tests/agent_runtime.rs`
- Modify: `src-tauri/tests/runtime_readiness.rs`

**Interfaces:**

- Consumes: both final workers and their deterministic local transports.
- Produces: seven-route fault/no-fallback/request-count evidence.

- [ ] **Write the red seven-route invocation-count test**

```rust
#[tokio::test]
async fn every_route_fails_once_without_fallback() {
    for route in SevenRoute::ALL {
        let fixture = FaultedRouteFixture::new(route, Fault::RateLimited);
        let failure = fixture.run().await.unwrap_err();
        assert_eq!(failure.category, AiRuntimeFailureCategory::RateLimited);
        assert_eq!(fixture.selected_invocations(), 1);
        assert_eq!(fixture.other_invocations(), 0);
    }
}
```

- [ ] **Run it and confirm red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ai_worker_contract every_route_fails_once_without_fallback --features runtime-fixture`

Expected: FAIL because the seven-route fault fixture is missing.

- [ ] Add a two-server redirect trap: the configured endpoint returns a redirect and the trap records all requests. Assert the adapter fails and the trap receives no request or credential.
- [ ] Add custom endpoint tests for DNS/connection failure, mixed/special-address DNS, TLS/hostname failure, all redirect codes (301/302/303/307/308), 401, 403, a 404 model-list route followed only by the Engineer's explicit model probe, 408, 429 with retry-after, 5xx, malformed JSON, decompression/response overflow, malformed stream, and invalid tool delta. Every failure is typed/redacted.
- [ ] Add process tests for blocked spawn, handshake timeout, operation timeout, cancellation during text/tool/result, output/stderr limits, worker crash before/after provider acceptance, descendant creation, and abrupt Host termination. Assert complete Job Object cleanup.
- [ ] Add Codex tests proving isolated `CODEX_HOME` has no `auth.json`, user config is not inherited, all disabled features remain disabled in `config/read`, and unknown/built-in tool requests fail closed.
- [ ] Inspect the exact `ProcessSpec` for both workers and assert the sentinel is absent from executable, arguments, current directory, and environment; it exists only in stdin bytes before zeroization.
- [ ] Add a seven-route no-fallback matrix: Codex account, OpenAI key, Anthropic key, Gemini key, xAI key, OpenAI-compatible, and Anthropic-compatible. For each route, force authentication, rate limit, capability, crash, reroute, and protocol failures and assert invocation count remains exactly one for the selected connection/model.
- [ ] Run the worker/Python/Agent Runtime focused tests; expect at least one new redirect, containment, retry-count, or no-fallback assertion to fail before implementation hardening.
- [ ] Add exact request-count tests proving every general-provider SDK and Pydantic perform zero hidden retries and the Host never retries a Codex turn. Bind Codex's accepted four-request/five-stream internal limits to runtime version evidence, test the single same-request authentication-refresh continuation separately, and never report missing internal retry usage as zero. One Host-authorized general-provider transient retry uses the same connection/revision/model and creates attributable retry evidence.
- [ ] Add same-account refresh tests proving `credential_generation` changes without staling execution revision/probes, and changed account/plan/residency tests requiring re-login and retest.
- [ ] Add lock-order/deadlock tests, duplicate dynamic-tool ID/idempotency tests, Pydantic validation-error and raw-stderr leakage tests, proxy/netrc-disabled tests, DNS destination-class drift tests, and a real pinned Codex schema/config smoke test.
- [ ] Run `npm run test:ai-worker`, `cargo test --manifest-path src-tauri/Cargo.toml --test ai_worker_contract --features runtime-fixture`, and `cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime --features runtime-fixture`; expect success.
- [ ] Commit as `test: prove provider isolation and no fallback`.

---

### Task 3: Add the Layer 1 Product Acceptance gate

**Files:**

- Modify: `src-tauri/src/acceptance.rs`
- Modify: `src-tauri/src/bin/product_acceptance.rs`
- Modify: `src-tauri/src/release_gate.rs`
- Modify: `src-tauri/tests/release_configuration.rs`
- Modify: `fixtures/acceptance/v1/oracle.json`
- Modify: `fixtures/acceptance/v1/README.md`
- Modify generated: `src/bindings/RunDeterministicAcceptanceCommand.ts`
- Modify generated: `src/bindings/ProductAcceptanceRun.ts`

**New deterministic area:** `ai_connections`.

The measured rehearsal must prove:

1. fresh setup has no active connection/model/reasoning;
2. all four methods and all seven concrete adapter routes validate through deterministic fixtures;
3. Test does not activate;
4. an exact tested model/reasoning activates;
5. run A pins revision A and new run B pins newly activated B;
6. failure produces no fallback;
7. vault/backup/diagnostic/renderer scans find no sentinel;
8. cancellation/crash reaches one truthful terminal/recovery state.

**Interfaces:**

- Consumes: deterministic seven-route fixtures and existing Product Acceptance driver.
- Produces: required `ai_connections` check plus provider-neutral lock/runtime hashes in `ProductAcceptanceRun`.

```rust
pub struct AiAcceptanceEvidence {
    pub dependency_locks: Vec<AcceptanceArtifactHash>,
    pub ai_runtime_contract_version: u32,
    pub ai_adapter_evidence: Vec<AcceptanceArtifactHash>,
}
```

- [ ] **Write the red required-area test**

```rust
#[test]
fn deterministic_acceptance_requires_ai_connections_and_three_locks() {
    assert!(REQUIRED_DETERMINISTIC_AREAS.contains(&"ai_connections"));
    let run = deterministic_fixture_run();
    assert!(run.checks.iter().any(|check| check.area == "ai_connections" && check.passed));
    assert_eq!(run.ai_evidence.dependency_locks.len(), 3);
    assert!(run.ai_evidence.dependency_locks.iter().all(|lock| lock.sha256.len() == 64));
}
```

- [ ] **Run it and confirm red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib acceptance::tests::deterministic_acceptance_requires_ai_connections_and_three_locks`

Expected: FAIL because the new area/lock collection is missing.

- [ ] Add `ai_connections` to `REQUIRED_DETERMINISTIC_AREAS` and emit one measured check whose detail describes evidence without connection secrets or raw endpoint values.
- [ ] Replace ChatGPT-direct acceptance fields with a provider-neutral matrix: runtime contract version, adapter IDs/versions, connection methods exercised, catalogue hashes, model IDs, and account-login commercial gate state.
- [ ] Change the deterministic command from one `dependency_lock_path` to a bounded `dependency_lock_paths` list and hash `package-lock.json`, `src-tauri/Cargo.lock`, and `src-tauri/runtime/ai/uv.lock` independently. Do not retain the singular compatibility field.
- [ ] Update deterministic fixture driving so the application creates fixture connections/active configurations through the same Host commands as the renderer, then executes the existing Tender lifecycle with deterministic workers.
- [ ] Keep the existing Tender oracle and lifecycle gates intact; Layer 1 adds a gate rather than replacing evidence/EITL/calculation/recovery checks.
- [ ] Update private/live qualification to record the exact selected method/connection revision/model/reasoning/adapter/catalogue. Live command files contain references only, never keys or tokens.
- [ ] Keep public release blocked unless the exact account-login integration-terms decision is approved. Direct API-key/compatible technical success cannot silently waive that decision for an account-backed release claim.
- [ ] Regenerate bindings and run the exporter twice, requiring no second diff.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml --lib acceptance::tests`, `cargo test --manifest-path src-tauri/Cargo.toml --test release_configuration --features runtime-fixture`, and `npm test`; expect success.
- [ ] Commit as `test: gate releases on ai connection acceptance`.

---

### Task 4: Update CI caching and deterministic dependency evidence

**Files:**

- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/candidate-windows.yml`
- Modify: `AGENTS.md`

**Interfaces:**

- Consumes: standalone `npm test`, runtime preparation, three lockfiles, and notice/provenance files.
- Produces: deterministic Windows PR gate and explicit release-stage candidate gate.

- [ ] **Write the workflow changes with exact cache scope**

```yaml
- name: Prepare verified runtimes
  run: npm run prepare:runtime

- name: Run deterministic verification
  run: npm run verify

- name: Confirm generated files are committed
  run: git diff --exit-code
```

The PR workflow contains no `npm run build` or `npm run build:desktop`. The candidate workflow keeps its explicit Tauri build after verification.

- [ ] Cache the verified Codex archive, uv cache, managed Python 3.12.13 download, and `.dev/runtime-provisioning/ai-worker-venv` using keys that include runtime provenance and `uv.lock` hashes. Never create/cache `src-tauri/runtime/ai/.venv`, `~/.quantix`, or a vault.
- [ ] Ensure PR CI runs `npm run prepare:runtime`, Python worker tests, Rust/renderer tests, and the generated-file diff. Remove the standalone `npm run build`; `npm run check` already typechecks, and production renderer/desktop builds remain release-stage only. Keep Windows as the only claimed platform.
- [ ] Ensure the candidate workflow verifies exact Codex binary provenance and AI worker lock before the explicit release-stage desktop build.
- [ ] Fail the candidate gate when `src-tauri/runtime/THIRD_PARTY_NOTICES.txt` or the AI license inventory is absent, stale, or not included in package resources.
- [ ] Document `npm run test:ai-worker` and the three dependency locks in `AGENTS.md`; preserve the existing production-build warning.
- [ ] Run `src-tauri/runtime/bin/uv.exe lock --check --project src-tauri/runtime/ai`, `npx prettier --check .github/workflows/ci.yml .github/workflows/candidate-windows.yml`, and `npm run verify`; expect success.
- [ ] Commit as `ci: verify the pinned ai runtimes`.

---

### Task 5: Make all current product documentation truthful

**Files:**

- Modify: `CONTEXT.md`
- Modify: `README.md`
- Modify: `IDEA.md`
- Modify: `docs/product/agentic-tender-workspace.md`
- Modify: `docs/research/multi-provider-auth-model-discovery.md`
- Modify: `docs/research/codex-subscription-integration.md`
- Modify: `docs/research/agent-framework-selection.md`
- Modify: `docs/superpowers/plans/2026-08-21-chatgpt-direct-provider.md`
- Modify: `docs/superpowers/plans/2026-08-22-codex-only-beginner-connection.md`
- Modify: `docs/superpowers/specs/2026-08-21-chatgpt-direct-provider-design.md`
- Modify: `docs/superpowers/specs/2026-08-22-codex-only-beginner-connection-design.md`

**Interfaces:**

- Consumes: verified Layer 1 behavior and ADR 0017.
- Produces: one present-tense product/glossary story; historical files remain visibly superseded.

- [ ] **Apply the canonical current-capability paragraph**

```markdown
Quantix can save several Engineer-configured AI connections and uses zero or one
explicit global Active AI Configuration for future Agent Runs. It supplies no
provider, model, reasoning, or fallback default. Persistent credentials at rest
exist only in the current Windows user's DPAPI-encrypted AI connection vault.
```

- [ ] Update glossary definitions for Quantix Application Home, Application Settings, AI Provider, Provider Connection, Provider Credential, Provider Capability Catalogue, Active AI Configuration, Waiting for AI Provider, and Provider Turn.
- [ ] State exactly four connection methods, multiple saved connections, one global Engineer-selected active configuration, no defaults, no per-Tender selection, and no fallback.
- [ ] State that persistent credentials at rest live only in the DPAPI vault and restoring on another Windows account/device requires reconnecting.
- [ ] State the threat boundary: pagefile/hibernation, operating-system crash dumps, administrator compromise, kernel compromise, and a malicious process already running as the same Windows user are outside the protection promised by application-level DPAPI storage.
- [ ] Describe Codex app-server as the official harness boundary and the general Pydantic worker as a provider adapter only. Do not claim SDK production execution, private backend access, or a complete third-party framework embedded whole.
- [ ] Add prominent historical/superseded banners to the two obsolete plans/specs and direct implementers to this Layer 1 suite. Preserve history; do not leave them executable-looking.
- [ ] Keep future dynamic employees, recursive swarm, memory, tools/MCP, Tool Workshop, and Improvement Lab clearly marked as later layers rather than shipped Layer 1 features.
- [ ] Run `npx prettier --check CONTEXT.md README.md IDEA.md docs AGENTS.md`; expect success.
- [ ] Run `rg -n -S "ChatGPT-only|auth\.json|per-Tender AI|direct ChatGPT|default model|fallback" CONTEXT.md README.md IDEA.md docs/product docs/research docs/adr`; classify every remaining historical occurrence and remove every false current-capability claim.
- [ ] Commit as `docs: publish the layer one ai connection contract`.

---

### Task 6: Run final verification and record the Layer 1 handoff

- [ ] Run targeted suites:
  - `npm run test:ai-worker`
  - `cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_vault --features runtime-fixture`
  - `cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_repository --features runtime-fixture`
  - `cargo test --manifest-path src-tauri/Cargo.toml --test ai_worker_contract --features runtime-fixture`
  - `cargo test --manifest-path src-tauri/Cargo.toml --test runtime_readiness --features runtime-fixture`
  - `cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime --features runtime-fixture`
  - `cargo test --manifest-path src-tauri/Cargo.toml --test quantix_setup --features runtime-fixture`
  - `npx vitest run src/AiConnectionsSettings.test.tsx src/ApplicationSettings.test.tsx src/applicationAiSelectionReadiness.test.ts src/quantixHost.test.ts src/ManagerWorkspace.test.tsx`
- [ ] Run repository gates: `npm run format:check`, `npm run check`, `npm test`, and `npm run verify`; expect success.
- [ ] Run binding export twice and `git diff --exit-code` after the first intentional generated state is staged; expect no second change.
- [ ] Run the final production-source legacy audit from Layer 1C; expect zero production references.
- [ ] Use `superpowers:requesting-code-review` for the entire Layer 1 range. Require reviewers to trace create → vault → probe → active reference → run snapshot → worker → terminal result and every failure/secret boundary.
- [ ] Apply valid findings and rerun the complete gate.
- [ ] Use `superpowers:verification-before-completion`; cite command output rather than claiming success from inspection.
- [ ] Do not run live qualification or a production desktop build. Record those as explicit release-stage work after the Engineer opts in with a real connection and the account-login commercial gate is satisfied.
- [ ] Commit any final corrections, require a clean working tree, and hand off Layer 1 as the prerequisite for the separately approved Layer 2 dynamic-team plan.
