# Layer 1C AI Runtime and Settings Cutover Implementation Plan

> **Superseded — do not execute.** ADR 0018 and the
> [SDK-first runtime design](../specs/2026-08-25-sdk-first-ai-runtime-cutover-design.md)
> replace its runtime, Settings, and cutover assumptions. A replacement plan will
> be written after the revised design is approved.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mount the new connection system end to end, switch all future Agent Runs to the one global Active AI Configuration, and delete every private-backend, plaintext-auth, ChatGPT-only, and per-Tender selection path.

**Architecture:** Prepare the new Host commands and renderer components independently, then perform one vertical cutover across schemas, runtime dispatch, generated DTOs, and mounted Settings UI. The immutable Agent Run provider binding remains the historical boundary. No compatibility layer survives the cutover commit.

**Tech Stack:** Rust/Tauri Host, Layer 1A vault/connection contract, Layer 1B provider workers, SQLite exact schemas 26/46, ts-rs, React 19, TypeScript 7, Vitest.

**Spec:** `docs/superpowers/specs/2026-08-24-layer-1-ai-connection-foundation-design.md`, parent plan `docs/superpowers/plans/2026-08-24-layer-1-ai-connection-foundation.md`, and completed Layer 1A/1B plans.

## Global Constraints

- Start only after Layer 1A and 1B are reviewed and `npm run verify` passes.
- The primary/integration agent owns hotspot files: `lib.rs`, `setup.rs`, `application_settings.rs`, `agent_runtime.rs`, `tender_store.rs`, manifests/locks, binding exporter, `quantixHost.ts`, and `ApplicationSettings.tsx`.
- Parallel agents may create only non-overlapping new components/tests or update disjoint Tender modules after the integration agent freezes exact interfaces.
- Do not preserve old commands, DTO aliases, database tables, provider enums, fixture modes, auth stores, backend URLs, or compatibility deserializers.
- Do not implement Layer 2 dynamic employees, memory, tools catalogue, Tool Workshop, or Improvement Lab here.
- Keep all existing deterministic Tender functions working through the new active binding. If no exact active configuration is ready, local work remains usable and AI work reports `Waiting for AI Provider`.
- Do not run a production build. Do not reset or delete the user's real `~/.quantix`; use temporary application homes for tests and acceptance.

---

### Task 1: Expose the new typed Host command surface without mounting it

**Files:**

- Modify: `src-tauri/src/ai/connections.rs`
- Create: `src-tauri/src/ai/codex_auth/mod.rs`
- Create: `src-tauri/src/ai/codex_auth/authorize.rs`
- Create: `src-tauri/src/ai/codex_auth/callback_server.rs`
- Create: `src-tauri/src/ai/codex_auth/crypto.rs`
- Create: `src-tauri/src/ai/codex_auth/device.rs`
- Create: `src-tauri/src/ai/codex_auth/jwt.rs`
- Create: `src-tauri/src/ai/codex_auth/tokens.rs`
- Modify: `src-tauri/src/application_settings.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/bin/export_bindings.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src/quantixHost.ts`
- Modify: `src/quantixHost.test.ts`
- Generate: `src/bindings/` new Layer 1 connection/command declarations

**Final commands:**

```text
inspect_application_settings
create_ai_connection
update_ai_connection
discover_ai_connection_models
test_ai_connection
set_ai_connection_enabled
disconnect_ai_connection
delete_ai_connection
set_active_ai_configuration
clear_active_ai_configuration
reset_ai_connections_vault
start_codex_account_login
start_codex_device_login
cancel_codex_account_login
```

Every mutating command returns a fresh credential-free `ApplicationAiSettingsView` or a narrow login result. Only create/update/login request DTOs may contain secrets.

The generated input DTOs are exactly `CreateAiConnectionCommand`, `UpdateAiConnectionCommand`, `DiscoverAiConnectionModelsCommand`, `TestAiConnectionCommand`, `SetAiConnectionEnabledCommand`, `DisconnectAiConnectionCommand`, `DeleteAiConnectionCommand`, `SetActiveAiConfigurationCommand`, `ClearActiveAiConfigurationCommand`, `ResetAiConnectionsVaultCommand`, `StartCodexAccountLoginCommand`, `StartCodexDeviceLoginCommand`, and `CancelCodexAccountLoginCommand`. Login starts return `AccountLoginStartResult`; all other results are the view above or `AiConnectionView`. No secret-bearing input type appears in a result union.

```rust
pub struct ApplicationAiSettingsView {
    pub connections: Vec<AiConnectionView>,
    pub active_configuration: Option<ActiveAiConfigurationView>,
    pub readiness: ActiveAiReadiness,
    pub login: Option<AccountLoginProgress>,
}
```

`discover_ai_connection_models` authenticates and loads a bounded catalogue but records no capability proof and cannot authorize activation. `test_ai_connection` always carries an exact `connection_id`, current `execution_revision`, explicit `model_id`, and explicit `AiReasoningSelection`.

**Interfaces:**

- Consumes: `AiConnectionRepository`, `CodexWorker`, `GeneralWorker`, OpenID Connect flow, and `QuantixHost`.
- Produces: the exact Tauri commands/DTOs above and renderer wrappers with `{ command }` payloads.

- [ ] **Write the red write-only command test**

```rust
#[tokio::test]
async fn create_test_and_activate_are_three_explicit_actions() {
    let host = fixture_host();
    let created = host.create_ai_connection(openai_create("sentinel-key")).await.unwrap();
    assert!(created.active_configuration.is_none());
    let tested = host.test_ai_connection(test_exact(&created.connections[0], "gpt-test", "low")).await.unwrap();
    assert!(tested.active_configuration.is_none());
    let active = host.set_active_ai_configuration(activate_exact(&tested.connections[0], "gpt-test", "low")).await.unwrap();
    assert_eq!(active.readiness, ActiveAiReadiness::Ready);
    assert!(!serde_json::to_string(&active).unwrap().contains("sentinel-key"));
}
```

- [ ] **Run it and confirm red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib ai::connections::tests::create_test_and_activate_are_three_explicit_actions`

Expected: FAIL because Host commands are not exposed.

- [ ] **Implement one representative Host/Tauri wrapper pair**

```rust
impl QuantixHost {
    pub async fn create_ai_connection(
        &self,
        command: CreateAiConnectionCommand,
    ) -> Result<ApplicationAiSettingsView, TenderCommandError> {
        self.ai_connections().create(command).map_err(Into::into)?;
        self.inspect_ai_settings()
    }
}

#[tauri::command]
async fn create_ai_connection(
    host: tauri::State<'_, QuantixHost>,
    command: CreateAiConnectionCommand,
) -> Result<ApplicationAiSettingsView, TenderCommandError> {
    host.create_ai_connection(command).await
}
```

- [ ] Add failing Rust command tests for strict payload validation, write-only secrets, exact connection targeting, current-revision compare-and-swap, test-before-activate, delete guards, login cancellation, and redacted errors.
- [ ] Add a failing corrupt/unsupported-vault recovery test. `reset_ai_connections_vault` is available only in that state, requires exact `RESET AI CONNECTIONS` confirmation, validates the vault/lock paths without decrypting them as empty, clears the active reference, cancels connection work, removes only the exact encrypted vault, records redacted hash/size evidence, and creates a new encrypted empty vault. The Engineer must reconnect.
- [ ] Implement Host methods that load/mutate the vault under its lock, call the exact worker for discovery/test, and return only secret-free views. Discovery records no capability proof, and Test never activates.
- [ ] Serialize disconnect against login/refresh/run/tool-result handling. Cancel every nonterminal provider operation on that connection, invalidate pending approvals bound to its revision, reject late results, then remove the credential; never resume or retry the disconnected turn.
- [ ] Make browser login primary and device code an explicit fallback. Both target one account-login connection ID and serialize against refresh/disconnect/delete.
- [ ] Browser login binds `127.0.0.1` before opening the system browser, tries registered loopback ports 1455 then 1457, accepts only `GET /auth/callback` with a 16 KiB request-head limit, validates PKCE/state/nonce/issuer/signature/audience/expiry/account, and expires after five minutes. Only one browser/device attempt may own the global fixed-port/login guard at a time.
- [ ] Device login shows only the verification URL and one-time user code, polls at the provider interval plus safety margin, supports cancellation between polls, and expires after fifteen minutes.
- [ ] Add maintained `openidconnect = "4.0.1"` and rebuild browser PKCE/state/nonce, issuer discovery/JWKS, token exchange, ID-token signature/issuer/audience/expiry/nonce validation, and refresh under `ai::codex_auth`. Reuse only bounded callback/device mechanics from the old code; remove its hand-written query encoder, payload-only JWT trust, and `auth.json` dependency.
- [ ] Re-audit every request/claim check against the pinned official Codex 0.149.1 auth implementation and add focused authorize/callback/device/signature/state/nonce/issuer/audience/account/refresh tests. Do not preserve old OAuth behavior merely for compatibility.
- [ ] Build URLs with the library URL API, disable redirects, bound response bodies/times, bind callbacks only to exact loopback addresses/path/method, and zero/discard ID-token bytes after verified projection.
- [ ] A refresh must return the same verified account identity and stable plan/residency. Any identity/destination change fails, disconnects the credential, and requires explicit re-login, re-test, and re-activation.
- [ ] Add Tauri wrappers and temporary registration of the new commands while retaining old registrations until the atomic cutover task. Do not mount new UI yet.
- [ ] Update the binding exporter for the new DTOs. Make it generate into a temporary directory and remove only stale files bearing the ts-rs generated-file header before atomic publication, so obsolete generated declarations disappear through the generator rather than manual editing.
- [ ] Run `cargo run --manifest-path src-tauri/Cargo.toml --bin export_bindings --features runtime-fixture`; expect new committed declarations while old declarations still exist because their Rust types remain until Task 3.
- [ ] Add exact `invoke` wrappers and tests for command names and `{ command }` payloads. Clear JS references to request objects after awaited secret-bearing calls.
- [ ] Run `npx vitest run src/quantixHost.test.ts`; expect all new and old bridge tests to pass during this preparation checkpoint.
- [ ] Commit as `feat: expose multi provider connection commands`.

---

### Task 2: Build the new AI Settings components while they are still unmounted

**Files:**

- Create: `src/AiConnectionsSettings.tsx`
- Create: `src/AiConnectionsSettings.test.tsx`
- Create: `src/AiConnectionForm.tsx`
- Create: `src/AiConnectionList.tsx`
- Create: `src/ActiveAiConfigurationControl.tsx`
- Create: `src/aiSettingsCopy.ts`
- Modify: `src/ManagerWorkspace.css`

**User flow:** `AI Settings → Connections → Add/Sign in → Test → Choose model/reasoning → Set Active`.

**Interfaces:**

- Consumes: generated `ApplicationAiSettingsView`/command types and callback props; no direct `invoke` inside leaf components.
- Produces: unmounted `AiConnectionsSettings` composition ready for Task 4.

- [ ] **Write the red no-auto-selection/secret-clear test**

```tsx
it("requires explicit model activation and clears the submitted key", async () => {
  const onCreate = vi.fn().mockResolvedValue(undefined);
  const onSetActive = vi.fn();
  render(<AiConnectionsSettings value={emptyAiSettings()} onCreate={onCreate} onSetActive={onSetActive} />);
  await userEvent.click(screen.getByRole("button", { name: "Add connection" }));
  await userEvent.selectOptions(screen.getByLabelText("Connection method"), "direct_provider_key");
  await userEvent.selectOptions(screen.getByLabelText("Provider"), "open_ai");
  const key = screen.getByLabelText("API key");
  await userEvent.type(key, "sentinel-key");
  await userEvent.click(screen.getByRole("button", { name: "Save connection" }));
  expect(onCreate).toHaveBeenCalledTimes(1);
  expect(key).toHaveValue("");
  expect(onSetActive).not.toHaveBeenCalled();
});
```

- [ ] **Run it and confirm red**

Run: `npx vitest run src/AiConnectionsSettings.test.tsx`

Expected: FAIL because the components do not exist.

- [ ] **Implement the controlled shell/form boundary**

```tsx
type Props = {
  value: ApplicationAiSettingsView;
  onCreate(command: CreateAiConnectionCommand): Promise<void>;
  onSetActive(command: SetActiveAiConfigurationCommand): Promise<void>;
};

export function AiConnectionsSettings({ value, onCreate, onSetActive }: Props) {
  return (
    <section aria-labelledby="ai-connections-heading">
      <h2 id="ai-connections-heading">AI connections</h2>
      <AiConnectionList connections={value.connections} />
      <AiConnectionForm onSubmit={onCreate} />
      <ActiveAiConfigurationControl
        value={value.active_configuration}
        connections={value.connections}
        onSubmit={onSetActive}
      />
    </section>
  );
}
```

- [ ] Write failing component tests for all four connection methods and direct-provider choices. Use generated DTOs and mocked callbacks only.
- [ ] Add tests for create, test, edit, rename, replace credential, disable/enable, disconnect, guarded delete, browser login, device fallback, cancel, and refresh.
- [ ] Add tests proving no provider/model/reasoning is selected automatically, a test never activates, a disabled/stale/unprobed connection cannot activate, and only one active card exists.
- [ ] Add a sentinel test proving submitted keys/tokens/header values/query values never render in DOM, confirmation copy, error copy, or subsequent edit fields.
- [ ] Clear secret fields in `finally` on success, rejection, cancellation, dialog close, section navigation, and component unmount; never place them in URL, local/session storage, reducer history, or browser preview fixtures.
- [ ] Render secret values only in password inputs with spellcheck disabled and password-manager/autocomplete suppression; never offer reveal/copy for stored credentials. A newly typed value may be shown only while the Engineer is actively entering it.
- [ ] Implement `AiConnectionForm` as a tagged method form:
  - account login: friendly name and sign-in action;
  - direct key: provider, friendly name, key;
  - OpenAI-compatible: friendly name, HTTPS/loopback URL, credential, exact model ID, optional advanced headers/query;
  - Anthropic-compatible: the corresponding Messages-compatible fields.
- [ ] After account login or direct-key creation, run explicit model discovery under the same visible busy action, then require the Engineer to choose model/reasoning and press `Test model`. For compatible endpoints, keep the entered model ID when discovery is unsupported.
- [ ] Explain before Test that a small provider request may consume account/API usage. Keep raw URLs/model IDs left-to-right inside any future RTL layout.
- [ ] Implement list/status/actions and one separate active-configuration control. Use an explicit placeholder (`Choose a connection`, `Choose a model`, `Choose reasoning`) rather than the first option.
- [ ] Centralize user-facing English strings and stable copy keys in `aiSettingsCopy.ts`; do not add a translation framework or claim bilingual completion in Layer 1.
- [ ] Ensure keyboard operation, focus restoration after dialogs, accessible status announcements, reduced motion, high contrast, and 760 px minimum layout work with existing Settings styling.
- [ ] Run `npx vitest run src/AiConnectionsSettings.test.tsx`; expect all component tests to pass.
- [ ] Commit as `feat: build the ai connections settings flow`.

---

### Task 3: Switch schemas and Agent Runs to the global active binding

**Files:**

- Modify: `src-tauri/src/setup.rs`
- Modify: `src-tauri/src/application_settings.rs`
- Modify: `src-tauri/src/agent_runtime.rs`
- Modify: `src-tauri/src/host.rs`
- Modify: `src-tauri/src/tender_store.rs`
- Modify: `src-tauri/src/tender_store/agent_records.rs`
- Modify: `src-tauri/src/tender_store/backups.rs`
- Modify: `src-tauri/src/tender_store/bid_decisions.rs`
- Modify: `src-tauri/src/tender_store/calculations.rs`
- Modify: `src-tauri/src/tender_store/estimates.rs`
- Modify: `src-tauri/src/tender_store/external_rfis.rs`
- Modify: `src-tauri/src/tender_store/manager_intake.rs`
- Modify: `src-tauri/src/tender_store/package_validation.rs`
- Modify: `src-tauri/src/tender_store/production_scheduler.rs`
- Modify: `src-tauri/src/tender_store/tender_records.rs`
- Modify: `src-tauri/src/tender_store/workspace.rs`
- Modify: `src-tauri/src/doctor.rs`
- Modify: `src-tauri/src/diagnostics.rs`
- Modify: `src-tauri/src/acceptance.rs`
- Modify: `src-tauri/src/release_gate.rs`
- Modify: `src-tauri/src/update.rs`
- Modify tests: `src-tauri/tests/agent_runtime.rs`
- Modify tests: `src-tauri/tests/manager_workspace.rs`
- Modify tests: `src-tauri/tests/quantix_setup.rs`
- Modify tests: `src-tauri/tests/release_configuration.rs`
- Modify tests: `src-tauri/tests/safe_updates.rs`
- Modify tests: `src-tauri/tests/tender_commands.rs`
- Modify tests: `src-tauri/tests/tender_intake.rs`
- Modify tests: `src-tauri/tests/tender_records.rs`
- Modify tests: `src-tauri/tests/tender_backup.rs`

**Schema cutover:**

- Installation schema `26` has no `provider_connections` table. `application_settings.settings_json` contains `general_preferences` and `active_ai_configuration` only.
- Tender schema `46` has no `tender_ai_execution_binding` table. It retains immutable `agent_run_provider_bindings` with the new `ActiveAiConfiguration` snapshot.
- Queued manager-intake/work items store no provider binding. The global configuration is resolved and pinned only at the state transition that creates an actual Agent Run; that transition defines “future run.”

**Interfaces:**

- Consumes: final Host commands/workers and current Agent Run preparation callbacks.
- Produces: installation schema 26, Tender schema 46, `prepare_ai_runtime_binding() -> PreparedAiRuntimeBinding`, and exhaustive `dispatch_ai_turn(binding, request)`.

- [ ] **Write the red schema/run-pinning test**

```rust
#[tokio::test]
async fn run_binding_is_global_at_run_creation_and_immutable() {
    let host = ready_host_with_active("connection-a", 1, "model-a");
    let run_a = host.run_bootstrap_agent(fixture_run_command()).await.unwrap();
    host.activate_fixture_connection("connection-b", 1, "model-b").unwrap();
    let run_b = host.run_bootstrap_agent(fixture_run_command()).await.unwrap();
    assert_eq!(run_a.ai_binding.connection_id, "connection-a");
    assert_eq!(run_a.ai_binding.model_id, "model-a");
    assert_eq!(run_b.ai_binding.connection_id, "connection-b");
    assert_eq!(run_b.ai_binding.model_id, "model-b");
    assert_eq!(host.table_exists(&run_a.tender_id, "tender_ai_execution_binding"), false);
}
```

- [ ] **Run it and confirm red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime run_binding_is_global_at_run_creation_and_immutable --features runtime-fixture`

Expected: FAIL because the old per-Tender selection is still authoritative.

- [ ] **Implement the exact schema constants and run binding transaction**

```rust
pub(crate) const INSTALLATION_SCHEMA_VERSION: i64 = 26;
pub(crate) const TENDER_SCHEMA_VERSION: i64 = 46;

pub(crate) struct PreparedAiRuntimeBinding {
    pub active: ActiveAiConfiguration,
    pub secret: Zeroizing<Vec<u8>>,
}

async fn prepare_ai_runtime_binding(
    host: &QuantixHost,
    store: &mut TenderStore,
    run_id: &str,
) -> Result<PreparedAiRuntimeBinding, TenderCommandError> {
    let _gate = host.ai_connection_gate().lock().await;
    host.ai_connections().with_exact_active(|active, secret| {
        store.insert_agent_run_and_ai_binding(run_id, active)?;
        Ok(PreparedAiRuntimeBinding::new(active.clone(), secret.clone_zeroizing()))
    })
}
```

- [ ] **Implement exhaustive dispatch with no wildcard fallback**

```rust
match binding.method {
    AiConnectionMethod::AccountLogin => host.codex_worker().run_turn(binding, request).await,
    AiConnectionMethod::DirectProviderKey
    | AiConnectionMethod::OpenAiCompatible
    | AiConnectionMethod::AnthropicCompatible => host.general_worker().run_turn(binding, request).await,
}
```

- [ ] First rewrite tests to expect schema 26/46, no defaults, no per-Tender binding, and exact global snapshot behavior. Run the focused Rust tests; expect schema/type/dispatch failures.
- [ ] Classify known schema-25/45 development data as `UnsupportedVersion`, never `Corrupt`, and offer no migration or automatic repair. Preserve every Tender byte. Detect legacy plaintext `auth.json`, explain that it is unusable, and require an explicit exact-path removal/revocation decision; never silently purge it or the Application Home.
- [ ] Update setup ownership to allow only `ai-connections.vault`, its exact lock/temp replacement names, and the managed provider runtime/state directories. Remove `auth.json` from every allowed-file list and setup fact.
- [ ] Replace `AiExecutionSelection`/approval readiness with `ActiveAiConfiguration` readiness in the Host. One activation action is the attributable data-destination decision; remove the duplicate approval record.
- [ ] At every Agent Run creation boundary, hold the fixed connection/account-auth → vault → installation SQLite → Tender SQLite order through creation of the Agent Run and immutable provider binding. Copy the secret into zeroizing Host memory, commit the Tender transaction, release every store lock, then start the worker.
- [ ] Add run-start-versus-edit/delete/disable/disconnect concurrency tests proving a run has either one complete old binding/credential snapshot or no run; no mixed revision is possible.
- [ ] Ensure run A keeps revision A after global revision/configuration B is activated; a new run captures B. Active workers use their in-memory A secret. Restart recovery without A's exact credential blocks instead of rebinding.
- [ ] Replace the direct backend/provider singleton with exact dispatch: Codex account → `CodexWorker`; every other method → `GeneralWorker`. Match exhaustively and include no fallback arm.
- [ ] Preserve normalized event, usage, failure, terminal, candidate validation, permission callbacks, and result publication semantics under the final `AiRuntimeEvent`/`AiRuntimeUsage`/`AiRuntimeFailure` names. Move them from `agent_runtime.rs` to `ai::contract`, update imports, and keep no `Provider*` type aliases.
- [ ] Delete all methods that inspect/update/default/refresh Tender AI selection and every `RebindManagerIntakeProvider` path. Manager workspace readiness now projects the global active state.
- [ ] Update backups/archives to preserve immutable historical run bindings while excluding `ai-connections.vault`, vault lock/temp files, provider runtimes/venvs, raw worker state/checkpoints, every `codex-home`, and all credentials. Only registered Agent Run evidence/workspace artifacts follow existing backup policy.
- [ ] Update Doctor/diagnostics/update compatibility to schema 26/46, worker readiness, vault state categories, and redacted connection IDs. No repair may select a connection or replace credentials.
- [ ] Convert acceptance/release DTOs and gates to the minimal provider-neutral connection/runtime fields required to compile at cutover; Layer 1D adds the full seven-route evidence and hard gate.
- [ ] Run focused Rust suites; expect success before proceeding:
  - `cargo test --manifest-path src-tauri/Cargo.toml --test quantix_setup --features runtime-fixture`
  - `cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime --features runtime-fixture`
  - `cargo test --manifest-path src-tauri/Cargo.toml --test manager_workspace --features runtime-fixture`
  - `cargo test --manifest-path src-tauri/Cargo.toml --test tender_backup --features runtime-fixture`
  - `cargo test --manifest-path src-tauri/Cargo.toml --test release_configuration --features runtime-fixture`
- [ ] Do not commit yet. Continue directly to Task 4 so the renderer and generated contract cut over in the same integration commit.

---

### Task 4: Delete legacy production paths and mount the new Settings UI

**Files:**

- Delete: `src-tauri/src/agent_backend/client.rs`
- Delete: `src-tauri/src/agent_backend/fixture_client.rs`
- Delete: `src-tauri/src/agent_backend/mod.rs`
- Delete: `src-tauri/src/agent_backend/turn_executor.rs`
- Delete: `src-tauri/src/agent_runtime/codex_actor.rs`
- Delete: `src-tauri/src/agent_runtime/codex_protocol.rs`
- Delete: `src-tauri/src/chatgpt_login.rs`
- Delete: `src-tauri/src/chatgpt_oauth/authorize.rs`
- Delete: `src-tauri/src/chatgpt_oauth/callback_server.rs`
- Delete: `src-tauri/src/chatgpt_oauth/crypto.rs`
- Delete: `src-tauri/src/chatgpt_oauth/device.rs`
- Delete: `src-tauri/src/chatgpt_oauth/jwt.rs`
- Delete: `src-tauri/src/chatgpt_oauth/mod.rs`
- Delete: `src-tauri/src/chatgpt_oauth/store.rs`
- Delete: `src-tauri/src/chatgpt_oauth/tokens.rs`
- Delete: `src-tauri/tests/support/backend_scripts/failed-response.sse`
- Delete: `src-tauri/tests/support/backend_scripts/happy-text.sse`
- Delete: `src-tauri/tests/support/backend_scripts/midstream-abort.sse`
- Delete: `src-tauri/tests/support/backend_scripts/tool-roundtrip.sse`
- Delete: `src-tauri/tests/support/backend_scripts/unauthorized-401.sse`
- Delete: `src/TenderAiSelectionControl.tsx`
- Delete obsolete generated declarations through: `src-tauri/src/bin/export_bindings.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/bin/export_bindings.rs`
- Modify: `src/ApplicationSettings.tsx`
- Modify: `src/ApplicationSettings.test.tsx`
- Modify: `src/applicationAiSelectionReadiness.ts`
- Create: `src/applicationAiSelectionReadiness.test.ts`
- Modify: `src/quantixHost.ts`
- Modify: `src/quantixHost.test.ts`
- Modify: `src/ManagerWorkspace.tsx`
- Modify: `src/ManagerWorkspace.test.tsx`
- Modify: `src/browserPreviewHost.ts`
- Modify: `src/ManagerWorkspace.css`
- Regenerate: `src/bindings/`

**Interfaces:**

- Consumes: compiled schema/runtime cutover and unmounted Settings components.
- Produces: one mounted AI Settings path and one generated TypeScript contract with no legacy AI surface.

- [ ] **Write the red mounted-settings regression**

```tsx
it("mounts global AI Connections and exposes no Tender provider selector", async () => {
  render(<ManagerWorkspace initialSection="ai" />);
  expect(await screen.findByRole("heading", { name: "AI connections" })).toBeVisible();
  expect(screen.queryByText("ChatGPT & Models")).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /rebind provider/i })).not.toBeInTheDocument();
});
```

- [ ] **Run it and confirm red**

Run: `npx vitest run src/ManagerWorkspace.test.tsx src/ApplicationSettings.test.tsx`

Expected: FAIL because the old settings/control is still mounted.

- [ ] **Mount the prepared component and generic readiness helper**

```tsx
{section === "ai" ? (
  <AiConnectionsSettings
    value={settings.ai}
    onCreate={createAiConnection}
    onSetActive={setActiveAiConfiguration}
  />
) : null}
```

```ts
export function applicationAiIsReady(settings: ApplicationSettingsView): boolean {
  return settings.ai.readiness === "ready" && settings.ai.active_configuration !== null;
}
```

- [ ] Remove only obsolete AI module declarations, exports, Tauri wrappers/registrations, binding exports, and renderer bridge functions. Keep the final AI command list from Task 1 and preserve `update_general_application_preferences` plus every unrelated Host command.
- [ ] Remove every ChatGPT-only settings field and UI state: `chatgpt`, `ai_execution_selection`, `ai_execution_approval`, provider defaults/recommendations, per-Tender notes, account fingerprinting, and old browser/device command names.
- [ ] Mount `AiConnectionsSettings` as the AI Settings section. Keep `ApplicationSettings` as the shell for general/data/update/about sections.
- [ ] Replace `exactApplicationAiSelectionIsReady` with a generic pure readiness function that requires an exact non-stale active revision/model/reasoning/catalogue and ready worker state. It performs no hashing or provider-specific account comparison in the renderer.
- [ ] Update browser preview fixtures and Manager workspace tests. The Manager sees only ready/unavailable global AI status; it cannot expose connection editing or provider choice inside a Tender.
- [ ] Regenerate bindings from Rust. The exporter must remove stale generated ChatGPT/AiExecution/TenderAi files and leave unrelated generated DTOs unchanged.
- [ ] Run the exporter twice:
  - `cargo run --manifest-path src-tauri/Cargo.toml --bin export_bindings --features runtime-fixture`
  - `git diff --exit-code -- src/bindings` after staging the first intentional result and running the exporter again.
- [ ] Run `npx vitest run src/AiConnectionsSettings.test.tsx src/ApplicationSettings.test.tsx src/applicationAiSelectionReadiness.test.ts src/quantixHost.test.ts src/ManagerWorkspace.test.tsx`; expect success.
- [ ] Run `npm run check`; expect the new Rust/TypeScript contract to compile with zero old-symbol compatibility aliases.
- [ ] Commit Tasks 3 and 4 together as `feat: cut over to global multi provider ai`.

---

### Task 5: Prove the end-to-end deterministic Settings-to-run flow

**Files:**

- Modify: `src-tauri/tests/agent_runtime.rs`
- Modify: `src-tauri/tests/manager_workspace.rs`
- Modify: `src-tauri/tests/quantix_setup.rs`
- Modify: `src/ApplicationSettings.test.tsx`
- Modify: `src/ManagerWorkspace.test.tsx`

**Interfaces:**

- Consumes: the mounted Host/renderer cutover.
- Produces: deterministic vertical regression evidence used by Layer 1D acceptance.

- [ ] **Write the vertical test before adding fixture helpers**

```rust
#[tokio::test]
async fn settings_to_run_flow_has_no_default_or_fallback() {
    let host = clean_fixture_host();
    assert_eq!(host.inspect_application_settings().unwrap().ai.readiness, ActiveAiReadiness::NotConfigured);
    let a = host.fixture_create_test_activate("a", "model-a").await;
    let run_a = host.run_bootstrap_agent(fixture_run_command()).await.unwrap();
    let _b = host.fixture_create_test_activate("b", "model-b").await;
    host.fixture_fail_connection("b", AiRuntimeFailureCategory::AuthenticationRequired);
    let failed_b = host.run_bootstrap_agent(fixture_run_command()).await.unwrap();
    assert_eq!(run_a.ai_binding.connection_id, a.connection_id);
    assert_eq!(failed_b.state, AgentRunState::Failed);
    assert_eq!(host.worker_invocations("a"), 1);
    assert_eq!(host.worker_invocations("b"), 1);
}
```

- [ ] **Run it and confirm red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime settings_to_run_flow_has_no_default_or_fallback --features runtime-fixture`

Expected: FAIL until the deterministic connection/worker fixture helpers exist.

- [ ] Add one deterministic vertical test: clean setup → zero active config → create fake connection → test explicit model → choose explicit reasoning → activate → start Manager work → persist exact run binding → change active config → prove old run A remains A and new run B is B.
- [ ] Add no-fallback vertical tests for authentication failure, rate limit, worker crash, capability drift, and model reroute. Assert no second worker/process/connection/model is invoked.
- [ ] Add restart tests proving the global active reference persists, stale revisions block, historical bindings remain inspectable, and no Tender-local selection is recreated.
- [ ] Add renderer tests proving novice copy, busy/polling states, device fallback, confirmation consequences, and secret clearing work across the real bridge mock.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime --features runtime-fixture` and `npx vitest run src/ApplicationSettings.test.tsx src/ManagerWorkspace.test.tsx`; expect success.
- [ ] Commit as `test: prove the global ai connection cutover`.

---

### Task 6: Audit complete legacy removal and review Layer 1C

- [ ] Run this production-source audit:

```powershell
rg -n -S "chatgpt\.com/backend-api|auth\.json|chatgpt_oauth|start_chatgpt_|codex_chatgpt|chatgpt-direct-v1|TenderAiExecutionBinding|TenderAiSelectionReadiness|UpdateTenderAiExecution|InspectTenderAiExecution|ConfirmAiExecutionSelection|RebindManagerIntakeProvider|require_current_tender_ai_selection" src src-tauri/src src-tauri/tests
```

Expected result: zero production matches. A test name may mention an explicitly rejected legacy path only if it asserts absence; historical docs are outside this command.

- [ ] Run `rg -n "is_default|provider_default|recommended model|recommended provider" src src-tauri/src/ai`; classify every match and remove any product default behavior.
- [ ] Run `npm run format:check`, `npm run check`, `npm test`, and `npm run verify`; expect success.
- [ ] Use `superpowers:requesting-code-review`. Require review of the schema switch, every run-creation boundary, command secret projections, renderer secret lifetime, exhaustive dispatch, generated bindings, and legacy audit.
- [ ] Apply valid findings, rerun the focused vertical tests and `npm run verify`, and commit fixes separately.
- [ ] Use a temporary application home for manual deterministic inspection. Do not modify the user's real schema-25 `~/.quantix` in this subplan.
