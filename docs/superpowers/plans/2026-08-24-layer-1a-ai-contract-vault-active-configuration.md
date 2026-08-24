# Layer 1A AI Contract, Vault, and Active Configuration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish the versioned multi-provider domain contract, user-scoped encrypted connection vault, revision rules, and optional global Active AI Configuration while leaving the current visible AI runtime operational until Layer 1C.

**Architecture:** New `ai` modules own strict connection/capability DTOs and all connection persistence. The vault is the source of truth for complete connection revisions and secrets. `installation.sqlite` stores only general preferences and an optional non-secret active reference. New code is tested independently and is not mounted in the renderer or Agent Run path in this subplan.

**Tech Stack:** Rust 1.97.1, Serde, Garde, rusqlite, fs4, Windows 0.62.2 DPAPI APIs, zeroize 1.9.0, SHA-256, ts-rs.

**Spec:** `docs/superpowers/specs/2026-08-24-layer-1-ai-connection-foundation-design.md` and the parent plan `docs/superpowers/plans/2026-08-24-layer-1-ai-connection-foundation.md`.

## Global Constraints

- Follow every parent-plan constraint. Do not create a temporary default, fallback, `auth.json`, keyring path, or migration.
- No worker/network call is implemented here. Tests inject signed-looking fake probe evidence without real credentials or providers.
- Keep the old UI/runtime compiling and working. Do not register the new renderer commands or change live schemas in this subplan.
- New public Rust types exist only to avoid dead-code scaffolding and support integration tests; renderer bindings are generated only after the Layer 1C command surface is final.
- Internal stored/decrypted secret types must have redacted `Debug`, no `Display`, no `TS`, and `ZeroizeOnDrop`. Write-only IPC request DTOs may derive `Deserialize` and `TS` so the renderer can submit a new secret, but never `Serialize` into a response and must also use redacted `Debug`/`ZeroizeOnDrop`.
- Every command and persisted object denies unknown fields and enforces byte/count limits.

---

### Task 1: Record the replacement decision and exact framework boundary

**Files:**

- Create: `docs/adr/0017-run-multiple-ai-connections-through-one-active-configuration.md`
- Create: `docs/research/ai-worker-runtime-selection.md`
- Modify: `docs/adr/0004-run-agent-profiles-through-host-controlled-codex-threads.md`
- Modify: `docs/adr/0005-compose-tender-teams-through-controlled-capability-demands.md`
- Modify: `docs/adr/0008-keep-codex-behind-a-quantix-owned-ai-provider-contract.md`
- Modify: `docs/adr/0009-run-one-local-host-over-self-contained-tender-stores.md`
- Modify: `docs/adr/0010-qualify-v0-through-layered-product-acceptance.md`
- Modify: `docs/adr/0012-connect-provider-neutral-ai-without-silent-fallback.md`
- Modify: `docs/adr/0014-scope-ai-execution-and-asa-operations-per-tender.md`
- Modify: `docs/adr/0016-connect-chatgpt-through-quantix-owned-oauth.md`
- Modify: `docs/research/agent-framework-selection.md`

**Decision text must establish:**

- Direct Codex app-server 0.149.1 is the account-backed product integration; the Python SDK is not the production security boundary.
- Quantix privately prototypes the 0.149.1 `chatgptAuthTokens` seam only to pass Host-owned, vault-backed access tokens into the pinned app-server. Its schema says unstable, OpenAI-internal-only, and “do not use,” so account activation is non-shippable without written OpenAI approval. Existing browser/device OAuth and refresh code moves behind `ai::codex_auth` at cutover.
- Quantix never constructs or calls the private ChatGPT execution backend. The official Codex runtime owns its upstream service route as an opaque implementation detail.
- Account login remains blocked from public release until OpenAI approves the Quantix client/integration. Protocol removal makes only that connection incompatible.
- Pydantic AI is adopted only inside the disposable general-provider worker because the newly approved second-provider condition invalidates the old v0 rejection. It is not a Host workflow or durability engine.
- Clarify the vault rule to say persistent secrets **at rest** exist only in the encrypted vault; the exact Host and assigned worker may hold the selected secret in memory/private IPC during an authorized operation.

**Interfaces:**

- Consumes: focused Layer 1 design and existing ADR status front matter.
- Produces: accepted ADR 0017 and a dated research record; no runtime behavior changes.

- [ ] **Create ADR 0017 with exact status/supersession header**

```yaml
---
status: accepted
supersedes:
  - 0016-connect-chatgpt-through-quantix-owned-oauth
  - 0014-scope-ai-execution-and-asa-operations-per-tender#ai-selection
  - 0009-run-one-local-host-over-self-contained-tender-stores#provider-runtime
  - 0010-qualify-v0-through-layered-product-acceptance#provider-evidence
---
```

- [ ] Write ADR 0017 with `status: accepted`, exact supersession scope, consequences, rejected alternatives, release gate, and primary-source evidence.
- [ ] Mark ADR 0016 superseded in full; mark only the provider-selection consequences of ADR 0014 and the Codex runtime consequences of ADRs 0004/0008 superseded. Preserve Host permission, run binding, and canonical-state principles.
- [ ] Record that ADR 0017 supersedes ADR 0009's direct-HTTPS/cross-platform worker consequences and ADR 0010's ChatGPT-direct acceptance fields while preserving the Rust Host, single writer, Windows qualification, and layered gates. Note that ADR 0005's fixed Bootstrap Team is superseded by the master design but remains an implementation fact until Layer 2; Layer 1 must not create a compatibility roster.
- [ ] Update the old framework research with a dated supersession note; do not rewrite its historical analysis.
- [ ] In the new research document, compare direct app-server versus SDK, and Pydantic AI versus Vercel AI SDK versus raw Rust. Record exact versions, runtime cost, tool boundary, retry behavior, and licenses.
- [ ] Run `npx prettier --check docs/adr docs/research docs/superpowers/specs docs/superpowers/plans`; expect success.
- [ ] Commit as `docs: decide the layer one ai runtime boundary`.

---

### Task 2: Define the strict connection and provider-runtime contract

**Files:**

- Create: `src-tauri/src/ai/mod.rs`
- Create: `src-tauri/src/ai/contract.rs`
- Modify: `src-tauri/src/lib.rs`

**Core contract:**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AiConnectionMethod {
    AccountLogin,
    DirectProviderKey,
    OpenAiCompatible,
    AnthropicCompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderKind {
    Codex,
    OpenAi,
    Anthropic,
    GoogleGemini,
    XAi,
    OpenAiCompatible,
    AnthropicCompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySupport {
    Supported,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AiReasoningSelection {
    Unsupported,
    Effort { id: String },
}
```

**Interfaces:**

- Consumes: Serde/Garde/ts-rs and the parent-plan limits.
- Produces: `validate_method_provider(AiConnectionMethod, AiProviderKind) -> Result<(), AiContractError>`, `CompatibleEndpointConfiguration::parse(base_url, credential, headers, query, model_id)`, `catalogue_sha256(&AiProbeEvidence) -> Result<String, AiContractError>`, and every final DTO named below.

Define and test these additional types:

- `AiConnectionId`, semantic `AiConnectionRevision` serialized as `execution_revision`, and secret-only `CredentialGeneration` validated newtypes.
- `AiConnectionConfiguration` tagged union enforcing valid method/provider combinations.
- `CompatibleEndpointConfiguration` with normalized base URL, credential kind, custom header names, custom query names, and explicit model ID.
- `AiCapabilitySet` with tri-state streaming, tools, images, reasoning, and reroute detection; exact `AiStructuredOutputMode::{NativeJsonSchema, Tool, Prompted, Unsupported, Unknown}`; plus optional context size.
- `AiReasoningOption`, `AiModelView`, `AiProbeEvidence`, and deterministic catalogue SHA-256.
- `AiConnectionStatus::{Untested, Testing, Ready, Disabled, AuthenticationRequired, TemporarilyUnavailable, Incompatible}` and credential-free `AiConnectionView` with `secret_configured: bool` only.
- `ActiveAiReadiness::{NotConfigured, Ready, StaleRevision, Disabled, AuthenticationRequired, WorkerUnavailable, CapabilityChanged, VaultUnavailable}` and credential-free `ActiveAiConfigurationView`.
- `AccountLoginProgress::{Idle, OpeningBrowser, AwaitingBrowser, AwaitingDeviceCode, Completing, Failed}`; only `AwaitingDeviceCode` may carry the transient verification URL/user code.
- `ActiveAiConfiguration` pinning connection ID/revision, provider, endpoint fingerprint, model ID, reasoning, adapter version, catalogue hash, capabilities, data destination, and activation time.
- `AiRuntimeRequest`, `AiRuntimeEvent`, `AiRuntimeResult`, `AiRuntimeUsage`, `AiRuntimeRateLimit`, and `AiRuntimeFailure`. These are the final names; do not retain `ProviderEvent`/`ProviderUsage`/`ProviderFailure` aliases at cutover.
- Failure categories covering authentication, quota/rate limit, capability missing, invalid request, protocol drift, timeout, cancellation, transport, invalid output, model reroute, and indeterminate outcome.

- [ ] **Step 1: Create the module and write the red method/provider matrix**

```rust
#[test]
fn method_provider_matrix_is_closed() {
    use AiConnectionMethod::*;
    use AiProviderKind::*;
    let cases = [
        (AccountLogin, Codex, true),
        (DirectProviderKey, OpenAi, true),
        (DirectProviderKey, Anthropic, true),
        (DirectProviderKey, GoogleGemini, true),
        (DirectProviderKey, XAi, true),
        (OpenAiCompatible, OpenAiCompatible, true),
        (AnthropicCompatible, AnthropicCompatible, true),
        (AccountLogin, OpenAi, false),
        (DirectProviderKey, Codex, false),
        (OpenAiCompatible, AnthropicCompatible, false),
    ];
    for (method, provider, valid) in cases {
        assert_eq!(validate_method_provider(method, provider).is_ok(), valid);
    }
}
```

- [ ] **Step 2: Run the contract test and confirm red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib ai::contract::tests::method_provider_matrix_is_closed`

Expected: FAIL because `ai::contract`/`validate_method_provider` does not exist.

- [ ] **Step 3: Implement the closed pairing function and exported enums**

```rust
pub fn validate_method_provider(
    method: AiConnectionMethod,
    provider: AiProviderKind,
) -> Result<(), AiContractError> {
    let valid = matches!(
        (method, provider),
        (AiConnectionMethod::AccountLogin, AiProviderKind::Codex)
            | (AiConnectionMethod::DirectProviderKey, AiProviderKind::OpenAi)
            | (AiConnectionMethod::DirectProviderKey, AiProviderKind::Anthropic)
            | (AiConnectionMethod::DirectProviderKey, AiProviderKind::GoogleGemini)
            | (AiConnectionMethod::DirectProviderKey, AiProviderKind::XAi)
            | (AiConnectionMethod::OpenAiCompatible, AiProviderKind::OpenAiCompatible)
            | (AiConnectionMethod::AnthropicCompatible, AiProviderKind::AnthropicCompatible)
    );
    valid.then_some(()).ok_or(AiContractError::InvalidPairing)
}
```

- [ ] **Step 4: Add a table-driven endpoint/header red test**

```rust
#[test]
fn compatible_endpoint_policy_is_fail_closed() {
    for rejected in [
        "http://localhost:11434/v1",
        "http://10.0.0.2/v1",
        "https://user:pass@example.com/v1",
        "https://example.com/v1?key=value",
        "https://example.com/v1#fragment",
    ] {
        assert!(CompatibleEndpointConfiguration::parse(rejected, vec![], vec![]).is_err());
    }
    assert!(CompatibleEndpointConfiguration::parse("http://127.0.0.1:11434/v1", vec![], vec![]).is_ok());
    assert!(CompatibleEndpointConfiguration::parse("http://[::1]:11434/v1", vec![], vec![]).is_ok());
    for name in ["authorization", "host", "content-length", "proxy-authorization"] {
        assert!(validate_custom_header_name(name).is_err());
    }
}
```

- [ ] **Step 5: Run the endpoint test and confirm red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib ai::contract::tests::compatible_endpoint_policy_is_fail_closed`

Expected: FAIL because endpoint/header validators are missing.

- [ ] **Step 6: Implement the remaining DTOs and pure validators**

Implement the exact types listed above, the parent-plan byte/count limits, NFC label normalization, URL/path-prefix canonicalization, case-insensitive reserved-header checks, and `catalogue_sha256`. Credential/header/query values are never trimmed or included in hashes/views.

```rust
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AiCapabilitySet {
    pub streaming: CapabilitySupport,
    pub tools: CapabilitySupport,
    pub images: CapabilitySupport,
    pub reasoning: CapabilitySupport,
    pub reroute_detection: CapabilitySupport,
    pub structured_output: AiStructuredOutputMode,
    pub context_window_tokens: Option<u64>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AiConnectionView {
    pub connection_id: String,
    pub execution_revision: u64,
    pub method: AiConnectionMethod,
    pub provider: AiProviderKind,
    pub display_name: String,
    pub enabled: bool,
    pub status: AiConnectionStatus,
    pub secret_configured: bool,
    pub models: Vec<AiModelView>,
    pub status_summary: String,
}

pub fn catalogue_sha256(evidence: &AiProbeEvidence) -> Result<String, AiContractError> {
    let semantic = evidence.semantic_projection(); // excludes observed_at and labels
    let bytes = serde_json_canonicalizer::to_vec(&semantic)
        .map_err(|_| AiContractError::InvalidCatalogue)?;
    Ok(sha256_hex(&bytes))
}
```

- [ ] **Step 7: Add and run the secret/default projection test**

```rust
#[test]
fn connection_view_has_no_secret_or_default_surface() {
    let json = serde_json::to_string(&ready_connection_view()).unwrap();
    for forbidden in ["api_key", "access_token", "refresh_token", "header_value", "query_value", "is_default", "recommended"] {
        assert!(!json.contains(forbidden), "forbidden projection field: {forbidden}");
    }
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib ai::contract::tests`

Expected: PASS for pairing, endpoint, DTO round-trip, hash determinism, and projection tests.

- [ ] **Step 8: Format, lint, and commit**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml` then `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --features runtime-fixture -- -D warnings`.

Commit: `git add src-tauri/src/ai src-tauri/src/lib.rs && git commit -m "feat: define the ai connection contract"`

---

### Task 3: Add the user-scoped DPAPI boundary

**Files:**

- Create: `src-tauri/src/ai/windows_dpapi.rs`
- Modify: `src-tauri/src/ai/mod.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`

**Interface:**

```rust
pub fn protect_for_current_user(
    plaintext: zeroize::Zeroizing<Vec<u8>>,
) -> Result<Vec<u8>, VaultCryptoError>;

pub fn unprotect_for_current_user(
    ciphertext: &[u8],
) -> Result<zeroize::Zeroizing<Vec<u8>>, VaultCryptoError>;
```

- Consumes: windows-rs 0.62.2 and `Zeroizing<Vec<u8>>`.
- Produces: current-user DPAPI encrypt/decrypt with no prompt or machine scope.

- [ ] **Step 1: Add dependencies and write the red DPAPI test**

Add `zeroize = { version = "1.9.0", features = ["derive"] }` and windows-rs features `Win32_Foundation`/`Win32_Security_Cryptography`, then add:

```rust
#[cfg(windows)]
#[test]
fn current_user_dpapi_round_trips_and_rejects_corruption() {
    let clear = Zeroizing::new("مفتاح-Quantix-123".as_bytes().to_vec());
    let encrypted = protect_for_current_user(clear).unwrap();
    assert!(!encrypted.windows(7).any(|part| part == b"Quantix"));
    assert_eq!(&*unprotect_for_current_user(&encrypted).unwrap(), "مفتاح-Quantix-123".as_bytes());
    let mut corrupt = encrypted;
    corrupt[corrupt.len() / 2] ^= 0x80;
    assert!(unprotect_for_current_user(&corrupt).is_err());
}
```

- [ ] **Step 2: Run the DPAPI test and confirm red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib ai::windows_dpapi::tests::current_user_dpapi_round_trips_and_rejects_corruption`

Expected: FAIL because the DPAPI functions/module are missing.

- [ ] **Step 3: Implement the guarded windows-rs calls**

```rust
pub fn unprotect_for_current_user(ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>, VaultCryptoError> {
    let input_len = u32::try_from(ciphertext.len()).map_err(|_| VaultCryptoError)?;
    let input = CRYPT_INTEGER_BLOB { cbData: input_len, pbData: ciphertext.as_ptr().cast_mut() };
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    unsafe {
        CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut output)
            .map_err(|_| VaultCryptoError)?;
        let source = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let clear = Zeroizing::new(source.to_vec());
        source.zeroize();
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        Ok(clear)
    }
}
```

Implement the matching protect guard, output cleanup guards, pre-cleanup error capture, length checks, fixed description, and redacted `VaultCryptoError`. Do not add machine-scope or UI flags.

- [ ] **Step 4: Add boundary cases and run green**

Add empty, 1 MiB, truncated, and random-ciphertext cases. Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib ai::windows_dpapi::tests`

Expected: PASS; corrupt inputs return redacted errors.

- [ ] **Step 5: Audit, format, and commit**

Run: `rg -n "CRYPTPROTECT_LOCAL_MACHINE|Credential Manager|keyring" src-tauri/src/ai src-tauri/Cargo.toml` (expect zero production matches), then `cargo fmt --manifest-path src-tauri/Cargo.toml`.

Commit: `git add src-tauri/src/ai src-tauri/Cargo.toml src-tauri/Cargo.lock && git commit -m "feat: protect ai secrets with user scoped dpapi"`

---

### Task 4: Implement the versioned atomic AI connection vault

**Files:**

- Create: `src-tauri/src/ai/vault.rs`
- Create: `src-tauri/tests/ai_connection_vault.rs`
- Modify: `src-tauri/src/ai/mod.rs`
- Modify: `src-tauri/src/setup.rs` only to expose test-safe application-home path validation helpers; do not change schema/version yet.

**Encrypted payload:**

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VaultPayload {
    schema_version: u32, // exactly 1
    mutation_revision: u64,
    connections: BTreeMap<String, StoredAiConnection>,
}
```

The on-disk `ai-connections.vault` bytes are the raw DPAPI ciphertext. The version is inside the encrypted JSON so the complete payload is protected.

**Interfaces:**

- Consumes: `protect_for_current_user`, `unprotect_for_current_user`, exact application-home path, and `StoredAiConnection`.
- Produces: `AiConnectionVault::load() -> Result<VaultLoadState, VaultError>`, CAS `mutate(expected_revision, mutation)`, current-state `mutate_current(mutation)`, and locked read/activation `with_locked_payload(operation)` methods returning secret-free snapshots/errors.

- [ ] **Step 1: Write the red encrypted round-trip/CAS test**

```rust
#[test]
fn encrypted_vault_round_trips_and_rejects_stale_cas() {
    let home = initialized_private_home("vault-cas");
    let vault = AiConnectionVault::new(&home).unwrap();
    assert!(matches!(vault.load().unwrap(), VaultLoadState::Missing));
    let first = vault.mutate(0, |payload| payload.insert(fake_connection("secret-A"))).unwrap();
    let bytes = std::fs::read(home.join("ai-connections.vault")).unwrap();
    assert!(!bytes.windows(8).any(|part| part == b"secret-A"));
    assert!(matches!(
        vault.mutate(0, |payload| payload.insert(fake_connection("secret-B"))),
        Err(VaultError::RevisionConflict)
    ));
    assert_eq!(vault.load().unwrap().ready().unwrap().mutation_revision, first.mutation_revision);
}
```

- [ ] **Step 2: Run the vault test and confirm red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_vault encrypted_vault_round_trips_and_rejects_stale_cas --features runtime-fixture`

Expected: FAIL because `AiConnectionVault` is undefined.

- [ ] **Step 3: Implement load/mutate under the fixed locks**

```rust
pub fn mutate<F>(
    &self,
    expected_revision: u64,
    mutation: F,
) -> Result<VaultSnapshot, VaultError>
where
    F: FnOnce(&mut VaultPayload) -> Result<(), VaultError>,
{
    let _process = vault_mutex().lock().map_err(|_| VaultError::Unavailable)?;
    let lock = self.open_validated_lock()?;
    lock.lock_exclusive().map_err(|_| VaultError::Unavailable)?;
    let mut payload = self.load_payload_locked()?;
    if payload.mutation_revision != expected_revision {
        return Err(VaultError::RevisionConflict);
    }
    mutation(&mut payload)?;
    payload.mutation_revision = payload.mutation_revision.checked_add(1).ok_or(VaultError::Invalid)?;
    self.publish_locked(&payload)?;
    Ok(payload.secret_free_snapshot())
}
```

Implement missing/ready/corrupt/unsupported states, 4 MiB/8 MiB bounds, redacted errors, and zeroize-on-drop payloads.

- [ ] **Step 4: Write the red atomic-publication/path test table**

```rust
#[test]
fn vault_never_treats_invalid_storage_as_empty() {
    for fault in [VaultFault::CorruptCiphertext, VaultFault::TruncatedCiphertext, VaultFault::WrongVersion, VaultFault::ReparsePoint, VaultFault::HardLink] {
        let home = home_with_fault(fault);
        assert!(!matches!(AiConnectionVault::new(&home).unwrap().load(), Ok(VaultLoadState::Missing)));
    }
}
```

- [ ] **Step 5: Run the path test and confirm red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_vault vault_never_treats_invalid_storage_as_empty --features runtime-fixture`

Expected: FAIL until handle-based validation/publication is implemented.

- [ ] **Step 6: Implement Windows publication exactly**

Create the same-directory temp with `create_new`, write/flush/`sync_all`, call `ReplaceFileW` with its target/replacement paths and flags `0` for a validated existing target, and call `MoveFileExW` with `MOVEFILE_WRITE_THROUGH` for first publication. Reject reparse points/directories/ADS/hard links; after ambiguous errors reopen under lock and compare intended ciphertext hash/revision. Never delete the persistent lock file or promote orphan temps.

```rust
match self.validated_target_state()? {
    TargetState::Existing => replace_file_with_supported_flags(&self.path, &staged)?,
    TargetState::Missing => move_file_write_through(&staged, &self.path)?,
}
self.reopen_and_verify(expected_ciphertext_sha256, expected_mutation_revision)?;
```

- [ ] **Step 7: Add contention tests and run green**

Run two vault handles behind a barrier, assert every committed connection exists exactly once and revisions are contiguous, then run: `cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_vault --features runtime-fixture`.

Expected: PASS for round-trip, CAS, faults, contention, atomicity, and redaction.

- [ ] **Step 8: Commit**

Commit: `git add src-tauri/src/ai src-tauri/src/setup.rs src-tauri/tests/ai_connection_vault.rs && git commit -m "feat: add the encrypted ai connection vault"`

---

### Task 5: Implement connection revisions and the optional global active reference

**Files:**

- Create: `src-tauri/src/ai/connections.rs`
- Create: `src-tauri/tests/ai_connection_repository.rs`
- Modify: `src-tauri/src/ai/mod.rs`

**Repository behavior:**

- Create stores one new enabled-but-untested connection revision with no active side effect.
- Rename changes only display metadata.
- Any endpoint, provider, credential placement, direct/custom secret replacement, headers, or query-parameter change increments execution revision and clears prior probe evidence.
- A verified OAuth refresh for the same provider account increments only `credential_generation`; it preserves execution revision and probe evidence. Reauthentication as a different account, direct/custom key replacement, credential placement change, or unverified account identity increments both.
- `record_probe` accepts only evidence for the exact current revision and exact adapter version.
- Activation requires enabled, credential-present, successful current probe evidence, exact model, explicit reasoning/unsupported state, and a matching catalogue hash.
- Disable/disconnect leaves an active reference stale/unavailable. Delete rejects an active connection or any connection referenced by a nonterminal run.
- Views sort by normalized display name then connection ID and never contain secret/header/query values.

**Interfaces:**

- Consumes: `AiConnectionVault`, Layer 1A DTOs, and caller-owned installation SQLite connection.
- Produces: `create_connection`, `rename_connection`, `replace_connection_configuration`, `record_probe`, `set_enabled`, `disconnect`, `delete_connection`, `activate`, `clear_active`, and `inspect` methods on `AiConnectionRepository`.

- [ ] **Step 1: Write the red no-default/activation test**

```rust
#[test]
fn fresh_repository_has_no_default_and_test_never_activates() {
    let fixture = RepositoryFixture::new();
    assert!(fixture.repo.inspect().unwrap().active_configuration.is_none());
    let connection = fixture.repo.create_connection(openai_key_command("sk-test")).unwrap();
    fixture.repo.record_probe(current_probe(&connection, "gpt-test", "low")).unwrap();
    let view = fixture.repo.inspect().unwrap();
    assert!(view.active_configuration.is_none());
    assert_eq!(view.connections[0].models[0].model_id, "gpt-test");
}
```

- [ ] **Step 2: Run and confirm red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_repository fresh_repository_has_no_default_and_test_never_activates --features runtime-fixture`

Expected: FAIL because `AiConnectionRepository` is undefined.

- [ ] **Step 3: Implement create/probe/inspect**

```rust
impl AiConnectionRepository {
    pub fn create_connection(&self, command: CreateAiConnectionCommand) -> Result<AiConnectionView, AiConnectionError> {
        command.validate().map_err(|_| AiConnectionError::InvalidCommand)?;
        self.vault.mutate_current(|payload| {
            payload.insert(StoredAiConnection::from_command(new_connection_id()?, command)?)
        })?;
        self.connection_view_for_last_mutation()
    }

    pub fn record_probe(&self, evidence: AiProbeEvidence) -> Result<AiConnectionView, AiConnectionError> {
        self.vault.mutate_current(|payload| payload.require_exact_revision_mut(&evidence)?.record_probe(evidence))?;
        self.connection_view_for_last_mutation()
    }
}
```

- [ ] **Step 4: Write the red revision/generation table test**

```rust
#[test]
fn semantic_revision_and_credential_generation_are_independent() {
    let fixture = ready_codex_repository();
    let before = fixture.current_record();
    fixture.repo.rotate_same_account_tokens(before.execution_revision, before.credential_generation, rotated_tokens()).unwrap();
    let rotated = fixture.current_record();
    assert_eq!(rotated.execution_revision, before.execution_revision);
    assert_eq!(rotated.credential_generation, before.credential_generation + 1);
    assert!(fixture.repo.inspect().unwrap().active_configuration.is_some());
    fixture.repo.replace_api_key(openai_replacement_command()).unwrap();
    assert!(fixture.repo.inspect().unwrap().readiness == ActiveAiReadiness::StaleRevision);
}
```

- [ ] **Step 5: Run and confirm red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_repository semantic_revision_and_credential_generation_are_independent --features runtime-fixture`

Expected: FAIL until revision/generation mutation rules exist.

- [ ] **Step 6: Implement activation and mutation invariants**

```rust
pub fn activate(&self, command: SetActiveAiConfigurationCommand) -> Result<ApplicationAiSettingsView, AiConnectionError> {
    let _gate = self.connection_gate.lock()?;
    self.vault.with_locked_payload(|payload| {
        let exact = payload.require_activatable(&command)?;
        let mut installation = self.open_installation()?;
        let tx = installation.transaction_with_behavior(TransactionBehavior::Immediate)?;
        store_active_reference(&tx, &ActiveAiConfiguration::from_exact(exact, command)?)?;
        tx.commit()?;
        Ok(())
    })?;
    self.inspect()
}
```

Implement name-only edits, material revision changes, same-account generation changes, explicit reasoning checks, adapter/catalogue staleness, enable/disable/disconnect/delete guards, and 128-bit IDs. Secret-bearing input DTOs are TS/Deserialize-only with redacted `Debug` and zeroize-on-drop; views remain secret-free.

- [ ] **Step 7: Run the full repository suite green**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_repository --features runtime-fixture` and `cargo test --manifest-path src-tauri/Cargo.toml --lib ai`.

Expected: PASS for all four methods, seven routes, revisions, reasoning, staleness, guards, sorting, and projections.

- [ ] **Step 8: Commit**

Commit: `git add src-tauri/src/ai src-tauri/tests/ai_connection_repository.rs && git commit -m "feat: add revisioned ai connections and activation"`

---

### Task 6: Verify and review Layer 1A

- [ ] Run `npm run format:check`; expect success.
- [ ] Run `npm run check`; expect TypeScript and Rust Clippy success.
- [ ] Run `npm test`; expect the current product suite plus new foundation tests to pass.
- [ ] Run `git diff --check`; expect no whitespace errors.
- [ ] Use `superpowers:requesting-code-review` on the Layer 1A diff. Require explicit review of secret projections, unsafe DPAPI code, CAS/lock behavior, URL/header validation, revision invalidation, and absence of defaults/fallbacks.
- [ ] Apply valid findings, rerun the focused tests and `npm run verify`, and commit fixes separately.
- [ ] Confirm the existing visible ChatGPT UI/runtime still works deterministically; Layer 1A must not expose a half-built connection flow.
