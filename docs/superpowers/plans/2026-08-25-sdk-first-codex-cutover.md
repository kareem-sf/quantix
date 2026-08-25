# SDK-First Codex Managed-Runtime Cutover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Quantix's custom ChatGPT OAuth/private backend with the pinned official Codex app-server, Codex-managed ChatGPT login, explicit model and reasoning selection, and one successful governed Test Project Agent Run.

**Architecture:** The Rust Host supervises one official Codex app-server process over strict stdio JSON-RPC. Codex owns managed authentication and upstream transport. Quantix owns account gating, explicit selection, permissions, budgets, event persistence, tool approval and idempotency, validation, audit, and canonical Tender writes. Every app-server event and dynamic-tool resolution crosses an awaited Host callback before the adapter continues. The lane remains test-only in a sanitized Windows account or VM until pre-execution suppression of Codex built-ins or an OS filesystem sandbox is separately proven.

**Tech Stack:** Rust 1.97.1, Tauri 2, Tokio, ProcessKit, SQLite, official `@openai/codex@0.149.1`, generated app-server schema, React 19, TypeScript 7, Vitest.

**Spec:** `docs/superpowers/specs/2026-08-25-sdk-first-ai-runtime-cutover-design.md`

## Global Constraints

- Work in a `superpowers:using-git-worktrees` isolated worktree created at execution time.
- Keep the ordinary/main dev server running for the entire cutover. Never stop it to ask a question, free a port, or run a test. Do not send its process Ctrl+C.
- Do not start a second Tauri dev process on the main server's port. After reviewed integration, use the already-running main dev process for interactive verification; if it exited independently, restart it immediately and leave it running.
- Remove `chatgpt-direct-v1`, the private ChatGPT backend URL, custom OAuth/JWT/refresh/store code, direct SSE parser, and every compatibility alias in the same landing.
- Pin `@openai/codex` to exact version `0.149.1`; package integrity, executable hash, generated-schema hash, and provenance are committed evidence.
- Use managed `account/login/start` browser or device-code flows only; never use `chatgptAuthTokens`.
- Resolve the configured Application Home's `codex` child to an absolute validated Windows path; use `cli_auth_credentials_store = "file"` and exactly one managed ChatGPT account.
- Use only ephemeral threads, the exact Engineer-selected model/reasoning/output schema and discovered `modelProvider`, an empty staging cwd, read-only sandbox, and `approvalPolicy: "never"`. Supply no alternative provider/model and terminate on any reroute.
- Treat Codex core built-in tool execution as not proven preventable. Quarantine detected built-in actions before canonical publication; live tests run only in a sanitized account or VM.
- Add no outer Codex retry. Persist missing internal request or usage facts as unknown.
- Delete duplicate settings provider/reasoning/selection types. Slice 1 retains and completes the canonical immutable `ai::contract::ActiveAiConfiguration`; Slice 2 consumes it without adding a parallel selection.
- Preserve the existing Host loop/non-progress detector and cumulative Agent Resource Budget. Enforce at most 32 Host dynamic-tool rounds; check time, token, byte, cost, and tool ceilings before every continuation, interrupt on exhaustion, and reject late output.
- Private runtime types stay `pub(crate)` and are tested in `#[cfg(test)] mod tests` inside their source modules. Files under `src-tauri/tests/` may use only public `QuantixHost` APIs, Tauri IPC, or public methods explicitly gated by `#[cfg(any(test, feature = "runtime-fixture"))]`; they must not import `CodexHome`, `CodexAppServer`, `CodexProtocolState`, or other private internals.
- Do not hand-edit `src/bindings`. Run the exporter added in Task 3, review its owned diff, and commit generated output.
- Do not run `npm run build` or `npm run build:desktop` during ordinary development. Formal live/private acceptance consumes an already-built release-stage candidate; it does not build one.
- Before Task 14 touches the main app, the parent suite's explicitly approved Slice 1 Fresh-State Schema Cutover must archive the schema-45 home and recreate `%USERPROFILE%\.quantix`; no Task 1-13 binary may open the older home.
- Finish deterministic worktree verification and independent review before integration. Run interactive/provider/live acceptance only after the reviewed revision is on main.
- Dynamic-tool bridge qualification does not authorize distribution. Public release remains blocked independently on Codex built-in pre-execution control, OS isolation, and approved distribution/managed-subscription terms.

---

### Task 1: Pin, Stage, Commit, and Verify Codex 0.149.1

**Files:**
- Modify: `.gitignore`
- Modify: `package.json`
- Modify: `package-lock.json`
- Modify: `scripts/prepare-runtime.mjs`
- Modify: `src-tauri/runtime/runtime-provenance.json`
- Create by generation: `src-tauri/runtime/protocol/codex-app-server-0.149.1.schemas.json`
- Modify: `src-tauri/src/agent_runtime.rs`
- Modify: `src-tauri/src/runtime_readiness.rs`
- Modify: `src-tauri/tests/runtime_readiness.rs`
- Delete: `src-tauri/tests/fixtures/codex_app_server_protocol.schemas.json`

**Interfaces:**
- Consumes: existing `RuntimeLayout`, public `QuantixHost::inspect_runtime_readiness`, runtime provenance verifier, and runtime preparation workflow.
- Produces:

```rust
pub(crate) struct CodexRuntimeError {
    pub code: &'static str,
}

impl RuntimeLayout {
    pub(crate) fn codex_executable(&self) -> Result<PathBuf, CodexRuntimeError>;
    pub(crate) fn codex_schema(&self) -> Result<PathBuf, CodexRuntimeError>;
}
```

- Adds `CodexExecutableMissing`, `CodexPackageVersionIncompatible`, `CodexRuntimeIntegrityFailed`, and `CodexSchemaIntegrityFailed` to `RuntimeReadinessIssue` so the public Host projection is specific without exposing private paths.

- [ ] **Step 1: Add the public-Host provenance RED test and define its fixtures**

Step 1 uses the already tracked baseline `src-tauri/tests/fixtures/codex_app_server_protocol.schemas.json`; it does not read the runtime-protocol target that Step 4 has not generated yet. Prove the baseline exists, then extend the existing `RuntimeHarness` with `codex_executable`, `codex_schema`, and `provenance` paths. Copy `CARGO_BIN_EXE_quantix-runtime-fixture` to the executable path and copy that baseline schema into the fixture runtime layout:

```powershell
git ls-files --error-unmatch src-tauri/tests/fixtures/codex_app_server_protocol.schemas.json
if (-not (Test-Path -LiteralPath src-tauri/tests/fixtures/codex_app_server_protocol.schemas.json -PathType Leaf)) {
    throw 'tracked baseline Codex schema fixture is missing'
}
```

Add these concrete local helpers in the integration test. Step 4 generates the single production runtime schema, updates production/test `include_str!` paths to it, and deletes the old baseline in the same commit so no duplicate remains after Task 1:

```rust
#[derive(Clone, Copy)]
enum CodexRuntimeMutation {
    MissingExecutable,
    WrongPackageVersion,
    ExecutableHashMismatch,
    SchemaHashMismatch,
    StaleProvenance,
}

fn rewrite_codex_provenance(path: &Path, mutate: impl FnOnce(&mut serde_json::Value)) {
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(path).expect("read provenance"))
            .expect("parse provenance");
    mutate(&mut value);
    fs::write(
        path,
        serde_json_canonicalizer::to_vec(&value).expect("canonical provenance"),
    )
    .expect("write provenance");
}

async fn inspect_mutated_codex_runtime(
    mutation: CodexRuntimeMutation,
) -> quantix_lib::RuntimeReadiness {
    let harness = RuntimeHarness::new();
    match mutation {
        CodexRuntimeMutation::MissingExecutable => {
            fs::remove_file(&harness.codex_executable).expect("remove Codex executable");
        }
        CodexRuntimeMutation::WrongPackageVersion => rewrite_codex_provenance(
            &harness.provenance,
            |value| value["codex"]["package_version"] = serde_json::json!("0.149.0"),
        ),
        CodexRuntimeMutation::ExecutableHashMismatch => {
            fs::write(&harness.codex_executable, b"changed").expect("mutate executable");
        }
        CodexRuntimeMutation::SchemaHashMismatch => {
            fs::write(&harness.codex_schema, b"{}").expect("mutate schema");
        }
        CodexRuntimeMutation::StaleProvenance => rewrite_codex_provenance(
            &harness.provenance,
            |value| value["codex"]["package_lock_sha256"] = serde_json::json!("0".repeat(64)),
        ),
    }
    harness.host.inspect_runtime_readiness().await
}
```

Add the table test:

```rust
#[tokio::test]
async fn codex_runtime_requires_exact_package_executable_and_schema() {
    for mutation in [
        CodexRuntimeMutation::MissingExecutable,
        CodexRuntimeMutation::WrongPackageVersion,
        CodexRuntimeMutation::ExecutableHashMismatch,
        CodexRuntimeMutation::SchemaHashMismatch,
        CodexRuntimeMutation::StaleProvenance,
    ] {
        let result = inspect_mutated_codex_runtime(mutation).await;
        assert_eq!(result.state, RuntimeReadinessState::RepairRequired);
        assert!(result.issues.iter().any(|issue| matches!(
            issue,
            RuntimeReadinessIssue::CodexExecutableMissing
                | RuntimeReadinessIssue::CodexPackageVersionIncompatible
                | RuntimeReadinessIssue::CodexRuntimeIntegrityFailed
                | RuntimeReadinessIssue::CodexSchemaIntegrityFailed
        )));
    }
}
```

Expected RED: the test fails with missing Codex provenance/schema readiness fields and `RuntimeReadinessState` is not the required `RepairRequired` result.

- [ ] **Step 2: Run the focused test and confirm RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test runtime_readiness --features runtime-fixture codex_runtime_requires_exact_package_executable_and_schema -- --exact --nocapture
```

Expected: FAIL because production provenance has no pinned Codex artifact/schema contract.

- [ ] **Step 3: Make provenance a committed generated artifact**

Keep the current ignore followed immediately by an exact negation:

```gitignore
src-tauri/runtime/runtime-provenance.json
!src-tauri/runtime/runtime-provenance.json
```

Add a test assertion that `scripts/prepare-runtime.mjs` writes only `src-tauri/runtime/runtime-provenance.json`, never a user-home copy. After generation, prove Git can track it:

```powershell
git check-ignore -q src-tauri/runtime/runtime-provenance.json
if ($LASTEXITCODE -eq 0) { throw "runtime provenance is still ignored" }
git add src-tauri/runtime/runtime-provenance.json
git ls-files --error-unmatch src-tauri/runtime/runtime-provenance.json
```

- [ ] **Step 4: Stage the exact official package and schema**

Add exact dependency and lock integrity, then make `prepare-runtime.mjs` implement this contract:

```javascript
const CODEX_VERSION = "0.149.1";
const runtimeProtocol = path.join(runtimeRoot, "protocol");
const generatedSchemaDirectory = path.join(
  developmentRoot,
  `codex-schema-${CODEX_VERSION}`,
);
const trackedSchema = path.join(
  runtimeProtocol,
  `codex-app-server-${CODEX_VERSION}.schemas.json`,
);
const codexPackage = resolveLockedPackage("@openai/codex", CODEX_VERSION);
await stageVerifiedCodex({
  packageRoot: codexPackage.root,
  executableTarget: path.join(runtimeBin, `codex${extension}`),
  schemaArguments: [
    "app-server",
    "generate-json-schema",
    "--experimental",
    "--out",
    generatedSchemaDirectory,
  ],
  generatedSchemaDirectory,
  trackedSchema,
  updateTrackedSchema: process.env.QUANTIX_UPDATE_CODEX_SCHEMA === "1",
});
```

The staging function resolves the actual locked package layout and executes exactly `codex.exe app-server generate-json-schema --experimental --out .dev/runtime-provisioning/codex-schema-0.149.1`. It rejects any non-JSON, linked, extra, or over-limit output; sorts relative schema paths; parses every JSON file; and builds one canonical bundle. With `QUANTIX_UPDATE_CODEX_SCHEMA=1`, it atomically replaces the tracked bundle. Without that opt-in, it fails if the generated bytes differ from the tracked bundle. Runtime readiness and packaging read that one tracked bundle and bind its hash into provenance. The `--experimental` flag is mandatory because the reviewed dynamic-tools schema is not in the stable surface. The function rejects an unexpected package tree rather than searching for a similar binary.

Run the dependency and runtime preparation commands exactly:

```powershell
npm install --save-exact @openai/codex@0.149.1
$env:QUANTIX_UPDATE_CODEX_SCHEMA = '1'
npm run prepare:runtime
if ($LASTEXITCODE -ne 0) { throw 'Codex runtime/schema update failed' }
Remove-Item Env:QUANTIX_UPDATE_CODEX_SCHEMA
npm run prepare:runtime
if ($LASTEXITCODE -ne 0) { throw 'tracked Codex schema drifted immediately' }
```

- [ ] **Step 5: Promote readiness checks and rerun GREEN**

Implement `RuntimeLayout::codex_executable` and `RuntimeLayout::codex_schema` without feature gates. The integration test observes them only through `QuantixHost::inspect_runtime_readiness`.

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test runtime_readiness --features runtime-fixture codex_runtime_requires_exact_package_executable_and_schema -- --exact --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit Task 1**

```powershell
git add .gitignore package.json package-lock.json scripts/prepare-runtime.mjs src-tauri/runtime/runtime-provenance.json src-tauri/runtime/protocol/codex-app-server-0.149.1.schemas.json src-tauri/src/agent_runtime.rs src-tauri/src/runtime_readiness.rs src-tauri/tests/runtime_readiness.rs
git add -A src-tauri/tests/fixtures/codex_app_server_protocol.schemas.json
git commit -m "feat: stage verified Codex app server"
```

### Task 2: Create the Persistent Isolated Codex Home

**Files:**
- Create: `src-tauri/src/codex_home.rs`
- Modify: `src-tauri/src/setup.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: validated Application Home, setup DACL/path validators, exact Codex version.
- Produces private runtime state:

```rust
pub(crate) struct CodexHome {
    root: PathBuf,
    auth_file: PathBuf,
    config_file: PathBuf,
}

impl CodexHome {
    pub(crate) fn prepare(application_home: &Path) -> Result<Self, CodexHomeError>;
    pub(crate) fn root(&self) -> &Path;
    pub(crate) fn sweep_transient_state(
        &self,
        server_stopped: bool,
    ) -> Result<(), CodexHomeError>;
    pub(crate) fn recovery_delete_auth(
        &self,
        server_stopped: bool,
        exact_confirmation: &str,
    ) -> Result<(), CodexHomeError>;
}
```

- [ ] **Step 1: Add private path, DACL, config, sweep, and recovery RED tests**

Put `#[cfg(test)] mod tests` at the bottom of `codex_home.rs`; do not create an integration test that imports `CodexHome`. Define the test helper there:

```rust
fn prepared_application_home() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("temporary application home");
    let application_home = root.path().join(".quantix");
    fs::create_dir(&application_home).expect("create application home");
    crate::setup::restrict_directory_for_tests(&application_home)
        .expect("restrict application home");
    root
}
```

If the existing setup test helper has a different name, rename it once to `restrict_directory_for_tests` and expose it only with `#[cfg(test)] pub(crate)`. Test outside-home paths, reparse points, weak DACLs, unexpected persistent files, bounded lock/temp files, exact config bytes, safe transient sweeping, and recovery deletion requiring a stopped process plus exact phrase `DELETE CORRUPT CODEX AUTH`.

```rust
#[test]
fn codex_home_preserves_only_approved_persistent_content() {
    let root = prepared_application_home();
    let application_home = root.path().join(".quantix");
    let codex = CodexHome::prepare(&application_home).expect("prepare Codex home");
    assert_eq!(codex.root(), application_home.join("codex"));
    assert!(codex.root().join("config.toml").is_file());
    assert!(!codex.root().join("sessions").exists());
}
```

Expected RED: `CodexHome` is undefined; after a stub, config/sweep/recovery assertions fail because no isolated policy exists.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib codex_home::tests --features runtime-fixture -- --nocapture
```

Expected: compile failure because `CodexHome` and its inline tests do not exist.

- [ ] **Step 3: Implement the exact home/config policy**

Write and contract-test these exact deterministic 0.149.1 settings; `config/read` must return the matching effective values or startup is incompatible:

```toml
cli_auth_credentials_store = "file"
approval_policy = "never"
sandbox_mode = "read-only"
web_search = "disabled"
check_for_update_on_startup = false
file_opener = "none"

[features]
apps = false
auth_elicitation = false
browser_use = false
browser_use_external = false
computer_use = false
plugins = false
memories = false
hooks = false
goals = false
multi_agent = false
multi_agent_v2 = false
shell_tool = false
shell_snapshot = false
unified_exec = false
fast_mode = false
image_generation = false
in_app_browser = false
plugin_sharing = false
remote_plugin = false
skill_mcp_dependency_install = false
tool_call_mcp_elicitation = false
tool_suggest = false
view_image = false
workspace_dependencies = false

[agents]
enabled = false

[history]
persistence = "none"

[memories]
use_memories = false

[feedback]
enabled = false

[analytics]
enabled = false

[otel]
exporter = "none"
trace_exporter = "none"
metrics_exporter = "none"
log_user_prompt = false
```

Do not set a model/model provider, MCP, app, plugin, skill, or ambient user profile. Validate the root and every child by handle without following links. At every stopped-process boundary the only persistent allowlist is exactly `auth.json` and `config.toml`; delete every other validated ordinary child after bounding its size, and fail recovery on any reparse point or path escape. Do not inspect/sweep during an auth write or while app-server runs. This removes stopped-process lock/temp/session/cache content without inventing version-specific wildcard filenames.

- [ ] **Step 4: Run GREEN and setup regression**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib codex_home::tests --features runtime-fixture -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test quantix_setup --features runtime-fixture
```

- [ ] **Step 5: Commit Task 2**

```powershell
git add src-tauri/src/codex_home.rs src-tauri/src/setup.rs src-tauri/src/lib.rs
git commit -m "feat: add isolated managed Codex home"
```

### Task 3: Make Binding Export Own and Remove Stale Generated DTOs

**Files:**
- Modify: `package.json`
- Modify: `src-tauri/src/bin/export_bindings.rs`
- Create by generation: `src/bindings/.quantix-generated.json`

**Interfaces:**
- Consumes: every Rust `TS` export currently registered by `export_bindings`.
- Produces `src/bindings/.quantix-generated.json`, a sorted list of exporter-owned file basenames.
- Adds npm command `bindings:generate` and makes `npm test` run it before renderer/Rust tests.

```rust
#[derive(Clone)]
struct GeneratedBinding {
    basename: String,
    bytes: Vec<u8>,
}

fn generated(basename: &str, contents: &str) -> GeneratedBinding;
fn write_owned_manifest(
    output: &Path,
    basenames: &[&str],
) -> Result<(), BindingExportError>;
fn bootstrap_owned_manifest(
    output: &Path,
    staged: &[GeneratedBinding],
) -> Result<(), BindingExportError>;
fn publish_staged_bindings(
    output: &Path,
    staged: &[GeneratedBinding],
) -> Result<(), BindingExportError>;
```

- [ ] **Step 1: Add exporter ownership RED tests in the binary module**

Add `#[cfg(test)] mod tests` in `export_bindings.rs` for two concrete cases:

```rust
#[test]
fn export_removes_only_files_named_by_the_previous_manifest() {
    let output = tempfile::tempdir().expect("output");
    fs::write(output.path().join("OldGenerated.ts"), "old").expect("old generated");
    fs::write(output.path().join("EngineerNote.ts"), "keep").expect("unowned file");
    write_owned_manifest(output.path(), &["OldGenerated.ts"]).expect("manifest");
    publish_staged_bindings(output.path(), &[generated("Current.ts", "current")])
        .expect("publish");
    assert!(!output.path().join("OldGenerated.ts").exists());
    assert_eq!(
        fs::read_to_string(output.path().join("EngineerNote.ts")).expect("note"),
        "keep"
    );
}

#[test]
fn first_export_refuses_to_claim_an_unknown_existing_typescript_file() {
    let output = tempfile::tempdir().expect("output");
    fs::write(output.path().join("Unknown.ts"), "unknown").expect("unknown");
    let error = bootstrap_owned_manifest(output.path(), &[generated("Current.ts", "current")])
        .expect_err("unknown file must block bootstrap");
    assert_eq!(error.code(), "binding_export_unowned_file");
}
```

Define `generated`, `write_owned_manifest`, `publish_staged_bindings`, and `bootstrap_owned_manifest` in the same binary module before these tests. They accept basenames only, reject separators and non-`.ts` suffixes, canonicalize the output directory, and never delete a file absent from the prior manifest.

Expected RED: ownership helpers do not compile; after stubs, stale owned output remains or the unknown file is wrongly claimed/deleted.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --bin export_bindings --features runtime-fixture export_ -- --nocapture
```

Expected RED: compile failure for the ownership helpers; after signatures are stubbed, `OldGenerated.ts` remains or the unknown unowned file is incorrectly claimed.

- [ ] **Step 3: Implement stage-then-publish export**

Export all `TS` types into a temporary sibling directory. Scan that directory for generated `.ts` basenames, compare existing files byte-for-byte on the first ownership bootstrap, remove only basenames from the prior manifest, atomically copy current staged files, then atomically replace `.quantix-generated.json`. Do not implement prefix scans or delete unknown files.

Add this exact script and test order:

```json
{
  "bindings:generate": "cargo run --manifest-path src-tauri/Cargo.toml --bin export_bindings --features runtime-fixture",
  "test": "npm run bindings:generate && npm run test:renderer && cargo test --manifest-path src-tauri/Cargo.toml --target-dir src-tauri/target/tests --features runtime-fixture -- --test-threads=4"
}
```

The publish algorithm must be implemented as this single ownership transaction:

```rust
fn publish_staged_bindings(
    output: &Path,
    staged: &[GeneratedBinding],
) -> Result<(), BindingExportError> {
    let output = output.canonicalize()?;
    let prior = read_owned_manifest(&output)?;
    let current = validate_and_index_staged(staged)?;
    for basename in prior.iter().filter(|name| !current.contains_key(*name)) {
        remove_owned_file(&output, basename)?;
    }
    for (basename, binding) in &current {
        atomic_replace_file(&output.join(basename), &binding.bytes)?;
    }
    let basenames = current.keys().cloned().collect::<Vec<_>>();
    atomic_replace_owned_manifest(&output, &basenames)?;
    Ok(())
}
```

`validate_and_index_staged` rejects separators, non-`.ts` names, duplicates, and symlinks. `remove_owned_file` joins one validated basename and rechecks its parent is exactly `output`. Both atomic replace helpers write a same-directory temporary file, flush it, and use the repository's Windows replace-file primitive; no scan-derived or unowned path is ever removed.

- [ ] **Step 4: Generate the initial ownership manifest and prove idempotence**

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin export_bindings --features runtime-fixture
git diff -- src/bindings
git add src/bindings
cargo run --manifest-path src-tauri/Cargo.toml --bin export_bindings --features runtime-fixture
git diff --exit-code -- src/bindings
git diff --cached --stat -- src/bindings
```

The first diff contains only the ownership manifest unless the current generated bindings were already stale. Staging that reviewed output makes `git diff --exit-code` measure only whether the second export changed it; the second export must change nothing.

- [ ] **Step 5: Commit Task 3**

```powershell
git add package.json src-tauri/src/bin/export_bindings.rs src/bindings/.quantix-generated.json src/bindings
git commit -m "build: own generated TypeScript bindings"
```

### Task 4: Canonicalize Provider Identity and the Immutable Execution Selection

**Files:**
- Modify: `src-tauri/src/ai/contract.rs`
- Modify: `src-tauri/src/ai/connections.rs`
- Modify: `src-tauri/src/ai/vault.rs`
- Modify: `src-tauri/src/application_settings.rs`
- Modify: `src-tauri/src/agent_runtime.rs`
- Modify: `src-tauri/src/agent_runtime/codex_actor.rs`
- Modify: `src-tauri/src/acceptance.rs`
- Modify: `src-tauri/src/chatgpt_login.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/tender_store.rs`
- Modify: `src-tauri/src/tender_store/agent_records.rs`
- Modify: `src-tauri/src/tender_store/backups.rs`
- Modify: `src-tauri/src/tender_store/estimates.rs`
- Modify: `src-tauri/src/tender_store/manager_intake.rs`
- Modify: `src-tauri/src/tender_store/production_scheduler.rs`
- Modify: `src-tauri/src/tender_store/tender_records.rs`
- Modify: `src-tauri/tests/agent_runtime.rs`
- Modify: `src-tauri/tests/ai_connection_repository.rs`
- Modify: `src-tauri/tests/manager_workspace.rs`
- Modify: `src/ApplicationSettings.tsx`
- Modify: `src/ApplicationSettings.test.tsx`
- Modify: `src/ManagerWorkspace.test.tsx`
- Modify: `src/TenderAiSelectionControl.tsx`
- Modify: `src/quantixHost.ts`
- Modify: `src-tauri/src/bin/export_bindings.rs`
- Delete by generation: `src/bindings/ProviderReasoningSelection.ts`
- Delete by generation: `src/bindings/ProviderReasoningOption.ts`
- Delete by generation: `src/bindings/ProviderModelOption.ts`
- Delete by generation: `src/bindings/ProviderConnectionView.ts`
- Delete by generation: `src/bindings/ProviderConnectionStatus.ts`
- Modify by generation: `src/bindings/AiProviderKind.ts` from the canonical contract
- Modify by generation: `src/bindings/ActiveAiConfiguration.ts`
- Modify by generation: `src/bindings/AiModelView.ts`
- Create by generation: `src/bindings/AiPricingIdentity.ts`
- Create by generation: `src/bindings/AiReasoningSelection.ts`

**Interfaces:**
- Deletes `application_settings::AiProviderKind`, `ProviderConnectionStatus`, `ProviderReasoningSelection`, `ProviderReasoningOption`, `ProviderModelOption`, `ProviderConnectionView`, and the short legacy `application_settings::AiExecutionSelection`; Settings uses canonical `ApplicationAiSettingsView`, `AiConnectionStatus`, `AiConnectionView`, `AiModelView`, and `AiReasoningOption`. Add `SubscriptionRequired` to canonical `AiConnectionStatus` rather than retaining a second status enum.
- Retains the canonical `ai::contract::ActiveAiConfiguration`, extends it with the exact provider-route/capability/pricing identities below, and deletes the weaker settings `AiExecutionSelection`; no rename, alias, or parallel active-selection type remains.
- Uses only canonical `ai::contract::AiProviderKind` and `AiReasoningSelection` everywhere.

```rust
pub enum AiReasoningSelection {
    Unsupported,
    Effort { id: String },
}

pub struct AiPricingIdentity {
    pub snapshot_sha256: String,
}

pub struct AiModelView {
    pub model_id: String,
    pub reported_model_id: Option<String>,
    pub provider_route_id: Option<String>,
    pub display_name: String,
    pub capabilities: AiCapabilitySet,
    pub reasoning_options: Vec<AiReasoningOption>,
}

pub struct ActiveAiConfiguration {
    pub connection_id: AiConnectionId,
    pub execution_revision: AiConnectionRevision,
    pub provider: AiProviderKind,
    pub endpoint_fingerprint: String,
    pub destination_class: AiNetworkDestinationClass,
    pub data_destination: String,
    pub model_id: String,
    pub provider_route_id: Option<String>,
    pub reasoning: AiReasoningSelection,
    pub catalogue_sha256: String,
    pub capability_sha256: String,
    pub capabilities: AiCapabilitySet,
    pub adapter_version: String,
    pub pricing_identity: Option<AiPricingIdentity>,
    pub activated_at: String,
}

pub fn capability_sha256(
    model: &AiModelView,
    reasoning: &AiReasoningSelection,
) -> Result<String, AiContractError>;

pub(crate) fn selection_from_qualified_probe(
    configuration: &AiConnectionConfiguration,
    connection_id: AiConnectionId,
    execution_revision: AiConnectionRevision,
    evidence: &AiProbeEvidence,
    model_id: &str,
    reasoning: &AiReasoningSelection,
    pricing_identity: Option<AiPricingIdentity>,
    activated_at: String,
) -> Result<ActiveAiConfiguration, AiConnectionError>;
```

`provider_route_id` is the bounded app-server `modelProvider` identity discovered with the selected Codex model. It is mandatory for `AiProviderKind::Codex`, must be `None` for the six general routes, participates in catalogue/capability hashes, and is copied exactly into `thread/start.modelProvider`. `Unsupported` is an explicit Engineer selection only when probe evidence says reasoning is unsupported. `Effort { id }` must equal one discovered option. Codex pricing is `None` until an approved pricing snapshot exists; Slice 2 may construct `Some(AiPricingIdentity)` but may not change the selection shape.

`AiRuntimeRequest`, `PreparedAgentRun`, `TenderAiExecutionBinding`, `AiExecutionApproval`, stored final settings, run bindings, backups, renderer DTOs, and every production task constructor consume this same immutable selection. `AiExecutionApproval` wraps the complete selection, optional managed-account fingerprint, and approval time; general provider routes use `None` rather than inventing an account identity. It does not copy a weaker subset. The canonical connection repository's activation transaction is the only production caller of `selection_from_qualified_probe`.

- [ ] **Step 1: Add RED canonical-provider, route-ID, hash, and constructor tests**

In `ai::contract` source tests, table-test all seven canonical providers through `validate_method_provider`; test `provider_route_id` empty, over-limit, control-character, Codex-missing, general-present, and changed-value cases. The Rust compiler must force every current `AiModelView` constructor to set `provider_route_id`. Add this exact constructor assertion:

```rust
#[test]
fn qualified_selection_binds_every_execution_identity() {
    let evidence = fixture_codex_probe_with_efforts(["low", "medium"]);
    let selection = selection_from_qualified_probe(
        &fixture_codex_configuration(),
        evidence.connection_id.clone(),
        evidence.execution_revision,
        &evidence,
        "gpt-codex-fixture",
        &AiReasoningSelection::Effort { id: "low".into() },
        None,
        "2026-08-25T12:05:00Z".into(),
    )
    .expect("qualified Codex selection");
    assert_eq!(selection.provider, AiProviderKind::Codex);
    assert_eq!(selection.provider_route_id.as_deref(), Some("openai"));
    assert_eq!(selection.catalogue_sha256, catalogue_sha256(&evidence).unwrap());
    assert_eq!(
        selection.capability_sha256,
        capability_sha256(&evidence.models[0], &selection.reasoning).unwrap()
    );
    assert!(selection.pricing_identity.is_none());
}

#[test]
fn canonical_method_provider_pairs_are_exhaustive() {
    let valid = [
        (AiConnectionMethod::AccountLogin, AiProviderKind::Codex),
        (AiConnectionMethod::DirectProviderKey, AiProviderKind::OpenAi),
        (AiConnectionMethod::DirectProviderKey, AiProviderKind::Anthropic),
        (AiConnectionMethod::DirectProviderKey, AiProviderKind::GoogleGemini),
        (AiConnectionMethod::DirectProviderKey, AiProviderKind::XAi),
        (AiConnectionMethod::OpenAiCompatible, AiProviderKind::OpenAiCompatible),
        (AiConnectionMethod::AnthropicCompatible, AiProviderKind::AnthropicCompatible),
    ];
    for method in [
        AiConnectionMethod::AccountLogin,
        AiConnectionMethod::DirectProviderKey,
        AiConnectionMethod::OpenAiCompatible,
        AiConnectionMethod::AnthropicCompatible,
    ] {
        for provider in [
            AiProviderKind::Codex,
            AiProviderKind::OpenAi,
            AiProviderKind::Anthropic,
            AiProviderKind::GoogleGemini,
            AiProviderKind::XAi,
            AiProviderKind::OpenAiCompatible,
            AiProviderKind::AnthropicCompatible,
        ] {
            assert_eq!(
                validate_method_provider(method, provider).is_ok(),
                valid.contains(&(method, provider)),
                "unexpected pairing: {method:?}/{provider:?}",
            );
        }
    }
}

#[test]
fn provider_route_id_rules_are_provider_specific_and_bounded() {
    for (provider, route, expected_ok) in [
        (AiProviderKind::Codex, Some("openai"), true),
        (AiProviderKind::Codex, None, false),
        (AiProviderKind::Codex, Some(""), false),
        (AiProviderKind::Codex, Some("bad\nroute"), false),
        (AiProviderKind::OpenAi, None, true),
        (AiProviderKind::OpenAi, Some("openai"), false),
    ] {
        let (configuration, mut evidence) = fixture_probe_for_provider(provider);
        evidence.models[0].provider_route_id = route.map(str::to_owned);
        let result = selection_from_qualified_probe(
            &configuration,
            evidence.connection_id.clone(),
            evidence.execution_revision,
            &evidence,
            &evidence.tested_model_id,
            &evidence.tested_reasoning,
            None,
            "2026-08-25T12:05:00Z".into(),
        );
        assert_eq!(result.is_ok(), expected_ok, "provider={provider:?}, route={route:?}");
    }
}
```

Keep `fixture_codex_probe_with_efforts`, `fixture_codex_configuration`, and `fixture_probe_for_provider` inside the source test module. Make the Codex fixture set canonical provider `Codex`, endpoint fingerprint `sha256("https://chatgpt.com")`, public destination, selected/reported model equality, `provider_route_id: Some("openai")`, exact reasoning options, and adapter `codex-app-server-0.149.1`. Add one separate over-limit route case using `"x".repeat(MAX_PROVIDER_ROUTE_ID_BYTES + 1)`.

Expected RED: current model/active constructors lack route/capability/pricing fields, and invalid provider-route combinations are accepted or cannot be expressed by the old types.

- [ ] **Step 2: Add RED persistence and renderer tests for the complete selection**

Update `ai_connection_repository`, Agent Run, Manager Workspace, Application Settings, and renderer fixtures to include every field. Test activation rejects changed connection revision, canonical provider, endpoint, destination class, model, provider route ID, reasoning, catalogue hash, capability hash, adapter, and pricing hash. Test serialized old/partial selections fail rather than defaulting fields. Assert model/reasoning remain unselected until Engineer choice.

```rust
#[test]
fn activation_rejects_every_changed_qualified_identity() {
    for mutation in [
        SelectionMutation::ExecutionRevision,
        SelectionMutation::Provider,
        SelectionMutation::EndpointFingerprint,
        SelectionMutation::DestinationClass,
        SelectionMutation::ModelId,
        SelectionMutation::ProviderRouteId,
        SelectionMutation::Reasoning,
        SelectionMutation::CatalogueSha256,
        SelectionMutation::CapabilitySha256,
        SelectionMutation::AdapterVersion,
        SelectionMutation::PricingSha256,
    ] {
        let harness = ReadyConnectionHarness::new();
        let command = harness.activation_command_with(mutation);
        assert_eq!(harness.repository.activate(command), Err(AiConnectionError::CapabilityChanged));
    }
}
```

```typescript
it("keeps activation disabled until the exact tuple is qualified", async () => {
  render(<ApplicationSettings />);
  await userEvent.selectOptions(screen.getByLabelText("Model"), "gpt-codex-fixture");
  await userEvent.selectOptions(screen.getByLabelText("Reasoning"), "low");
  expect(screen.getByRole("button", { name: "Activate" })).toBeDisabled();
  await userEvent.click(screen.getByRole("button", { name: "Test selection" }));
  expect(host.probeCodexSelection).toHaveBeenCalledWith(
    expect.objectContaining({ model_id: "gpt-codex-fixture", provider_route_id: "openai" }),
  );
  expect(await screen.findByRole("button", { name: "Activate" })).toBeEnabled();
});
```

Define the test-only `SelectionMutation` and `ReadyConnectionHarness::activation_command_with` beside the repository tests; each variant mutates exactly one field. Expected RED: Rust fails to compile while legacy selection constructors omit new fields, then the assertions fail with unexpected successful activation; Vitest fails because Activate is currently enabled without committed qualification.

- [ ] **Step 3: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib ai::contract::tests --features runtime-fixture
cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_repository --test agent_runtime --test manager_workspace --features runtime-fixture
npx vitest run src/ApplicationSettings.test.tsx src/ManagerWorkspace.test.tsx
```

Expected RED: nonzero exit from missing canonical fields/duplicate type mismatches, followed by the mutation and disabled-activation assertions failing until Task 4 is implemented.

- [ ] **Step 4: Implement one canonical provider and selection path**

Retain the canonical active configuration/view name, add the missing route/capability/pricing identities, and delete duplicate settings types. Replace `acceptance.rs`'s direct `crate::ProviderConnectionStatus` use with canonical `AiConnectionStatus`. `capability_sha256` canonicalizes only model ID, reported model ID, provider route ID, capabilities, and exact selected reasoning; display text, option ordering, observation time, secrets, and raw responses cannot affect it. Update every constructor named in the Files list; do not deserialize old selection JSON or add aliases.

The minimal hash/constructor implementation is:

```rust
#[derive(Serialize)]
struct CapabilityProjection<'a> {
    model_id: &'a str,
    reported_model_id: Option<&'a str>,
    provider_route_id: Option<&'a str>,
    capabilities: &'a AiCapabilitySet,
    reasoning: &'a AiReasoningSelection,
}

pub fn capability_sha256(
    model: &AiModelView,
    reasoning: &AiReasoningSelection,
) -> Result<String, AiContractError> {
    model.validate()?;
    if !reasoning_is_available(model, reasoning) {
        return Err(AiContractError::InvalidReasoningSelection);
    }
    let bytes = serde_json_canonicalizer::to_vec(&CapabilityProjection {
        model_id: &model.model_id,
        reported_model_id: model.reported_model_id.as_deref(),
        provider_route_id: model.provider_route_id.as_deref(),
        capabilities: &model.capabilities,
        reasoning,
    })
    .map_err(|_| AiContractError::InvalidCatalogue)?;
    Ok(sha256_hex(&bytes))
}

fn valid_provider_route_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PROVIDER_ROUTE_ID_BYTES
        && !value.chars().any(char::is_control)
}

pub(crate) fn selection_from_qualified_probe(
    configuration: &AiConnectionConfiguration,
    connection_id: AiConnectionId,
    execution_revision: AiConnectionRevision,
    evidence: &AiProbeEvidence,
    model_id: &str,
    reasoning: &AiReasoningSelection,
    pricing_identity: Option<AiPricingIdentity>,
    activated_at: String,
) -> Result<ActiveAiConfiguration, AiConnectionError> {
    evidence.validate().map_err(|_| AiConnectionError::CapabilityChanged)?;
    let provider = configuration.provider();
    let endpoint_fingerprint = configuration
        .endpoint_fingerprint()
        .map_err(|_| AiConnectionError::InvalidCommand)?;
    if evidence.connection_id != connection_id
        || evidence.execution_revision != execution_revision
        || evidence.provider != provider
        || evidence.endpoint_fingerprint != endpoint_fingerprint
        || !configuration.accepts_destination_class(evidence.destination_class)
        || evidence.tested_model_id != model_id
        || &evidence.tested_reasoning != reasoning
    {
        return Err(AiConnectionError::CapabilityChanged);
    }
    let model = evidence
        .models
        .iter()
        .find(|candidate| candidate.model_id == model_id)
        .ok_or(AiConnectionError::CapabilityChanged)?;
    let provider_route_id = match (provider, model.provider_route_id.as_deref()) {
        (AiProviderKind::Codex, Some(route)) if valid_provider_route_id(route) => {
            Some(route.to_owned())
        }
        (AiProviderKind::Codex, _) => return Err(AiConnectionError::CapabilityChanged),
        (_, None) => None,
        (_, Some(_)) => return Err(AiConnectionError::CapabilityChanged),
    };
    if let Some(identity) = &pricing_identity {
        validate_catalogue_sha256(&identity.snapshot_sha256)
            .map_err(|_| AiConnectionError::CapabilityChanged)?;
    }
    Ok(ActiveAiConfiguration {
        connection_id,
        execution_revision,
        provider,
        endpoint_fingerprint,
        destination_class: evidence.destination_class,
        data_destination: configuration
            .data_destination()
            .map_err(|_| AiConnectionError::InvalidCommand)?
            .to_owned(),
        model_id: model.model_id.clone(),
        provider_route_id,
        reasoning: reasoning.clone(),
        catalogue_sha256: catalogue_sha256(evidence)
            .map_err(|_| AiConnectionError::CapabilityChanged)?,
        capability_sha256: capability_sha256(model, reasoning)
            .map_err(|_| AiConnectionError::CapabilityChanged)?,
        capabilities: model.capabilities.clone(),
        adapter_version: evidence.adapter_version.clone(),
        pricing_identity,
        activated_at,
    })
}
```

No call site may construct a partial active configuration or recompute an identity from display metadata.

- [ ] **Step 5: Regenerate and run GREEN**

```powershell
npm run bindings:generate
cargo test --manifest-path src-tauri/Cargo.toml --lib ai::contract::tests --features runtime-fixture
cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_repository --test agent_runtime --test manager_workspace --features runtime-fixture
npx vitest run src/ApplicationSettings.test.tsx src/ManagerWorkspace.test.tsx src/quantixHost.test.ts
$obsoleteSelection = rg -n "ProviderReasoningSelection|ProviderConnectionStatus|ProviderConnectionView|ProviderModelOption|application_settings::AiProviderKind|application_settings::AiExecutionSelection" src-tauri/src src
if ($LASTEXITCODE -eq 0) { throw "duplicate provider/selection types remain`n$obsoleteSelection" }
```

- [ ] **Step 6: Commit Task 4**

```powershell
git add src-tauri/src/ai/contract.rs src-tauri/src/ai/connections.rs src-tauri/src/ai/vault.rs src-tauri/src/application_settings.rs src-tauri/src/agent_runtime.rs src-tauri/src/agent_runtime/codex_actor.rs src-tauri/src/acceptance.rs src-tauri/src/chatgpt_login.rs src-tauri/src/lib.rs src-tauri/src/tender_store.rs src-tauri/src/tender_store src-tauri/tests/agent_runtime.rs src-tauri/tests/ai_connection_repository.rs src-tauri/tests/manager_workspace.rs src/ApplicationSettings.tsx src/ApplicationSettings.test.tsx src/ManagerWorkspace.test.tsx src/TenderAiSelectionControl.tsx src/quantixHost.ts src-tauri/src/bin/export_bindings.rs src/bindings
git commit -m "refactor: canonicalize immutable AI execution selection"
```

### Task 5: Define the Exact Codex Protocol and Configuration Policy

**Files:**
- Create: `src-tauri/src/agent_runtime/codex_policy.rs`
- Modify: `src-tauri/src/agent_runtime.rs`
- Modify: `src-tauri/src/agent_runtime/codex_protocol.rs`
- Modify: `src-tauri/tests/support/runtime_fixture.rs`

**Interfaces:**
- Consumes: generated `0.149.1` schema from Task 1 and `AiReasoningSelection` from Task 4.
- Produces private policy state:

```rust
pub(crate) enum CodexManagedLoginMethod {
    Browser,
    DeviceCode,
}

pub(crate) enum CodexProtocolState {
    AwaitingInitialize,
    Ready,
    LoginInFlight { login_id: String },
    TurnActive { thread_id: String, turn_id: String },
    Terminal,
}

pub(crate) struct CodexTurnRequest {
    pub run_id: String,
    pub selection: ActiveAiConfiguration,
    pub output_schema: Value,
    pub instruction_bundle: String,
    pub staging_cwd: PathBuf,
}

pub(crate) fn validate_client_request(value: &Value) -> Result<(), ProviderFailure>;
pub(crate) fn classify_server_message(
    value: &Value,
) -> Result<CodexServerMessage, ProviderFailure>;
```

- [ ] **Step 1: Add private allowlist/denylist RED tests and define `fixture_item`**

Put policy tests in `codex_policy.rs` under `#[cfg(test)] mod tests`. Define the helper in that module:

```rust
fn fixture_item(kind: &str) -> serde_json::Value {
    serde_json::json!({
        "method": "item/started",
        "params": {
            "threadId": "019c0000-0000-7000-8000-000000000001",
            "turnId": "019c0000-0000-7000-8000-000000000002",
            "item": { "id": "item-1", "type": kind }
        }
    })
}
```

Test exactly: initialize/initialized, account read/login/cancel/completed/logout, account/rate-limit notifications, model/list, ephemeral thread/start, turn/start/interrupt, allowed deltas/usage/status, and reviewed dynamic-tool calls. Table-test `chatgptAuthTokens` plus every forbidden shell/file/patch/web/image/app/plugin/MCP/memory/goal/environment/subagent/collaboration/config-write method/item and every unknown type.

```rust
#[test]
fn policy_quarantines_codex_builtin_actions_before_host_execution_or_publication() {
    for kind in [
        "commandExecution",
        "fileChange",
        "mcpToolCall",
        "collabAgentToolCall",
    ] {
        let error = classify_server_message(&fixture_item(kind)).unwrap_err();
        assert_eq!(error.category, ProviderFailureCategory::ProtocolInvalid);
    }
}
```

Expected RED: `codex_policy` is absent or at least one forbidden/unknown item—including `chatgptAuthTokens`—is accepted instead of returning `ProtocolInvalid`.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib agent_runtime::codex_policy::tests --features runtime-fixture -- --nocapture
```

Expected RED: `codex_policy` is missing or forbidden fixture messages are accepted; the test must specifically show `chatgptAuthTokens` and each built-in action returning `ProtocolInvalid`.

- [ ] **Step 3: Implement generated-schema-backed policy**

Require `run_id` to be the existing 32-character Agent Run identity and the selection to be canonical `Codex`. Build `thread/start` from that immutable selection: exact model, exact `modelProvider`, `ephemeral: true`, staging cwd, `approvalPolicy: "never"`, `sandbox: "read-only"`, and exact discovered Codex `Effort { id }`. Supply no alternate provider/model, reject `model/rerouted` or any observed model/provider/selection mismatch, and reject schema/runtime drift. Classify detected internal built-in actions as quarantine events; do not claim app-server pre-execution suppression.

The request builder must be equivalent to:

```rust
pub(crate) fn build_thread_start(request: &CodexTurnRequest) -> Result<Value, ProviderFailure> {
    validate_run_id(&request.run_id)?;
    if request.selection.provider != AiProviderKind::Codex {
        return Err(protocol_failure("codex_provider_mismatch"));
    }
    let provider_route = request
        .selection
        .provider_route_id
        .as_deref()
        .ok_or_else(|| protocol_failure("codex_model_provider_missing"))?;
    let effort = match &request.selection.reasoning {
        AiReasoningSelection::Effort { id } => id,
        AiReasoningSelection::Unsupported => {
            return Err(protocol_failure("codex_reasoning_effort_required"));
        }
    };
    let value = json!({
        "method": "thread/start",
        "params": {
            "model": &request.selection.model_id,
            "modelProvider": provider_route,
            "ephemeral": true,
            "cwd": &request.staging_cwd,
            "approvalPolicy": "never",
            "sandbox": "read-only",
            "reasoningEffort": effort,
        }
    });
    validate_client_request(&value)?;
    Ok(value)
}
```

`classify_server_message` first validates against the tracked schema, then matches only the explicit allowlist; `model/rerouted`, `chatgptAuthTokens`, unknown methods/items, and built-in action items all return one redacted `ProtocolInvalid` failure.

- [ ] **Step 4: Run GREEN**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib agent_runtime::codex_policy::tests --features runtime-fixture -- --nocapture
```

- [ ] **Step 5: Commit Task 5**

```powershell
git add src-tauri/src/agent_runtime.rs src-tauri/src/agent_runtime/codex_policy.rs src-tauri/src/agent_runtime/codex_protocol.rs src-tauri/tests/support/runtime_fixture.rs
git commit -m "feat: enforce pinned Codex protocol policy"
```

### Task 6: Promote Framed Conversation Supervision to Production

**Files:**
- Modify: `src-tauri/src/process_supervisor.rs`

**Interfaces:**
- Consumes: existing `ProcessSupervisor` and `ProcessGroup` Job Object cleanup.
- Produces production `pub(crate) SupervisedConversation`; no integration-test visibility is added.

- [ ] **Step 1: Add private production-configuration RED tests**

In `process_supervisor.rs`'s existing inline test module, add cases for split JSON-RPC frames, a 1 MiB frame limit, 16 MiB cumulative stdout, 256 KiB stderr, cancellation control writes, five-second terminal grace, dropped-client Job Object cleanup, and child cleanup. The tests must instantiate production limits rather than fixture-only branches.

```rust
#[tokio::test]
async fn production_conversation_limits_fail_closed_and_reap() {
    for (fault, expected) in [
        (ConversationFault::FrameBytes(1024 * 1024 + 1), ProcessError::OutputLimitExceeded),
        (ConversationFault::StdoutBytes(16 * 1024 * 1024 + 1), ProcessError::OutputLimitExceeded),
        (ConversationFault::StderrBytes(256 * 1024 + 1), ProcessError::OutputLimitExceeded),
        (ConversationFault::PartialFrameThenEof, ProcessError::ObservationFailed),
        (ConversationFault::CancelWhileReading, ProcessError::Cancelled),
    ] {
        let mut harness = ConversationHarness::spawn(fault).await;
        assert_eq!(harness.read_terminal().await.unwrap_err(), expected);
        assert!(harness.finish().await.process_tree_reaped);
    }
}
```

Define `ConversationFault`/`ConversationHarness` in the source test module and add a separate split-frame happy case. Expected RED: production code cannot name `SupervisedConversation` because it is feature-gated, or an over-limit/partial frame is accepted and the fixture reports an unreaped child.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib process_supervisor::tests --features runtime-fixture -- --nocapture
```

- [ ] **Step 3: Remove only the fixture gate and keep the bounded API**

Remove `#[cfg(any(test, feature = "runtime-fixture"))]` from `SupervisedConversation` and its implementation. Keep its fields private, reset operation budgets between requests, reject partial EOF frames, and preserve bounded terminate/kill/reap behavior.

The production promotion is intentionally mechanical:

```rust
fn take_complete_line(buffer: &mut Vec<u8>, eof: bool) -> Result<Option<Vec<u8>>, ProcessError> {
    if let Some(index) = buffer.iter().position(|byte| *byte == b'\n') {
        return Ok(Some(buffer.drain(..=index).collect()));
    }
    if eof && !buffer.is_empty() {
        return Err(ProcessError::ObservationFailed);
    }
    Ok(None)
}
```

Retain the existing remaining fields/budgets and termination methods unchanged; do not add a fixture/production branch or an unbounded convenience reader.

- [ ] **Step 4: Run GREEN**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib process_supervisor::tests --features runtime-fixture -- --nocapture
```

- [ ] **Step 5: Commit Task 6**

```powershell
git add src-tauri/src/process_supervisor.rs
git commit -m "refactor: promote supervised conversation runtime"
```

### Task 7: Build the Codex Adapter with Awaited Host Authority Callbacks

**Files:**
- Create: `src-tauri/src/agent_runtime/codex_app_server.rs`
- Modify: `src-tauri/src/agent_runtime.rs`
- Replace implementation: `src-tauri/src/agent_runtime/codex_actor.rs`
- Modify: `src-tauri/src/host.rs`
- Modify: `src-tauri/src/tender_store.rs`
- Modify: `src-tauri/src/tender_store/agent_records.rs`
- Modify: `src-tauri/tests/runtime_readiness.rs`
- Modify: `src-tauri/tests/safe_updates.rs`
- Create: `src-tauri/tests/support/codex_host_fixture.rs`
- Modify: `src-tauri/tests/support/runtime_fixture.rs`
- Modify: `src-tauri/tests/agent_runtime.rs`

**Interfaces:**
- Consumes: `CodexHome`, strict policy, `ProcessSupervisor`, existing Agent Access records, typed-tool validation/execution, and canonical Tender event persistence.
- Produces these private adapter boundaries:

```rust
use std::{future::Future, pin::Pin};

pub(crate) type CodexCallbackFuture<T> =
    Pin<Box<dyn Future<Output = Result<T, ProviderFailure>> + Send + 'static>>;

pub(crate) struct CodexEventEnvelope {
    pub run_id: String,
    pub event_ordinal: u64,
    pub event: PendingProviderEvent,
}

pub(crate) struct CodexEventReceipt {
    run_id: String,
    event_ordinal: u64,
    persisted_sequence: u64,
}

pub(crate) enum CodexToolOutcome {
    Approved { canonical_result: Value },
    Denied { reason_code: String },
    Failed { reason_code: String },
}

pub(crate) struct CodexDynamicToolCall {
    pub run_id: String,
    pub provider_call_id: String,
    pub tool_name: String,
    pub canonical_arguments: Value,
}

pub(crate) struct CodexToolResolutionReceipt {
    run_id: String,
    provider_call_id: String,
    idempotency_key: String,
    persisted_sequence: u64,
    outcome: CodexToolOutcome,
}

impl CodexEventReceipt {
    pub(super) fn committed(
        run_id: String,
        event_ordinal: u64,
        persisted_sequence: u64,
    ) -> Result<Self, ProviderFailure>;
}

impl CodexToolResolutionReceipt {
    pub(super) fn committed(
        run_id: String,
        provider_call_id: String,
        idempotency_key: String,
        persisted_sequence: u64,
        outcome: CodexToolOutcome,
    ) -> Result<Self, ProviderFailure>;
}

pub(crate) fn codex_tool_idempotency_key(
    run_id: &str,
    provider_call_id: &str,
    tool_name: &str,
    canonical_arguments: &Value,
) -> Result<String, ProviderFailure>;

pub(crate) trait CodexTurnCallbacks: Send + Sync + 'static {
    fn on_event(
        &self,
        envelope: CodexEventEnvelope,
    ) -> CodexCallbackFuture<CodexEventReceipt>;

    fn resolve_tool(
        &self,
        call: CodexDynamicToolCall,
    ) -> CodexCallbackFuture<CodexToolResolutionReceipt>;
}

pub(crate) struct CodexAppServer {
    conversation: SupervisedConversation,
    home: CodexHome,
    protocol: CodexProtocolState,
}

impl CodexAppServer {
    pub(crate) async fn start(
        supervisor: &ProcessSupervisor,
        runtime: &RuntimeLayout,
        home: CodexHome,
        cancellation: CancellationToken,
    ) -> Result<Self, ProviderFailure>;

    pub(crate) async fn run_ephemeral_turn(
        &mut self,
        request: CodexTurnRequest,
        callbacks: Arc<dyn CodexTurnCallbacks>,
        cancellation: CancellationToken,
    ) -> ProviderExecution;
}

pub(crate) enum CodexActorCommand {
    Run {
        request: CodexTurnRequest,
        callbacks: Arc<dyn CodexTurnCallbacks>,
        cancellation: CancellationToken,
        reply: oneshot::Sender<ProviderExecution>,
    },
    Shutdown {
        reply: oneshot::Sender<Result<(), ProviderFailure>>,
    },
}

#[derive(Clone)]
pub(crate) struct CodexRuntimeHandle {
    sender: mpsc::Sender<CodexActorCommand>,
}

impl CodexRuntimeHandle {
    pub(crate) async fn run(
        &self,
        request: CodexTurnRequest,
        callbacks: Arc<dyn CodexTurnCallbacks>,
        cancellation: CancellationToken,
    ) -> ProviderExecution;
    pub(crate) async fn shutdown(&self) -> Result<(), ProviderFailure>;
}
```

The replacement `codex_actor.rs` contains no legacy direct-provider logic. One actor owns the only `CodexAppServer`; its bounded channel is the only Host-to-adapter entry. `QuantixHost` creates an `Arc<HostCodexTurnCallbacks>` bound to `request.run_id`, sends it with the immutable request, and never exposes the raw app-server. `CodexEventReceipt` and `CodexToolResolutionReceipt` fields stay private to the adapter module. Their `pub(super)` constructors reject empty/mismatched run IDs, event ordinal zero, empty call/key, and persisted sequence zero. The adapter recomputes `codex_tool_idempotency_key` and requires request, callback, event receipt, tool call, and tool receipt to carry the same `run_id`; it requires event ordinal and provider call ID to match before any continuation bytes are written.

The Tender schema moves from version 45 to 46 without a compatibility migration and adds the replay record required for exactly-once resolution:

```sql
CREATE TABLE agent_tool_call_resolutions (
  run_id TEXT NOT NULL,
  provider_call_id TEXT NOT NULL,
  idempotency_key TEXT NOT NULL UNIQUE CHECK (length(idempotency_key) = 64),
  outcome TEXT NOT NULL CHECK (outcome IN ('approved', 'denied', 'failed')),
  canonical_result_json TEXT CHECK (
    canonical_result_json IS NULL OR json_valid(canonical_result_json)
  ),
  reason_code TEXT,
  persisted_sequence INTEGER NOT NULL CHECK (persisted_sequence > 0),
  resolved_at TEXT NOT NULL,
  PRIMARY KEY (run_id, provider_call_id),
  FOREIGN KEY (run_id) REFERENCES agent_runs(run_id),
  FOREIGN KEY (run_id, persisted_sequence)
    REFERENCES provider_events(run_id, sequence),
  CHECK (
    (outcome = 'approved' AND canonical_result_json IS NOT NULL AND reason_code IS NULL)
    OR (outcome IN ('denied', 'failed') AND canonical_result_json IS NULL
      AND reason_code IS NOT NULL)
  )
);
```

Create this table after `provider_events`. Bound `canonical_result_json` to the existing tool-result byte ceiling before the insert. Update runtime/update fixtures that state tender schema version 45 to 46; do not add a schema fallback or migration.

- [ ] **Step 1: Add private adapter RED tests**

Put protocol/router tests in `codex_app_server.rs` under `#[cfg(test)] mod tests`. Use a `RecordingCallbacks` implementation whose futures block on `tokio::sync::oneshot` until the test releases persistence or tool resolution. Prove:

1. the adapter does not read or acknowledge a later event before `on_event` returns a receipt;
2. the adapter sends no dynamic-tool result before `resolve_tool` returns a matching receipt;
3. a mismatched `run_id`, event ordinal, `provider_call_id`, recomputed idempotency key, or zero persisted sequence quarantines the run;
4. cancellation wins while either callback is pending, sends `turn/interrupt`, and reaps the process;
5. an exact repeated provider call receives the stored result and never executes twice;
6. reuse of a provider call ID with different tool name or arguments quarantines the run;
7. a Host tool timeout/execution failure commits `CodexToolOutcome::Failed` with a bounded redacted reason and replays that same failure without executing twice.
8. the 33rd dynamic-tool proposal, repeated equivalent call/output cycles, usage exhaustion, and a late event after cancellation produce a bounded terminal/quarantine outcome and no continuation.

```rust
#[tokio::test]
async fn adapter_rejects_every_mismatched_committed_receipt() {
    for mutation in [
        ReceiptMutation::RunId,
        ReceiptMutation::EventOrdinal,
        ReceiptMutation::ProviderCallId,
        ReceiptMutation::IdempotencyKey,
        ReceiptMutation::ZeroPersistedSequence,
    ] {
        let harness = AdapterHarness::with_receipt_mutation(mutation).await;
        let execution = harness.run().await;
        assert_eq!(execution.state, AgentRunState::Failed);
        assert_eq!(execution.failure.unwrap().category, ProviderFailureCategory::ProtocolInvalid);
        assert_eq!(harness.continuation_writes(), 0);
    }
}

#[tokio::test]
async fn adapter_writes_nothing_while_host_resolution_is_pending() {
    let (release, callbacks) = RecordingCallbacks::blocked_tool_resolution();
    let harness = AdapterHarness::start(callbacks).await;
    harness.wait_for_tool_call().await;
    assert_eq!(harness.continuation_writes(), 0);
    release.send(()).expect("release callback");
    assert_eq!(harness.finish().await.state, AgentRunState::Completed);
}
```

Define `ReceiptMutation` and `AdapterHarness` in the source test module. Expected RED: `CodexAppServer`/receipt types do not compile initially; after stubs exist, the continuation count is 1 before callback release or mutated receipts are accepted.

- [ ] **Step 2: Add the public-Host integration RED test**

Update `src-tauri/tests/agent_runtime.rs` to use only public `QuantixHost` setup, Agent Access approval, Agent Run inspection, interruption, and run commands. Extend `quantix-runtime-fixture` with exact scenarios `dynamic_tool_duplicate`, `crash_before_event_commit`, `commit_then_callback_error`, `crash_after_tool_commit`, and `actor_drop_pending`. Add these integration tests:

```rust
#[tokio::test]
async fn codex_race_duplicate_tool_is_executed_once_and_replays_committed_result() {
    let fixture = CodexHostFixture::start(CodexFixtureScenario::DynamicToolDuplicate).await;
    let run_id = fixture.start_agent_run().await;
    let request = fixture.wait_for_access_request(&run_id).await;
    fixture.approve_access(request).await;
    let run = fixture.wait_for_terminal_run(&run_id).await;
    let records = fixture.inspect_tool_records(&run_id);
    assert_eq!(run.state, AgentRunState::Completed);
    assert_eq!(records.reservations, 1);
    assert_eq!(records.executions, 1);
    assert_eq!(records.provider_replies, 2);
    assert_eq!(records.committed_resolutions, 1);
}

#[tokio::test]
async fn codex_crash_before_event_commit_sends_no_continuation() {
    let fixture = CodexHostFixture::start(CodexFixtureScenario::CrashBeforeEventCommit).await;
    let run_id = fixture.start_agent_run().await;
    let run = fixture.wait_for_terminal_run(&run_id).await;
    assert!(matches!(run.state, AgentRunState::Failed | AgentRunState::Indeterminate));
    assert_eq!(fixture.fixture_continuation_count(), 0);
    assert!(fixture.process_tree_reaped());
}

#[tokio::test]
async fn codex_race_commit_then_callback_error_rereads_the_committed_receipt() {
    let fixture = CodexHostFixture::start(CodexFixtureScenario::CommitThenCallbackError).await;
    fixture.inject_next_tool_commit_outcome(FixtureToolCommitOutcome::ErrorAfterCommit);
    let run_id = fixture.start_and_approve_agent_run().await;
    let run = fixture.wait_for_terminal_run(&run_id).await;
    assert_eq!(run.state, AgentRunState::Completed);
    assert_eq!(fixture.inspect_tool_records(&run_id).executions, 1);
}

#[tokio::test]
async fn codex_crash_after_tool_commit_is_indeterminate_without_retry_or_reexecution() {
    let fixture = CodexHostFixture::start(CodexFixtureScenario::CrashAfterToolCommit).await;
    let run_id = fixture.start_and_approve_agent_run().await;
    let run = fixture.wait_for_terminal_run(&run_id).await;
    assert_eq!(run.state, AgentRunState::Indeterminate);
    assert!(!run.has_linked_retry);
    assert_eq!(fixture.inspect_tool_records(&run_id).executions, 1);
    assert_eq!(fixture.provider_process_start_count(), 1);
}

#[tokio::test]
async fn codex_crash_actor_drop_reaps_the_job_and_resolves_the_waiter() {
    let fixture = CodexHostFixture::start(CodexFixtureScenario::ActorDropPending).await;
    let run_id = fixture.start_agent_run().await;
    fixture.drop_runtime_handle().await;
    let run = fixture.wait_for_terminal_run(&run_id).await;
    assert_ne!(run.state, AgentRunState::Running);
    assert!(fixture.process_tree_reaped());
}
```

Define `CodexHostFixture` and `CodexFixtureScenario` in `tests/support/codex_host_fixture.rs`. Add public `#[cfg(any(test, feature = "runtime-fixture"))] enum FixtureToolCommitOutcome { ErrorBeforeCommit, ErrorAfterCommit }` plus a public Host injection method; production cannot construct it. The helper may configure named sidecar scenarios and read fixture counters, but all product actions and observations go through public `QuantixHost`; it cannot import `CodexAppServer`, actor commands, callbacks, or receipt types. The first test observes and approves the persisted access request, then asserts one reservation, one tool execution, two identical provider replies, and one committed resolution. The commit/error test injects `ErrorAfterCommit`, forces a read-back, and still executes once. The two crash tests assert no automatic actor/process restart, no linked retry, no canonical result publication, a truthful interrupted/indeterminate failure, and full Job Object cleanup.

Expected RED: integration fixtures/actor route are missing, or observable results show duplicate execution, continuation before commit, automatic retry, a running terminal state, or an unreaped process.

- [ ] **Step 3: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib agent_runtime::codex_app_server::tests --features runtime-fixture -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime --features runtime-fixture codex_race_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime --features runtime-fixture codex_crash_ -- --nocapture
```

Expected RED: the private adapter module/actor fixtures are missing, or at least one race/crash test reports duplicate execution, continuation before commit, a still-running Agent Run, or an unreaped child.

- [ ] **Step 4: Implement Host callback ordering and idempotency**

Implement `HostCodexTurnCallbacks` in `agent_runtime.rs`. For each event it opens a short Tender transaction, persists the event, commits, and only then returns a receipt. For each dynamic-tool call it:

1. validates the callback-bound run ID, canonicalizes arguments, and calls `codex_tool_idempotency_key(run_id, provider_call_id, tool_name, canonical_arguments)`;
2. claims that key in a unique SQLite row and returns the previously committed resolution if already complete;
3. persists the proposal before requesting access;
4. validates the typed tool and current permission grant;
5. when Engineer approval is required, persists the Agent Access request, releases every store lock, and awaits a `tokio::sync::Notify` keyed by run and call while also selecting on cancellation;
6. reloads the committed Engineer decision, executes an approved tool exactly once, and persists approved, denied, or failed result plus audit event in one transaction; timeouts/errors use only a bounded redacted reason code;
7. returns the private receipt only after that transaction commits.

Before steps 2 and 7, re-run the existing loop detector and cumulative time/token/byte/cost/tool budget against persisted state. A repeated-equivalent/non-progress verdict or the 32-round ceiling persists the terminal reason, interrupts the turn, and returns no tool continuation.

`approve_agent_access` and `resolve_agent_access` notify the keyed waiter after their own commits. They never hold a store mutex while notifying. A callback error or invalid receipt interrupts and quarantines the turn; the adapter never fabricates a denial or sends an unpersisted result.

Implement the shared key and store boundary with these exact shapes:

```rust
pub(crate) fn codex_tool_idempotency_key(
    run_id: &str,
    provider_call_id: &str,
    tool_name: &str,
    canonical_arguments: &Value,
) -> Result<String, ProviderFailure> {
    validate_run_and_call_ids(run_id, provider_call_id)?;
    let bytes = serde_json_canonicalizer::to_vec(&json!({
        "run_id": run_id,
        "provider_call_id": provider_call_id,
        "tool_name": tool_name,
        "arguments": canonical_arguments,
    }))
    .map_err(|_| protocol_failure("codex_tool_arguments_invalid"))?;
    Ok(sha256_hex(&bytes))
}

pub(crate) enum ToolResolutionClaim {
    New,
    Committed(StoredCodexToolResolution),
    Conflicting,
}

pub(crate) struct StoredCodexToolResolution {
    pub run_id: String,
    pub provider_call_id: String,
    pub idempotency_key: String,
    pub persisted_sequence: u64,
    pub outcome: CodexToolOutcome,
}

impl TenderStore {
    pub(crate) fn claim_codex_tool_resolution(
        &self,
        run_id: &str,
        call: &CodexDynamicToolCall,
        idempotency_key: &str,
    ) -> Result<ToolResolutionClaim, TenderCommandError>;
    pub(crate) fn commit_codex_tool_resolution(
        &self,
        resolution: &StoredCodexToolResolution,
    ) -> Result<StoredCodexToolResolution, TenderCommandError>;
}
```

`claim_codex_tool_resolution` executes one immediate transaction: exact key returns `Committed`, same run/call with another key returns `Conflicting`, otherwise it inserts the reservation. `commit_codex_tool_resolution` inserts the provider event and resolution row in one transaction, then rereads after `ErrorAfterCommit` before returning. `HostCodexTurnCallbacks` converts only that returned stored row into the adapter-private receipt through its validated `pub(super)` constructor.

- [ ] **Step 5: Implement the adapter event loop and cancellation**

The actor serializes app-server ownership and acquires the Host Codex account/auth gate before accepting a run envelope; it releases every Tender store lock before process launch or callback await. Publish only permission-granted Quantix Typed Tool definitions through the reviewed dynamic-tool protocol; never expose Codex built-ins as Host tools. For each allowed event, await `callbacks.on_event` and verify the same run/ordinal before consuming a continuation. For each dynamic tool request, bind the request run ID, await `callbacks.resolve_tool`, verify run/call/key/sequence, then serialize only the stored outcome. Reconcile normalized usage and every cumulative budget before accepting the next event/request. On cancellation or exhaustion, send `turn/interrupt`, reject late frames, wait five seconds, then kill and reap the Job Object. Actor exit resolves every pending oneshot and never auto-restarts the process or Provider Turn.

The write-ordering core must be visibly equivalent to:

```rust
let receipt = tokio::select! {
    result = callbacks.resolve_tool(call.clone()) => result?,
    _ = cancellation.cancelled() => return interrupt_and_reap(self, &request.run_id).await,
};
validate_tool_receipt(&request.run_id, &call, &receipt)?;
enforce_cumulative_limits(&request.run_id)?;
self.conversation
    .write(&encode_dynamic_tool_result(&receipt)?)
    .await
    .map_err(map_process_failure)?;
```

For ordinary events, replace `resolve_tool`/`validate_tool_receipt` with `on_event`/`validate_event_receipt` and still write nothing before validation. This is the minimal implementation; a background fire-and-forget callback is forbidden.

- [ ] **Step 6: Run GREEN and the supervisor regression**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib agent_runtime::codex_app_server::tests --features runtime-fixture -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime --features runtime-fixture codex_race_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime --features runtime-fixture codex_crash_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib process_supervisor::tests --features runtime-fixture
cargo test --manifest-path src-tauri/Cargo.toml --test runtime_readiness --test safe_updates --features runtime-fixture
```

- [ ] **Step 7: Commit Task 7**

```powershell
git add src-tauri/src/agent_runtime/codex_app_server.rs src-tauri/src/agent_runtime/codex_actor.rs src-tauri/src/agent_runtime.rs src-tauri/src/host.rs src-tauri/src/tender_store.rs src-tauri/src/tender_store/agent_records.rs src-tauri/tests/runtime_readiness.rs src-tauri/tests/safe_updates.rs src-tauri/tests/support/codex_host_fixture.rs src-tauri/tests/support/runtime_fixture.rs src-tauri/tests/agent_runtime.rs
git commit -m "feat: govern Codex turns through Host callbacks"
```

### Task 8: Implement Managed Login, Account Replacement, and Corrupt-Auth Recovery

**Files:**
- Modify: `src-tauri/src/agent_runtime/codex_app_server.rs`
- Modify: `src-tauri/src/agent_runtime/codex_actor.rs`
- Modify: `src-tauri/src/ai/contract.rs`
- Modify: `src-tauri/src/ai/connections.rs`
- Modify: `src-tauri/src/ai/vault.rs`
- Modify: `src-tauri/src/application_settings.rs`
- Modify: `src-tauri/src/host.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/support/runtime_fixture.rs`
- Modify: `src-tauri/tests/ai_connection_repository.rs`
- Create: `src-tauri/tests/managed_codex_account.rs`

**Interfaces:**
- Consumes: Task 7 app-server, isolated `CodexHome`, Tauri `OpenerExt`, and Tauri `Emitter`.
- Produces exported command DTOs with no browser URL or token in the browser-login response:

```rust
pub enum CodexLoginMethod {
    Browser,
    DeviceCode,
}

pub struct StartCodexAccountLoginCommand {
    pub method: CodexLoginMethod,
}

pub struct StartCodexAccountLoginResult {
    pub login_id: String,
    pub verification_url: Option<String>,
    pub user_code: Option<String>,
}

pub struct ReplaceCodexAccountCommand {
    pub method: CodexLoginMethod,
    pub expected_account_fingerprint: String,
    pub expected_account_revision: u64,
    pub exact_confirmation: String,
}

pub struct RecoverCodexAuthCommand {
    pub expected_account_fingerprint: Option<String>,
    pub expected_account_revision: u64,
    pub exact_confirmation: String,
}

pub struct CancelCodexAccountLoginCommand { pub login_id: String }
pub struct LogoutCodexAccountCommand {
    pub expected_account_fingerprint: String,
    pub expected_account_revision: u64,
}

#[tauri::command]
async fn start_codex_account_login(
    app: tauri::AppHandle,
    host: tauri::State<'_, QuantixHost>,
    command: StartCodexAccountLoginCommand,
) -> Result<StartCodexAccountLoginResult, TenderCommandError>;
#[tauri::command]
async fn replace_codex_account(
    app: tauri::AppHandle,
    host: tauri::State<'_, QuantixHost>,
    command: ReplaceCodexAccountCommand,
) -> Result<StartCodexAccountLoginResult, TenderCommandError>;
#[tauri::command]
async fn cancel_codex_account_login(
    host: tauri::State<'_, QuantixHost>,
    command: CancelCodexAccountLoginCommand,
) -> Result<(), TenderCommandError>;
#[tauri::command]
async fn logout_codex_account(
    host: tauri::State<'_, QuantixHost>,
    command: LogoutCodexAccountCommand,
) -> Result<ApplicationSettingsView, TenderCommandError>;
#[tauri::command]
async fn recover_codex_auth(
    host: tauri::State<'_, QuantixHost>,
    command: RecoverCodexAuthCommand,
) -> Result<ApplicationSettingsView, TenderCommandError>;
```

The actor returns this exact private challenge. It derives neither `Clone`, `Debug`, `Serialize`, nor `TS`; device code and browser URL are zeroized on drop.

```rust
pub(crate) enum CodexLoginChallenge {
    Browser {
        login_id: String,
        auth_url: Zeroizing<String>,
    },
    DeviceCode {
        login_id: String,
        verification_url: String,
        user_code: Zeroizing<String>,
    },
}

pub(crate) struct CodexLoginOperation {
    pub challenge: CodexLoginChallenge,
    pub completion: oneshot::Receiver<Result<CodexAccountSnapshot, ProviderFailure>>,
}

pub(crate) struct CodexAccountSnapshot {
    pub account_fingerprint: String,
    pub account_label: Option<String>,
    pub plan_type: Option<String>,
    pub models: Vec<AiModelView>,
}

impl CodexAppServer {
    pub(crate) async fn begin_login(
        &mut self,
        method: CodexManagedLoginMethod,
    ) -> Result<CodexLoginChallenge, ProviderFailure>;
    pub(crate) async fn wait_for_login_completion(
        &mut self,
        login_id: &str,
        cancellation: CancellationToken,
    ) -> Result<CodexAccountSnapshot, ProviderFailure>;
    pub(crate) async fn cancel_login(&mut self) -> Result<(), ProviderFailure>;
    pub(crate) async fn logout(&mut self) -> Result<(), ProviderFailure>;
    pub(crate) async fn refresh_account_and_models(
        &mut self,
    ) -> Result<CodexAccountSnapshot, ProviderFailure>;
}

// Task 8 adds CodexActorCommand::Account(CodexAccountActorCommand). The actor
// remains the only CodexAppServer owner and multiplexes app-server notifications
// with bounded Host commands while login is pending.
pub(crate) enum CodexAccountActorCommand {
    BeginLogin {
        method: CodexManagedLoginMethod,
        cancellation: CancellationToken,
        reply: oneshot::Sender<Result<CodexLoginOperation, ProviderFailure>>,
    },
    CancelLogin {
        login_id: String,
        reply: oneshot::Sender<Result<(), ProviderFailure>>,
    },
    Logout {
        reply: oneshot::Sender<Result<(), ProviderFailure>>,
    },
    RefreshAccount {
        reply: oneshot::Sender<Result<CodexAccountSnapshot, ProviderFailure>>,
    },
}

impl CodexRuntimeHandle {
    pub(crate) async fn begin_login(
        &self,
        method: CodexManagedLoginMethod,
        cancellation: CancellationToken,
    ) -> Result<CodexLoginOperation, ProviderFailure>;
    pub(crate) async fn cancel_login(&self, login_id: String)
        -> Result<(), ProviderFailure>;
    pub(crate) async fn logout(&self) -> Result<(), ProviderFailure>;
    pub(crate) async fn refresh_account(&self)
        -> Result<CodexAccountSnapshot, ProviderFailure>;
}
```

`QuantixHost` owns account revision, connection CAS, and replacement policy. The actor owns protocol order and app-server state only. The Host begins login through `CodexRuntimeHandle`, the native Tauri layer consumes the private challenge, and the background completion task awaits `CodexLoginOperation::completion` before persisting and emitting the sanitized view. Corrupt-auth recovery calls `CodexRuntimeHandle::shutdown`, proves the actor and Job Object stopped, invokes `CodexHome::recovery_delete_auth`, then creates a new handle; no Host method calls `CodexAppServer` directly.

- [ ] **Step 1: Add private login state-machine RED tests**

In `codex_app_server.rs`'s inline tests, add tests whose names begin `codex_login_` and cover both valid notification orders, unrelated login IDs, cancellation, timeout, and protocol errors. A successful flow must consume matching `account/login/completed`, then consume or reconcile `account/updated`, call `account/read`, refresh `model/list`, and publish one sanitized account revision.

```rust
#[tokio::test]
async fn codex_login_accepts_only_documented_matching_sequences() {
    for (sequence, expected) in [
        (LoginFixtureSequence::CompletedThenUpdated, Ok(LoginTerminal::Ready)),
        (LoginFixtureSequence::UpdatedThenCompleted, Ok(LoginTerminal::Ready)),
        (LoginFixtureSequence::UnrelatedLoginId, Err(ProviderFailureCategory::ProtocolInvalid)),
        (LoginFixtureSequence::CompletedWithError, Err(ProviderFailureCategory::AuthenticationRequired)),
        (LoginFixtureSequence::TimedOut, Err(ProviderFailureCategory::ProcessFailed)),
    ] {
        let result = LoginStateHarness::run(sequence).await;
        match expected {
            Ok(terminal) => assert_eq!(result.unwrap().terminal, terminal),
            Err(category) => assert_eq!(result.unwrap_err().category, category),
        }
    }
}
```

Define `LoginFixtureSequence`, `LoginTerminal`, and `LoginStateHarness` in the inline test module. Expected RED: the login operation/state router does not exist; after a stub, unrelated IDs or missing notifications incorrectly produce `Ready`.

- [ ] **Step 2: Add public IPC/Host RED tests**

In `src-tauri/tests/managed_codex_account.rs`, invoke Tauri commands through `configure_tauri_builder` and `get_ipc_response`, and use only `QuantixHost` public views. Assert:

- browser login opens the app-server URL through `app.opener().open_url` inside the native command and the IPC JSON never contains that URL;
- device login returns only its verification URL/code to the initiating call;
- completion emits `quantix://codex-account-state-changed` with a sanitized `ApplicationSettingsView`;
- replacing an existing account fails unless the fingerprint matches and exact confirmation is `REPLACE CODEX ACCOUNT`;
- replacement invalidates the confirmed AI selection revision before new login begins;
- cancel rejects a stale or different `login_id`, and logout/recovery reject stale account revisions/fingerprints;
- normal logout calls `account/logout` and never directly deletes `auth.json`;
- `recover_codex_auth` fails unless confirmation is exactly `DELETE CORRUPT CODEX AUTH`, stops/reaps the app-server, deletes only the corrupt auth file through `CodexHome::recovery_delete_auth`, then restarts disconnected.
- a run-start/account-replacement race has only two legal outcomes: the run acquires the old account/revision gate first and replacement waits for its governed terminal, or replacement commits first and run preparation rejects the stale immutable selection; no request may use mixed account/selection identity.

Add a public integration test named `codex_account_race_replacement_never_mixes_run_identity`. Use two barriers in the fixture: one immediately before run-start gate acquisition and one immediately before replacement commit. Execute both deterministic orderings, then assert every accepted Provider Turn's stored selection/account fingerprint pair came from one committed revision and the stale ordering launched zero provider requests.

```rust
#[tokio::test]
async fn browser_login_opens_native_url_without_serializing_it() {
    let harness = ManagedAccountHarness::browser_login();
    let response = harness.invoke_start_login().await;
    assert_eq!(response.status(), 200);
    assert_eq!(harness.opened_urls().len(), 1);
    let json = response.into_json();
    assert!(json.get("login_id").is_some());
    assert!(json.get("auth_url").is_none());
    assert!(!json.to_string().contains("https://auth.example"));
}

#[tokio::test]
async fn codex_account_race_replacement_never_mixes_run_identity() {
    for ordering in [RaceOrdering::RunWins, RaceOrdering::ReplacementWins] {
        let harness = ManagedAccountHarness::race(ordering).await;
        let result = harness.release_both_and_wait().await;
        assert!(result.accepted_pairs.iter().all(|pair| pair.is_one_committed_revision()));
        if ordering == RaceOrdering::ReplacementWins {
            assert_eq!(result.provider_request_count, 0);
        }
    }
}
```

`ManagedAccountHarness` wraps the existing mock Tauri builder and public Host methods; its opener records URLs without launching a browser. Expected RED: old login commands serialize an auth URL, no account event is emitted, or the replacement-wins ordering still launches a provider request.

- [ ] **Step 3: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib codex_login_ --features runtime-fixture -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test managed_codex_account --features runtime-fixture -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test managed_codex_account --features runtime-fixture codex_account_race_ -- --nocapture
```

Expected RED: compile failure for missing managed-login DTO/actor methods, followed by nonzero tests for URL leakage, stale CAS acceptance, or unreaped recovery process.

- [ ] **Step 4: Implement managed browser/device login and async publication**

The Tauri browser command takes the private browser challenge, immediately calls `app.opener().open_url(auth_url, None::<&str>)`, zeroizes and drops the URL, returns a sanitized result, and spawns a bounded login-completion task. The task consumes `account/login/completed` and `account/updated`, refreshes account/models, persists non-secret fingerprint/label/plan/catalogue metadata, and emits `quantix://codex-account-state-changed`. Device login uses the same completion task but returns verification URL/code only to its initiating IPC result.

Use the single Host auth mutex for start, replace, cancel, logout, recovery, and account refresh. Do not hold it while the renderer decides whether to replace; recheck expected fingerprint/revision after reacquiring it.

The native command must consume the private challenge before constructing the public result:

```rust
let CodexLoginOperation { challenge, completion } =
    host.inner().begin_codex_login(command.method).await?;
let result = match challenge {
    CodexLoginChallenge::Browser { login_id, mut auth_url } => {
        app.opener().open_url(auth_url.as_str(), None::<&str>)?;
        auth_url.zeroize();
        StartCodexAccountLoginResult {
            login_id,
            verification_url: None,
            user_code: None,
        }
    }
    CodexLoginChallenge::DeviceCode {
        login_id,
        verification_url,
        user_code,
    } => StartCodexAccountLoginResult {
        login_id,
        verification_url: Some(verification_url),
        user_code: Some(user_code.to_string()),
    },
};
spawn_login_completion(app.clone(), host.inner().clone(), completion);
Ok(result)
```

`spawn_login_completion` awaits the oneshot, persists the matching account snapshot/revision, then emits the sanitized settings view. On error it persists a bounded failure state and emits no URL/code/token.

- [ ] **Step 5: Remove account secrets from Quantix storage and implement explicit destructive paths**

Delete Codex access/refresh/ID-token variants from the vault contract. Persist only non-secret account fingerprint, optional label, plan, model catalogue identity, and revision. Account replacement first performs normal app-server logout, invalidates approval, then starts a new login. Corrupt-auth recovery is the only direct auth-file deletion path and is unavailable while the process is running.

Implement recovery in this strict order:

```rust
let _gate = self.inner.codex_account_gate.lock().await;
validate_recovery_confirmation(&command, self.inspect_codex_account()?)?;
let handle = self.take_codex_runtime_handle()?;
handle.shutdown().await?;
let home = CodexHome::prepare(self.application_home())?;
home.recovery_delete_auth(true, &command.exact_confirmation)?;
self.install_codex_runtime_handle(CodexRuntimeHandle::start(self)?);
self.persist_codex_disconnected_state()?;
self.inspect_application_settings()
```

No normal logout/replacement path calls `recovery_delete_auth`; that assertion belongs in the source-module test.

- [ ] **Step 6: Run GREEN**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib agent_runtime::codex_app_server::tests --features runtime-fixture
cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_repository --features runtime-fixture
cargo test --manifest-path src-tauri/Cargo.toml --test managed_codex_account --features runtime-fixture -- --nocapture
```

- [ ] **Step 7: Commit Task 8**

```powershell
git add src-tauri/src/agent_runtime/codex_app_server.rs src-tauri/src/agent_runtime/codex_actor.rs src-tauri/src/ai src-tauri/src/application_settings.rs src-tauri/src/host.rs src-tauri/src/lib.rs src-tauri/tests/support/runtime_fixture.rs src-tauri/tests/ai_connection_repository.rs src-tauri/tests/managed_codex_account.rs
git commit -m "feat: add Codex managed account lifecycle"
```

### Task 9: Qualify and Persist the Selected Codex Capability Tuple

**Files:**
- Modify: `src-tauri/src/agent_runtime/codex_app_server.rs`
- Modify: `src-tauri/src/agent_runtime/codex_actor.rs`
- Modify: `src-tauri/src/agent_runtime.rs`
- Modify: `src-tauri/src/ai/contract.rs`
- Modify: `src-tauri/src/ai/connections.rs`
- Modify: `src-tauri/src/application_settings.rs`
- Modify: `src-tauri/src/diagnostics.rs`
- Modify: `src-tauri/src/host.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/support/codex_host_fixture.rs`
- Modify: `src-tauri/tests/support/runtime_fixture.rs`
- Modify: `src-tauri/tests/ai_connection_repository.rs`
- Create: `src-tauri/tests/codex_qualification.rs`
- Modify: `src-tauri/src/bin/export_bindings.rs`
- Create by generation: `src/bindings/ProbeCodexSelectionCommand.ts`
- Create by generation: `src/bindings/CodexQualificationView.ts`

**Interfaces:**

```rust
pub struct ProbeCodexSelectionCommand {
    pub connection_id: String,
    pub expected_execution_revision: u64,
    pub expected_account_fingerprint: String,
    pub model_id: String,
    pub provider_route_id: String,
    pub reasoning: AiReasoningSelection,
    pub expected_catalogue_sha256: String,
    pub disclosed_billable_probe_accepted: bool,
}

pub struct CodexQualificationView {
    pub connection_id: String,
    pub execution_revision: u64,
    pub model_id: String,
    pub provider_route_id: String,
    pub reasoning: AiReasoningSelection,
    pub catalogue_sha256: String,
    pub capability_sha256: String,
    pub capabilities: AiCapabilitySet,
    pub observed_at: String,
}

pub(crate) struct CodexProbeRequest {
    pub probe_id: String,
    pub connection_id: AiConnectionId,
    pub execution_revision: AiConnectionRevision,
    pub account_fingerprint: String,
    pub model: AiModelView,
    pub reasoning: AiReasoningSelection,
    pub expected_catalogue_sha256: String,
}

pub(crate) trait CodexProbeCallbacks: Send + Sync + 'static {
    fn record_event(
        &self,
        probe_id: String,
        ordinal: u64,
        event: PendingProviderEvent,
    ) -> CodexCallbackFuture<u64>;
    fn resolve_echo_tool(
        &self,
        probe_id: String,
        call_id: String,
        nonce: String,
    ) -> CodexCallbackFuture<String>;
}

impl CodexAppServer {
    pub(crate) async fn probe_selected_tuple(
        &mut self,
        request: CodexProbeRequest,
        callbacks: Arc<dyn CodexProbeCallbacks>,
        cancellation: CancellationToken,
    ) -> Result<AiProbeEvidence, ProviderFailure>;
}

impl CodexRuntimeHandle {
    pub(crate) async fn probe(
        &self,
        request: CodexProbeRequest,
        callbacks: Arc<dyn CodexProbeCallbacks>,
        cancellation: CancellationToken,
    ) -> Result<AiProbeEvidence, ProviderFailure>;
}

pub(crate) struct CodexProbeActorCommand {
    pub request: CodexProbeRequest,
    pub callbacks: Arc<dyn CodexProbeCallbacks>,
    pub cancellation: CancellationToken,
    pub reply: oneshot::Sender<Result<AiProbeEvidence, ProviderFailure>>,
}

impl QuantixHost {
    pub async fn probe_codex_selection(
        &self,
        command: ProbeCodexSelectionCommand,
    ) -> Result<CodexQualificationView, TenderCommandError>;
}
```

Task 9 adds `CodexActorCommand::Probe(CodexProbeActorCommand)`. The final actor command surface is exactly `Run`, `Account`, `Probe`, and `Shutdown`; no raw RPC command or app-server reference crosses into Host code. The Host callback accepts only `quantix_capability_probe_echo` with one bounded random nonce, persists each probe event before reply, and returns that same nonce. It exposes no Tender, filesystem, MCP, web, or arbitrary tool. The probe uses the exact selected model/provider route/reasoning, empty staging cwd, read-only sandbox, no fallback, strict JSON Schema output, at most two tool rounds, at most 1,024 output tokens, a 60-second overall deadline, one structured/tool turn, and one bounded intentional-cancellation turn. It has no outer retry and records unknown where app-server cannot prove a fact.

Only a successful probe may report exact destination/authentication, requested/reported model equality, streaming, strict output, the Host dynamic-tool echo bridge, reasoning, usage/request identity, context limits, and cancellation. The Host re-acquires the account/connection gate and calls `AiConnectionRepository::record_probe` with CAS on connection revision, account fingerprint, endpoint, adapter, model/provider route, reasoning, and pre-probe catalogue hash. The repository commits `AiProbeEvidence` before returning `CodexQualificationView`. Activation remains a separate explicit Engineer command and constructs Task 4's immutable selection from this committed evidence; a process crash can never auto-activate.

- [ ] **Step 1: Add RED private probe-limit and bridge tests**

In `codex_app_server.rs`, add `codex_probe_` tests for wrong model/provider route/reasoning, reroute, missing strict output, missing/duplicate/malformed echo call, more than two tool rounds, token/time/output overflow, cancellation refusal, late output, missing usage/request facts, and app-server crash. Assert no probe callback reply is written before its persisted ordinal or matching echo resolution.

```rust
#[tokio::test]
async fn codex_probe_faults_fail_closed_without_early_reply() {
    for (fault, category) in [
        (ProbeFault::WrongModel, ProviderFailureCategory::ProtocolInvalid),
        (ProbeFault::WrongProviderRoute, ProviderFailureCategory::ProtocolInvalid),
        (ProbeFault::WrongReasoning, ProviderFailureCategory::ProtocolInvalid),
        (ProbeFault::DuplicateEchoCall, ProviderFailureCategory::ProtocolInvalid),
        (ProbeFault::ThirdToolRound, ProviderFailureCategory::RequestBudgetExceeded),
        (ProbeFault::OutputOverflow, ProviderFailureCategory::RequestBudgetExceeded),
        (ProbeFault::LateAfterCancel, ProviderFailureCategory::Interrupted),
        (ProbeFault::ProcessCrash, ProviderFailureCategory::ProcessFailed),
    ] {
        let harness = ProbeAdapterHarness::new(fault).await;
        let failure = harness.run().await.unwrap_err();
        assert_eq!(failure.category, category, "fault={fault:?}");
        assert_eq!(harness.reply_before_persist_count(), 0);
    }
}
```

Define `ProbeFault` and `ProbeAdapterHarness` in the source test module. Expected RED: the probe entry point is absent; once stubbed, at least one malformed/rerouted/over-budget transcript returns evidence or writes an echo result before persistence.

- [ ] **Step 2: Add RED public persistence, race, and crash tests**

Use only public Host/repository commands in `codex_qualification.rs`. Add these exact cases:

```rust
#[tokio::test]
async fn codex_probe_persists_evidence_before_explicit_activation() {
    let fixture = CodexHostFixture::start(CodexFixtureScenario::ProbeHappy).await;
    let command = fixture.codex_probe_command();
    assert_eq!(fixture.active_selection(), None);
    let qualified = fixture.host.probe_codex_selection(command).await.unwrap();
    assert!(fixture.connection_has_probe_evidence());
    assert_eq!(fixture.active_selection(), None);
    let activated = fixture.activate_qualified_selection(&qualified).await;
    assert_eq!(activated.capability_sha256, qualified.capability_sha256);
}

#[tokio::test]
async fn codex_probe_crash_after_provider_success_before_commit_leaves_no_evidence() {
    let fixture = CodexHostFixture::start(CodexFixtureScenario::ProbeCrashBeforeCommit).await;
    assert!(fixture.host.probe_codex_selection(fixture.codex_probe_command()).await.is_err());
    assert!(!fixture.connection_has_probe_evidence());
    assert!(fixture.activate_current_tuple().await.is_err());
}

#[tokio::test]
async fn codex_probe_race_account_or_catalogue_change_rejects_stale_evidence() {
    for change in [ProbeRaceChange::AccountReplacement, ProbeRaceChange::CatalogueRefresh] {
        let fixture = CodexHostFixture::start(CodexFixtureScenario::ProbeHeldBeforeCommit).await;
        let pending = fixture.spawn_probe();
        fixture.wait_for_probe_commit_barrier().await;
        fixture.commit_probe_race_change(change).await;
        fixture.release_probe_commit_barrier();
        assert!(pending.await.unwrap().is_err());
        assert!(!fixture.connection_has_probe_evidence());
    }
}
```

Add `ProbeHappy`, `ProbeCrashBeforeCommit`, and `ProbeHeldBeforeCommit` to the named fixture scenario enum. The test-only helper may expose barriers/counters but all connection, account, probe, and activation mutations must call public Host APIs.

Expected RED: the probe command/actor path is missing, or evidence activates automatically, survives a pre-commit crash, or commits after account/catalogue CAS drift.

- [ ] **Step 3: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib codex_probe_ --features runtime-fixture -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test codex_qualification --test ai_connection_repository --features runtime-fixture -- --nocapture
```

Expected RED: missing probe DTO/actor command causes compile failure, or persistence/race tests show evidence activated automatically, stale evidence committed, or a crash leaving probe evidence.

- [ ] **Step 4: Implement bounded qualification and commit-before-activation**

Implement the exact interface above. Never infer capability support from SDK presence or `model/list` alone. `HostCodexProbeCallbacks` appends redacted probe events and echo-resolution hashes through the existing `DiagnosticsStore` and fsyncs that record before returning its ordinal/result; it creates no new installation/Tender schema and never stores the nonce or provider text. Persist the selected model entry with its `provider_route_id`, normalize semantic evidence, compute catalogue/capability hashes, then atomically record it under unchanged connection/account identities. On callback/store `ErrorAfterCommit`, reread and return only matching committed evidence. On every other failure, leave the connection untested or retain its prior revision's evidence without activating it.

The Host implementation must follow this bounded sequence:

```rust
pub async fn probe_codex_selection(
    &self,
    command: ProbeCodexSelectionCommand,
) -> Result<CodexQualificationView, TenderCommandError> {
    command.validate()?;
    let prepared = self.prepare_codex_probe(&command)?;
    let cancellation = CancellationToken::new();
    let callbacks = Arc::new(HostCodexProbeCallbacks::new(
        self.clone(),
        prepared.probe_id.clone(),
    ));
    let evidence = self
        .codex_runtime_handle()?
        .probe(prepared.request, callbacks, cancellation)
        .await
        .map_err(map_provider_failure)?;
    self.revalidate_codex_probe_cas(&command, &evidence)?;
    let connection = match self.ai_connections().record_probe(evidence.clone()) {
        Ok(connection) => connection,
        Err(AiConnectionError::StoreIndeterminate) =>
            self.reread_matching_probe(&evidence)?,
        Err(error) => return Err(map_ai_connection_error(error)),
    };
    CodexQualificationView::from_committed(&connection, &evidence)
}
```

`prepare_codex_probe` snapshots but does not activate; `revalidate_codex_probe_cas` reacquires the gate and compares every command identity; `from_committed` rejects a view whose stored hashes differ from `evidence`.

- [ ] **Step 5: Run GREEN, regenerate, and commit**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib codex_probe_ --features runtime-fixture
cargo test --manifest-path src-tauri/Cargo.toml --test codex_qualification --test ai_connection_repository --features runtime-fixture
npm run bindings:generate
git add src-tauri/src/agent_runtime/codex_app_server.rs src-tauri/src/agent_runtime/codex_actor.rs src-tauri/src/agent_runtime.rs src-tauri/src/ai/contract.rs src-tauri/src/ai/connections.rs src-tauri/src/application_settings.rs src-tauri/src/diagnostics.rs src-tauri/src/host.rs src-tauri/src/lib.rs src-tauri/tests/support/codex_host_fixture.rs src-tauri/tests/support/runtime_fixture.rs src-tauri/tests/ai_connection_repository.rs src-tauri/tests/codex_qualification.rs src-tauri/src/bin/export_bindings.rs src/bindings
git commit -m "feat: qualify selected Codex capability tuple"
```

### Task 10: Replace Settings UX and Regenerate the Managed-Login Bindings

**Files:**
- Modify: `src/ApplicationSettings.tsx`
- Modify: `src/ApplicationSettings.test.tsx`
- Modify: `src/quantixHost.ts`
- Modify: `src/quantixHost.test.ts`
- Modify: `src-tauri/src/bin/export_bindings.rs`
- Create by generation: `src/bindings/CodexLoginMethod.ts`
- Create by generation: `src/bindings/StartCodexAccountLoginCommand.ts`
- Create by generation: `src/bindings/StartCodexAccountLoginResult.ts`
- Create by generation: `src/bindings/ReplaceCodexAccountCommand.ts`
- Create by generation: `src/bindings/CancelCodexAccountLoginCommand.ts`
- Create by generation: `src/bindings/LogoutCodexAccountCommand.ts`
- Create by generation: `src/bindings/RecoverCodexAuthCommand.ts`
- Delete by generation: obsolete `ChatGpt*` login bindings recorded in `src/bindings/.quantix-generated.json`

**Interfaces:**
- Consumes: Task 8 account IPC, Task 9 `probe_codex_selection`, and `quantix://codex-account-state-changed`.
- Produces one simple Settings flow with explicit Browser login, Device Code login, model/reasoning draft, disclosed qualification, explicit activation of committed evidence, confirmed replacement, normal logout, and clearly separated recovery.

- [ ] **Step 1: Write renderer RED tests**

Assert browser/device are separate explicit actions; the renderer never receives a browser auth URL or any token; device verification URL/code exist only in the initiating device flow; async account-change events refresh the view; model/reasoning controls stay unselected until Engineer choice; Activate stays disabled before a successful disclosed probe; the probe command contains the exact connection revision/account/model/provider route/reasoning/catalogue identities; probe failure/race leaves no activation; activation submits the returned hashes unchanged; replacement requires the exact phrase; recovery is labelled destructive and separate from logout; device details clear on settle, cancel, navigation, and unmount; the test-only isolation warning is visible.

```typescript
it("clears device and qualification state when the account revision changes", async () => {
  const { unmount } = render(<ApplicationSettings />);
  await userEvent.click(screen.getByRole("button", { name: "Use device code" }));
  expect(await screen.findByText("ABCD-EFGH")).toBeVisible();
  await qualifyVisibleCodexTuple();
  expect(screen.getByRole("button", { name: "Activate" })).toBeEnabled();
  emitCodexAccountState(connectedSettings({ account_revision: 2 }));
  await waitFor(() => {
    expect(screen.queryByText("ABCD-EFGH")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Activate" })).toBeDisabled();
  });
  unmount();
  expect(unlistenCodexAccountState).toHaveBeenCalledTimes(1);
});

it("never receives the browser authorization URL", async () => {
  host.startCodexAccountLogin.mockResolvedValue({
    login_id: "login-1",
    verification_url: null,
    user_code: null,
  });
  render(<ApplicationSettings />);
  await userEvent.click(screen.getByRole("button", { name: "Continue in browser" }));
  expect(JSON.stringify(host.startCodexAccountLogin.mock.results)).not.toContain("auth.example");
});
```

Define `emitCodexAccountState`, `connectedSettings`, and `qualifyVisibleCodexTuple` in the renderer test file using the existing mocked Host/event layer. Expected RED: device code remains visible, Activate remains enabled for a stale revision, event unsubscribe is absent, or an auth URL appears in the public result.

- [ ] **Step 2: Run RED**

```powershell
npx vitest run src/ApplicationSettings.test.tsx src/quantixHost.test.ts
```

Expected RED: nonzero Vitest exit for missing managed wrappers/event listener and stale draft/qualification state.

- [ ] **Step 3: Implement the renderer and Host wrappers**

Subscribe once to `quantix://codex-account-state-changed`, discard stale revisions, and unsubscribe on unmount. Keep browser login's pending state keyed only by `login_id`. Device URL/code remain component-local and are never persisted. Show upstream `isDefault` only as informational text; never use it to select. Treat the chosen tuple as a draft until `probeCodexSelection` returns committed qualification. Any account/catalogue/draft change clears that qualification. Activate only by passing the exact returned capability/catalogue identities to the canonical activation command.

The component state transition must be equivalent to:

```typescript
useEffect(() => {
  let disposed = false;
  let unlisten: (() => void) | undefined;
  void listen<ApplicationSettingsView>(CODEX_ACCOUNT_EVENT, ({ payload }) => {
    if (disposed || payload.ai.account_revision <= settings.ai.account_revision) return;
    setDeviceChallenge(null);
    setDraft(null);
    setQualification(null);
    setSettings(payload);
  }).then((dispose) => {
    if (disposed) dispose();
    else unlisten = dispose;
  });
  return () => {
    disposed = true;
    unlisten?.();
    setDeviceChallenge(null);
  };
}, [settings.ai.account_revision]);

const testSelection = async () => {
  if (!draft) return;
  setQualification(null);
  setQualification(await probeCodexSelection(toProbeCommand(settings, draft)));
};
```

`toProbeCommand` copies exact IDs/revision/hash from current canonical settings; the Activate handler copies exact hashes from `qualification` and refuses when the current draft no longer matches it.

- [ ] **Step 4: Register exports, regenerate, and prove stale bindings are removed by ownership**

Remove obsolete ChatGPT exports and add the seven managed-Codex DTOs to `export_bindings.rs`, then run:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin export_bindings --features runtime-fixture
git status --short src/bindings
npx vitest run src/ApplicationSettings.test.tsx src/quantixHost.test.ts
```

Expected: obsolete manifest-owned bindings are deleted automatically, new bindings are present, and no binding was hand-edited.

- [ ] **Step 5: Commit Task 10**

```powershell
git add src/ApplicationSettings.tsx src/ApplicationSettings.test.tsx src/quantixHost.ts src/quantixHost.test.ts src-tauri/src/bin/export_bindings.rs src/bindings
git commit -m "feat: connect Settings to managed Codex qualification"
```

### Task 11: Update Diagnostics, Backups, Acceptance Evidence, and Release Blockers

**Files:**
- Modify: `src-tauri/src/acceptance.rs`
- Modify: `src-tauri/src/release_gate.rs`
- Modify: `src-tauri/src/diagnostics.rs`
- Modify: `src-tauri/src/doctor.rs`
- Modify: `src-tauri/src/tender_store/backups.rs`
- Modify: `src-tauri/src/bin/export_bindings.rs`
- Modify: `src-tauri/tests/release_configuration.rs`
- Modify: `src-tauri/tests/manager_workspace.rs`
- Modify: `scripts/setup-windows-private-release.sh`
- Modify: `README.md`
- Modify: `CONTEXT.md`
- Create by generation: `src/bindings/AiRuntimeEvidence.ts`
- Create by generation: `src/bindings/CodexManagedRuntimeEvidence.ts`
- Delete by generation: `src/bindings/ChatGptProductionAssuranceEvidence.ts`

**Interfaces:**
- Consumes: managed Codex route, exact provenance, and existing product-acceptance CLI.
- Produces deterministic/live records containing runtime package version/hash, schema hash, selected account fingerprint, selected model/reasoning identity, ephemeral-thread evidence, built-in quarantine evidence, and test-only isolation limitation; records contain no URLs, codes, tokens, raw prompts, or Tender content.
- Replaces `ChatGptProductionAssuranceEvidence` with `CodexManagedRuntimeEvidence`; no obsolete type alias remains.
- Produces explicit release blocker codes `codex_dynamic_tool_bridge_unqualified`, `codex_builtin_preexecution_control_unproven`, `codex_distribution_terms_unapproved`, and `codex_os_isolation_unproven`.

```rust
pub enum AiRuntimeEvidence {
    CodexManaged(CodexManagedRuntimeEvidence),
}

pub struct CodexManagedRuntimeEvidence {
    pub package_version: String,
    pub executable_sha256: String,
    pub schema_sha256: String,
    pub account_fingerprint: String,
    pub account_revision: u64,
    pub selection: ActiveAiConfiguration,
    pub reported_model_id: Option<String>,
    pub reported_provider_route_id: Option<String>,
    pub dynamic_tool_bridge_qualified: bool,
    pub provider_request_id: Option<String>,
    pub request_count: Option<u32>,
    pub quantix_retry_count: u32,
    pub isolation_limitation: String,
}
```

Slice 2 extends `AiRuntimeEvidence` with its general-provider variant; it reuses `ActiveAiConfiguration` unchanged.

- [ ] **Step 1: Add RED acceptance and security tests**

Cover Codex-home exclusion from backup/archive/export/diagnostics; ephemeral-thread evidence; managed-auth restart; account replacement; protocol/config drift; built-in action quarantine; complete immutable selection/capability hashes; Task 9 dynamic-tool echo evidence; five-run sequence integrity; and each exact public-release blocker. A qualified bridge clears only `codex_dynamic_tool_bridge_unqualified`; built-in suppression, OS isolation, and distribution/managed-subscription terms remain independent blockers.

```rust
#[test]
fn qualified_dynamic_bridge_does_not_clear_distribution_or_isolation_blockers() {
    let record = evaluate_codex_release_fixture(CodexReleaseFixture {
        dynamic_tool_bridge_qualified: true,
        built_in_preexecution_control_proven: false,
        os_isolation_proven: false,
        distribution_terms_approved: false,
    });
    assert!(!record.blockers.contains(&"codex_dynamic_tool_bridge_unqualified".into()));
    assert!(record.blockers.contains(&"codex_builtin_preexecution_control_unproven".into()));
    assert!(record.blockers.contains(&"codex_os_isolation_unproven".into()));
    assert!(record.blockers.contains(&"codex_distribution_terms_unapproved".into()));
    assert!(!record.public_production_ready);
}

#[test]
fn codex_evidence_and_exports_exclude_auth_and_raw_content() {
    let harness = AcceptanceHarness::qualified_codex();
    let evidence = harness.record_codex_runtime_evidence();
    let json = serde_json::to_string(&evidence).unwrap().to_ascii_lowercase();
    for forbidden in [
        "access_token",
        "refresh_token",
        "auth_url",
        "user_code",
        "authorization",
        "raw_prompt",
        "tender content sentinel",
    ] {
        assert!(!json.contains(forbidden), "leaked {forbidden}");
    }
    for inventory in [
        harness.backup_inventory(),
        harness.archive_inventory(),
        harness.diagnostic_inventory(),
    ] {
        assert!(!inventory.iter().any(|path| path.starts_with("codex/")));
    }
}
```

Define `evaluate_codex_release_fixture` inside `release_configuration.rs`; it invokes the public release-evaluation Host command with fixture evidence rather than importing private release-gate helpers.

Expected RED: obsolete evidence fails to compile, Codex Home appears in an export, a secret/raw sentinel leaks, or bridge qualification incorrectly clears terms/isolation blockers.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test release_configuration --test manager_workspace --features runtime-fixture -- --nocapture
```

Expected RED: obsolete `ChatGptProductionAssuranceEvidence` fields fail the new type checks, at least one export contains `codex/`, or release evaluation incorrectly clears terms/isolation blockers when only the bridge is qualified.

- [ ] **Step 3: Implement redacted evidence and truthful release blockers**

Replace direct-provider evidence fields and type names with `CodexManagedRuntimeEvidence` containing package/executable/schema identities, account fingerprint/revision, complete immutable selection, observed reported model/provider route, capability hash, bridge qualification, request/usage facts when known, zero Quantix retries, quarantine/loop/budget outcome, and the test-only isolation statement. Populate it only from committed probe, Agent Run, and provider-event records. Update exporter registration so the owned binding manifest deletes the obsolete ChatGPT DTO. Remove durable thread archive/delete cleanup because only ephemeral threads are permitted. Ensure no document claims read-only sandboxing prevents Codex core reads.

Update `scripts/setup-windows-private-release.sh` only to remove obsolete private-direct-provider wording and to display the new blocker codes. Task 14 creates non-secret live command JSON with Windows-native PowerShell; this task must not add or invoke a Bash-only command-generation mode.

Construct evidence only from committed records:

```rust
fn codex_runtime_evidence(
    provenance: &VerifiedCodexProvenance,
    account: &StoredCodexAccountProjection,
    run: &AgentRunInspection,
    qualification: &StoredCodexQualification,
    observations: &CommittedCodexObservations,
) -> Result<CodexManagedRuntimeEvidence, TenderCommandError> {
    let selection = run.provider_selection.clone();
    if selection.capability_sha256 != qualification.capability_sha256
        || selection.catalogue_sha256 != qualification.catalogue_sha256
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(CodexManagedRuntimeEvidence {
        package_version: provenance.package_version.clone(),
        executable_sha256: provenance.executable_sha256.clone(),
        schema_sha256: provenance.schema_sha256.clone(),
        account_fingerprint: account.fingerprint.clone(),
        account_revision: account.revision,
        selection,
        reported_model_id: observations.reported_model_id.clone(),
        reported_provider_route_id: qualification.reported_provider_route_id.clone(),
        dynamic_tool_bridge_qualified: qualification.dynamic_tool_bridge_qualified,
        provider_request_id: observations.provider_request_id.clone(),
        request_count: observations.request_count,
        quantix_retry_count: 0,
        isolation_limitation: CODEX_TEST_ONLY_ISOLATION_STATEMENT.into(),
    })
}

fn codex_release_blockers(assurance: &CodexReleaseAssurance) -> BTreeSet<&'static str> {
    let mut blockers = BTreeSet::new();
    if !assurance.dynamic_tool_bridge_qualified {
        blockers.insert("codex_dynamic_tool_bridge_unqualified");
    }
    if !assurance.built_in_preexecution_control_proven {
        blockers.insert("codex_builtin_preexecution_control_unproven");
    }
    if !assurance.os_isolation_proven {
        blockers.insert("codex_os_isolation_unproven");
    }
    if !assurance.distribution_terms_approved {
        blockers.insert("codex_distribution_terms_unapproved");
    }
    blockers
}
```

Do not accept evidence constructors that receive raw provider responses, auth state, URLs, codes, or Tender payloads.

- [ ] **Step 4: Run GREEN, regenerate bindings, and run repository checks**

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin export_bindings --features runtime-fixture
npm run format:check
npm run check
npm test
```

- [ ] **Step 5: Commit Task 11**

```powershell
git add src-tauri/src/acceptance.rs src-tauri/src/release_gate.rs src-tauri/src/diagnostics.rs src-tauri/src/doctor.rs src-tauri/src/tender_store/backups.rs src-tauri/src/bin/export_bindings.rs src-tauri/tests/release_configuration.rs src-tauri/tests/manager_workspace.rs scripts/setup-windows-private-release.sh README.md CONTEXT.md src/bindings
git commit -m "test: qualify managed Codex runtime"
```

### Task 12: Delete the Private Connector and Custom OAuth, Then Run the Static Scan

**Files:**
- Delete: `src-tauri/src/agent_backend/`
- Delete: `src-tauri/src/chatgpt_login.rs`
- Delete: `src-tauri/src/chatgpt_oauth/`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/agent_runtime.rs`
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/tests/codex_cutover_static.rs`
- Delete by generation: any remaining obsolete bindings under `src/bindings/`

**Interfaces:**
- Consumes: Tasks 7-11, including acceptance/diagnostic cleanup.
- Preserves the new `agent_runtime/codex_actor.rs`; it is the sole actor/Host app-server boundary.
- Produces exactly one production `AiProviderKind::Codex` route through `CodexRuntimeHandle` and the official app-server.

- [ ] **Step 1: Add the static RED scan after acceptance cleanup**

Create `src-tauri/tests/codex_cutover_static.rs` with a bounded `WalkDir` helper over production `.rs`, `.ts`, and `.tsx` files. Skip test files. Assert the three legacy paths do not exist and scan for construction/import identifiers, not denylist text:

```rust
use std::{
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

fn production_sources() -> Vec<PathBuf> {
    let root = repository_root();
    [root.join("src-tauri/src"), root.join("src")]
        .into_iter()
        .flat_map(|directory| WalkDir::new(directory).into_iter().filter_map(Result::ok))
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| {
            matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("rs" | "ts" | "tsx")
            ) && !path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.ends_with(".test.ts") || name.ends_with(".test.tsx"))
        })
        .collect()
}

fn assert_zero_matches(patterns: &[&str]) {
    let mut matches = Vec::new();
    for path in production_sources() {
        let text = fs::read_to_string(&path).expect("read production source");
        for pattern in patterns {
            if text.contains(pattern) {
                matches.push(format!("{}: {pattern}", path.display()));
            }
        }
    }
    assert!(matches.is_empty(), "obsolete paths remain:\n{}", matches.join("\n"));
}

#[test]
fn production_tree_contains_no_private_chatgpt_or_custom_oauth_path() {
    assert!(!repository_root().join("src-tauri/src/agent_backend").exists());
    assert!(!repository_root().join("src-tauri/src/chatgpt_login.rs").exists());
    assert!(!repository_root().join("src-tauri/src/chatgpt_oauth").exists());
    assert_zero_matches(&[
        "CHATGPT_DIRECT_ADAPTER_VERSION",
        "CHATGPT_DIRECT_CATALOGUE_VERSION",
        "backend-api/codex/responses",
        "ProviderReasoningSelection",
        "ProviderDefault",
        "mod chatgpt_oauth",
        "use crate::chatgpt_oauth",
        "ChatGptProductionAssuranceEvidence",
    ]);
}
```

Fail on unreadable source. Do not scan for `chatgptAuthTokens`, because Task 5 must retain that text in its explicit protocol denylist and test it as rejected input.

Expected RED: at least one legacy path exists or the scan reports one of the obsolete construction/import identifiers.

- [ ] **Step 2: Run RED and record exact surviving production references**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test codex_cutover_static --features runtime-fixture -- --nocapture
```

- [ ] **Step 3: Delete obsolete code, commands, dependencies, and bindings**

Use `apply_patch` deletions. Preserve historical docs but do not preserve compatibility parsers, root `auth.json`, direct-provider catalogue IDs, token vault variants, legacy Tauri commands, or fixture-only direct actors. Retain the new app-server actor, managed-account route, qualification path, and Host callbacks.

- [ ] **Step 4: Regenerate and run the now-green scan plus focused regressions**

```powershell
npm run bindings:generate
cargo test --manifest-path src-tauri/Cargo.toml --test codex_cutover_static --test agent_runtime --test managed_codex_account --test codex_qualification --features runtime-fixture
npx vitest run src/ApplicationSettings.test.tsx src/quantixHost.test.ts src/ManagerWorkspace.test.tsx
```

- [ ] **Step 5: Commit Task 12**

```powershell
git add -A src-tauri/src/agent_backend src-tauri/src/chatgpt_login.rs src-tauri/src/chatgpt_oauth src-tauri/src/agent_runtime src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tests/codex_cutover_static.rs src/bindings
git commit -m "refactor: remove private ChatGPT provider"
```

### Task 13: Pass the Deterministic Pre-Integration Gate

**Files:**
- Create: `src-tauri/src/schema_cutover_gate.rs`
- Modify: `src-tauri/build.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/tests/schema_cutover_gate.rs`
- Create: `scripts/run-schema-cutover.ps1`

**Interfaces:**
- Consumes: the complete worktree branch from Tasks 1-12.
- Produces the permanent pre-Host startup interlock used by all three parent-suite schema cutovers plus reviewable deterministic evidence. It performs no provider login, package build, or live acceptance.

```rust
pub(crate) const SCHEMA_CUTOVER_MARKER_SCHEMA: u32 = 1;
pub(crate) const COMPILED_SOURCE_SHA: &str = env!("QUANTIX_COMPILED_SOURCE_SHA");
pub(crate) const COMPILED_SOURCE_DIRTY_VALUE: &str = env!("QUANTIX_COMPILED_SOURCE_DIRTY");

pub(crate) fn compiled_source_is_dirty() -> bool {
    matches!(COMPILED_SOURCE_DIRTY_VALUE, "1")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum SchemaCutoverStatus { Armed, Released, Completed }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SchemaCutoverMarker {
    pub schema_version: u32,
    pub status: SchemaCutoverStatus,
    pub slice_number: u8,
    pub application_home: PathBuf,
    pub old_source_sha: String,
    pub new_source_sha: String,
    pub nonce_sha256: String,
    pub expected_installation_schema: i64,
    pub expected_vault_schema: u32,
    pub expected_tender_schema: i64,
    pub acknowledgement_path: PathBuf,
    pub completion_receipt_path: PathBuf,
    pub archive_path: PathBuf,
    pub archive_inventory_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SchemaCutoverAcknowledgement {
    pub schema_version: u32,
    pub nonce_sha256: String,
    pub compiled_source_sha: String,
    pub compiled_source_dirty: bool,
    pub slice_number: u8,
    pub expected_installation_schema: i64,
    pub expected_vault_schema: u32,
    pub expected_tender_schema: i64,
    pub process_id: u32,
}

pub(crate) struct SchemaCutoverPermit {
    marker_path: PathBuf,
    nonce_sha256: String,
    mutex: NamedCutoverMutexGuard,
}

pub(crate) fn wait_before_quantix_host(
    default_application_home: &Path,
) -> Result<Option<SchemaCutoverPermit>, SchemaCutoverError>;

pub(crate) fn complete_after_setup(
    permit: SchemaCutoverPermit,
    observed_installation_schema: i64,
    observed_vault_schema: u32,
    observed_tender_schema: i64,
) -> Result<(), SchemaCutoverError>;
```

`build.rs` preserves all existing Tauri/resource behavior and embeds one 40-character Git SHA plus a dirty flag. It resolves/emits `cargo:rerun-if-changed` for the worktree Git `HEAD`, current symbolic branch ref, common `packed-refs`, and every tracked file returned by `git ls-files`; later slice commits therefore invalidate cached build-script output without restarting the dev PTY. It takes an explicit `QUANTIX_BUILD_SOURCE_SHA` only when that value equals `git rev-parse HEAD`. Ordinary dirty development builds are allowed but embed `QUANTIX_COMPILED_SOURCE_DIRTY=1`; when `active-cutover.json` is armed, `build.rs` fails the build unless status is clean. `run()` calls `wait_before_quantix_host` before constructing the Tauri builder, resolving Application Home, or creating any `QuantixHost`/store/runtime object. With no active marker it returns immediately. With `Completed`, it validates nonce/schema/inventory/receipt destination under the named mutex, atomically finishes the interrupted marker-to-receipt move, and returns the no-marker path. With `Armed`, it validates exact non-reparse marker/home/archive/ack/receipt paths and expected schema constants, requires `compiled_source_is_dirty() == false` and constant-time equality between `COMPILED_SOURCE_SHA` and `marker.new_source_sha`, acquires the mutex, atomically writes `SchemaCutoverAcknowledgement` using the compiled SHA/dirty flag (never an echoed marker SHA), and polls only the marker file. It never stats/opens Application Home. It proceeds only after the same marker atomically changes to `Released` with a valid archive inventory hash and the old home is absent; timeout or malformed state exits without Host creation. `complete_after_setup` verifies fresh schemas, atomically changes the active marker to `Completed`, then renames it to the exact immutable completion-receipt path before releasing the mutex. General and MCP slices reuse this code and add tests for their later expected constants; no slice adds a second gate.

`scripts/run-schema-cutover.ps1` has two explicit parameter sets. `-Phase Arm` validates slice/revisions/schema versions/paths, creates a 32-byte CSPRNG nonce, assigns a unique immutable completion-receipt path, and atomically writes the marker before integration; it permits prior receipts but refuses an existing active marker. `-Phase Release` waits for the matching compiled-source acknowledgement, validates/removes only the three approved credential targets, moves the remaining home, writes a canonical sorted SHA-256 inventory beside the archive, rejects credential path/sentinel matches, atomically releases the marker, and waits for the completion receipt plus absence of the active marker. It uses native PowerShell end-to-end, literal paths, no globs, and exits nonzero without removing the armed marker on any failure.

- [ ] **Step 1: Add the RED atomic-interlock tests**

Use a test-owned marker root, Git worktree, Cargo target, and named-mutex namespace. Test no-marker immediate start; armed marker acknowledgement before any home observation; a simulated watcher relaunch racing the archive cannot pass the mutex; released marker with old home present fails; wrong/stale compiled source SHA, dirty checkout, nonce, path, schema, reparse, or inventory fails; a new commit/ref without restarting the parent build process changes the embedded SHA; valid release permits setup; completion requires exact observed schemas and moves the active marker to the receipt. Simulate a crash precisely after the `Completed` write and prove next startup finalizes the receipt then starts normally. Re-arm after a Slice 1 receipt with a Slice 2 marker and prove only the new active marker participates. The integration fixture records every attempted home access and asserts zero before release.

```rust
#[test]
fn armed_cutover_acknowledges_before_home_access_and_waits_for_release() {
    let harness = CutoverGateHarness::armed();
    let default_home = harness.application_home.clone();
    let waiter = std::thread::spawn(move || wait_before_quantix_host(&default_home));
    harness.wait_for_acknowledgement();
    assert_eq!(harness.home_access_attempts(), 0);
    harness.publish_valid_release();
    let permit = waiter.join().unwrap().unwrap().expect("cutover permit");
    assert_eq!(harness.home_access_attempts(), 1);
    harness.create_fresh_home_with_expected_schemas();
    complete_after_setup(permit, 25, 1, 46).unwrap();
    assert_eq!(harness.marker_status(), SchemaCutoverStatus::Completed);
}

#[test]
fn malformed_or_racing_cutovers_fail_closed() {
    for fault in [
        CutoverFault::WrongNonce,
        CutoverFault::WrongCompiledSource,
        CutoverFault::DirtyCompiledSource,
        CutoverFault::WrongSchema,
        CutoverFault::ReparseMarker,
        CutoverFault::WrongInventory,
        CutoverFault::OldHomeStillPresent,
        CutoverFault::SecondWatcher,
    ] {
        let harness = CutoverGateHarness::released_with(fault);
        assert!(wait_before_quantix_host(&harness.application_home).is_err(), "fault={fault:?}");
        assert_eq!(harness.host_construction_count(), 0);
    }
}

#[test]
fn completed_active_marker_is_finalized_after_crash_and_can_rearm() {
    let harness = CutoverGateHarness::crashed_after_completed_write();
    assert!(wait_before_quantix_host(&harness.application_home).unwrap().is_none());
    assert!(!harness.active_marker_exists());
    assert!(harness.completion_receipt_exists());
    harness.arm_next_slice(2, 26, 1, 46);
    assert!(harness.wait_for_next_slice_acknowledgement().is_ok());
}

pub(crate) fn complete_after_setup(
    permit: SchemaCutoverPermit,
    observed_installation_schema: i64,
    observed_vault_schema: u32,
    observed_tender_schema: i64,
) -> Result<(), SchemaCutoverError> {
    let mut marker = read_same_marker(&permit.marker_path, &permit.nonce_sha256)?;
    require_exact_observed_schemas(
        &marker,
        observed_installation_schema,
        observed_vault_schema,
        observed_tender_schema,
    )?;
    marker.status = SchemaCutoverStatus::Completed;
    write_marker_atomic(&permit.marker_path, &marker)?;
    rename_active_marker_to_receipt(&permit.marker_path, &marker.completion_receipt_path)?;
    drop(permit.mutex);
    Ok(())
}
```

Define `CutoverGateHarness`/`CutoverFault` in the source test module. The integration fixture repeats the armed/released case through the public startup entry and records home-open attempts. The expected schema values in this Slice 1 test are exact constants, not configurable defaults.

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib schema_cutover_gate::tests --features runtime-fixture -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test schema_cutover_gate --features runtime-fixture -- --nocapture
powershell -NoProfile -File scripts/run-schema-cutover.ps1 -? *> $null
```

Expected RED: compile/file-not-found failure because the startup interlock, fixture, and PowerShell marker contract do not exist.

- [ ] **Step 2: Implement the pre-Host gate and run GREEN**

Use atomic same-directory temporary-file replacement for marker/ack writes, bounded 250 ms polling with a ten-minute hard timeout, the Windows named mutex, handle-based reparse checks, and constant-time nonce-hash comparison. Do not read credentials or archive contents; validate only the inventory file/hash named by the released marker.

Preserve the existing `build.rs` body and prepend this exact revision publication:

```rust
fn git_output(arguments: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(arguments)
        .output()
        .expect("run git for Quantix build identity");
    assert!(output.status.success(), "git rev-parse HEAD failed");
    String::from_utf8(output.stdout).expect("UTF-8 git output")
}

fn register_git_inputs() {
    for tracked in git_output(&["ls-files"]).lines() {
        println!("cargo:rerun-if-changed=../{tracked}");
    }
    for git_path in ["HEAD", "packed-refs"] {
        let resolved = git_output(&["rev-parse", "--git-path", git_path]);
        println!("cargo:rerun-if-changed={}", resolved.trim());
    }
    if let Ok(output) = std::process::Command::new("git")
        .args(["symbolic-ref", "-q", "HEAD"])
        .output()
        && output.status.success()
    {
        let reference = String::from_utf8(output.stdout).expect("UTF-8 symbolic ref");
        let resolved = git_output(&["rev-parse", "--git-path", reference.trim()]);
        println!("cargo:rerun-if-changed={}", resolved.trim());
    }
}

fn quantix_source_state() -> (String, bool) {
    let checkout = git_output(&["rev-parse", "HEAD"]).trim().to_ascii_lowercase();
    assert!(checkout.len() == 40 && checkout.bytes().all(|b| b.is_ascii_hexdigit()));
    if let Ok(explicit) = std::env::var("QUANTIX_BUILD_SOURCE_SHA") {
        assert_eq!(explicit.to_ascii_lowercase(), checkout, "explicit source SHA mismatch");
    }
    let dirty = !git_output(&["status", "--porcelain=v1", "--untracked-files=all"])
        .trim()
        .is_empty();
    (checkout, dirty)
}

fn main() {
    register_git_inputs();
    println!("cargo:rerun-if-env-changed=QUANTIX_BUILD_SOURCE_SHA");
    let (source_sha, dirty) = quantix_source_state();
    if std::path::Path::new(r"C:\QuantixAcceptance\sdk-first-schema-cutovers\active-cutover.json").is_file() {
        assert!(!dirty, "an armed schema cutover requires a clean reviewed checkout");
    }
    println!("cargo:rustc-env=QUANTIX_COMPILED_SOURCE_SHA={source_sha}");
    println!("cargo:rustc-env=QUANTIX_COMPILED_SOURCE_DIRTY={}", if dirty { "1" } else { "0" });
    println!("cargo:rerun-if-env-changed=QUANTIX_UPDATE_ENDPOINT");
    println!("cargo:rerun-if-env-changed=QUANTIX_UPDATE_PUBLIC_KEY");
    println!("cargo:rerun-if-env-changed=TAURI_SIGNING_PRIVATE_KEY");
    println!("cargo:rerun-if-env-changed=TAURI_SIGNING_PRIVATE_KEY_PASSWORD");
    println!("cargo:rerun-if-changed=icons/icon.ico");
    tauri_build::build();
    embed_resource::compile_for_tests("windows-test-manifest.rc", embed_resource::NONE)
        .manifest_required()
        .expect("compile the Windows test manifest");
}
```

The startup loop must have this ordering:

```rust
pub(crate) fn wait_before_quantix_host(
    default_application_home: &Path,
) -> Result<Option<SchemaCutoverPermit>, SchemaCutoverError> {
    let marker_path = marker_path_for(default_application_home)?;
    let Some(mut marker) = read_marker_if_present(&marker_path)? else {
        return Ok(None);
    };
    validate_marker_paths_and_schemas(&marker, default_application_home)?;
    constant_time_require_equal(COMPILED_SOURCE_SHA.as_bytes(), marker.new_source_sha.as_bytes())?;
    if compiled_source_is_dirty() {
        return Err(SchemaCutoverError::DirtyCompiledSource);
    }
    let mutex = acquire_cutover_mutex(&marker, Duration::from_secs(600))?;
    if marker.status == SchemaCutoverStatus::Completed {
        validate_completed_marker(&marker)?;
        rename_active_marker_to_receipt(&marker_path, &marker.completion_receipt_path)?;
        drop(mutex);
        return Ok(None);
    }
    if marker.status == SchemaCutoverStatus::Released {
        validate_released_inventory(&marker)?;
        require_old_home_absent(default_application_home)?;
        return Ok(Some(SchemaCutoverPermit {
            marker_path,
            nonce_sha256: marker.nonce_sha256,
            mutex,
        }));
    }
    validate_armed_marker(&marker)?;
    write_acknowledgement_atomic(&SchemaCutoverAcknowledgement {
        schema_version: SCHEMA_CUTOVER_MARKER_SCHEMA,
        nonce_sha256: marker.nonce_sha256.clone(),
        compiled_source_sha: COMPILED_SOURCE_SHA.to_owned(),
        compiled_source_dirty: compiled_source_is_dirty(),
        slice_number: marker.slice_number,
        expected_installation_schema: marker.expected_installation_schema,
        expected_vault_schema: marker.expected_vault_schema,
        expected_tender_schema: marker.expected_tender_schema,
        process_id: std::process::id(),
    }, &marker.acknowledgement_path)?;
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        if Instant::now() >= deadline {
            return Err(SchemaCutoverError::TimedOut);
        }
        marker = read_same_marker(&marker_path, &marker.nonce_sha256)?;
        if marker.status == SchemaCutoverStatus::Released {
            validate_released_inventory(&marker)?;
            require_old_home_absent(default_application_home)?;
            return Ok(Some(SchemaCutoverPermit {
                marker_path,
                nonce_sha256: marker.nonce_sha256,
                mutex,
            }));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}
```

Before `Released`, no helper called above may stat/open `default_application_home`; only marker, acknowledgement, inventory, and mutex paths are touched. `run-schema-cutover.ps1` starts with `[CmdletBinding(DefaultParameterSetName='Arm')]` and two mandatory parameter sets `Arm`/`Release`; both route all writes through one same-directory `Write-AtomicUtf8` helper using `System.IO.File` and literal paths.

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib schema_cutover_gate::tests --features runtime-fixture
cargo test --manifest-path src-tauri/Cargo.toml --test schema_cutover_gate --features runtime-fixture
powershell -NoProfile -File scripts/run-schema-cutover.ps1 -? *> $null
git add src-tauri/build.rs src-tauri/src/schema_cutover_gate.rs src-tauri/src/lib.rs src-tauri/tests/schema_cutover_gate.rs scripts/run-schema-cutover.ps1
git commit -m "feat: interlock schema cutovers before Host startup"
```

- [ ] **Step 3: Prove generated/runtime evidence is stable**

```powershell
npm run bindings:generate
git diff --exit-code -- src/bindings
npm run prepare:runtime
if ($LASTEXITCODE -ne 0) { throw 'runtime preparation or tracked schema comparison failed' }
git diff --exit-code -- src-tauri/runtime/protocol src-tauri/runtime/runtime-provenance.json
```

- [ ] **Step 4: Run the deterministic repository gate**

```powershell
npm run format:check
npm run check
npm test
npm run verify
```

Expected: every command exits zero, the static cutover test passes, no production build runs, and the ordinary/main dev server remains running.

- [ ] **Step 5: Obtain independent spec/code review before integration**

Review the complete diff against the design and all Task 1-12 acceptance bullets. Any correction starts with a deterministic RED test and repeats Step 2. When review is clean, use `superpowers:finishing-a-development-branch` to perform the user-approved local integration. Do not begin Task 14 before the reviewed commit is on main.

### Task 14: Run Post-Integration Interactive and Live/Private Acceptance

**Files:**
- Modify only if the live run exposes a defect first reproduced by a deterministic RED test.
- Store private evidence in the sanitized Windows account's real Application Home; never commit it.

**Interfaces:**
- Consumes: reviewed Slice 1 integrated on main, `C:\Users\kareem\Desktop\Test Project`, the still-running main dev app, and one already-built unchanged release-stage candidate.
- Produces one post-integration interactive Agent Run, five clean candidate-installation live records, and one private aggregation for the same binary.

- [ ] **Step 1: Keep the main dev server alive and perform the post-integration smoke run**

Confirm the already-running main dev app has rebuilt the integrated revision. Do not stop or restart it unless it exited independently. In that dev app, explicitly login, qualify, and activate one discovered Codex tuple, import `C:\Users\kareem\Desktop\Test Project`, and complete one governed Agent Run. Confirm ephemeral thread, persisted approvals before tool continuation, one execution per idempotency key, complete immutable selection evidence, zero Quantix retry, loop/budget outcome, and no built-in action publication.

- [ ] **Step 2: Configure the formal candidate through the exact acceptance Application Home**

Use a separate sanitized Windows account/VM whose profile contains no unrelated files. Set:

```powershell
$acceptanceRoot = 'C:\QuantixAcceptance\codex-sdk-cutover'
$acceptanceHome = Join-Path $env:USERPROFILE '.quantix'
$candidateRoot = Join-Path $acceptanceRoot 'candidate'
$candidateExecutable = Join-Path $candidateRoot 'quantix.exe'
$candidateResources = Join-Path $candidateRoot 'resources'
$candidateUninstaller = Join-Path $candidateRoot 'uninstall.exe'
$recordHashPath = Join-Path $acceptanceRoot 'deterministic-record-sha256.txt'
```

Install and launch the candidate normally in that sanitized account. Production Quantix uses `$env:USERPROFILE\.quantix`, so its managed Codex auth, canonical settings/probe/selection, and `installation.sqlite` now live in exactly `$acceptanceHome`. In candidate Settings, perform managed login, wait for account completion, explicitly choose the discovered tuple, accept/run the disclosed probe, activate its committed evidence, import the small Test Project copy, and complete one Agent Run. Close the candidate before CLI acceptance. Do not set `QUANTIX_APPLICATION_HOME`, copy auth files, or use credentials from the ordinary dev profile.

- [ ] **Step 3: Record the formal candidate's deterministic record in that same Application Home**

This release-stage record is separate from Task 13's pre-integration repository gate, but live acceptance requires it in the same `installation.sqlite`. Run:

```powershell
$acceptanceRoot = 'C:\QuantixAcceptance\codex-sdk-cutover'
$acceptanceHome = Join-Path $env:USERPROFILE '.quantix'
$candidateRoot = Join-Path $acceptanceRoot 'candidate'
$candidateExecutable = Join-Path $candidateRoot 'quantix.exe'
$candidateResources = Join-Path $candidateRoot 'resources'
$recordHashPath = Join-Path $acceptanceRoot 'deterministic-record-sha256.txt'
$sourceRevision = (& git rev-parse HEAD).Trim()
$deterministicCommandPath = Join-Path $acceptanceRoot 'codex-deterministic.json'
$deterministicCommand = [ordered]@{
    source_revision = $sourceRevision
    application_artifact_path = $candidateExecutable
    application_resource_directory_path = $candidateResources
    dependency_lock_path = (Resolve-Path -LiteralPath 'src-tauri\Cargo.lock').Path
}
[IO.Directory]::CreateDirectory($acceptanceRoot) | Out-Null
[IO.File]::WriteAllText($deterministicCommandPath, ($deterministicCommand | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
& npm run acceptance:deterministic -- $acceptanceHome $deterministicCommandPath
if ($LASTEXITCODE -ne 0) { throw 'formal deterministic acceptance failed' }
$aggregateOutput = & npm run acceptance:aggregate -- $acceptanceHome $sourceRevision 2>&1
$aggregateOutput | ForEach-Object { Write-Host $_ }
if ($LASTEXITCODE -ne 0) { throw 'formal deterministic aggregation failed' }
$aggregateJson = $aggregateOutput | Where-Object { $_ -match '^\{' } | Select-Object -Last 1 | ConvertFrom-Json
$recordHash = [string]$aggregateJson.manifest_sha256
if ($recordHash -notmatch '^[0-9a-f]{64}$') { throw 'invalid deterministic record hash' }
[IO.File]::WriteAllText($recordHashPath, $recordHash, [Text.UTF8Encoding]::new($false))
```

- [ ] **Step 4: Preserve the exact Application Home across five clean candidate installations**

Each `acceptance:live` run uninstalls the candidate. Reinstall the unchanged candidate before the next run, but never delete, restore, snapshot-revert, or replace `$acceptanceHome`: `codex\auth.json`, canonical settings/probe/selection, Tender records, and the acceptance rows in `installation.sqlite` must remain together. This persistence is intentional and required for managed login and sequence aggregation; “clean” refers to the candidate installation, not a new credential/home.

- [ ] **Step 5: Create each non-secret command and record five runs with Windows-native PowerShell**

Run this in one PowerShell session from the integrated repository. It uses no Bash and writes no credential:

```powershell
$acceptanceRoot = 'C:\QuantixAcceptance\codex-sdk-cutover'
$acceptanceHome = Join-Path $env:USERPROFILE '.quantix'
$candidateRoot = Join-Path $acceptanceRoot 'candidate'
$candidateExecutable = Join-Path $candidateRoot 'quantix.exe'
$candidateResources = Join-Path $candidateRoot 'resources'
$candidateUninstaller = Join-Path $candidateRoot 'uninstall.exe'
$recordHashPath = Join-Path $acceptanceRoot 'deterministic-record-sha256.txt'
$recordHash = (Get-Content -LiteralPath $recordHashPath -Raw).Trim().ToLowerInvariant()
if ($recordHash -notmatch '^[0-9a-f]{64}$') { throw 'invalid deterministic record hash' }
$releaseCandidateSha256 = $null
foreach ($sequence in 1..5) {
    Read-Host "Install and open the unchanged candidate for live run $sequence; verify the same managed account and active tuple are Ready, close it, then press Enter"
    foreach ($requiredPath in @($candidateExecutable, $candidateResources, $candidateUninstaller)) {
        if (-not (Test-Path -LiteralPath $requiredPath)) {
            throw "candidate installation path is missing: $requiredPath"
        }
    }
    $authPath = Join-Path $acceptanceHome 'codex\auth.json'
    $databasePath = Join-Path $acceptanceHome 'installation.sqlite'
    if (-not (Test-Path -LiteralPath $authPath -PathType Leaf)) { throw 'managed Codex auth is not in the acceptance home' }
    if (-not (Test-Path -LiteralPath $databasePath -PathType Leaf)) { throw 'acceptance/settings database is missing' }
    $measuredSha256 = (Get-FileHash -Algorithm SHA256 $candidateExecutable).Hash.ToLowerInvariant()
    if ($null -eq $releaseCandidateSha256) { $releaseCandidateSha256 = $measuredSha256 }
    elseif ($releaseCandidateSha256 -ne $measuredSha256) { throw "candidate changed before live run $sequence" }
    $commandPath = Join-Path $acceptanceRoot ("codex-live-{0:D2}.json" -f $sequence)
    $command = [ordered]@{
        opted_in = $true
        application_artifact_path = $candidateExecutable
        application_resource_directory_path = $candidateResources
        application_uninstaller_path = $candidateUninstaller
        deterministic_acceptance_record_sha256 = $recordHash
    }
    [IO.Directory]::CreateDirectory($acceptanceRoot) | Out-Null
    [IO.File]::WriteAllText($commandPath, ($command | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
    $liveOutput = & npm run acceptance:live -- $acceptanceHome $commandPath 2>&1
    $liveOutput | ForEach-Object { Write-Host $_ }
    if ($LASTEXITCODE -ne 0) { throw "live acceptance run $sequence failed" }
    $liveJson = $liveOutput | Where-Object { $_ -match '^\{' } | Select-Object -Last 1 | ConvertFrom-Json
    if ($liveJson.sequence_number -ne $sequence -or $liveJson.outcome -ne 'passed') { throw "live run $sequence returned invalid evidence" }
    if ($liveJson.release_candidate_sha256 -ne $releaseCandidateSha256) { throw "live run $sequence measured another candidate" }
}
$privateOutput = & npm run acceptance:private -- $acceptanceHome $releaseCandidateSha256 2>&1
$privateOutput | ForEach-Object { Write-Host $_ }
if ($LASTEXITCODE -ne 0) { throw 'private acceptance aggregation failed' }
```

- [ ] **Step 6: Verify evidence and handle a real defect**

The private record must contain sequence numbers 1-5 for one hash, the complete Codex runtime/schema/selection/capability/bridge identity, zero Quantix retries, and no URL/code/token/prompt/Tender content. Public release remains blocked unless separately approved evidence clears built-in pre-execution control, OS isolation, and distribution/managed-subscription terms. If live work exposes a defect, stop qualification, reproduce it with a deterministic RED test in a new bounded branch, fix/review/reintegrate it, then restart live sequence at 1 for a new candidate. Never commit Application Home data or live command files.

## Plan Completion Gate

Slice 1 is complete only when:

- only canonical `ai::contract::AiProviderKind`, `AiReasoningSelection`, and the complete immutable `ActiveAiConfiguration` remain, and Slice 2 imports them unchanged;
- every Codex activation is backed by committed selected-tuple `AiProbeEvidence` with exact provider route, catalogue, and capability identities;
- every app-server request/event/tool receipt carries and verifies the same Agent Run, event ordinal, call ID, and idempotency identity through the actor/Host boundary;
- race/crash tests prove commit-before-continuation, no duplicate tool execution, no automatic Provider Turn retry, truthful indeterminate outcomes, and complete process-tree cleanup;
- the static scan finds no production private ChatGPT/custom OAuth/direct adapter path;
- private runtime tests stay in their source modules and integration tests exercise only public Host/IPC boundaries;
- the binding exporter removes stale owned DTOs and a second export is a no-op;
- the tracked generated app-server schema and runtime provenance are committed and reproduce byte-for-byte;
- the Task 13 deterministic gate and independent review pass before integration;
- after integration, the interactive selected-model Test Project run succeeds once in the dev app and once from the formal candidate using the exact acceptance Application Home;
- five live acceptance runs and private aggregation succeed for one unchanged release-stage candidate;
- dynamic-tool bridge qualification is recorded, while public release still truthfully blocks on unproven built-in suppression, OS isolation, and distribution/managed-subscription terms; and
- the ordinary/main dev server remains running throughout every pre- and post-integration task.
