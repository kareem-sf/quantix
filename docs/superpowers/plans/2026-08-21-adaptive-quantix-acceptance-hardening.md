# Adaptive Quantix: Acceptance Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove with deterministic, restart, Windows-native, privacy, and Product Acceptance evidence that adaptive Quantix isolates failures, never replays uncertain work, and never lets automatic repair mutate canonical Tender facts.

**Architecture:** Extend the existing deterministic provider fixture and Product Acceptance driver instead of creating another test framework. Faults are closed enums compiled only for tests or the existing `runtime-fixture` feature. Focused integration tests exercise Doctor and restart behavior through public Host commands. The deterministic Product Acceptance suite gains one required `resilience` area, so aggregate, private Windows, native-package, and public release gates inherit the new hard requirement.

**Tech Stack:** Rust, Tokio tests, existing runtime fixtures, tempfile, Windows file-handle APIs already present in the repository, diagnostics ZIP support, existing Product Acceptance and release-gate types.

**Spec:** `docs/superpowers/specs/2026-08-21-adaptive-quantix-doctor-design.md`

## Global Constraints

- Execute this plan only after the contract-evolution, dynamic-metadata, health-and-repair, and resilient-UX plans are green.
- Reuse `ProviderFailureCategory`, `ReadinessState`, `QuantixDoctorReport`, `AutomaticRepairResult`, and the domain-owned `ResolveIndeterminateAgentRunCommand`. Do not create parallel failure or recovery enums.
- Fault injection is available only under `#[cfg(any(test, feature = "runtime-fixture"))]`. No user-controlled production fault switch, environment variable, hidden IPC command, or query parameter is allowed.
- A possibly accepted provider Turn is always persisted as `Indeterminate`; restart never replays it; only an attributable Engineer recovery command may permit a linked retry or close it.
- Windows cleanup tests use an exact temporary workspace path. They never recursively remove a computed broad path.
- Privacy assertions inspect decompressed support-bundle entries, not just compressed archive bytes.
- Add no schema migration and no compatibility path. Existing immutable records remain unchanged.
- Normal development ends with `npm run verify`. Production and native package builds remain explicit release-stage operations and are not run by this plan.

---

### Task 1: Extend the deterministic provider with exact failure boundaries

**Files:**

- Modify: `src-tauri/src/agent_runtime.rs`
- Modify: `src-tauri/tests/agent_runtime.rs`
- Modify: `src-tauri/tests/support/runtime_fixture.rs`

**Interfaces:**

- Extends the existing test-only `DeterministicProviderOutcome` with:

```rust
ProtocolInvalidBeforeAcceptance,
ProvenanceInvalidAfterAcceptance,
OutcomeUnknownAfterAcceptance,
```

- Produces exact persisted facts:
  - protocol invalid: `Failed`, `ProtocolInvalid`, `retry_safe = true`, no Turn ref;
  - provenance invalid: `Failed`, `OutputInvalid`, `retry_safe = false`, accepted Turn ref, no proposed result;
  - unknown outcome: `Indeterminate`, `OutcomeUnknown`, `retry_safe = false`, accepted Turn ref, no proposed result.

- [ ] **Step 1: Write failing tests beside the existing deterministic-provider test.**

```rust
#[tokio::test]
async fn deterministic_failure_boundaries_preserve_retry_truth() {
    let (host, tender) = deterministic_host_and_tender();

    let protocol = host
        .run_bootstrap_agent_with_deterministic_provider(
            RunBootstrapAgentCommand {
                tender_id: tender.tender_id.clone(),
                retry_of_run_id: None,
            },
            DeterministicProviderOutcome::ProtocolInvalidBeforeAcceptance,
        )
        .await
        .expect("persist protocol failure");
    assert_eq!(protocol.state, AgentRunState::Failed);
    assert_eq!(protocol.provider_turn_ref, None);
    assert_eq!(
        protocol.failure.as_ref().map(|failure| failure.category),
        Some(ProviderFailureCategory::ProtocolInvalid)
    );
    assert!(protocol.failure.as_ref().is_some_and(|failure| failure.retry_safe));

    let provenance = host
        .run_bootstrap_agent_with_deterministic_provider(
            RunBootstrapAgentCommand {
                tender_id: tender.tender_id.clone(),
                retry_of_run_id: None,
            },
            DeterministicProviderOutcome::ProvenanceInvalidAfterAcceptance,
        )
        .await
        .expect("persist provenance failure");
    assert_eq!(provenance.state, AgentRunState::Failed);
    assert!(provenance.provider_turn_ref.is_some());
    assert!(provenance.proposed_result.is_none());
    assert!(provenance.failure.as_ref().is_some_and(|failure| !failure.retry_safe));

    let unknown = host
        .run_bootstrap_agent_with_deterministic_provider(
            RunBootstrapAgentCommand {
                tender_id: tender.tender_id,
                retry_of_run_id: None,
            },
            DeterministicProviderOutcome::OutcomeUnknownAfterAcceptance,
        )
        .await
        .expect("persist uncertain Turn");
    assert_eq!(unknown.state, AgentRunState::Indeterminate);
    assert!(unknown.provider_turn_ref.is_some());
    assert_eq!(
        unknown.failure.as_ref().map(|failure| failure.category),
        Some(ProviderFailureCategory::OutcomeUnknown)
    );
}
```

- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml --lib deterministic_failure_boundaries_preserve_retry_truth --features runtime-fixture`.
- Expected: FAIL because the three outcome variants do not exist.
- [ ] **Step 3: Add the closed enum variants and exact executions.** The new match arms use only existing `ProviderExecution`, `ProviderFailure`, and `indeterminate_execution`:

```rust
DeterministicProviderOutcome::ProtocolInvalidBeforeAcceptance => failed_execution(
    ProviderFailure::new(
        ProviderFailureCategory::ProtocolInvalid,
        true,
        "Retry only after provider readiness is re-established.",
        Some("The deterministic provider rejected the protocol before Turn acceptance."),
    ),
    Instant::now(),
),
DeterministicProviderOutcome::ProvenanceInvalidAfterAcceptance => ProviderExecution {
    state: AgentRunState::Failed,
    provider_thread_ref: Some(format!("acceptance-thread-{}", prepared.run_id)),
    provider_turn_ref: Some(format!("acceptance-turn-{}", prepared.run_id)),
    events: vec![PendingProviderEvent::new(
        ProviderEventKind::Terminal,
        "Deterministic candidate provenance validation failed",
        Some("quantix-deterministic-provider-v1"),
    )],
    usage: ProviderUsage::default(),
    failure: Some(ProviderFailure::new(
        ProviderFailureCategory::OutputInvalid,
        false,
        "Review the rejected candidate provenance before starting separate work.",
        Some("Candidate provenance did not satisfy the declared output contract."),
    )),
    candidate_payload_json: None,
},
DeterministicProviderOutcome::OutcomeUnknownAfterAcceptance => indeterminate_execution(
    &format!("acceptance-thread-{}", prepared.run_id),
    Some(format!("acceptance-turn-{}", prepared.run_id)),
    turn_acceptance_unknown(),
    Instant::now(),
),
```

- [ ] **Step 4: Add equivalent named runtime fixture scenarios** (`protocol-before-turn`, `provenance-after-turn`, `outcome-unknown-after-turn`) only where the integration test needs the real adapter boundary. Fixture scripts emit the existing provider protocol; they do not add an application runtime switch.
- [ ] **Step 5: Run** the focused unit test and `cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime --features runtime-fixture malformed_protocol_before_turn_acceptance_is_a_retryable_failure`. Expected: PASS.
- [ ] **Step 6: Commit** `test: add deterministic Doctor fault boundaries`

---

### Task 2: Prove typed Doctor findings and capability isolation end to end

**Files:**

- Create: `src-tauri/tests/doctor_acceptance.rs`
- Modify: `src-tauri/tests/support/runtime_fixture.rs`

**Interfaces:**

- Consumes: public `QuantixHost` commands, `inspect_quantix_doctor`, the runtime fixture, and stable finding codes from the health-and-repair slice.
- Verifies exact codes:
  - `provider_protocol_invalid`
  - `candidate_provenance_invalid`
  - `provider_turn_indeterminate`
- Verifies unrelated Tender records and registered Files remain readable.

- [ ] **Step 1: Create the integration harness.** Reuse the setup platform, fixture installer, and AI-binding code from `src-tauri/tests/agent_runtime.rs`, but let each test create its own named Tenders:

```rust
struct DoctorHarness {
    _root: tempfile::TempDir,
    application_home: PathBuf,
    resources: PathBuf,
    codex: PathBuf,
    host: QuantixHost,
}

impl DoctorHarness {
    fn new(scenario: &str) -> Self {
        let root = tempfile::tempdir().expect("temporary Doctor acceptance home");
        let application_home = root.path().join(".quantix");
        let resources = root.path().join("resources");
        let codex = install_codex_fixture(&resources, scenario);
        let host = QuantixHost::with_setup_platform_and_runtime(
            &application_home,
            Arc::new(ReadySetupPlatform),
            RuntimeLayout::bundled(resources.clone()),
        );
        host.accept_runtime_fixture();
        assert_eq!(ensure_quantix_setup(&host).state, SetupState::Ready);
        Self { _root: root, application_home, resources, codex, host }
    }

    fn create_tender(&self, name: &str) -> TenderSummary {
        let tender = self
            .host
            .create_tender(CreateTenderCommand { name: name.into() })
            .expect("create Doctor acceptance Tender");
        bind_current_ai_selection(&self.host, &tender.tender_id);
        tender
    }

    fn restart(&self) -> QuantixHost {
        let host = QuantixHost::with_setup_platform_and_runtime(
            &self.application_home,
            Arc::new(ReadySetupPlatform),
            RuntimeLayout::bundled(self.resources.clone()),
        );
        host.accept_runtime_fixture();
        host
    }
}
```

Create two Tenders and register one content object in each. `set_scenario` writes only `self.codex.with_extension("agent-scenario")`, matching the existing fixture.
- [ ] **Step 2: Write the failing isolation test.**

```rust
#[tokio::test]
async fn doctor_reports_exact_agent_failures_without_blocking_another_tender() {
    let mut harness = DoctorHarness::new("protocol-before-turn");
    let affected = harness.create_tender("Affected Tender");
    let healthy = harness.create_tender("Healthy Tender");
    harness.register_text(&affected.tender_id, "affected.txt", b"affected");
    harness.register_text(&healthy.tender_id, "healthy.txt", b"healthy");

    let failed = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: affected.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("persist protocol failure");
    assert_eq!(failed.state, AgentRunState::Failed);

    let report = harness
        .host
        .inspect_quantix_doctor(InspectQuantixDoctorCommand {
            tender_id: Some(affected.tender_id.clone()),
        })
        .await
        .expect("inspect affected Tender");
    let finding = finding(&report, "provider_protocol_invalid");
    assert_eq!(finding.readiness_state, ReadinessState::Blocked);
    assert_eq!(finding.tender_id.as_deref(), Some(affected.tender_id.as_str()));
    assert!(!finding.correlation_id.is_empty());
    assert_eq!(finding.retry_safety, RetrySafety::SafeBeforeExternalAcceptance);

    let additional = harness
        .host
        .register_tender_content(RegisterTenderContentCommand {
            tender_id: healthy.tender_id.clone(),
            logical_id: "healthy-after-provider-failure".into(),
            media_type: "text/plain".into(),
            bytes: b"still usable".to_vec(),
        })
        .expect("unrelated Tender file work remains usable");
    assert_eq!(additional.size_bytes, 12);
    assert_eq!(
        harness
            .host
            .inspect_capability_preflight(PreflightRequest::for_tender(
                &healthy.tender_id,
                [Capability::RegisteredFiles],
            ))
            .expect("inspect healthy Tender")[0]
            .readiness_state,
        ReadinessState::Ready,
    );
}
```

- [ ] **Step 3: Add table-driven variants** for the provenance and unknown-outcome scenarios. Assert the provenance finding is `candidate_provenance_invalid`; the unknown finding is `provider_turn_indeterminate`, `ReviewRequired`, and `NeverReplayPossibleAcceptance`.
- [ ] **Step 4: Run** `cargo test --manifest-path src-tauri/Cargo.toml --test doctor_acceptance --features runtime-fixture`.
- Expected: FAIL until all stable Doctor code mappings and preflight projections from the health-and-repair slice are wired to persisted Agent Run facts.
- [ ] **Step 5: Make only test-fixture corrections required to reach the public boundaries.** Reuse the health plan's `#[cfg(any(test, feature = "runtime-fixture"))]` recognition of the one exact deterministic provenance summary; do not broaden the production `OutputInvalid` mapper or add an unscoped runtime branch.
- [ ] **Step 6: Re-run** the command. Expected: PASS.
- [ ] **Step 7: Commit** `test: verify Doctor failure isolation`

---

### Task 3: Prove restart never replays a possibly accepted Turn

**Files:**

- Modify: `src-tauri/tests/doctor_acceptance.rs`
- Modify: `src-tauri/tests/agent_runtime.rs` only to reuse or strengthen the existing restart assertion

**Interfaces:**

- Consumes: `AgentRunState::Indeterminate`, startup reconciliation, `ResolveIndeterminateAgentRunCommand`.
- Proves: same run ID and Turn ref survive restart; no new run exists; new execution is blocked; only one exact Engineer disposition changes eligibility.

- [ ] **Step 1: Write the restart test before changing production code.**

```rust
#[tokio::test]
async fn restart_preserves_uncertain_turn_and_never_replays_it() {
    let mut harness = DoctorHarness::new("outcome-unknown-after-turn");
    let tender = harness.create_tender("Restart Tender");
    let uncertain = harness
        .host
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: tender.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect("persist uncertain Turn");
    let original_turn = uncertain.provider_turn_ref.clone();
    let application_home = harness.application_home.clone();
    let resources = harness.resources.clone();
    let _root_guard = harness._root;
    drop(harness.host);

    let restarted = QuantixHost::with_setup_platform_and_runtime(
        &application_home,
        Arc::new(ReadySetupPlatform),
        RuntimeLayout::bundled(resources),
    );
    restarted.accept_runtime_fixture();
    assert_eq!(ensure_quantix_setup(&restarted).state, SetupState::Ready);
    let runs = restarted.inspect_agent_runs(&tender.tender_id).expect("inspect runs");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].run_id, uncertain.run_id);
    assert_eq!(runs[0].provider_turn_ref, original_turn);
    assert_eq!(runs[0].state, AgentRunState::Indeterminate);

    let blocked = restarted
        .run_bootstrap_agent(RunBootstrapAgentCommand {
            tender_id: tender.tender_id.clone(),
            retry_of_run_id: None,
        })
        .await
        .expect_err("unreviewed uncertain Turn blocks dependent execution");
    assert_eq!(blocked.code, TenderErrorCode::InvalidCommand);

    let decision = restarted
        .resolve_indeterminate_agent_run(ResolveIndeterminateAgentRunCommand {
            tender_id: tender.tender_id,
            run_id: uncertain.run_id,
            disposition: AgentRunRecoveryDisposition::CloseTask,
            rationale: "Tendering Engineer reviewed the uncertain Turn and closed the task.".into(),
        })
        .expect("record one Engineer disposition");
    assert_eq!(decision.decided_by, "engineer_user");
}
```

- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml --test doctor_acceptance --features runtime-fixture restart_preserves_uncertain_turn_and_never_replays_it`.
- Expected: PASS if current restart semantics remain intact. If it fails, the observed replay or mutation is a release blocker; fix the owning startup/Agent Run code before proceeding.
- [ ] **Step 3: Add a duplicate-resolution assertion.** A second disposition for the same run must return `InvalidCommand`; it must not append or replace history.
- [ ] **Step 4: Add a Doctor assertion after restart.** The same `provider_turn_indeterminate` code and correlation must remain visible until the Engineer disposition is recorded.
- [ ] **Step 5: Run** both this test and the existing `src-tauri/tests/agent_runtime.rs` restart/retry tests. Expected: PASS.
- [ ] **Step 6: Commit** `test: prove uncertain turns are never replayed`

---

### Task 4: Exercise Windows locked-workspace cleanup and idempotency

**Files:**

- Modify: `src-tauri/tests/doctor_acceptance.rs`
- Modify: `src-tauri/Cargo.toml` (enable the existing `windows` dependency's `Win32_Foundation` feature)
- Verify: `src-tauri/src/host.rs`
- Verify: `src-tauri/src/tender_store/agent_records.rs`

**Interfaces:**

- Consumes: `QuantixDoctorRepairCommand { code: "safe_repair_all", action: RunSafeRepairs, target: Application, .. }`, `AutomaticRepairAction::RemoveEmptyResidualWorkspace`, and the Doctor report.
- Verifies: locked cleanup fails truthfully, another Tender remains ready, release of the handle permits bounded cleanup, and a third current-state inspection returns no eligible action.

- [ ] **Step 1: Add a Windows-only directory-handle helper.** Use the repository's existing `windows` dependency with `FILE_FLAG_BACKUP_SEMANTICS`; omit `FILE_SHARE_DELETE` so the exact empty directory cannot be removed. Enable `Win32_Foundation` alongside the existing `Win32_Storage_FileSystem` feature:

```rust
#[cfg(windows)]
struct LockedDirectory(windows::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for LockedDirectory {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
fn lock_against_delete(path: &Path) -> LockedDirectory {
    use std::{iter, os::windows::ffi::OsStrExt};
    use windows::{
        core::PCWSTR,
        Win32::Storage::FileSystem::{
            CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_GENERIC_READ,
            FILE_SHARE_READ, OPEN_EXISTING,
        },
    };
    let wide = path.as_os_str().encode_wide().chain(iter::once(0)).collect::<Vec<_>>();
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_GENERIC_READ.0,
            FILE_SHARE_READ,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )
    }
    .expect("lock exact empty residual directory");
    LockedDirectory(handle)
}
```

- [ ] **Step 2: Write the failing Windows acceptance test.** Create the residual through the repair module's test fixture API so it carries the exact verified directory-only/empty classification; never hand an arbitrary path to the repair command.

```rust
#[cfg(windows)]
#[test]
fn locked_empty_workspace_is_isolated_then_repaired_once() {
    let harness = DoctorHarness::new("success");
    let affected = harness.create_tender("Locked Workspace Tender");
    let healthy = harness.create_tender("Unrelated Tender");
    let residual = harness.host
        .stage_verified_empty_workspace_fixture(&affected.tender_id)
        .expect("stage verified empty workspace");
    assert_eq!(std::fs::read_dir(&residual).unwrap().count(), 0);
    let lock = lock_against_delete(&residual);

    let before = harness.inspect_doctor(&affected.tender_id);
    let first = harness.run_safe_repairs(&before);
    assert!(first.automatic_actions.iter().any(|action| {
        action.action == AutomaticRepairAction::RemoveEmptyResidualWorkspace
            && action.outcome == AutomaticRepairOutcome::Failed
    }));
    assert!(residual.exists());
    assert_eq!(
        harness.preflight(&healthy.tender_id).readiness_state,
        ReadinessState::Ready,
    );

    drop(lock);
    let second = harness.run_safe_repairs(&first);
    assert!(!residual.exists());
    assert!(second.automatic_actions.iter().any(|action| {
        action.action == AutomaticRepairAction::RemoveEmptyResidualWorkspace
            && action.outcome == AutomaticRepairOutcome::Repaired
    }));
    let third = harness.run_safe_repairs(&second);
    assert!(third.automatic_actions.is_empty());
}
```

- [ ] **Step 3: Run on Windows** `cargo test --manifest-path src-tauri/Cargo.toml --test doctor_acceptance --features runtime-fixture locked_empty_workspace_is_isolated_then_repaired_once`.
- Expected: FAIL if cleanup is unbounded, hides the lock failure, affects the other Tender, or performs a second mutation.
- [ ] **Step 4: If required, correct only the named safe repair.** It must re-inspect the exact path, require directory-only/empty classification, use a bounded attempt, return `Failed` while locked, and return `NotNeeded` after successful cleanup. It must not quarantine or delete data-bearing content automatically.
- [ ] **Step 5: Add canonical invariants.** Capture Tender revision, record count, profile-head digests, approval count, and Audit Event count before and after all three repair runs; assert equality.
- [ ] **Step 6: Re-run** the focused Windows test. Expected: PASS.
- [ ] **Step 7: Commit** `test: verify bounded Windows workspace repair`

---

### Task 5: Prove Doctor and support-bundle redaction against hostile sentinels

**Files:**

- Modify: `src-tauri/tests/doctor_acceptance.rs`
- Verify: `src-tauri/src/diagnostics.rs`

**Interfaces:**

- Consumes: Doctor report serialization and `export_diagnostics_support_bundle`.
- Proves no Tender content, prompt, response, credential, raw path, or hidden-reasoning sentinel appears in any decompressed bundle entry.

- [ ] **Step 1: Write a hostile sentinel test.** Register the sentinel as Tender content and inject it only into fixture-side raw provider stderr/input; operational diagnostics must store bounded static summaries instead.

```rust
#[test]
fn doctor_and_support_bundle_exclude_tender_and_provider_content() {
    const SENTINELS: [&str; 6] = [
        "ACME_SECRET_TENDER_VALUE",
        "PROMPT_SENTINEL_DO_NOT_EXPORT",
        "RESPONSE_SENTINEL_DO_NOT_EXPORT",
        "sk-test-credential-sentinel",
        "C:\\Customers\\SecretBid\\pricing.xlsx",
        "HIDDEN_REASONING_SENTINEL",
    ];
    let harness = DoctorHarness::with_hostile_provider_output(&SENTINELS);
    let tender = harness.create_tender("Redaction Tender");
    harness.register_text(&tender.tender_id, "secret.txt", SENTINELS[0].as_bytes());
    harness.run_protocol_failure(&tender.tender_id);

    let report_json = serde_json::to_string(&harness.inspect_doctor(&tender.tender_id)).unwrap();
    for sentinel in SENTINELS {
        assert!(!report_json.contains(sentinel));
    }

    let bundle = harness.export_support_bundle(&tender.tender_id);
    let file = std::fs::File::open(bundle.path).expect("open support bundle");
    let mut archive = zip::ZipArchive::new(file).expect("read support bundle zip");
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).expect("read bundle entry");
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut bytes).expect("read entry bytes");
        let text = String::from_utf8_lossy(&bytes);
        for sentinel in SENTINELS {
            assert!(!text.contains(sentinel), "{} leaked into {}", sentinel, entry.name());
        }
    }
}
```

- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml --test doctor_acceptance --features runtime-fixture doctor_and_support_bundle_exclude_tender_and_provider_content`.
- Expected: PASS if the current diagnostics policy and new Doctor fields remain bounded. Any leak is a release blocker.
- [ ] **Step 3: Assert positive evidence.** The bundle must contain the stable finding code and opaque correlation ID, proving the test did not merely export an empty archive.
- [ ] **Step 4: If a gap exists, fix the fact at ingestion.** Replace dynamic diagnostic summaries with closed static text and normalized codes. Do not add an output-only string blacklist that leaves raw content stored on disk.
- [ ] **Step 5: Re-run** the test and existing diagnostics redaction/retention tests. Expected: PASS.
- [ ] **Step 6: Commit** `test: harden Doctor diagnostic redaction`

---

### Task 6: Make resilience a deterministic Product Acceptance hard gate

**Files:**

- Modify: `src-tauri/src/acceptance.rs`
- Modify: `src-tauri/tests/release_configuration.rs`
- Modify: acceptance unit tests in `src-tauri/src/acceptance.rs`

**Interfaces:**

- Extends `REQUIRED_DETERMINISTIC_AREAS` from 15 to 16 with `"resilience"`.
- Produces one `AcceptanceCheckResult { area: "resilience", ... }` based on executed Host evidence, not a static declaration.
- Carries through existing aggregate/private/native/release gates because they already validate all required deterministic areas and exact manifest hashes.

- [ ] **Step 1: Write the failing required-area test.**

```rust
#[test]
fn deterministic_acceptance_requires_resilience_evidence() {
    assert!(REQUIRED_DETERMINISTIC_AREAS.contains(&"resilience"));
    let measurement = failed_driver_measurement(
        Vec::new(),
        true,
        true,
        Instant::now(),
    );
    assert!(measurement.checks.iter().any(|check| check.area == "resilience"));
}
```

- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml --lib deterministic_acceptance_requires_resilience_evidence --features runtime-fixture`.
- Expected: FAIL because `resilience` is not required or measured.
- [ ] **Step 3: Add an internal resilience measurement to `drive_candidate_host_lifecycle`.** Execute, on a separate acceptance Tender, the deterministic protocol failure, provenance rejection, uncertain Turn, Doctor inspection, restart-equivalent reinspection, and explicit Engineer close. Return one boolean only after checking:

```rust
let resilience_passed = protocol.state == AgentRunState::Failed
    && protocol.failure.as_ref().is_some_and(|failure| {
        failure.category == ProviderFailureCategory::ProtocolInvalid && failure.retry_safe
    })
    && provenance.state == AgentRunState::Failed
    && provenance.proposed_result.is_none()
    && uncertain.state == AgentRunState::Indeterminate
    && uncertain.provider_turn_ref.is_some()
    && doctor_codes.contains("provider_protocol_invalid")
    && doctor_codes.contains("candidate_provenance_invalid")
    && doctor_codes.contains("provider_turn_indeterminate")
    && no_automatic_canonical_change
    && explicit_disposition.decided_by == "engineer_user";
checks.push(measured_check(
    "resilience",
    resilience_passed,
    "measured typed Doctor findings, scoped isolation, idempotent safe repair, restart preservation, and Engineer-only uncertain-Turn disposition",
));
```

Do not mark the check passed from unit-test names, compile flags, or the presence of a Doctor report.
- [ ] **Step 4: Extend `failed_driver_measurement`.** It must always include a failed `resilience` check so early lifecycle failure cannot accidentally omit the required area and produce ambiguous evidence.
- [ ] **Step 5: Add aggregate rejection coverage.** A synthetic deterministic run without a passing resilience check must produce `failed:resilience` or `missing:resilience`; private qualification must not infer `full_lifecycle` from that run.
- [ ] **Step 6: Run** `cargo test --manifest-path src-tauri/Cargo.toml --lib acceptance --features runtime-fixture` and `cargo test --manifest-path src-tauri/Cargo.toml --test release_configuration --features runtime-fixture`. Expected: PASS.
- [ ] **Step 7: Commit** `test: gate product acceptance on resilience`

---

### Task 7: Run the complete development and acceptance verification matrix

**Files:**

- Verify only; modify only files that fail a named invariant from Tasks 1–6.

**Interfaces:**

- Verifies deterministic test evidence and repository gates. It does not create release records or build a production package.

- [ ] **Step 1: Run focused adaptive suites:**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test contract_evolution --features runtime-fixture
cargo test --manifest-path src-tauri/Cargo.toml --test tender_records --features runtime-fixture
cargo test --manifest-path src-tauri/Cargo.toml --test doctor_acceptance --features runtime-fixture
cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime --features runtime-fixture
```

Expected: PASS.
- [ ] **Step 2: Run renderer resilience tests:**

```powershell
npm run test:renderer -- src/ProjectCharacteristicFields.test.tsx src/TenderRecordsPanel.test.tsx src/SurfaceErrorBoundary.test.tsx src/DoctorIndicator.test.tsx src/ApplicationSettings.test.tsx src/ManagerWorkspace.test.tsx
```

Expected: PASS.
- [ ] **Step 3: Run formatting and static checks:** `npm run format:check` then `npm run check`. Expected: PASS.
- [ ] **Step 4: Run deterministic tests and regenerate Rust-owned DTOs:** `npm test`. Expected: PASS and no unstaged generated-binding drift after intended outputs are staged.
- [ ] **Step 5: Run the repository development gate:** `npm run verify`. Expected: PASS.
- [ ] **Step 6: Inspect repository state:** `git diff --check` and `git status --short`. Verify unrelated maintainer WIP is untouched and all generated declarations changed by these slices are included.
- [ ] **Step 7: Record the release-stage follow-up without executing it.** The release candidate must later pass, on the exact packaged artifact and clean homes, `acceptance:deterministic`, five opted-in private Windows runs, `acceptance:native`, and `acceptance:release`. This task does not manufacture those external records.
- [ ] **Step 8: Commit** `test: complete adaptive Quantix acceptance hardening`
