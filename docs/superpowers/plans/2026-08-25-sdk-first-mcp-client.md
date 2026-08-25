# SDK-First Host-Owned MCP Client Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a reviewed, tools-only MCP client lane through official `rmcp`, with local stdio and guarded remote Streamable HTTP connections governed by existing Quantix permissions, approvals, vault, budgets, idempotency, and audit.

**Architecture:** The Rust Host is the only MCP client and policy authority. Persisted `McpConnectionConfig` contains only non-secret transport metadata; credentials are referenced by ID and held in the DPAPI vault. Local servers use a Host-owned supervised transport, while remote servers use a guarded connector that validates the resolved and connected peer before any TLS/HTTP bytes are sent. Discovered tools remain untrusted until mapped into a reviewed Host tool catalogue and reserved through an unforgeable Host reservation before dispatch.

**Tech Stack:** Rust 1.97.1, `rmcp = "=3.1.4"`, MCP `2026-07-28`, Tokio, ProcessKit, Hyper/Hyper-Rustls, SQLite, DPAPI vault, Tauri 2, React 19.

**Spec:** `docs/superpowers/specs/2026-08-25-sdk-first-ai-runtime-cutover-design.md`

## Global Constraints

- Execute only after the Codex and general-provider plans pass their completion gates.
- Host-as-client only; do not create a Quantix MCP server or Codex MCP bridge.
- Negotiate only MCP protocol `2026-07-28`; reject every older/newer/unknown version.
- Enable tools only. Reject resources, prompts, completions, logging, sampling, elicitation, roots, tasks, subscriptions, list-change/progress semantics, MRTR, and InputRequired responses.
- Local integrations use an Engineer-selected absolute executable and stdio under `ProcessSupervisor`; no package marketplace or runtime download exists.
- Remote integrations require an Engineer-created guarded HTTPS connection; no ambient auth, redirects, retries, private/metadata destinations, or hidden session replay.
- `McpConnectionConfig` is persisted in installation SQLite. This slice supports no OAuth; only none, bearer, and named-header API credentials are accepted, and those credential values live only in the DPAPI vault.
- Provider-native MCP is disabled. MCP calls always return through Host permission, approval, quota, idempotency, reservation, and audit.
- Raw wire send/receive helpers are private to `mcp_client.rs`; only a Host-facing governed call can dispatch a tool.
- Private API tests belong in source-module `#[cfg(test)]` modules. Integration tests use only public Host-facing commands and fixture-only binaries; they do not import private modules.
- Do not hand-edit generated bindings; regenerate with `npm test`.
- Do not run production builds during ordinary development.
- Keep the existing main development server running for the whole slice. Never stop it for a question, test, commit, or after completion; if it exits unexpectedly, restart it immediately and record that event.
- Before Task 10 touches the main app, the parent suite's explicitly approved Slice 3 Fresh-State Schema Cutover must archive the preceding home and recreate `%USERPROFILE%\.quantix`; no Slice 3 binary may open installation schema 26, vault schema 1, or Tender schema 46.

---

### Task 1: Pin rmcp and Define the Strict Tools-Only Contract

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Create: `src-tauri/src/mcp_contract.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**

```rust
pub(crate) const MCP_PROTOCOL_VERSION: &str = "2026-07-28";
pub(crate) const MCP_SDK_VERSION: &str = "3.1.4";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum McpTransportKind { Stdio, StreamableHttp }

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct McpError {
    pub code: &'static str,
    pub retry_safe: bool,
    pub required_user_action: &'static str,
}

impl std::fmt::Display for McpError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for McpError {}

pub(crate) struct RawInitializeValidation {
    pub protocol_version: String,
    pub server_name: String,
    pub server_version: String,
    pub capabilities_sha256: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawInitializeEnvelope {
    jsonrpc: String,
    id: serde_json::Value,
    result: RawInitializeResult,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawInitializeResult {
    protocol_version: String,
    capabilities: RawServerCapabilities,
    server_info: RawImplementation,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawServerCapabilities {
    tools: Option<RawToolsCapability>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawToolsCapability {
    list_changed: Option<bool>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawImplementation {
    name: String,
    version: String,
}

pub(crate) struct ValidatedMcpTool {
    pub name: String,
    pub description: String,
    pub input_schema_json: String,
    pub output_schema_json: String,
    pub server_definition_sha256: String,
}

pub(crate) fn validate_protocol_version(
    version: &rmcp::model::ProtocolVersion,
) -> Result<(), McpError>;

pub(crate) fn validate_initialize_response_json(
    raw_json: &[u8],
) -> Result<RawInitializeValidation, McpError>;

pub(crate) fn validate_server_capabilities(
    capabilities: &rmcp::model::ServerCapabilities,
) -> Result<(), McpError>;

pub(crate) fn validate_tool_schema(
    tool: &rmcp::model::Tool,
) -> Result<ValidatedMcpTool, McpError>;

pub(crate) fn validate_tool_catalogue(
    tools: &[rmcp::model::Tool],
) -> Result<Vec<ValidatedMcpTool>, McpError>;
```

- [ ] **Step 1: Write the RED contract tests in the source module.**

  In `mcp_contract.rs`, add `#[cfg(test)] mod tests` with concrete fixtures and helpers. These tests cover only raw initialize/capability/schema validation; handler behavior and post-handshake message dispatch belong to Task 5.

  ```rust
  fn fixture_capability(kind: &str) -> ServerCapabilities;
  fn fixture_tool(name: &str, input: Value, output: Option<Value>) -> Tool;
  fn fixture_protocol(value: &str) -> ProtocolVersion;
  ```

  Add this raw-wire regression first:

  ```rust
  #[test]
  fn initialize_capabilities_are_rejected_before_rmcp_typed_normalization() {
      let raw = br#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2026-07-28","capabilities":{"tools":{},"resources":{}},"serverInfo":{"name":"fixture","version":"1"}}}"#;
      let error = validate_initialize_response_json(raw).unwrap_err();
      assert_eq!(error.code, "MCP_CAPABILITY_NOT_ALLOWED");
  }
  ```

  `validate_initialize_response_json` must deserialize a local `#[serde(deny_unknown_fields)]` raw envelope and raw capability DTO before calling `serde_json::from_value::<ServerJsonRpcMessage>`. It must reject unknown capability keys, unknown result keys, duplicate/unknown capability fields, `tools.listChanged: true`, wrong protocol, and non-object tools before rmcp typed normalization.

  Test typed `fixture_capability("tools")` succeeds and typed `resources`, `prompts`, `completions`, `logging`, and experimental extensions fail. Test raw initialize JSON containing `sampling`, `elicitation`, `roots`, `tasks`, `subscriptions`, `progress`, or any unknown top-level capability fails before rmcp normalization. Test missing tools and protocol mismatch here. Test duplicate names through `validate_tool_catalogue(&[Tool])`; test oversized/recursive/malformed/unknown-field/non-object schemas through `validate_tool_schema`. Client-only server requests and post-handshake notification semantics remain exclusively in Task 5 handler/message tests.

- [ ] **Step 2: Run the RED source tests.**

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_contract::tests --features runtime-fixture -- --nocapture
  ```

  Expected: compile/test failure because the contract and fixture helpers are not implemented.

- [ ] **Step 3: Add the exact dependency and explicit client protocol.**

  Add:

  ```toml
  rmcp = { version = "=3.1.4", default-features = false, features = [
      "client", "transport-streamable-http-client",
  ] }
  hyper = { version = "=1.11.0", features = ["client", "http1"] }
  hyper-util = { version = "=0.1.20", features = ["client", "client-legacy", "http1", "tokio"] }
  hyper-rustls = { version = "=0.27.9", default-features = false, features = ["native-tokio", "http1", "tls12", "aws-lc-rs"] }
  http-body-util = "=0.1.5"
  http = "=1.5.0"
  bytes = "=1.12.1"
  futures-util = "=0.3.34"
  tower-service = "=0.3.3"
  sse-stream = "=0.2.4"
  ```

  Add Tokio's `net` feature to the existing exact Tokio dependency. These are the pinned crates used to implement `rmcp::transport::streamable_http_client::StreamableHttpClient` over a Host-validated TCP connector; the production MCP lane must not enable or instantiate rmcp's stock reqwest transport or child-process transport.

  The client identity must be constructed as:

  ```rust
  ClientInfo::new(ClientCapabilities::default(), Implementation::new("Quantix", version))
      .with_protocol_version(ProtocolVersion::V_2026_07_28)
  ```

  Never use `ClientLifecycleMode::Auto`, legacy fallback, server/macros features, or client capability flags for roots/sampling/elicitation/tasks.

- [ ] **Step 4: Implement bounded canonical validation and run GREEN.**

  `validate_server_capabilities` must require only `tools.is_some()`, reject every other representable typed field/extension, and reject non-empty tool-list-change semantics. `validate_tool_schema` must parse both schemas, require bounded object schemas, canonicalize with `serde_json_canonicalizer`, enforce the per-schema byte/depth/property limits, and hash the canonical name/description/input/output tuple. `validate_tool_catalogue` calls it for every bounded tool, rejects duplicate names/hashes, and returns the reviewed canonical order.

  Minimal catalogue implementation:

  ```rust
  pub(crate) fn validate_tool_catalogue(tools: &[Tool]) -> Result<Vec<ValidatedMcpTool>, McpError> {
      let mut validated = tools.iter().map(validate_tool_schema).collect::<Result<Vec<_>, _>>()?;
      validated.sort_by(|left, right| left.name.cmp(&right.name));
      if validated.windows(2).any(|pair| pair[0].name == pair[1].name || pair[0].server_definition_sha256 == pair[1].server_definition_sha256) {
          return Err(McpError { code: "MCP_DUPLICATE_TOOL", retry_safe: false, required_user_action: "Review the MCP server catalogue" });
      }
      Ok(validated)
  }
  ```

- [ ] **Step 5: Run and commit Task 1.**

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_contract::tests --features runtime-fixture
  git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/mcp_contract.rs src-tauri/src/lib.rs
  git commit -m "feat: define strict MCP client contract"
  ```

### Task 2: Persist Exact Connection Configurations and DPAPI Credential References

**Files:**
- Create: `src-tauri/src/mcp_store.rs`
- Create: `src-tauri/tests/fixtures/installation-v27/catalogue.sql`
- Create: `src-tauri/tests/fixtures/installation-v27/expected-schema.json`
- Create: `src-tauri/tests/installation_schema.rs`
- Modify: `src-tauri/src/setup.rs`
- Modify: `src-tauri/src/ai/vault.rs`
- Modify: `src-tauri/tests/ai_connection_vault.rs`
- Modify: `src-tauri/tests/runtime_readiness.rs`
- Modify: `src-tauri/tests/safe_updates.rs`
- Modify: `src-tauri/tests/quantix_setup.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**

```rust
pub(crate) struct McpConnectionConfig {
    pub connection_id: String,
    pub display_name: String,
    pub transport: McpTransportConfig,
    pub enabled: bool,
    pub execution_revision: u64,
}

pub(crate) enum McpTransportConfig {
    Stdio {
        executable: String,
        arguments: Vec<String>,
        cwd: String,
    },
    StreamableHttp {
        url: String,
        auth: McpAuthPlacement,
        destination_fingerprint: String,
        allow_stateless: bool,
    },
}

pub(crate) enum McpAuthPlacement {
    None,
    AuthorizationBearer { credential_ref: String },
    Header { name: String, credential_ref: String },
}

pub(crate) struct McpServerMetadata {
    pub connection_id: String,
    pub protocol_version: String,
    pub server_name: String,
    pub server_version: String,
    pub capabilities_sha256: String,
    pub tools_hash: String,
    pub discovery_revision: u64,
}

pub(crate) struct McpToolRecord {
    pub tool_id: String,
    pub connection_id: String,
    pub server_tool_name: String,
    pub input_schema_json: String,
    pub output_schema_json: String,
    pub server_definition_sha256: String,
    pub policy_sha256: Option<String>,
    pub definition_version: u64,
    pub catalogue_revision: u64,
    pub state: McpToolState,
    pub policy: Option<McpToolPolicy>,
    pub approved: bool,
}

pub(crate) struct McpToolPolicy {
    pub quantix_tool_name: String,
    pub required_capability: String,
    pub required_action: String,
    pub required_data_scopes: Vec<String>,
    pub allowed_data_classifications: Vec<DataClassification>,
    pub side_effect_class: ToolSideEffectClass,
    pub quota: TypedToolQuota,
    pub idempotency: ToolIdempotency,
}

pub(crate) enum McpToolState {
    Discovered,
    Reviewed,
    Approved,
    Revoked,
    Tombstoned,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum McpStoreError {
    #[error("MCP connection configuration is invalid")]
    InvalidConfig,
    #[error("MCP connection or tool was not found")]
    NotFound,
    #[error("MCP connection revision conflict")]
    RevisionConflict,
    #[error("MCP store is unavailable")]
    StoreUnavailable,
    #[error("MCP secret boundary violation")]
    SecretBoundaryViolation,
}

pub(crate) fn load_mcp_connection(
    database: &rusqlite::Connection,
    connection_id: &str,
) -> Result<McpConnectionConfig, McpStoreError>;

pub(crate) fn save_mcp_connection(
    database: &rusqlite::Connection,
    config: &McpConnectionConfig,
    expected_revision: Option<u64>,
) -> Result<McpConnectionConfig, McpStoreError>;

pub(crate) struct McpStore {
    application_home: std::path::PathBuf,
}

impl McpStore {
    pub(crate) fn new(application_home: &std::path::Path) -> Result<Self, McpStoreError>;
    pub(crate) fn load_connection(
        &self,
        connection_id: &str,
    ) -> Result<McpConnectionConfig, McpStoreError>;
    pub(crate) fn load_credential(
        &self,
        credential_ref: &str,
    ) -> Result<Option<zeroize::Zeroizing<String>>, McpStoreError>;
}
```

- [ ] **Step 1: Write RED persistence tests with exact configuration fixtures.**

  Define source-module helpers `fn fixture_stdio_config() -> McpConnectionConfig`, `fn fixture_remote_config() -> McpConnectionConfig`, `fn fixture_bearer_credential() -> Zeroizing<String>`, `fn fixture_named_header_credential() -> Zeroizing<String>`, and `fn assert_no_secret_bytes(bytes: &[u8], secret: &str)`. Assert that SQLite retains stdio executable/arguments/cwd and remote URL/auth placement/destination fingerprint/`allow_stateless`, but never credential material. Test URL query/userinfo rejection, OAuth variants rejected as invalid input, and required remote fingerprint. Test CAS conflicts, revision increments, duplicate IDs, invalid paths, and no public marketplace/install fields.

  Use a table-driven fixture so each transport policy is explicit:

  ```rust
  #[test]
  fn persisted_connection_cases_keep_metadata_secret_free() {
      for (label, config, expected_transport) in [
          ("stdio", fixture_stdio_config(), "stdio"),
          ("remote", fixture_remote_config(), "streamable_http"),
      ] {
          let fixture = fixture_store(label);
          let saved = fixture.store.save_connection(&config, None).unwrap();
          assert_eq!(saved.transport.kind(), expected_transport);
          assert_no_secret_bytes(&fixture.sqlite_bytes(), "mcp-sentinel");
      }
  }
  ```

- [ ] **Step 2: Run RED.**

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_store::tests --features runtime-fixture -- --nocapture
  ```

  Expected RED failure: `fixture_store`/`save_connection` is unresolved and the fresh schema does not contain the MCP tables.

- [ ] **Step 3: Add fresh installation tables and vault credential map.**

  Slice 2 leaves the fresh-installation schema at `26` after adding pricing approvals. In `setup.rs`, change `INSTALLATION_SCHEMA_VERSION` and the literal installation-table check from `26` to `27`, then add `mcp_connections` plus editable versioned `mcp_tool_definitions`/`mcp_tool_events` with JSON checks, transport/auth placement checks, and revision/CAS columns. Tool definitions are changed by appending a new version; revoke/tombstone is another appended event/version. Run-specific dispatch reservations do not belong in installation SQLite; Task 6 adds them to the exact Tender Store that owns the Agent Run. Do not add SQL UPDATE/DELETE triggers that contradict editable versioned storage. There is no migration or compatibility path. Credential references are Host-generated opaque IDs; the renderer never chooses a vault key. In `vault.rs`, change `VAULT_SCHEMA_VERSION` from `1` to `2`, extend `VaultPayload` with a separately validated `BTreeMap<String, StoredCredential>` for MCP credentials, and reject version 1 rather than treating its missing map as empty. Keep all credential values zeroizing and expose only secret-free snapshots.

  Minimal storage shape after the RED test is green:

  ```rust
  let canonical = serde_json_canonicalizer::to_vec(&config).map_err(|_| McpStoreError::InvalidConfig)?;
  transaction.execute(
      "INSERT INTO mcp_connections(connection_id, config_json, execution_revision, status, created_at, updated_at)
       VALUES (?1, ?2, 1, 'configured', ?3, ?3)",
      rusqlite::params![config.connection_id, canonical, now],
  )?;
  vault.mutate_current_project(|payload| payload.put_mcp_credential(credential_ref, secret))?;
  ```

- [ ] **Step 4: Extend vault RED tests and make them GREEN.**

  Add `installation-v27/catalogue.sql` containing the complete fresh schema-27 catalogue, including `installation`, every schema-26 table/index/trigger (including `ai_pricing_approvals`) unchanged, plus `mcp_connections`, `mcp_tool_definitions`, and `mcp_tool_events`. `expected-schema.json` is an MCP-specific subset assertion with this exact top-level shape and column map: `{"user_version":27,"mcp_tables":["mcp_connections","mcp_tool_definitions","mcp_tool_events"],"columns":{"installation":["singleton","schema_version"],"mcp_connections":["connection_id","config_json","execution_revision","status","created_at","updated_at"],"mcp_tool_definitions":["tool_id","connection_id","definition_version","server_definition_sha256","server_definition_json","policy_sha256","policy_json","state","catalogue_revision","created_at"],"mcp_tool_events":["event_id","tool_id","event_kind","from_version","to_version","expected_server_definition_sha256","new_server_definition_sha256","expected_policy_sha256","new_policy_sha256","catalogue_revision","actor","rationale","created_at"]}}`. The SQL fixture must set `PRAGMA user_version = 27`, retain every existing table/trigger/index expected by schema 26, and enforce positive MCP revisions, canonical hashes, valid JSON, and allowed state/event values. In `installation_schema.rs`, add this concrete test:

  ```rust
  #[test]
  fn fresh_schema_27_has_mcp_catalogue_without_migration_path() {
      let fixture = include_str!("fixtures/installation-v27/catalogue.sql");
      let database = rusqlite::Connection::open_in_memory().unwrap();
      database.execute_batch(fixture).unwrap();
      assert_eq!(database.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0)).unwrap(), 27);
      for table in ["mcp_connections", "mcp_tool_definitions", "mcp_tool_events"] {
          assert!(database.query_row("SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1", [table], |_| Ok(1)).is_ok());
      }
  }
  ```

  Update every existing exact installation-version/schema assertion in `runtime_readiness.rs`, `safe_updates.rs`, and `quantix_setup.rs` from 26 to 27 while preserving the complete schema-26 pricing table. Add sentinel tests for wrong vault version, corruption, truncation, concurrent CAS, partial deserialization, and scans of vault, SQLite, diagnostics, backups, exports, logs, and renderer projections. Run:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_store::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test installation_schema --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_vault --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test runtime_readiness --test safe_updates --test quantix_setup --features runtime-fixture
  git add src-tauri/src/mcp_store.rs src-tauri/tests/fixtures/installation-v27 src-tauri/tests/installation_schema.rs src-tauri/src/setup.rs src-tauri/src/ai/vault.rs src-tauri/src/lib.rs src-tauri/tests/ai_connection_vault.rs src-tauri/tests/runtime_readiness.rs src-tauri/tests/safe_updates.rs src-tauri/tests/quantix_setup.rs
  git commit -m "feat: persist exact MCP configs securely"
  ```

### Task 3: Make Quantix ProcessSupervisor the Only Local MCP Process Owner

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/process_supervisor.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/src/mcp_transport.rs`
- Create: `src-tauri/tests/support/mcp_server_fixture.rs`
- Create: `src-tauri/tests/mcp_stdio.rs`

**Interfaces:**

```rust
#[derive(Debug, thiserror::Error)]
pub(crate) enum McpTransportError {
    #[error("MCP stdio frame is invalid")]
    InvalidFrame,
    #[error("MCP stdio frame exceeded its limit")]
    FrameTooLarge,
    #[error("MCP child process failed")]
    ProcessFailed,
    #[error("MCP child process was cancelled")]
    Cancelled,
    #[error("MCP child process did not close cleanly")]
    CloseFailed,
}

pub(crate) struct SupervisedMcpStdioTransport {
    actor: Arc<StdioActor>,
}

struct StdioActor {
    outbound: tokio::sync::mpsc::Sender<StdioCommand>,
    inbound: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<Result<Vec<u8>, McpTransportError>>>>,
    shutdown: CancellationToken,
    join: Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<Result<(), McpTransportError>>>>>,
}

enum StdioCommand {
    Send {
        frame: Vec<u8>,
        completion: tokio::sync::oneshot::Sender<Result<(), McpTransportError>>,
    },
    Close {
        completion: tokio::sync::oneshot::Sender<Result<(), McpTransportError>>,
    },
}

impl rmcp::transport::Transport<rmcp::RoleClient> for SupervisedMcpStdioTransport {
    type Error = McpTransportError;

    fn send(
        &mut self,
        message: TxJsonRpcMessage<RoleClient>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static;
    fn receive(
        &mut self,
    ) -> impl Future<Output = Option<RxJsonRpcMessage<RoleClient>>> + Send;
    fn close(
        &mut self,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

pub(crate) async fn spawn_supervised_mcp_stdio(
    spec: ProcessSpec,
    cancellation: CancellationToken,
) -> Result<SupervisedMcpStdioTransport, McpTransportError>;

pub(crate) struct SupervisedConversationParts {
    pub reader: SupervisedConversationReader,
    pub writer: SupervisedConversationWriter,
    pub guard: SupervisedConversationGuard,
}

impl SupervisedConversationReader {
    pub(crate) async fn read_line(&mut self) -> Result<Vec<u8>, ProcessError>;
}
impl SupervisedConversationWriter {
    pub(crate) async fn write(&mut self, bytes: &[u8]) -> Result<(), ProcessError>;
    pub(crate) async fn close_stdin(&mut self) -> Result<(), ProcessError>;
}
impl SupervisedConversationGuard {
    pub(crate) async fn finish(
        self,
        abort_reason: Option<ProcessTermination>,
    ) -> Result<ProcessOutput, ProcessError>;
}

impl ProcessSupervisor {
    pub(crate) async fn start_concurrent_conversation(
        &self,
        spec: ProcessSpec,
        cancellation: CancellationToken,
    ) -> Result<SupervisedConversationParts, ProcessError>;
}

#[cfg(feature = "runtime-fixture")]
pub enum McpStdioFixtureMode {
    ToolsOnly,
    CapabilityViolation,
    MalformedFrame,
    OversizedFrame,
    ServerCrash,
    Hang,
    SpawnDescendant,
}
#[cfg(feature = "runtime-fixture")]
pub struct McpStdioFixtureResult {
    pub terminal_code: String,
    pub frames_received: u32,
    pub process_tree_reaped: bool,
}
#[cfg(feature = "runtime-fixture")]
impl QuantixHost {
    pub async fn verify_mcp_stdio_fixture(
        &self,
        mode: McpStdioFixtureMode,
    ) -> Result<McpStdioFixtureResult, TenderCommandError>;
}
```

- [ ] **Step 1: Write RED public Host/fixture tests.**

  Add this concurrency regression before implementation:

  ```rust
  #[tokio::test]
  async fn stdio_send_and_receive_futures_are_static_and_concurrent() {
      let mut transport = spawn_supervised_mcp_stdio(fixture_spec(), CancellationToken::new()).await.unwrap();
      let send = transport.send(fixture_ping_request());
      let receive = transport.receive();
      let (_sent, _received) = tokio::join!(send, receive);
  }
  ```

  Expected RED failure: `SupervisedMcpStdioTransport` has no actor/channel constructor.

  `mcp_stdio.rs` calls only `QuantixHost::verify_mcp_stdio_fixture`; it must not import `process_supervisor`, `SupervisedConversation`, or another private module. The fixture binary accepts only the `McpStdioFixtureMode` values above. Define the test-binary helpers with these signatures: `fn fixture_executable() -> PathBuf`, `fn fixture_home() -> TempDir`, `fn fixture_command(mode: McpStdioFixtureMode) -> Command`, and `async fn wait_for_fixture_exit(child: &mut Child) -> ExitStatus`.

  Test absolute executable/cwd validation, cleared environment, Job Object descendant cleanup, bounded frame/stdout/stderr, malformed JSON, timeout, cancellation, crash, and exactly-once close.

- [ ] **Step 2: Run RED.**

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_transport::tests --features runtime-fixture -- --nocapture
  cargo test --manifest-path src-tauri/Cargo.toml --test mcp_stdio --features runtime-fixture -- --nocapture
  ```

- [ ] **Step 3: Consume Slice 1's production conversation primitive without reopening it.**

  Slice 1 already removed the production cfg gate from `SupervisedConversation` and `start_conversation`. Add the `ProcessSupervisor::start_concurrent_conversation` result used by the actor, with separate supervised reader/writer halves and one shared kill/reap guard. Use that interface from `mcp_transport.rs`; do not expose child handles or weaken `ProcessSpec` absolute executable, cleared-environment, cwd, timeout, cancellation, stdin/stdout/stderr, Job Object kill, and reap rules. The RED test for this task must fail on missing MCP behavior, not on a gate Slice 1 already removed.

- [ ] **Step 4: Implement private bounded rmcp framing.**

  `mcp_transport.rs` alone may serialize `TxJsonRpcMessage` to one bounded JSONL frame and send `StdioCommand::Send { frame, completion }` through the actor channel. The actor owns the supervised reader/writer halves, runs concurrent read/write loops, and resolves the completion only after the bounded frame write succeeds or fails; it sends raw bounded frames to `inbound`, and its decode step calls `validate_initialize_response_json` on an initialize result before `serde_json::from_slice::<RxJsonRpcMessage>`. Although rmcp's `Transport::receive` signature does not spell out `'static`, its returned future must be `'static` in this implementation: `send`, `receive`, and `close` own cloned `Arc`/channel handles and no future borrows the transport. `close` sends `Close { completion }`, awaits the actor's close result and `join` exactly once, and returns a redacted `McpTransportError`. Do not use `rmcp::TokioChildProcess`; it owns a separate cleanup path and bypasses Quantix Job Objects.

  Minimal actor loop:

  ```rust
  while !shutdown.is_cancelled() {
      tokio::select! {
          command = outbound.recv() => match command {
              Some(StdioCommand::Send { frame, completion }) => {
                  let result = writer.write_all(&frame).await.map_err(|_| McpTransportError::ProcessFailed);
                  let _ = completion.send(result);
              }
              Some(StdioCommand::Close { completion }) | None => {
                  let _ = completion.send(Ok(()));
                  break;
              }
          },
          line = reader.read_line() => {
              let raw = line.map_err(|_| McpTransportError::ProcessFailed)?;
              if raw.len() > MAX_FRAME_BYTES { return Err(McpTransportError::FrameTooLarge); }
              inbound.send(Ok(raw)).await.map_err(|_| McpTransportError::CloseFailed)?;
          }
      }
  }
  guard.kill_and_reap_if_needed().await?;
  ```

  Add this actual fixture target to `src-tauri/Cargo.toml`:

  ```toml
  [[bin]]
  name = "quantix-mcp-server-fixture"
  path = "tests/support/mcp_server_fixture.rs"
  required-features = ["runtime-fixture"]
  test = false
  bench = false
  ```

  `mcp_server_fixture.rs` must define `fn main()` and dispatch only the named `McpStdioFixtureMode` values. Verify the target exists with `cargo build --manifest-path src-tauri/Cargo.toml --bin quantix-mcp-server-fixture --features runtime-fixture` before running the integration harness.

- [ ] **Step 5: Run GREEN and commit.**

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_transport::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test mcp_stdio --features runtime-fixture
  cargo build --manifest-path src-tauri/Cargo.toml --bin quantix-mcp-server-fixture --features runtime-fixture
  git add src-tauri/Cargo.toml src-tauri/src/process_supervisor.rs src-tauri/src/lib.rs src-tauri/src/mcp_transport.rs src-tauri/tests/support/mcp_server_fixture.rs src-tauri/tests/mcp_stdio.rs
  git commit -m "feat: supervise local MCP stdio"
  ```

### Task 4: Add the Exact Guarded Remote Connector

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/mcp_transport.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/tests/support/mcp_http_fixture.rs`
- Create: `src-tauri/tests/mcp_http.rs`

**Interfaces:**

```rust
pub(crate) struct GuardedRemoteMcpConfig {
    pub connection_id: String,
    pub url: Url,
    pub destination_fingerprint: String,
    pub max_sse_event_bytes: usize,
    pub request_deadline: Duration,
    pub allow_stateless: bool,
}

pub(crate) struct ResolvedMcpDestination {
    pub url: Url,
    pub addresses: Vec<std::net::IpAddr>,
    pub fingerprint: String,
}

pub(crate) struct ConnectedMcpPeer {
    pub address: std::net::IpAddr,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum McpHttpError {
    #[error("MCP destination rejected")]
    DestinationRejected,
    #[error("MCP connection failed")]
    ConnectFailed,
    #[error("MCP TLS validation failed")]
    TlsFailed,
    #[error("MCP HTTP request failed")]
    RequestFailed,
    #[error("MCP response exceeded its byte limit")]
    ResponseTooLarge,
    #[error("MCP SSE event exceeded its byte limit")]
    SseEventTooLarge,
}

#[derive(Clone)]
pub(crate) struct GuardedMcpTcpConnector {
    expected: Arc<ResolvedMcpDestination>,
    connect_timeout: Duration,
}

impl tower_service::Service<http::Uri> for GuardedMcpTcpConnector {
    type Response = hyper_util::rt::TokioIo<tokio::net::TcpStream>;
    type Error = McpError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>>;
    fn call(&mut self, uri: http::Uri) -> Self::Future;
}

#[derive(Clone)]
pub(crate) struct QuantixMcpHttpClient {
    configured_uri: Arc<str>,
    client: hyper_util::client::legacy::Client<
        hyper_rustls::HttpsConnector<GuardedMcpTcpConnector>,
        http_body_util::Full<bytes::Bytes>,
    >,
    response_limit: usize,
    request_deadline: Duration,
}

impl rmcp::transport::streamable_http_client::StreamableHttpClient
    for QuantixMcpHttpClient
{
    type Error = McpHttpError;
    async fn post_message(
        &self, uri: Arc<str>, message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>, auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>>;
    async fn post_message_with_max_sse_event_size(
        &self, uri: Arc<str>, message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>, auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>, max_sse_event_size: usize,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>>;
    async fn delete_session(
        &self, uri: Arc<str>, session_id: Arc<str>,
        auth_header: Option<String>, custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), StreamableHttpError<Self::Error>>;
    async fn get_stream(
        &self, uri: Arc<str>, session_id: Option<Arc<str>>,
        last_event_id: Option<String>, auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, StreamableHttpError<Self::Error>>;
    async fn get_stream_with_max_sse_event_size(
        &self, uri: Arc<str>, session_id: Option<Arc<str>>,
        last_event_id: Option<String>, auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>, max_sse_event_size: usize,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, StreamableHttpError<Self::Error>>;
}

pub(crate) type GuardedRemoteMcpTransport =
    rmcp::transport::streamable_http_client::StreamableHttpClientTransport<
        QuantixMcpHttpClient,
    >;

#[cfg(feature = "runtime-fixture")]
pub enum McpHttpFixtureMode {
    PublicHttps,
    MixedResolution,
    ConnectedPeerDrift,
    Redirect,
    OversizedEvent,
    Unauthorized,
    Forbidden,
    SessionExpired,
}
#[cfg(feature = "runtime-fixture")]
pub struct McpHttpFixtureResult {
    pub terminal_code: String,
    pub http_bytes_before_peer_validation: u64,
    pub request_count: u32,
}
#[cfg(test)]
struct FixtureHttpServer {
    url: Url,
    request_count: Arc<AtomicU32>,
    bytes_received: Arc<AtomicU64>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}
#[cfg(test)]
struct FixtureDnsResolver { answers: Vec<std::net::IpAddr> }
#[cfg(test)]
struct FixtureTlsPeer { address: std::net::IpAddr, dns_name: String }
#[cfg(feature = "runtime-fixture")]
impl QuantixHost {
    pub async fn verify_mcp_http_fixture(
        &self,
        mode: McpHttpFixtureMode,
    ) -> Result<McpHttpFixtureResult, TenderCommandError>;
}

pub(crate) fn validate_remote_config(
    config: &McpConnectionConfig,
) -> Result<(), McpError>;

pub(crate) async fn resolve_and_validate_destination(
    url: &Url,
    expected_fingerprint: &str,
) -> Result<ResolvedMcpDestination, McpError>;

pub(crate) async fn validate_connected_peer(
    expected: &ResolvedMcpDestination,
    connected: &ConnectedMcpPeer,
) -> Result<(), McpError>;

pub(crate) async fn connect_guarded_remote(
    config: GuardedRemoteMcpConfig,
    credential: Option<Zeroizing<String>>,
    cancellation: CancellationToken,
) -> Result<GuardedRemoteMcpTransport, McpError>;
```

- [ ] **Step 1: Write RED HTTP tests.**

  `mcp_transport.rs` contains private connector tests in `#[cfg(test)] mod tests`. `mcp_http_fixture.rs` is a fixture-only HTTPS server with no production imports. The public `mcp_http.rs` harness calls only `QuantixHost::verify_mcp_http_fixture`. Define source-module helpers `fn fixture_https_url() -> Url`, `fn fixture_remote_config() -> McpConnectionConfig`, `async fn fixture_http_server() -> FixtureHttpServer`, `fn fixture_dns_resolver() -> FixtureDnsResolver`, `fn fixture_tls_peer() -> FixtureTlsPeer`, and `fn assert_no_auth_leak(bytes: &[u8], secret: &str)`; define the public harness helper `async fn start_fixture_http_server() -> FixtureHttpServer`.

  Keep the failure matrix table-driven:

  ```rust
  #[tokio::test]
  async fn guarded_http_modes_fail_closed() {
      for (mode, code, requests, bytes) in [
          (McpHttpFixtureMode::MixedResolution, "MCP_DESTINATION_REJECTED", 0, 0),
          (McpHttpFixtureMode::ConnectedPeerDrift, "MCP_DESTINATION_REJECTED", 0, 0),
          (McpHttpFixtureMode::Redirect, "MCP_REDIRECT_REJECTED", 1, 0),
          (McpHttpFixtureMode::OversizedEvent, "MCP_SSE_EVENT_TOO_LARGE", 1, 0),
      ] {
          let result = fixture_host().verify_mcp_http_fixture(mode).await.unwrap();
          assert_eq!((result.terminal_code.as_str(), result.request_count, result.http_bytes_before_peer_validation), (code, requests, bytes));
      }
  }
  ```

- [ ] **Step 2: Run RED.**

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_transport::tests --features runtime-fixture -- --nocapture
  cargo test --manifest-path src-tauri/Cargo.toml --test mcp_http --features runtime-fixture -- --nocapture
  ```

  Expected RED failure: `GuardedMcpTcpConnector` and `QuantixMcpHttpClient` are unresolved, so the fixture cannot prove zero HTTP bytes before peer validation.

- [ ] **Step 3: Implement the guarded client behind rmcp's official generic client trait.**

  `GuardedMcpTcpConnector::call` must require the one configured HTTPS origin, resolve every address, reject forbidden/mixed/class-drift answers, canonical-sort the approved set, make exactly one TCP attempt to its first address, read `peer_addr()`, and run `validate_connected_peer` before returning the stream. Wrap it with `hyper_rustls::HttpsConnectorBuilder::new().with_native_roots()?.https_only().enable_http1().wrap_connector(...)`, so TLS hostname verification begins only after connected-peer validation. Build the Hyper client with `hyper_util::client::legacy::Client::builder(TokioExecutor::new()).pool_max_idle_per_host(0)` and HTTP/1 only. There is no proxy, redirect handler, connection fallback, or pool reuse.

  Implement all five methods required by the pinned `StreamableHttpClient` trait on `QuantixMcpHttpClient`, including both `*_with_max_sse_event_size` overrides. Every method rejects a URI unequal to `configured_uri`, constructs one bounded HTTP/1.1 request, applies exactly one vault-derived bearer or named-header placement, classifies 401/403/404/session outcomes into rmcp's typed `StreamableHttpError`, and caps the raw body before JSON/SSE parsing. For a direct JSON initialize response, pass the bounded raw JSON bytes to `validate_initialize_response_json` before constructing any rmcp typed response. For an SSE response, the bounded adapter must parse raw event framing itself, identify each complete `data:` payload, run the same initialize validator on an initialize-result payload, and only then construct/yield `Sse`; no initialize event may reach rmcp normalization first. Tests cover unsupported capabilities in both JSON and SSE initialize responses and assert zero post-validation MCP messages. The SSE adapter counts raw event bytes before parsing; it must not call rmcp's private helper or copy rmcp's worker/session state machine. Configure `StreamableHttpClientTransport::with_client` with `NeverRetry`, exact `max_sse_event_size`, persisted `allow_stateless`, and `reinit_on_expired_session(false)`.

  OAuth is explicitly outside this slice: remove the `auth` rmcp feature, OAuth enum variants, OAuth commands, OAuth credential storage, OAuth discovery, token exchange, refresh, and OAuth tests. A future OAuth slice must add its own reviewed plan and connection/fingerprint policy; this plan contains no OAuth behavior.

  Minimal connector behavior after the RED test:

  ```rust
  fn call(&mut self, uri: Uri) -> Self::Future {
      let expected = Arc::clone(&self.expected);
      Box::pin(async move {
          ensure_exact_https_origin(&uri, &expected.url)?;
          let address = resolve_single_approved_address(&expected).await?;
          let stream = TcpStream::connect(address).await.map_err(|_| McpError { code: "MCP_CONNECT_FAILED", retry_safe: false, required_user_action: "Review the MCP connection" })?;
          let peer_address = stream.peer_addr().map_err(|_| McpError { code: "MCP_CONNECT_FAILED", retry_safe: false, required_user_action: "Review the MCP connection" })?.ip();
          validate_connected_peer(&expected, &ConnectedMcpPeer { address: peer_address })?;
          Ok(TokioIo::new(stream))
      })
  }
  ```

  Add this actual fixture target to `src-tauri/Cargo.toml`:

  ```toml
  [[bin]]
  name = "quantix-mcp-http-fixture"
  path = "tests/support/mcp_http_fixture.rs"
  required-features = ["runtime-fixture"]
  test = false
  bench = false
  ```

  `mcp_http_fixture.rs` must define `fn main()` and dispatch only `McpHttpFixtureMode`; verify it with `cargo build --manifest-path src-tauri/Cargo.toml --bin quantix-mcp-http-fixture --features runtime-fixture`.

- [ ] **Step 4: Run GREEN and commit.**

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_transport::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test mcp_http --features runtime-fixture
  cargo build --manifest-path src-tauri/Cargo.toml --bin quantix-mcp-http-fixture --features runtime-fixture
  git add src-tauri/Cargo.toml src-tauri/src/mcp_transport.rs src-tauri/src/lib.rs src-tauri/tests/support/mcp_http_fixture.rs src-tauri/tests/mcp_http.rs
  git commit -m "feat: guard remote MCP peers"
  ```

### Task 5: Connect, Discover, and Reject Every Unsupported MCP Surface

**Files:**
- Create: `src-tauri/src/mcp_client.rs`
- Create: `src-tauri/tests/mcp_client.rs`
- Modify: `src-tauri/src/mcp_contract.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**

```rust
pub(crate) enum QuantixMcpTransport {
    Stdio(SupervisedMcpStdioTransport),
    Remote(GuardedRemoteMcpTransport),
}

pub(crate) struct StrictMcpClientHandler {
    violation_tx: tokio::sync::mpsc::Sender<McpError>,
}

type RunningMcpService = rmcp::service::RunningService<
    rmcp::RoleClient,
    StrictMcpClientHandler,
>;

pub(crate) struct QuantixMcpClient {
    service: Option<RunningMcpService>,
    connection: McpConnectionConfig,
    metadata: McpServerMetadata,
    discovered_tools: Vec<McpToolRecord>,
    cancellation: CancellationToken,
}

impl QuantixMcpClient {
    pub(crate) async fn connect(
        connection_id: &str,
        store: &McpStore,
        cancellation: CancellationToken,
    ) -> Result<Self, McpError>;
    pub(crate) fn metadata(&self) -> &McpServerMetadata;
    pub(crate) async fn discover_tools(&self) -> Result<Vec<McpToolRecord>, McpError>;
    pub(crate) async fn close(&mut self) -> Result<(), McpError>;
}

impl Drop for QuantixMcpClient {
    fn drop(&mut self);
}

pub(crate) enum SingleRoundToolResult {
    Completed(rmcp::model::CallToolResult),
    InputRequired,
    Task,
}
pub(crate) struct ValidatedMcpToolResult {
    pub structured_content: Value,
    pub result_sha256: String,
}
pub(crate) fn validate_single_round_tool_result(
    result: SingleRoundToolResult,
    expected_output_schema_json: &str,
    maximum_canonical_bytes: usize,
) -> Result<ValidatedMcpToolResult, McpError>;
```

- [ ] **Step 1: Add RED capability-boundary tests in the source module.**

  `mcp_client.rs` capability tests must be separate from handler/message tests. The capability group calls `validate_initialize_response_json` on raw initialize bytes and asserts that malformed/unsupported capability JSON fails before `ServerJsonRpcMessage` normalization. It defines `fn fixture_capability(kind: &str) -> ServerCapabilities`, `fn fixture_tool(name: &str) -> Tool`, and `fn fixture_initialize() -> InitializeResult`; it does not implement or invoke `ClientHandler`.

  ```rust
  #[test]
  fn capability_group_never_constructs_a_handler() {
      let raw = fixture_initialize_json_with_capability("sampling");
      assert_eq!(validate_initialize_response_json(&raw).unwrap_err().code, "MCP_CAPABILITY_NOT_ALLOWED");
  }
  ```

  Run:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_client::capability_tests --features runtime-fixture -- --nocapture
  ```

  Expected RED failure: `validate_initialize_response_json` is not yet connected to the client handshake.

- [ ] **Step 2: Add RED strict-handler tests in a separate source module.**

  `#[cfg(test)] mod handler_tests` tests only `StrictMcpClientHandler`: roots/sampling/elicitation/custom requests return method-not-found; task/subscription/progress/list-change notifications send a violation and cancel; no capability fixture is constructed here. Define `fn fixture_unknown_request() -> ServerRequest` and `fn fixture_unknown_notification() -> ServerNotification`.

  ```rust
  #[tokio::test]
  async fn unsupported_server_request_records_violation() {
      let (handler, mut violations) = fixture_handler();
      let error = handler.handle_request(fixture_unknown_request(), fixture_context()).await.unwrap_err();
      assert_eq!(error.code, ErrorCode::METHOD_NOT_FOUND);
      assert_eq!(violations.recv().await.unwrap().code, "MCP_SERVER_REQUEST_NOT_ALLOWED");
  }
  ```

  Expected RED failure: `StrictMcpClientHandler::handle_request` currently has no policy-specific violation result or cancellation signal.

- [ ] **Step 3: Add RED wire-message tests in a third source module and public harness.**

  `#[cfg(test)] mod message_tests` defines `fn fixture_input_required() -> SingleRoundToolResult`, `fn fixture_task_result() -> SingleRoundToolResult`, and `fn fixture_tool_result() -> SingleRoundToolResult`. Test the pure `validate_single_round_tool_result`: InputRequired/task are terminal policy violations; structured content is mandatory, canonical JSON must match the reviewed output schema and byte/depth limits, `is_error` becomes a typed redacted failure, and every non-empty text/image/audio/resource content block is rejected in this initial structured-only slice. Task 5 performs no wire tool call. `src-tauri/tests/mcp_client.rs` uses only public fixture/Host commands for stdio/HTTP round trips, malformed responses, protocol mismatch, and cancellation; it never imports private rmcp client types or private Quantix modules.

  ```rust
  #[test]
  fn single_round_result_classifier_is_table_driven() {
      for (result, code) in [
          (fixture_input_required(), "MCP_MRTR_NOT_ALLOWED"),
          (fixture_task_result(), "MCP_TASKS_NOT_ALLOWED"),
      ] {
          assert_eq!(validate_single_round_tool_result(result, "{\"type\":\"object\"}", 4096).unwrap_err().code, code);
      }
  }
  ```

  Expected RED failure: `validate_single_round_tool_result` is unresolved and no terminal classifier exists.

- [ ] **Step 4: Run all RED groups.**

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_client::capability_tests --features runtime-fixture -- --nocapture
  cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_client::handler_tests --features runtime-fixture -- --nocapture
  cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_client::message_tests --features runtime-fixture -- --nocapture
  cargo test --manifest-path src-tauri/Cargo.toml --test mcp_client --features runtime-fixture -- --nocapture
  ```

- [ ] **Step 5: Implement persisted-config connection and lifecycle ownership.**

  `QuantixMcpClient::connect` must call `McpStore::load_connection(connection_id)`, resolve the configured credential reference in memory, construct the selected supervised/guarded transport, and retain the `RunningMcpService` in `service: Some(...)`. The raw initialize response is intercepted and passed to `validate_initialize_response_json` before rmcp typed normalization. Use `serve_client_with_ct` with explicit `ClientInfo` protocol and `ClientCapabilities::default()`. Before returning, call `peer.list_all_tools()` exactly once, validate/persist the canonical catalogue through the borrowed `McpStore`, capture its returned discovery revision, cache the resulting records in `discovered_tools`, and create `McpServerMetadata` containing connection ID, exact `2026-07-28`, server name/version, capabilities hash, tools hash, and that revision. `discover_tools()` returns only a clone of this cache and performs no second network request; `metadata()` returns the secret-free metadata. `close` owns the only lifecycle shutdown path and awaits `RunningService::close`; `Drop` cancels if the caller forgot to close. Override all `ClientHandler` request/notification hooks: return method-not-found for roots/sampling/elicitation/custom requests, record and cancel on task/subscription/progress/list-change notifications, and fail the service on unknown messages. Task 5 defines no wire tool call. Task 6 alone adds one private `call_tool_once_wire` and must pass its result through `validate_single_round_tool_result`.

  Minimal connection sequence:

  ```rust
  let config = store.load_connection(connection_id)?;
  let transport = build_transport(&config, store, cancellation.clone()).await?;
  let handler = StrictMcpClientHandler::new(cancellation.clone());
  let service = handler.serve_with_ct(transport, cancellation.clone()).await?;
  let metadata = metadata_from_validated_peer(service.peer_info().unwrap(), &config)?;
  let discovered = service.peer().list_all_tools().await?;
  let records = store.append_discovery(config.connection_id.clone(), metadata.clone(), discovered).await?;
  Ok(Self { service: Some(service), connection: config, metadata, discovered_tools: records, cancellation })
  ```

- [ ] **Step 6: Run GREEN and commit.**

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_client::capability_tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_client::handler_tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_client::message_tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test mcp_client --features runtime-fixture
  git add src-tauri/src/mcp_client.rs src-tauri/tests/mcp_client.rs src-tauri/src/mcp_contract.rs src-tauri/src/lib.rs
  git commit -m "feat: add strict MCP client lifecycle"
  ```

### Task 6: Govern Raw Dispatch Through Unforgeable Reservations

**Files:**
- Modify: `src-tauri/src/agent_runtime/permissions.rs`
- Modify: `src-tauri/src/agent_runtime.rs`
- Modify: `src-tauri/src/tender_store.rs`
- Modify: `src-tauri/src/tender_store/agent_records.rs`
- Modify: `src-tauri/src/mcp_client.rs`
- Create: `src-tauri/tests/mcp_execution.rs`
- Modify: `src-tauri/tests/runtime_readiness.rs`
- Modify: `src-tauri/tests/safe_updates.rs`

**Interfaces:**

```rust
pub(crate) struct McpToolCallRequest {
    pub tender_id: String,
    pub connection_id: String,
    pub execution_revision: u64,
    pub server_tool_name: String,
    pub call_id: String,
    pub arguments: Value,
    pub run_id: String,
    pub approval_id: Option<String>,
    pub idempotency_key: String,
}

pub(crate) struct McpReservation {
    token: [u8; 32],
    tender_id: String,
    run_id: String,
    reservation_row_key: String,
}

pub(crate) struct McpDispatchPayload {
    pub tender_id: String,
    pub connection_id: String,
    pub execution_revision: u64,
    pub tool_id: String,
    pub call_id: String,
    pub definition_version: u64,
    pub server_definition_sha256: String,
    pub policy_sha256: String,
    pub run_id: String,
    pub approval_id: Option<String>,
    pub grant_sha256: String,
    pub idempotency_key: String,
    pub canonical_arguments_json: String,
}

pub(crate) struct McpToolCallResult {
    pub structured_content: Value,
    pub result_sha256: String,
}

impl QuantixHost {
    pub(crate) async fn reserve_mcp_call(
        &self,
        request: &McpToolCallRequest,
        grant: &PermissionGrant,
        approval: Option<&OneRunAccessGrant>,
    ) -> Result<McpReservation, McpError>;

    pub(crate) async fn execute_reserved_mcp_call(
        &self,
        reservation: McpReservation,
        cancellation: CancellationToken,
    ) -> Result<McpToolCallResult, McpError>;

    pub(crate) async fn reload_reserved_dispatch(
        &self,
        reservation: &McpReservation,
    ) -> Result<McpDispatchPayload, McpError>;
}

impl QuantixMcpClient {
    async fn call_tool_once_wire(
        &self,
        reservation: &McpReservation,
        payload: &McpDispatchPayload,
    ) -> Result<rmcp::model::CallToolResult, McpError>;
}
```

The run-specific reservation is stored only in the Tender Store that owns `run_id`, never in installation SQLite:

```sql
CREATE TABLE mcp_dispatch_reservations (
  reservation_row_key TEXT PRIMARY KEY CHECK (length(reservation_row_key) = 64),
  token_sha256 TEXT NOT NULL CHECK (length(token_sha256) = 64),
  run_id TEXT NOT NULL,
  call_id TEXT NOT NULL,
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
  payload_sha256 TEXT NOT NULL CHECK (length(payload_sha256) = 64),
  idempotency_key TEXT NOT NULL UNIQUE CHECK (length(idempotency_key) = 64),
  status TEXT NOT NULL CHECK (status IN ('reserved','dispatched','completed','denied','indeterminate')),
  result_json TEXT CHECK (result_json IS NULL OR json_valid(result_json)),
  result_sha256 TEXT CHECK (result_sha256 IS NULL OR length(result_sha256) = 64),
  created_at TEXT NOT NULL,
  completed_at TEXT,
  FOREIGN KEY (run_id) REFERENCES agent_runs(run_id),
  UNIQUE (run_id, call_id),
  CHECK (
    (status IN ('completed','denied') AND result_json IS NOT NULL AND result_sha256 IS NOT NULL AND completed_at IS NOT NULL)
    OR (status IN ('reserved','dispatched','indeterminate') AND result_json IS NULL AND result_sha256 IS NULL)
  )
) STRICT;
```

Change the fresh Tender schema from Slice 1's version `46` to `47`, update every exact Tender-schema fixture/assertion in `runtime_readiness.rs` and `safe_updates.rs`, and add no compatibility migration. `payload_json` may contain classified Tender tool arguments because it remains inside that Tender's canonical store and its normal Tender backup boundary; installation SQLite, diagnostics, audit summaries, non-Tender exports, and acceptance evidence receive hashes/redacted metadata only.

`call_tool_once_wire` is added only in this task, remains private, requires both the private-field reservation and the canonical payload reloaded from its persisted row, and is not re-exported. It verifies the payload's tender/run/row/tool/idempotency identities against the reservation immediately before serializing `canonical_arguments_json`. A completed/denied duplicate reloads the bounded canonical `result_json`/denial from this Tender row and returns it without contacting the server; hashes alone are never presented as the original result. Unit tests may construct a reservation only inside the same source module; production callers cannot construct or bypass it.

- [ ] **Step 1: Add RED permission/idempotency/audit tests.**

  Define source-module helpers `fn fixture_permission_grant() -> PermissionGrant`, `fn fixture_mcp_tool_record() -> McpToolRecord`, `fn fixture_call_request() -> McpToolCallRequest`, `fn fixture_approval() -> OneRunAccessGrant`, `fn fixture_idempotency_key() -> String`, `fn fixture_dispatch_payload() -> McpDispatchPayload`, and `fn assert_audit_payload_redacted(payload: &Value, secret: &str)` inside the private permission/client modules. Test exact connection/tool/revision, schema validation, data scopes/classifications, secret denial, side-effect approval, per-tool and cumulative budgets, duplicate idempotency returning the first result, timeout/cancellation/crash, uncertain outcome, and audit sequence. `src-tauri/tests/mcp_execution.rs` drives these cases only through public Agent Run/permission commands on `QuantixHost` plus the named fixture server; it never imports `McpReservation`, `McpToolCallRequest`, or raw client methods.

  Use a table-driven authorization test and an explicit duplicate assertion:

  ```rust
  #[test]
  fn reservation_cases_fail_closed() {
      for (mut request, expected) in [
          (fixture_call_request(), "MCP_TOOL_NOT_APPROVED"),
          ({ let mut value = fixture_call_request(); value.tender_id = "other-tender".into(); value }, "MCP_TENDER_SCOPE_MISMATCH"),
      ] {
          let error = fixture_host().reserve_for_test(&mut request).unwrap_err();
          assert_eq!(error.code, expected);
      }
  }

  #[test]
  fn duplicate_idempotency_returns_the_recorded_result() {
      let fixture = fixture_host_with_completed_reservation();
      let result = fixture.host.execute_duplicate_for_test(fixture.reservation()).unwrap();
      assert_eq!(result.result_sha256, fixture.expected_result_sha256);
      assert_eq!(fixture.server_call_count(), 1);
  }
  ```

- [ ] **Step 2: Run RED.**

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib agent_runtime::permissions::tests --features runtime-fixture -- --nocapture
  cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_client::reservation_tests --features runtime-fixture -- --nocapture
  cargo test --manifest-path src-tauri/Cargo.toml --test mcp_execution --features runtime-fixture -- --nocapture
  cargo test --manifest-path src-tauri/Cargo.toml --test runtime_readiness --test safe_updates --features runtime-fixture mcp_dispatch_reservations -- --nocapture
  ```

  Expected RED failure: the Tender Store has no `mcp_dispatch_reservations` table and `reserve_for_test` cannot produce a Tender-bound reservation.

- [ ] **Step 3: Make raw wire dispatch private and reservation unforgeable.**

  `QuantixMcpClient` must not expose `call_tool`, `send_request`, or a raw `Peer` outside `mcp_client.rs`. Only `execute_reserved_mcp_call` may invoke private `call_tool_once_wire`; the reservation privately binds the validated `tender_id`, `run_id`, token, and row key after a transaction on that exact Tender Store atomically inserts the row above and stores canonical `McpDispatchPayload` JSON, grant/approval hash, tool definition hash/version, and quota reservation. `execute_reserved_mcp_call` opens only the reservation's validated Tender Store, calls `reload_reserved_dispatch`, canonical-parses the row by private key, verifies token/tender/run/status, and ignores any caller-owned arguments; it then passes that exact payload with the reservation to `call_tool_once_wire`. A completed/denied duplicate returns the recorded canonical result/denial without contacting the server. Validate arguments before reserve, pass new results through Task 5's structured-only validator before storing/model visibility, bound `result_json` by the tool output quota, and classify post-dispatch failures as terminal or indeterminate without retry.

  Minimal reservation transaction:

  ```rust
  let payload_json = canonical_json(&payload)?;
  let row_key = random_hex_32();
  transaction.execute(
      "INSERT INTO mcp_dispatch_reservations(reservation_row_key, token_sha256, run_id, payload_json, payload_sha256, idempotency_key, status, created_at)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'reserved', ?7)",
      params![row_key, sha256(token), payload.run_id, payload_json, sha256(payload_json), payload.idempotency_key, now],
  )?;
  Ok(McpReservation { token, tender_id, run_id, reservation_row_key: row_key })
  ```

- [ ] **Step 4: Run GREEN and commit.**

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib agent_runtime::permissions::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_client::reservation_tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test mcp_execution --test agent_runtime --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test runtime_readiness --test safe_updates --features runtime-fixture
  git add src-tauri/src/agent_runtime/permissions.rs src-tauri/src/agent_runtime.rs src-tauri/src/tender_store.rs src-tauri/src/tender_store/agent_records.rs src-tauri/src/mcp_client.rs src-tauri/tests/mcp_execution.rs src-tauri/tests/runtime_readiness.rs src-tauri/tests/safe_updates.rs
  git commit -m "feat: govern MCP dispatch through reservations"
  ```

### Task 7: Review, Approve, Update, and Revoke Versioned MCP Tools

**Files:**
- Modify: `src-tauri/src/mcp_store.rs`
- Modify: `src-tauri/src/setup.rs`
- Modify: `src-tauri/src/application_settings.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/tests/mcp_tool_catalogue.rs`

**Interfaces:**

```rust
pub struct ReviewMcpToolCommand {
    pub connection_id: String,
    pub tool_id: String,
    pub expected_catalogue_revision: u64,
    pub expected_definition_sha256: String,
    pub policy: McpToolPolicyInput,
    pub rationale: String,
}

pub struct ApproveMcpToolCommand {
    pub connection_id: String,
    pub tool_id: String,
    pub expected_catalogue_revision: u64,
    pub expected_definition_sha256: String,
    pub expected_policy_sha256: String,
}

pub struct UpdateMcpToolCommand {
    pub connection_id: String,
    pub tool_id: String,
    pub expected_catalogue_revision: u64,
    pub expected_definition_version: u64,
    pub expected_definition_sha256: String,
    pub expected_policy_sha256: String,
    pub replacement: McpToolPolicyInput,
}

pub struct RevokeMcpToolCommand {
    pub connection_id: String,
    pub tool_id: String,
    pub expected_catalogue_revision: u64,
    pub expected_definition_version: u64,
    pub expected_definition_sha256: String,
    pub expected_policy_sha256: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct McpToolPolicyInput {
    pub quantix_tool_name: String,
    pub required_capability: String,
    pub required_action: String,
    pub required_data_scopes: Vec<String>,
    pub allowed_data_classifications: Vec<DataClassification>,
    pub side_effect_class: ToolSideEffectClass,
    pub quota: TypedToolQuota,
    pub idempotency: ToolIdempotency,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct McpToolPolicyView {
    pub quantix_tool_name: String,
    pub required_capability: String,
    pub required_action: String,
    pub required_data_scopes: Vec<String>,
    pub allowed_data_classifications: Vec<DataClassification>,
    pub side_effect_class: ToolSideEffectClass,
    pub quota: TypedToolQuota,
    pub idempotency: ToolIdempotency,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct McpToolView {
    pub tool_id: String,
    pub server_tool_name: String,
    pub server_definition_sha256: String,
    pub policy_sha256: Option<String>,
    pub definition_version: u64,
    pub catalogue_revision: u64,
    pub state: String,
    pub policy: Option<McpToolPolicyView>,
    pub approved: bool,
}

impl QuantixHost {
    pub async fn review_mcp_tool(&self, command: ReviewMcpToolCommand) -> Result<McpToolView, TenderCommandError>;
    pub async fn approve_mcp_tool(&self, command: ApproveMcpToolCommand) -> Result<McpToolView, TenderCommandError>;
    pub async fn update_mcp_tool(&self, command: UpdateMcpToolCommand) -> Result<McpToolView, TenderCommandError>;
    pub async fn revoke_mcp_tool(&self, command: RevokeMcpToolCommand) -> Result<McpToolView, TenderCommandError>;
}
```

- [ ] **Step 1: Write the RED versioned-catalogue tests.**

  Define `fn fixture_discovered_tool() -> McpToolRecord`, `fn fixture_review_command() -> ReviewMcpToolCommand`, `fn fixture_approve_command() -> ApproveMcpToolCommand`, `fn fixture_update_command() -> UpdateMcpToolCommand`, `fn fixture_revoke_command() -> RevokeMcpToolCommand`, and `fn assert_tool_history_is_append_only(connection: &rusqlite::Connection, tool_id: &str)`. Add this concrete expected failure:

  ```rust
  #[tokio::test]
  async fn stale_definition_hash_cannot_approve_a_tool() {
      let fixture = fixture_host_with_discovered_tool();
      let mut command = fixture_approve_command();
      command.expected_definition_sha256 = "0".repeat(64);
      let error = fixture.host.approve_mcp_tool(command).await.unwrap_err();
      assert_eq!(error.code, TenderErrorCode::InvalidCommand);
  }
  ```

  A discovered record has `policy: None` and cannot be approved. `ReviewMcpToolCommand` must supply the first complete policy; approval requires state `Reviewed`, `policy: Some`, and matching server-definition/policy hashes. Test review/approval/policy-update/revoke, missing policy, stale catalogue revision, stale definition hash/version, invalid replacement policy, wrong connection, already-revoked/tombstoned tool, and separate tool IDs with colliding server names. Assert every mutation appends a version/event row and never SQL-updates or deletes an old definition. The Engineer cannot edit the server tool name or discovered input/output schemas; a changed server schema arrives only through rediscovery as a new unapproved definition hash/version.

- [ ] **Step 2: Run RED.**

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --test mcp_tool_catalogue --features runtime-fixture -- --nocapture
  ```

  Expected RED failure: `approve_mcp_tool` is unresolved and no append-only definition/event projection exists.

- [ ] **Step 3: Implement editable append/tombstone storage and exact CAS.**

  Replace the discovered-only `mcp_tools` shape with `mcp_tool_definitions` plus append-only `mcp_tool_events`. Each row stores the stable `server_definition_sha256` separately from optional `policy_sha256`. Review/approval/policy replacement/revoke changes only the policy hash/state; rediscovery may change only the server-definition hash and always clears policy approval until a new review. Every mutation appends a version/event carrying prior version, expected/new server and policy hashes, catalogue revision, actor, rationale, and timestamp. The active projection is a query over the newest non-tombstoned version; no contradictory immutable UPDATE/DELETE triggers are added. `UpdateMcpToolCommand` changes only Host policy, revalidates bounds/authority, and increments definition version/catalogue revision without changing the server hash. `RevokeMcpToolCommand` appends a revoked version and immediately prevents reservations.

  Minimal CAS append:

  ```rust
  let current = load_tool_revision(&transaction, &command.connection_id, &command.tool_id)?;
  if current.catalogue_revision != command.expected_catalogue_revision
      || current.server_definition_sha256 != command.expected_definition_sha256
  {
      return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
  }
  append_tool_version_and_event(&transaction, current.next_with_policy(policy)?)?;
  ```

- [ ] **Step 4: Wire named Host/Tauri commands and run GREEN.**

  Add only `review_mcp_tool`, `approve_mcp_tool`, `update_mcp_tool`, and `revoke_mcp_tool` to the Tauri allow-list. Bind every result to the exact definition hash/version and current catalogue revision. Run:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --test mcp_tool_catalogue --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --lib mcp_store::tests --features runtime-fixture
  git add src-tauri/src/mcp_store.rs src-tauri/src/setup.rs src-tauri/src/application_settings.rs src-tauri/src/lib.rs src-tauri/tests/mcp_tool_catalogue.rs
  git commit -m "feat: govern versioned MCP tool approvals"
  ```

### Task 8: Add Host Settings Commands and Generate Bindings

**Files:**
- Modify: `src-tauri/src/application_settings.rs`
- Modify: `src-tauri/src/ai/connections.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/bin/export_bindings.rs`
- Create: `src-tauri/tests/mcp_settings.rs`
- Create by generation: `src/bindings/McpConnectionInput.ts`
- Create by generation: `src/bindings/McpTransportKind.ts`
- Create by generation: `src/bindings/McpAuthPlacementInput.ts`
- Create by generation: `src/bindings/McpWriteOnlyCredential.ts`
- Create by generation: `src/bindings/McpConnectionView.ts`
- Create by generation: `src/bindings/McpTransportView.ts`
- Create by generation: `src/bindings/McpAuthPlacementView.ts`
- Create by generation: `src/bindings/McpToolView.ts`
- Create by generation: `src/bindings/McpToolPolicyView.ts`
- Create by generation: `src/bindings/InspectMcpConnectionsCommand.ts`
- Create by generation: `src/bindings/CreateMcpConnectionCommand.ts`
- Create by generation: `src/bindings/UpdateMcpConnectionCommand.ts`
- Create by generation: `src/bindings/TestMcpConnectionCommand.ts`
- Create by generation: `src/bindings/SetMcpConnectionEnabledCommand.ts`
- Create by generation: `src/bindings/DeleteMcpConnectionCommand.ts`
- Create by generation: `src/bindings/ReviewMcpToolCommand.ts`
- Create by generation: `src/bindings/ApproveMcpToolCommand.ts`
- Create by generation: `src/bindings/UpdateMcpToolCommand.ts`
- Create by generation: `src/bindings/RevokeMcpToolCommand.ts`
- Create by generation: `src/bindings/McpToolPolicyInput.ts`

**Interfaces:**

```rust
impl crate::ai::connections::SecretInput {
    pub(crate) fn take_for_host(
        &mut self,
    ) -> Result<crate::ai::vault::SecretString, AiConnectionError>;
}

pub struct McpConnectionView {
    pub connection_id: String,
    pub display_name: String,
    pub transport: McpTransportView,
    pub enabled: bool,
    pub execution_revision: u64,
    pub protocol_version: Option<String>,
    pub destination_fingerprint: Option<String>,
    pub status: String,
    pub tools: Vec<McpToolView>,
}

pub enum McpTransportView {
    Stdio { executable: String, arguments: Vec<String>, cwd: String },
    StreamableHttp {
        url: String,
        auth: McpAuthPlacementView,
        destination_fingerprint: String,
        allow_stateless: bool,
    },
}
pub enum McpAuthPlacementView {
    None,
    AuthorizationBearer,
    Header { name: String },
}

pub enum McpConnectionInput {
    Stdio {
        executable: String,
        arguments: Vec<String>,
        cwd: String,
    },
    StreamableHttp {
        url: String,
        auth: McpAuthPlacementInput,
        allow_stateless: bool,
    },
}

pub enum McpAuthPlacementInput {
    None,
    AuthorizationBearer,
    Header { name: String },
}

pub struct CreateMcpConnectionCommand {
    pub display_name: String,
    pub input: McpConnectionInput,
    pub credential: Option<McpWriteOnlyCredential>,
}
#[derive(Deserialize, TS, Zeroize, ZeroizeOnDrop)]
pub struct McpWriteOnlyCredential {
    #[ts(type = "string")]
    pub value: crate::ai::connections::SecretInput,
}
impl std::fmt::Debug for McpWriteOnlyCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("McpWriteOnlyCredential([REDACTED])")
    }
}
pub struct InspectMcpConnectionsCommand {}
pub struct UpdateMcpConnectionCommand {
    pub connection_id: String,
    pub display_name: String,
    pub input: McpConnectionInput,
    pub credential: Option<McpWriteOnlyCredential>,
    pub expected_revision: u64,
}
pub struct TestMcpConnectionCommand { pub connection_id: String, pub expected_revision: u64 }
pub struct SetMcpConnectionEnabledCommand { pub connection_id: String, pub expected_revision: u64, pub enabled: bool }
pub struct DeleteMcpConnectionCommand { pub connection_id: String, pub expected_revision: u64 }

impl QuantixHost {
    pub fn inspect_mcp_connections(&self) -> Result<Vec<McpConnectionView>, TenderCommandError>;
    pub async fn create_mcp_connection(&self, command: CreateMcpConnectionCommand) -> Result<McpConnectionView, TenderCommandError>;
    pub async fn update_mcp_connection(&self, command: UpdateMcpConnectionCommand) -> Result<McpConnectionView, TenderCommandError>;
    pub async fn test_mcp_connection(&self, command: TestMcpConnectionCommand) -> Result<McpConnectionView, TenderCommandError>;
    pub fn set_mcp_connection_enabled(&self, command: SetMcpConnectionEnabledCommand) -> Result<McpConnectionView, TenderCommandError>;
    pub fn delete_mcp_connection(&self, command: DeleteMcpConnectionCommand) -> Result<(), TenderCommandError>;
}
```

- [ ] **Step 1: Write RED Host command tests.**

  Reuse the existing `crate::ai::connections::SecretInput` and add only the crate-private consuming `take_for_host` transfer above; its returned `SecretString` remains zeroizing/redacted and never crosses IPC. Do not create a raw `String` credential field. Define source-module helpers `fn fixture_stdio_command() -> CreateMcpConnectionCommand`, `fn fixture_remote_command() -> CreateMcpConnectionCommand`, `fn fixture_write_only_credential() -> McpWriteOnlyCredential`, and `fn assert_secret_free_view(view: &McpConnectionView, secret: &str)`. Assert exact command validation, revision conflicts, Host-generated opaque credential references, credential write-only behavior, no credential or vault key in errors/views/SQLite, no marketplace fields, no provider-native MCP fields, and discovered tools remain unapproved until a separate Host approval. `delete_mcp_connection` rejects any nonterminal run/reservation, appends tombstone versions/events for every tool, marks the connection tombstoned without deleting its audit metadata, then removes only its exact vault credential; no foreign-key orphan or historical row is deleted.

  Add one command round-trip test and one TypeScript invocation test:

  ```rust
  #[tokio::test]
  async fn create_command_accepts_write_only_secret_without_returning_it() {
      let command = fixture_stdio_command();
      let view = fixture_host().create_mcp_connection(command).await.unwrap();
      assert_secret_free_view(&view, "mcp-sentinel");
  }
  ```

  ```ts
  it('clears the credential after the named Host command settles', async () => {
    const form = renderMcpConnectionForm();
    await userEvent.type(form.credential, 'mcp-sentinel');
    await userEvent.click(form.submit);
    await waitFor(() => expect(form.credential).toHaveValue(''));
    expect(screen.queryByText('mcp-sentinel')).toBeNull();
  });
  ```

- [ ] **Step 2: Run RED.**

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --test mcp_settings --features runtime-fixture -- --nocapture
  ```

  Expected RED failure: the named Tauri commands, `SecretInput::take_for_host`, and generated `Mcp*` bindings are missing.

- [ ] **Step 3: Wire explicit Tauri commands and binding exports.**

  Add only named connection and tool commands to `generate_handler!`: `inspect_mcp_connections`, `create_mcp_connection`, `update_mcp_connection`, `test_mcp_connection`, `set_mcp_connection_enabled`, `delete_mcp_connection`, `review_mcp_tool`, `approve_mcp_tool`, `update_mcp_tool`, and `revoke_mcp_tool`. Do not add a generic invoke-MCP command. The Host converts `McpConnectionInput` plus the one-way credential into a persisted `McpConnectionConfig`, generates the opaque vault credential reference, and never accepts a renderer-supplied reference. Export `McpWriteOnlyCredential` only as a command-input binding; it is never included in a view, result, error, audit payload, or persisted JSON. The generated TypeScript files are produced only by the repository generator.

  Minimal command transfer:

  ```rust
  let secret = command.credential
      .as_mut()
      .map(|input| input.value.take_for_host())
      .transpose()?;
  let credential_ref = secret.as_ref().map(|_| random_identifier());
  let config = normalize_connection_input(command.input, credential_ref.clone())?;
  store.save_connection(&config, Some(command.expected_revision))?;
  if let (Some(reference), Some(secret)) = (credential_ref, secret) {
      vault.put_mcp_credential(reference, secret)?;
  }
  ```

- [ ] **Step 4: Regenerate and run GREEN.**

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --test mcp_settings --features runtime-fixture
  npm test
  git add src-tauri/src/application_settings.rs src-tauri/src/ai/connections.rs src-tauri/src/lib.rs src-tauri/src/bin/export_bindings.rs src-tauri/tests/mcp_settings.rs src/bindings
  git commit -m "feat: expose credential-free MCP settings commands"
  ```

### Task 9: Implement the Renderer Surface and Pre-Integration Acceptance Gate

**Files:**
- Modify: `src/ApplicationSettings.tsx`
- Modify: `src/ApplicationSettings.test.tsx`
- Modify: `src/quantixHost.ts`
- Modify: `src/quantixHost.test.ts`
- Modify: `src-tauri/tests/mcp_settings.rs`
- Modify: `src-tauri/tests/agent_runtime.rs`
- Modify: `src-tauri/tests/support/mcp_server_fixture.rs`
- Modify: `src-tauri/src/acceptance.rs`
- Modify: `src-tauri/src/bin/product_acceptance.rs`
- Modify: `src-tauri/src/bin/export_bindings.rs`
- Modify: `package.json`
- Modify: `src-tauri/tests/release_configuration.rs`
- Create by generation: `src/bindings/McpRuntimeEvidence.ts`
- Create by generation: `src/bindings/McpAcceptanceSelection.ts`
- Create by generation: `src/bindings/RunMcpGovernedCallCommand.ts`
- Create by generation: `src/bindings/RecordMcpLiveQualificationRunCommand.ts`
- Create by generation: `src/bindings/McpLiveQualificationRun.ts`

**Interfaces:**

- Consume only generated `Mcp*` bindings and named Tauri commands from Task 8.
- Produce the beginner-facing installed-connection list, add/edit/test/enable/delete actions, plus review/approve/update/revoke flows for each discovered MCP tool. No marketplace, package install, raw protocol, executable shell, URL credential, OAuth, or provider-native MCP surface may appear.

```rust
pub struct McpRuntimeEvidence {
    pub sdk_version: String,
    pub protocol_version: String,
    pub tender_id: String,
    pub run_id: String,
    pub connection_id: String,
    pub execution_revision: u64,
    pub transport: McpTransportKind,
    pub destination_fingerprint: Option<String>,
    pub server_name: String,
    pub server_version: String,
    pub capabilities_sha256: String,
    pub tool_id: String,
    pub tool_definition_version: u64,
    pub server_definition_sha256: String,
    pub policy_sha256: String,
    pub grant_sha256: String,
    pub approval_id: Option<String>,
    pub reservation_row_key: String,
    pub call_id: String,
    pub idempotency_key_sha256: String,
    pub result_sha256: String,
    pub automatic_retry_count: u32,
    pub terminal_code: String,
}

pub struct McpAcceptanceSelection {
    pub tender_id: String,
    pub run_id: String,
    pub reservation_row_key: String,
    pub call_id: String,
    pub connection_id: String,
    pub execution_revision: u64,
    pub tool_id: String,
    pub tool_definition_version: u64,
    pub server_definition_sha256: String,
    pub policy_sha256: String,
}

pub struct RunMcpGovernedCallCommand {
    pub tender_id: String,
    pub connection_id: String,
    pub tool_id: String,
    pub opted_in: bool,
}

pub struct RecordMcpLiveQualificationRunCommand {
    pub opted_in: bool,
    pub application_artifact_path: String,
    pub application_resource_directory_path: String,
    pub application_uninstaller_path: String,
    pub deterministic_acceptance_record_sha256: String,
    pub selection: McpAcceptanceSelection,
}

pub struct McpLiveQualificationRun {
    pub run_id: String,
    pub release_candidate_sha256: String,
    pub selection: McpAcceptanceSelection,
    pub evidence: McpRuntimeEvidence,
    pub outcome: ProductAcceptanceOutcome,
}
```

`McpRuntimeEvidence` is populated only from committed connection/tool/reservation/audit records and is stored as the mandatory evidence field of `McpLiveQualificationRun`. It contains no credential reference/value, raw URL auth, raw arguments/results, raw frames, or local executable arguments. Task 9 adds three explicit acceptance CLI modes/scripts: `acceptance:mcp-governed-call -- <home> <command.json>` accepts only an approved idempotent/no-side-effect tool whose input schema is exactly an empty object and executes it through a normal Host-created Agent Run, grant, reservation, and wire call; it prints only committed identities/hashes. `acceptance:mcp-selection -- <home> <tender-id> <run-id> <reservation-row-key> <call-id>` reloads that completed call and only then prints one non-secret `McpAcceptanceSelection`; `acceptance:mcp-live -- <home> <command.json>` records exactly that selection. Neither selection nor live mode auto-chooses among installed connections/tools, and the governed-call mode cannot invoke a general arbitrary-argument tool.

- [ ] **Step 1: Write RED renderer tests.**

  Test stdio fields (executable, arguments, cwd), remote URL/auth placement/fingerprint/`allow_stateless`, write-only credential clearing, loading/error states, revision conflicts, and the absence of raw credentials from rendered text and Tauri results. Add tests that discovered tools display as unapproved, approve requires the exact definition hash/revision, update appends a new version, and revoke disables the tool. Add acceptance tests that first execute one governed Agent Run/tool call, then construct `McpRuntimeEvidence`/`McpAcceptanceSelection` from that committed result. Reject selection before a completed call, missing/mismatched/stale tender/run/reservation/call identities, connection/tool/grant/reservation identities, require zero automatic retries, persist `McpLiveQualificationRun`, and scan serialized evidence for fixture secrets/raw arguments/results. The acceptance harness uses only public Host commands and fixture modes; it does not import private MCP APIs.

  Use this table-driven UI state test:

  ```ts
  it.each([
    ['discovered', { approve: true, revoke: false }],
    ['approved', { approve: false, revoke: true }],
    ['revoked', { approve: false, revoke: false }],
  ])('%s exposes only its valid tool actions', (state, actions) => {
    renderToolCard({ state, definitionSha256: 'a'.repeat(64), catalogueRevision: 3 });
    if (actions.approve) expect(screen.getByRole('button', { name: 'Approve' })).toBeInTheDocument();
    else expect(screen.queryByRole('button', { name: 'Approve' })).not.toBeInTheDocument();
    if (actions.revoke) expect(screen.getByRole('button', { name: 'Revoke' })).toBeInTheDocument();
    else expect(screen.queryByRole('button', { name: 'Revoke' })).not.toBeInTheDocument();
  });
  ```

- [ ] **Step 2: Run the renderer and evidence tests to confirm RED.**

  ```powershell
  npx vitest run src/ApplicationSettings.test.tsx src/quantixHost.test.ts
  npm run check
  cargo test --manifest-path src-tauri/Cargo.toml --test mcp_settings --test mcp_tool_catalogue --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --lib acceptance::tests --features runtime-fixture mcp_
  cargo test --manifest-path src-tauri/Cargo.toml --test release_configuration --features runtime-fixture mcp_
  ```

  Expected: FAIL on missing MCP Settings actions/bindings, `McpRuntimeEvidence` construction, and the requirement that `acceptance:mcp-selection` cannot produce a selection without a completed governed call.

- [ ] **Step 3: Implement the renderer and deterministic evidence before browser verification.**

  Use only typed Host calls; clear credential state on success, error, cancel, navigation, and unmount. Render exact tool hash/version/revision and expose Review, Approve, Update, and Revoke actions with stale-CAS errors. Do not expose raw schemas, credentials, OAuth, marketplace, or generic tool invocation. Populate `McpRuntimeEvidence` only from committed records and register its generated binding. Add exact package scripts `acceptance:mcp-governed-call`, `acceptance:mcp-selection`, and `acceptance:mcp-live`, plus matching Windows-native `product_acceptance.rs` modes that validate argument counts, require one completed governed call before selection, and never print secrets.

  Minimal Host-call mapping:

  ```ts
  const approve = () => invoke('approve_mcp_tool', {
    command: {
      connection_id: tool.connectionId,
      tool_id: tool.toolId,
      expected_catalogue_revision: tool.catalogueRevision,
      expected_definition_sha256: tool.serverDefinitionSha256,
      expected_policy_sha256: tool.policySha256,
    },
  });
  ```

  Minimal evidence rule: `record_mcp_live_qualification_run` must query the committed reservation/result row by `tender_id`, `run_id`, `reservation_row_key`, and `call_id`, reject any selection without a completed governed result, reject any evidence whose `automatic_retry_count != 0` or `result_sha256` differs, and reject serialized values containing the fixture secret.

- [ ] **Step 4: Run GREEN and commit the renderer/pre-integration acceptance gate.**

  ```powershell
  npm test
  npm run verify
  cargo test --manifest-path src-tauri/Cargo.toml --test mcp_settings --test mcp_tool_catalogue --test agent_runtime --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --lib acceptance::tests --features runtime-fixture mcp_
  cargo test --manifest-path src-tauri/Cargo.toml --test release_configuration --features runtime-fixture mcp_
  git add package.json src/ApplicationSettings.tsx src/ApplicationSettings.test.tsx src/quantixHost.ts src/quantixHost.test.ts src-tauri/src/application_settings.rs src-tauri/src/lib.rs src-tauri/src/acceptance.rs src-tauri/src/bin/product_acceptance.rs src-tauri/src/bin/export_bindings.rs src/bindings src-tauri/tests/mcp_settings.rs src-tauri/tests/mcp_tool_catalogue.rs src-tauri/tests/release_configuration.rs src-tauri/tests/agent_runtime.rs src-tauri/tests/support/mcp_server_fixture.rs
  git commit -m "feat: add MCP settings and tool review UI"
  ```

  This is the pre-integration gate. Scan exact application-home outputs, SQLite, logs, diagnostics, backups, exports, and renderer payloads for fixture credentials and authorization headers. Do not run browser/live verification until this gate is green. Keep the existing main development server running throughout.

### Task 10: Run Post-Integration Browser and Controlled Live Verification

**Files:**
- No production source changes; use the integrated files from Tasks 1-9.
- Evidence: the normal `%USERPROFILE%\.quantix` Application Home, without credentials or raw MCP traffic.

**Interfaces:**

- Consume the running integrated renderer, public Host commands, fixture binaries, and generated bindings.
- Produce browser verification output, deterministic MCP acceptance records, and one explicitly opted-in controlled live MCP record. If the operator cannot provide/approve a guarded endpoint, Slice 3 remains incomplete rather than silently recording “not requested.”

- [ ] **Step 1: Confirm the pre-integration gate is green.**

  Verify the Task 9 commit exists and rerun:

  ```powershell
  npm run verify
  cargo test --manifest-path src-tauri/Cargo.toml --test installation_schema --test mcp_stdio --test mcp_http --test mcp_client --test mcp_tool_catalogue --test mcp_settings --test mcp_execution --features runtime-fixture
  ```

  Expected failure: stop and report the first failing pre-integration gate; do not hide it with browser retries.

- [ ] **Step 2: Verify the existing main dev server and browser flow.**

  Poll the existing `npm run tauri dev` session. If it exited unexpectedly, restart that same command immediately and record the restart; never start a duplicate. Use the browser verification skill against the live renderer. Exercise add/test/edit/enable/delete connection flows and review/approve/update/revoke tool flows, including stale revision errors and write-only credential clearing.

- [ ] **Step 3: Record deterministic and opt-in live evidence.**

  Perform formal candidate evidence in a separate sanitized Windows account/VM; the original environment's main dev server remains running. In the sanitized account, install the unchanged candidate and use its normal Application Home. Construct and aggregate the deterministic command with concrete paths and values:

  ```powershell
  $acceptanceRoot = 'C:\QuantixAcceptance\mcp-sdk-client'
  $acceptanceHome = Join-Path $env:USERPROFILE '.quantix'
  $candidateRoot = Join-Path $acceptanceRoot 'candidate'
  $candidateExecutable = Join-Path $candidateRoot 'quantix.exe'
  $candidateResources = Join-Path $candidateRoot 'resources'
  $candidateUninstaller = Join-Path $candidateRoot 'uninstall.exe'
  $sourceRevision = (& git rev-parse HEAD).Trim()
  $deterministicCommandPath = Join-Path $acceptanceRoot 'mcp-deterministic.json'
  [IO.Directory]::CreateDirectory($acceptanceRoot) | Out-Null
  $deterministicCommand = [ordered]@{
      source_revision = $sourceRevision
      application_artifact_path = $candidateExecutable
      application_resource_directory_path = $candidateResources
      dependency_lock_path = (Resolve-Path -LiteralPath 'src-tauri\Cargo.lock').Path
  }
  [IO.File]::WriteAllText(
      $deterministicCommandPath,
      ($deterministicCommand | ConvertTo-Json -Compress),
      [Text.UTF8Encoding]::new($false)
  )
  npm run acceptance:deterministic -- $acceptanceHome $deterministicCommandPath
  if ($LASTEXITCODE -ne 0) { throw 'MCP deterministic acceptance failed' }
  $aggregateOutput = & npm run acceptance:aggregate -- $acceptanceHome $sourceRevision 2>&1
  $aggregateOutput | ForEach-Object { Write-Host $_ }
  if ($LASTEXITCODE -ne 0) { throw 'MCP deterministic aggregation failed' }
  $aggregateJson = $aggregateOutput | Where-Object { $_ -match '^\{' } | Select-Object -Last 1 | ConvertFrom-Json
  $recordHash = [string]$aggregateJson.manifest_sha256
  if ($recordHash -notmatch '^[0-9a-f]{64}$') { throw 'invalid deterministic record hash' }
  ```

  Completion requires one controlled public HTTPS MCP endpoint explicitly opted in by the release operator. Configure/test/review/approve it through the installed candidate's Settings UI in this same `$acceptanceHome` and confirm that exact connection/tool definition is Ready. If no endpoint is authorized, stop and leave the completion gate pending. Before generating a selection, execute exactly one governed Agent Run/tool call on that approved connection/tool in the formal candidate. The governed-call output is the only source for tender/run/reservation/call identity:

  ```powershell
  $governedCommandPath = Join-Path $acceptanceRoot 'mcp-governed-call.json'
  $governedCommand = [ordered]@{
      opted_in = $true
      tender_id = (Read-Host 'Enter the candidate Tender ID').Trim()
      connection_id = (Read-Host 'Enter the reviewed MCP connection ID').Trim()
      tool_id = (Read-Host 'Enter the approved MCP tool ID').Trim()
  }
  [IO.File]::WriteAllText(
      $governedCommandPath,
      ($governedCommand | ConvertTo-Json -Compress),
      [Text.UTF8Encoding]::new($false)
  )
  $governedOutput = & npm run acceptance:mcp-governed-call -- $acceptanceHome $governedCommandPath 2>&1
  $governedOutput | ForEach-Object { Write-Host $_ }
  if ($LASTEXITCODE -ne 0) { throw 'formal governed MCP Agent Run/tool call failed' }
  $call = $governedOutput | Where-Object { $_ -match '^\{' } | Select-Object -Last 1 | ConvertFrom-Json
  foreach ($field in @('tender_id','run_id','reservation_row_key','call_id','connection_id','tool_id')) {
      if ([string]::IsNullOrWhiteSpace([string]$call.$field)) { throw "governed MCP call omitted $field" }
  }
  $selectionOutput = & npm run acceptance:mcp-selection -- $acceptanceHome $call.tender_id $call.run_id $call.reservation_row_key $call.call_id 2>&1
  $selectionOutput | ForEach-Object { Write-Host $_ }
  if ($LASTEXITCODE -ne 0) { throw 'MCP acceptance selection is not currently approved' }
  $selection = $selectionOutput | Where-Object { $_ -match '^\{' } | Select-Object -Last 1 | ConvertFrom-Json
  $liveCommandPath = Join-Path $acceptanceRoot 'mcp-live.json'
  $liveCommand = [ordered]@{
      opted_in = $true
      application_artifact_path = $candidateExecutable
      application_resource_directory_path = $candidateResources
      application_uninstaller_path = $candidateUninstaller
      deterministic_acceptance_record_sha256 = $recordHash
      selection = $selection
  }
  [IO.File]::WriteAllText(
      $liveCommandPath,
      ($liveCommand | ConvertTo-Json -Compress),
      [Text.UTF8Encoding]::new($false)
  )
  npm run acceptance:mcp-live -- $acceptanceHome $liveCommandPath
  if ($LASTEXITCODE -ne 0) { throw 'opt-in MCP live acceptance failed' }
  ```

  OAuth and provider-native MCP remain disabled. Never include secrets, raw MCP frames, or raw tool arguments/results in evidence.

- [ ] **Step 4: Perform the final completion check and keep the dev server running.**

  Confirm the browser flow, deterministic records, fixture target builds, secret scans, no unsupported surfaces, no duplicate side effects, no automatic retry, and `npm run verify` all pass. Confirm the existing main development session is still alive. Do not stop it.

## Plan Completion Gate

Slice 3 is complete only when fresh installation schema 27 and Tender schema 47 fixtures pass; actual stdio/HTTP fixture Cargo targets build; raw initialize capability validation precedes rmcp typed normalization; concurrent supervised stdio send/receive and RunningService lifecycle cleanup pass; guarded remote resolved/connected-peer checks pass; versioned MCP tool review/approval/update/revoke CAS tests pass; reservations reload canonical persisted dispatch payloads from the owning Tender only; generated bindings are current; renderer pre-integration and post-integration browser/deterministic gates pass; one explicitly opted-in controlled live MCP run passes for the exact recorded connection/tool selection; `npm run verify` exits zero; no OAuth, marketplace, provider-native MCP, or MCP server bridge exists; the existing main development server is still running; and independent review finds no Critical or Important issue.
