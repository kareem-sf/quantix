# SDK-First General Provider Worker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the six non-Codex provider routes through one disposable, pinned Pydantic AI worker. The Rust Host remains the sole authority for credentials, selection, pricing, budgets, tools, canonical writes, cancellation, and recovery.

**Architecture:** Slice 1 supplies the SDK-first active-selection and immutable run-binding boundary. For one probe or turn, the Host obtains one CAS-checked vault projection, starts one exact `python.exe -I -m quantix_ai_worker` in an empty Agent Run workspace, completes strict version-1 JSONL, then destroys the whole child tree. Compatible routes use a connect-first guarded transport; a `base_url` alone is never authorization.

**Tech Stack:** Rust 1.97.1, Python 3.12.13, uv 0.12.2, Pydantic AI 2.33.0, OpenAI 3.3.1, Anthropic 1.0.0, google-genai 2.19.0, xAI SDK 1.19.0, Tokio, ProcessKit, SQLite, DPAPI.

**Spec:** `docs/superpowers/specs/2026-08-25-sdk-first-ai-runtime-cutover-design.md`

## Global Constraints

- Execute after `2026-08-25-sdk-first-codex-cutover.md` delivers the SDK-first selection/run-binding types and removes the old direct-provider production path.
- Keep the existing main dev server running throughout. Never stop it to ask a question, run tests, alter resources, or commit; Tauri may rebuild itself.
- The product has seven provider routes: the Slice 1 Codex managed-account lane plus exactly six general-worker routes. This plan implements only the six general routes: direct OpenAI Responses, OpenAI-compatible Chat Completions, direct Anthropic Messages, Anthropic-compatible Messages, Gemini Developer API, and native xAI.
- The only reasoning union is Slice 1's `AiReasoningSelection::{Unsupported, Effort { id: String }}`. No default exists. Every probe/turn carries it and is matched exactly to evidence.
- Pin all Python dependencies in `uv.lock`; install non-editably, use the exact venv, and never reuse OCR's venv.
- Disable OpenAI/Anthropic retries, Google retries, xAI gRPC retries, Pydantic model/output/tool retries, fallbacks, telemetry, ambient credentials, proxies, `.netrc`, ADC/cloud profiles, and user packages. Do not construct `FallbackModel`, LiteLLM, native provider tools, or a default model.
- The worker receives no Application Home, Tender, SQLite, source-package, embedding-index, or arbitrary user path. This is not an OS filesystem-isolation claim.
- Any failure after provider dispatch starts is terminal or indeterminate; no Provider Turn is automatically retried.
- Discovery, probe, streaming, and continuation clients are explicit. Compatible requests all use the same guard. Provider-native hosted tools remain out of scope.
- Do not run production builds. At every task boundary confirm the main dev server remains running.
- Before Task 13 touches the main app, the parent suite's explicitly approved Slice 2 Fresh-State Schema Cutover must archive the preceding home and recreate `%USERPROFILE%\.quantix`; no Slice 2 binary may open schema-25 installation data or pre-expansion Tender JSON.

## Slice 1 input contract

Slice 2 imports `ai::contract::ActiveAiConfiguration` unchanged; it does not invent or extend another execution-selection record. Slice 1 deletes `application_settings::ProviderReasoningSelection`. The following complete configuration is the only record copied into an immutable Agent Run binding:

```rust
pub enum AiReasoningSelection {
    Unsupported,
    Effort { id: String },
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
```

`Unsupported` is an explicit Engineer choice meaning the qualified model exposes no reasoning control; it is not a provider default. `Effort { id }` is the exact bounded option discovered and qualified for the selected route. Slice 1 already defines `AiPricingIdentity { snapshot_sha256 }`; Slice 2 sets `pricing_identity` to `Some` when a shipped or Engineer-approved price identity exists and may leave it `None` for an otherwise qualified route. A run with a monetary ceiling rejects `None` before dispatch; a non-monetary run may still use token/tool/time limits. Slice 2 never invents a second provider, reasoning, pricing-identity, or active-selection union.

---

### Task 0: Expand immutable budgets, usage, and committed receipts before worker protocol work

**Files:**

- Modify: `src-tauri/src/agent_runtime.rs`
- Modify: `src-tauri/src/agent_runtime/bootstrap_profile.rs`
- Modify: `src-tauri/src/agent_runtime/codex_protocol.rs`
- Modify: `src-tauri/src/agent_runtime/permissions.rs`
- Modify: `src-tauri/src/host.rs`
- Modify: `src-tauri/src/tender_store.rs`
- Modify: `src-tauri/src/tender_store/agent_records.rs`
- Modify: `src-tauri/src/tender_store/bid_decisions.rs`
- Modify: `src-tauri/src/tender_store/manager_intake.rs`
- Modify: `src-tauri/src/tender_store/team_composer.rs`
- Modify: `src-tauri/src/tender_store/tender_records.rs`
- Modify: `src-tauri/src/bin/export_bindings.rs`
- Modify: `src-tauri/tests/agent_runtime.rs`
- Modify: `src-tauri/tests/bid_decisions.rs`
- Create by generation: `src/bindings/MonetaryBudget.ts`
- Regenerate: `src/bindings/AgentResourceBudget.ts`
- Regenerate: `src/bindings/ProviderUsage.ts`

**Interfaces:**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MonetaryBudget {
    pub currency: String,          // uppercase ISO 4217, exactly three ASCII bytes
    pub maximum_microunits: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AgentResourceBudget {
    pub provider_turns: u32,
    pub duration_seconds: u32,
    pub output_bytes: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub tool_rounds: u32,
    pub monetary: Option<MonetaryBudget>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProviderUsage {
    pub input_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub reasoning_output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub context_window: Option<u64>,
    pub elapsed_milliseconds: Option<u64>,
    pub request_count: u32,
    pub tool_rounds: u32,
    pub provider_request_id: Option<String>,
    pub reported_model_id: Option<String>,
    pub currency: Option<String>,
    pub estimated_microunits: Option<u64>,
    pub rate_limit: Option<ProviderRateLimit>,
}
pub(crate) enum WorkerToolResolution {
    Result { call_id: String, result: Value },
    Denied { call_id: String, reason: PermissionDenialReason },
}
pub(crate) struct CommittedProviderEventReceipt { run_id: String, event_sequence: u32, usage: ProviderUsage }
pub(crate) struct CommittedToolReceipt {
    run_id: String, event_sequence: u32, call_id: String, idempotency_key: String,
    resolution: WorkerToolResolution,
}
impl CommittedProviderEventReceipt {
    pub(crate) fn verify_for(
        &self, expected_run_id: &str, expected_sequence: u32,
    ) -> Result<(), ProviderFailure>;
}
impl CommittedToolReceipt {
    pub(crate) fn verify_for(
        &self, expected_run_id: &str, expected_call_id: &str,
        expected_idempotency_key: &str,
    ) -> Result<(), ProviderFailure>;
    pub(crate) fn resolution(&self) -> &WorkerToolResolution;
}
impl TenderStore {
    pub(crate) fn commit_worker_event(
        &mut self, run_id: &str, event_kind: ProviderEventKind,
        event_summary: &str, usage: ProviderUsage,
    ) -> Result<CommittedProviderEventReceipt, TenderCommandError>;
    pub(crate) fn commit_worker_tool_resolution(
        &mut self, run_id: &str, call_id: &str, idempotency_key: &str,
        resolution: WorkerToolResolution, usage: ProviderUsage,
    )
        -> Result<CommittedToolReceipt, TenderCommandError>;
}
```

`MonetaryBudget` has no default currency/amount. `ProviderUsage.currency` and `estimated_microunits` are both present or both absent. The two receipt types are crate-private, their fields and constructors are private to `agent_records.rs`, and they can only be returned after the transaction persists their event/tool result. The read-only verification methods expose no constructor or mutable field. Their run ID, ordered event sequence, call ID, and Host-reserved idempotency key bind a JSONL result to one immutable run.

- [ ] Write RED source-module tests for malformed currency, zero/overflow budgets, cumulative token/tool/cost ceilings, usage currency-pair invariants, receipt construction only after commit, different-idempotency duplicate call IDs, and terminal-event immutability.

  ```rust
  #[test]
  fn monetary_usage_and_receipts_require_an_exact_committed_binding() {
      assert!(AgentResourceBudget::validate(&AgentResourceBudget {
          provider_turns: 1, duration_seconds: 1, output_bytes: 1,
          input_tokens: 1, output_tokens: 1, total_tokens: 2, tool_rounds: 1,
          monetary: Some(MonetaryBudget { currency: "usd".into(), maximum_microunits: 1 }),
      }).is_err());
      assert!(ProviderUsage { currency: Some("USD".into()), estimated_microunits: None, ..Default::default() }.validate().is_err());
      let mut store = fixture_store();
      assert!(store.commit_worker_tool_resolution("run", "call", "idem-a", WorkerToolResolution::Denied {
          call_id: "call".into(), reason: PermissionDenialReason::ToolNotGranted,
      }, ProviderUsage::default()).is_err());
  }
  ```

  `fixture_store()` is a temporary TenderStore with one prepared running run ID `run`, and does not persist outside the test directory.

  Expected RED failure: `AgentResourceBudget::validate`, `ProviderUsage::validate`, and the commit-only receipt path do not exist.
- [ ] Run RED:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib agent_runtime::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --lib tender_store::agent_records::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime --features runtime-fixture
  ```

- [ ] Implement exact types, private receipt constructors, and generated bindings. Do not let a worker or callback manufacture a receipt.

  ```rust
  impl MonetaryBudget {
      fn validate(&self) -> Result<(), ProviderFailure> {
          (self.currency.len() == 3 && self.currency.bytes().all(|b| b.is_ascii_uppercase())
              && self.maximum_microunits > 0).then_some(()).ok_or_else(|| ProviderFailure::new(
                  ProviderFailureCategory::RequestBudgetExceeded, false,
                  "Choose a positive ISO-currency monetary budget.", None,
              ))
      }
  }
  transaction.commit().map_err(sql_error)?;
  Ok(CommittedToolReceipt { run_id: run_id.into(), event_sequence, call_id: call_id.into(),
      idempotency_key: idempotency_key.into(), resolution })
  ```
- [ ] Run GREEN and commit:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib agent_runtime::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --lib tender_store::agent_records::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime --features runtime-fixture
  npm run bindings:generate
  git add src-tauri/src/agent_runtime.rs src-tauri/src/agent_runtime/bootstrap_profile.rs src-tauri/src/agent_runtime/codex_protocol.rs src-tauri/src/agent_runtime/permissions.rs src-tauri/src/host.rs src-tauri/src/tender_store.rs src-tauri/src/tender_store/agent_records.rs src-tauri/src/tender_store/bid_decisions.rs src-tauri/src/tender_store/manager_intake.rs src-tauri/src/tender_store/team_composer.rs src-tauri/src/tender_store/tender_records.rs src-tauri/src/bin/export_bindings.rs src-tauri/tests/agent_runtime.rs src-tauri/tests/bid_decisions.rs src/bindings/MonetaryBudget.ts src/bindings/AgentResourceBudget.ts src/bindings/ProviderUsage.ts
  git commit -m "feat: bind explicit provider budgets and usage to agent runs"
  ```

  Confirm the main dev server remains running.

---

### Task 1: Add worker-specific bounded JSONL limits

**Files:**

- Create: `src-tauri/src/ai/worker.rs`
- Modify: `src-tauri/src/ai/mod.rs`
- Create: `src-tauri/src/bin/quantix_ai_worker_fixture.rs`
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/tests/ai_worker_contract.rs`
- Create: `src-tauri/tests/support/ai_worker_fixture.rs`
- Create: `src-tauri/tests/fixtures/ai_worker/happy.jsonl`
- Create: `src-tauri/tests/fixtures/ai_worker/oversized-frame.jsonl`
- Create: `src-tauri/tests/fixtures/ai_worker/aggregate-overflow.jsonl`
- Create: `src-tauri/tests/fixtures/ai_worker/invalid-newline.jsonl`

**Interfaces:**

This task consumes Slice 1's production conversation supervisor. It must not re-promote or change generic process supervision. Private parsing tests go in `src-tauri/src/ai/worker.rs` under `#[cfg(test)]`. `Cargo.toml` registers `quantix-ai-worker-fixture` at `src/bin/quantix_ai_worker_fixture.rs`, `required-features = ["runtime-fixture"]`, `test = false`, and `bench = false`. `AiWorkerFixture` runs only its `CARGO_BIN_EXE_quantix-ai-worker-fixture` child with one enum transcript argument; it never imports `pub(crate)` worker state.

```rust
pub(crate) const WORKER_PROTOCOL_VERSION: u32 = 1;
pub(crate) const WORKER_FRAME_BYTES: usize = 1024 * 1024;
pub(crate) const WORKER_AGGREGATE_STDOUT_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const WORKER_STDERR_BYTES: usize = 256 * 1024;
pub(crate) const WORKER_MAX_TOOL_ROUNDS: u32 = 32;

#[derive(Debug, thiserror::Error)]
pub(crate) enum WorkerProtocolError {
    #[error("worker frame is not valid UTF-8 JSON")]
    InvalidFrame,
    #[error("worker frame violates JSONL framing")]
    InvalidFraming,
    #[error("worker frame exceeds its byte limit")]
    FrameTooLarge,
    #[error("worker output exceeds its aggregate limit")]
    AggregateOutputTooLarge,
    #[error("worker output contains a secret-shaped field")]
    SecretShapedOutput,
    #[error("worker protocol state or sequence is invalid")]
    InvalidSequence,
}

pub(crate) struct WorkerLimits {
    pub frame_bytes: usize,
    pub aggregate_stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub tool_rounds: u32,
    pub deadline_epoch_ms: u64,
}

pub(crate) fn encode_jsonl_frame<T: Serialize>(value: &T)
    -> Result<Zeroizing<Vec<u8>>, WorkerProtocolError>;
pub(crate) fn decode_jsonl_frame(bytes: &[u8])
    -> Result<serde_json::Value, WorkerProtocolError>;
pub(crate) fn reject_secret_shaped_output(value: &serde_json::Value)
    -> Result<(), WorkerProtocolError>;

// Defined in src-tauri/tests/support/ai_worker_fixture.rs.
pub enum AiWorkerFixtureTranscript {
    Happy,
    OversizedFrame,
    AggregateOverflow,
    InvalidNewline,
}
pub struct AiWorkerFixtureResult {
    pub terminal_kind: String,
    pub tool_dispatch_count: u32,
    pub process_tree_reaped: bool,
}
pub struct AiWorkerFixture;
impl AiWorkerFixture {
    pub fn launch_named(
        transcript: AiWorkerFixtureTranscript,
    ) -> Result<AiWorkerFixtureResult, Box<dyn std::error::Error>>;
}
```

`encode_jsonl_frame` emits one UTF-8 JSON object plus one final LF and refuses embedded framing LF/CR or an over-limit frame. `decode_jsonl_frame` rejects invalid UTF-8, empty/non-object input, embedded LF/CR, and trailing bytes. `reject_secret_shaped_output` recursively rejects case-insensitive keys `api_key`, `authorization`, `credential`, `access_token`, `refresh_token`, `cookie`, and `set-cookie`.

- [ ] Write RED source-module tests for exact 1 MiB acceptance; 1 MiB + 1 rejection; aggregate 16 MiB + 1 rejection; malformed UTF-8; newline framing; and every secret-shaped key. Write public harness tests using only `AiWorkerFixture::launch_named` that assert typed failure, no tool execution, and complete cleanup.

  ```rust
  #[test]
  fn jsonl_rejects_a_frame_one_byte_over_the_protocol_limit() {
      let value = serde_json::json!({"type":"event", "text":"x".repeat(WORKER_FRAME_BYTES)});
      assert!(matches!(encode_jsonl_frame(&value), Err(WorkerProtocolError::FrameTooLarge)));
  }
  ```

  Expected RED failure: `encode_jsonl_frame` is undefined.
- [ ] Run RED:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::worker::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test ai_worker_contract --features runtime-fixture -- --nocapture
  ```

- [ ] Implement only the three helpers and immutable per-operation counters; retain the supervisor's cancellation/control/Job cleanup behavior.

  ```rust
  pub(crate) fn decode_jsonl_frame(bytes: &[u8]) -> Result<Value, WorkerProtocolError> {
      if bytes.is_empty() || bytes.len() > WORKER_FRAME_BYTES || bytes.contains(&b'\n') || bytes.contains(&b'\r') {
          return Err(WorkerProtocolError::InvalidFraming);
      }
      let value = serde_json::from_slice(bytes).map_err(|_| WorkerProtocolError::InvalidFrame)?;
      value.is_object().then_some(value).ok_or(WorkerProtocolError::InvalidFrame)
  }
  ```
- [ ] Run the two commands GREEN; then:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::worker::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test ai_worker_contract --features runtime-fixture
  git add src-tauri/src/ai/worker.rs src-tauri/src/ai/mod.rs src-tauri/src/bin/quantix_ai_worker_fixture.rs src-tauri/Cargo.toml src-tauri/tests/ai_worker_contract.rs src-tauri/tests/support/ai_worker_fixture.rs src-tauri/tests/fixtures/ai_worker
  git commit -m "feat: bound general worker JSONL frames"
  ```

  Confirm the main dev server remains running.

### Task 2: Define strict protocol states, compatible input, and route pairing

**Files:**

- Modify: `src-tauri/src/ai/worker.rs`
- Modify: `src-tauri/src/ai/contract.rs`
- Modify: `src-tauri/src/ai/connections.rs`
- Modify: `src-tauri/src/ai/vault.rs`
- Modify: `src-tauri/tests/ai_connection_repository.rs`
- Modify: `src-tauri/tests/ai_worker_contract.rs`
- Create: `src-tauri/tests/fixtures/ai_worker/tool-roundtrip.jsonl`
- Create: `src-tauri/tests/fixtures/ai_worker/malformed-sequence.jsonl`
- Create: `src-tauri/tests/fixtures/ai_worker/model-reroute.jsonl`
- Create: `src-tauri/tests/fixtures/ai_worker/cancel-late-output.jsonl`

**Interfaces:**

```rust
pub(crate) struct ProbeBudget {
    pub maximum_requests: u8,             // 1..=6
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub output_bytes: u32,
    pub tool_rounds: u32,                 // 0 or 1 for the no-op probe tool
    pub deadline_epoch_ms: u64,
}
pub(crate) struct WorkerProbeRequest {
    pub request_id: String,
    pub model_id: String,
    pub reasoning: AiReasoningSelection,
    pub budget: ProbeBudget,
}
pub(crate) struct WorkerDiscoveryRequest {
    pub request_id: String,
    pub budget: ProbeBudget,
}
pub(crate) struct WorkerTurnRequest {
    pub run_id: String,
    pub request_id: String,
    pub instructions: String,
    pub tools: Vec<TypedToolDefinition>,
    pub output_schema: Value,
    pub budget: AgentResourceBudget,
}
pub(crate) struct WorkerToolCall {
    pub call_id: String,
    pub tool_name: String,
    pub canonical_arguments: Value,
}
pub(crate) enum WorkerEvent {
    TextDelta { sequence: u64, text: String },
    Usage { sequence: u64, usage: ProviderUsage },
    Warning { sequence: u64, code: String },
}
pub(crate) enum WorkerOutputFrame {
    Ready { request_id: String, protocol_version: u32, sequence: u64 },
    Event { request_id: String, event: WorkerEvent },
    ToolCall { request_id: String, sequence: u64, call: WorkerToolCall },
    DiscoveryResult { request_id: String, sequence: u64, models: Vec<AiModelView> },
    ProbeResult { request_id: String, sequence: u64, evidence: AiProbeEvidence },
    Terminal { request_id: String, sequence: u64, output: Value, usage: ProviderUsage },
    Failure { request_id: String, sequence: u64, failure: ProviderFailure },
}
pub(crate) enum AcceptedWorkerFrame {
    Ready,
    Event(WorkerEvent),
    ToolCall(WorkerToolCall),
    DiscoveryResult(Vec<AiModelView>),
    ProbeResult(AiProbeEvidence),
    Terminal { output: Value, usage: ProviderUsage },
    Failure(ProviderFailure),
}
pub(crate) enum GeneralProviderRoute {
    DirectOpenAiResponses,
    OpenAiCompatibleChat,
    DirectAnthropicMessages,
    AnthropicCompatibleMessages,
    GeminiDeveloperApi,
    XaiNative,
}
pub(crate) fn route_for_selection(
    active: &ActiveAiConfiguration,
) -> Result<GeneralProviderRoute, ProviderFailure>;
pub(crate) fn validate_route_reasoning_pair(
    route: GeneralProviderRoute,
    active: &ActiveAiConfiguration,
    evidence: &AiProbeEvidence,
) -> Result<(), ProviderFailure>;
pub(crate) enum CompatibleAuthentication {
    Bearer { credential: Zeroizing<String> },
    ApiKeyHeader { name: String, credential: Zeroizing<String> },
}
pub(crate) enum DirectProviderCredential {
    OpenAiBearer { api_key: Zeroizing<String> },
    AnthropicApiKey { api_key: Zeroizing<String> },
    GeminiApiKey { api_key: Zeroizing<String> },
    XaiBearer { api_key: Zeroizing<String> },
}
pub(crate) struct CompatibleHeaderValue { pub name: String, pub value: Zeroizing<String> }
pub(crate) struct CompatibleQueryValue { pub name: String, pub value: Zeroizing<String> }
pub(crate) struct WorkerConnectionDescriptor {
    pub route: GeneralProviderRoute,
    pub model_id: String,
    pub reasoning: AiReasoningSelection,
    pub direct_credential: Option<DirectProviderCredential>,
    pub base_url: Option<String>,
    pub compatible_authentication: Option<CompatibleAuthentication>,
    pub compatible_headers: Vec<CompatibleHeaderValue>,
    pub compatible_query: Vec<CompatibleQueryValue>,
    pub destination_fingerprint: String,
}
pub(crate) struct GeneralWorkerConnection {
    pub descriptor: WorkerConnectionDescriptor,
    pub execution_revision: AiConnectionRevision,
    pub credential_generation: CredentialGeneration,
    pub approved_capability_sha256: String,
    pub approved_pricing_sha256: Option<String>,
}
pub(crate) enum GeneralDraftOperation {
    Discover,
    Probe { model_id: String, reasoning: AiReasoningSelection },
}
pub(crate) struct GeneralProbeConnection {
    pub route: GeneralProviderRoute,
    pub direct_credential: Option<DirectProviderCredential>,
    pub base_url: Option<String>,
    pub compatible_authentication: Option<CompatibleAuthentication>,
    pub compatible_headers: Vec<CompatibleHeaderValue>,
    pub compatible_query: Vec<CompatibleQueryValue>,
    pub destination_fingerprint: String,
    pub execution_revision: AiConnectionRevision,
    pub credential_generation: CredentialGeneration,
    pub operation: GeneralDraftOperation,
}
impl AiConnectionRepository {
    pub(crate) fn general_worker_connection(
        &self,
        active: &ActiveAiConfiguration,
    ) -> Result<GeneralWorkerConnection, AiConnectionError>;
    pub(crate) fn general_probe_connection(
        &self,
        connection_id: &str,
        expected_execution_revision: u64,
        operation: GeneralDraftOperation,
    ) -> Result<GeneralProbeConnection, AiConnectionError>;
}
pub(crate) enum WorkerInputFrame {
    Initialize { request_id: String, connection: WorkerConnectionDescriptor, limits: WorkerLimits },
    InitializeDraft { request_id: String, connection: GeneralProbeConnection, limits: WorkerLimits },
    Discover(WorkerDiscoveryRequest),
    Probe(WorkerProbeRequest),
    TurnStart(WorkerTurnRequest),
    ToolResult { request_id: String, call_id: String, result: Value },
    ToolDenied { request_id: String, call_id: String, reason_code: String },
    Cancel { request_id: String },
    Shutdown { request_id: String },
}
pub(crate) enum WorkerState {
    AwaitingReady,
    AwaitingOperation,
    Running { next_sequence: u64, open_calls: BTreeSet<String>, tool_rounds: u32 },
    Terminal,
}
pub(crate) fn accept_worker_frame(
    state: &mut WorkerState, expected_request_id: &str, frame: WorkerOutputFrame,
) -> Result<AcceptedWorkerFrame, WorkerProtocolError>;
pub(crate) struct GeneralWorkerClient {
    conversation: SupervisedConversation,
    request_id: String,
    limits: WorkerLimits,
    state: WorkerState,
}
impl GeneralWorkerClient {
    pub(crate) async fn run_discovery(
        &mut self,
        request: WorkerDiscoveryRequest,
    ) -> Result<Vec<AiModelView>, ProviderFailure>;
    pub(crate) async fn run_probe(
        &mut self,
        request: WorkerProbeRequest,
    ) -> Result<AiProbeEvidence, ProviderFailure>;
}
```

`GeneralWorkerConnection` and `GeneralProbeConnection` implement neither `Clone` nor `Debug`; their drop paths zeroize every secret. They implement only the controlled initialize-frame serialization needed to write one zeroizing buffer. `general_probe_connection` validates connection/revision/credential/destination plus the exact draft operation and does not require an active configuration; `general_worker_connection` additionally validates the complete active model/reasoning/capability/pricing identity. Both release vault/installation locks before returning. Exactly one credential branch is legal: direct OpenAI projects `OpenAiBearer`, direct Anthropic `AnthropicApiKey`, Gemini `GeminiApiKey`, and xAI `XaiBearer`; compatible routes project only `CompatibleAuthentication`. OpenAI passes its key to `AsyncOpenAI(api_key=...)`; Anthropic to `AsyncAnthropic(api_key=...)`; Gemini to `genai.Client(api_key=...)`; xAI to `xai_sdk.AsyncClient(api_key=...)`, whose SDK owns the gRPC bearer metadata. Compatible bearer writes only `Authorization: Bearer <credential>`; named-key writes only the validated named key header and never placeholder `Authorization`. The initialize frame carries bounded compatible values once: maximum 16 headers and 16 query entries, 128-byte names, 4 KiB values, headers deduplicated case-insensitively, query names exactly deduplicated, and no credential in ordinary additions. No subsequent frame contains them.

The Host sends exactly one active `initialize` or pre-activation `initialize_draft`, then exactly one `discover`, `probe`, or `turn_start`; only a probe/turn may exchange correlated `tool_result`/`tool_denied`; optional `cancel` precedes final `shutdown`. Output is `ready` at sequence 0 followed by `event`, `tool_call`, `discovery_result`, `probe_result`, `terminal`, or `failure`; only one operation result/final frame is allowed.

- [ ] Write RED table tests containing six valid provider/method/route pairs and every cross-pair invalid. For each valid row, test matching and mismatching `Unsupported`/`Effort { id }` evidence. Add draft projection tests proving discovery/probe work with no active configuration, plus active projection tests for exact revision/model/reasoning/capability/pricing identity, bearer/named-header values, zeroization, and no held vault/installation lock after return. Add `run_discovery` tests for bounded pagination/500-model maximum and `run_probe` tests for a maximum of one through six requests, zero/over-six rejection, deadline/token/output/tool budget rejection before wire output, exactly one operation frame, and one final result/failure. Put direct private-contract tests in `ai::worker`/`ai::contract`/`ai::connections` source `#[cfg(test)]` modules. Add integration transcript tests for version/request-ID mismatch, duplicate/gap/order, second operation, unknown/duplicate tool result, second final, post-terminal event, reroute, cancellation late output, and bounded compatible serialization through `AiWorkerFixture`; the integration crate imports no `pub(crate)` contract.

  ```rust
  #[test]
  fn each_provider_method_maps_to_only_its_single_worker_route() {
      for (active, expected) in six_route_active_configurations() {
          assert_eq!(route_for_selection(&active).unwrap(), expected);
      }
      let mut wrong = six_route_active_configurations().into_iter().next().unwrap().0;
      wrong.provider = AiProviderKind::AnthropicCompatible;
      assert!(route_for_selection(&wrong).is_err());
  }

  #[tokio::test]
  async fn probe_rejects_a_seventh_request_before_writing_jsonl() {
      let mut client = fixture_worker_client();
      let request = fixture_probe(ProbeBudget { maximum_requests: 6, ..fixture_probe_budget() });
      assert!(client.run_probe(request).await.is_err());
      assert_eq!(client.fixture_operation_frame_count(), 0);
  }
  ```

  `six_route_active_configurations()` returns six owned, valid `ActiveAiConfiguration` values in the route-table order; `fixture_worker_client()` uses only the committed fixture binary and `fixture_probe_budget()` has `maximum_requests = 6` with positive remaining bounds.

  Expected RED failure: the closed route mapping, draft projection, and `run_discovery`/`run_probe` state transitions do not exist.
- [ ] Run RED:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::worker::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::contract::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test ai_worker_contract --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_repository --features runtime-fixture general_worker_connection
  ```

- [ ] Implement closed `#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]` unions. `accept_worker_frame` validates version/ID, accepts only ready sequence zero, increments exactly one, tracks calls, increments only new tool-call rounds, and irreversibly enters `Terminal`. Implement the one short-lived CAS-checked `general_worker_connection` projection; it returns no clone/debug/serialized secret type and releases every vault/installation lock before any process starts.

  ```rust
  pub(crate) fn route_for_selection(active: &ActiveAiConfiguration) -> Result<GeneralProviderRoute, ProviderFailure> {
      match (active.provider, active.provider_route_id.as_deref()) {
          (AiProviderKind::OpenAi, None) => Ok(GeneralProviderRoute::DirectOpenAiResponses),
          (AiProviderKind::OpenAiCompatible, Some("chat_completions")) => Ok(GeneralProviderRoute::OpenAiCompatibleChat),
          (AiProviderKind::Anthropic, None) => Ok(GeneralProviderRoute::DirectAnthropicMessages),
          (AiProviderKind::AnthropicCompatible, Some("messages")) => Ok(GeneralProviderRoute::AnthropicCompatibleMessages),
          (AiProviderKind::GoogleGemini, None) => Ok(GeneralProviderRoute::GeminiDeveloperApi),
          (AiProviderKind::XAi, None) => Ok(GeneralProviderRoute::XaiNative),
          _ => Err(protocol_failure(false)),
      }
  }
  ```
- [ ] Run GREEN, then commit:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::worker::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::contract::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test ai_worker_contract --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_repository --features runtime-fixture general_worker_connection
  git add src-tauri/src/ai/worker.rs src-tauri/src/ai/contract.rs src-tauri/src/ai/connections.rs src-tauri/src/ai/vault.rs src-tauri/tests/ai_worker_contract.rs src-tauri/tests/ai_connection_repository.rs src-tauri/tests/fixtures/ai_worker
  git commit -m "feat: validate general worker protocol and route pairs"
  ```

  Confirm the main dev server remains running.

### Task 3: Create the exact locked Python worker package

**Files:**

- Create: `src-tauri/runtime/ai/pyproject.toml`
- Create: `src-tauri/runtime/ai/uv.lock`
- Create: `src-tauri/runtime/ai/THIRD_PARTY_LICENSES.json`
- Create: `src-tauri/runtime/ai/python-downloads.json`
- Create: `src-tauri/runtime/ai/quantix_ai_worker/__init__.py`
- Create: `src-tauri/runtime/ai/quantix_ai_worker/__main__.py`
- Create: `src-tauri/runtime/ai/quantix_ai_worker/protocol.py`
- Create: `src-tauri/runtime/ai/tests/__init__.py`
- Create: `src-tauri/runtime/ai/tests/fakes.py`
- Create: `src-tauri/runtime/ai/tests/test_protocol.py`
- Create: `scripts/run-ai-worker-tests.mjs`
- Modify: `package.json`

**Interfaces:**

```toml
[project]
name = "quantix-ai-worker"
version = "0.0.0"
requires-python = "==3.12.13"
dependencies = [
  "pydantic-ai-slim[anthropic,google,openai,xai]==2.33.0",
  "openai==3.3.1", "anthropic==1.0.0", "google-genai==2.19.0", "xai-sdk==1.19.0",
]
[build-system]
requires = ["uv_build==0.12.2"]
build-backend = "uv_build"
[tool.uv.build-backend]
module-root = ""
module-name = "quantix_ai_worker"
```

`protocol.py` defines strict input/output unions, `ProtocolFailure`, `parse_input_line`, `emit_output_frame`, and `WorkerStateMachine.accept`. It contains both active `InitializeFrame` and pre-activation `InitializeDraftFrame`; `ProbeBudget(maximum_requests, input_tokens, output_tokens, total_tokens, output_bytes, tool_rounds, deadline_epoch_ms)`; `DiscoverFrame(type="discover", request_id, budget)`/`DiscoveryResult`; and `ProbeFrame(type="probe", request_id, model_id, reasoning, budget)` with precisely the Task 2 Rust JSON names and bounds. The draft initialize validates `Discover` versus exact `Probe { model_id, reasoning }` and never accepts capability/pricing identities that do not yet exist. Each model has `extra="forbid"` and `hide_input_in_errors=True`. `parse_input_line` accepts one bounded UTF-8 JSON line. `emit_output_frame` accepts output models only and writes one line. `__main__.py` emits a single typed redacted failure for uncaught non-secret exceptions and never prints request/error representation, headers, or stack traces.

- [ ] Write RED tests for unknown fields, secret input rejected as output, secret-shaped output, ready/sequence/final state, byte/newline caps, and each Rust fixture transcript.

  ```python
  class ProtocolTests(unittest.TestCase):
      def test_draft_discovery_cannot_be_misread_as_an_active_initialize(self) -> None:
          frame = InitializeDraftFrame.model_validate({
              "protocol_version": 1, "request_id": "a" * 32,
              "type": "initialize_draft", "operation": {"type": "discover"},
              "connection": draft_openai_connection(), "limits": worker_limits(),
          })
          self.assertEqual(frame.operation.type, "discover")
          with self.assertRaises(ValidationError):
              WorkerOutputAdapter.validate_python({"type": "terminal", "api_key": "secret"})
  ```

  `draft_openai_connection()` and `worker_limits()` are non-secret factories in `tests/fakes.py`; they return the smallest valid direct OpenAI draft and positive protocol limits.

  Expected RED failure: `InitializeDraftFrame` and `WorkerOutputAdapter` are not importable.
- [ ] Run RED:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::worker::tests --features runtime-fixture
  node scripts/run-ai-worker-tests.mjs
  ```
- [ ] Create/check/sync the exact lock:

  ```powershell
  & .\src-tauri\runtime\bin\uv.exe lock --project .\src-tauri\runtime\ai --python 3.12.13
  & .\src-tauri\runtime\bin\uv.exe lock --check --project .\src-tauri\runtime\ai --python 3.12.13
  New-Item -ItemType Directory -Force .\.dev\runtime-provisioning\ai-worker-venv | Out-Null
  $env:UV_PROJECT_ENVIRONMENT = (Resolve-Path .\.dev\runtime-provisioning\ai-worker-venv)
  & .\src-tauri\runtime\bin\uv.exe sync --locked --no-editable --no-dev --managed-python --python 3.12.13 --project .\src-tauri\runtime\ai --no-config
  & "$env:UV_PROJECT_ENVIRONMENT\Scripts\python.exe" -I -m unittest discover -s .\src-tauri\runtime\ai\tests -t .\src-tauri\runtime\ai
  ```

  `run-ai-worker-tests.mjs` executes this sync from the absolute worker-project cwd and rejects a lock that does not resolve the five exact package versions.
- [ ] Implement, run GREEN, then commit. Do not add `package-lock.json`, since no Node dependency is added.

  ```python
  class StrictFrame(BaseModel):
      model_config = ConfigDict(extra="forbid", hide_input_in_errors=True)
      protocol_version: Literal[1]
      request_id: Annotated[str, StringConstraints(pattern=r"^[0-9a-f]{32}$")]
  ```

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::worker::tests --features runtime-fixture
  node scripts/run-ai-worker-tests.mjs
  git add src-tauri/runtime/ai scripts/run-ai-worker-tests.mjs package.json
  git commit -m "feat: add locked Pydantic AI worker"
  ```

  Confirm the main dev server remains running.

### Task 4: Package, provenance-check, and start the isolated runtime

**Files:**

- Create: `src-tauri/src/ai/runtime.rs`
- Modify: `src-tauri/src/ai/mod.rs`
- Modify: `src-tauri/src/ai/worker.rs`
- Modify: `scripts/prepare-runtime.mjs`
- Modify: `src-tauri/runtime/runtime-provenance.json`
- Modify: `src-tauri/src/runtime_readiness.rs`
- Modify: `src-tauri/tests/runtime_readiness.rs`
- Modify: `src-tauri/tests/support/runtime_fixture.rs`
- Modify: `src-tauri/tauri.conf.json`

**Interfaces:**

```rust
pub(crate) struct AiRuntimeLayout {
    pub worker_project: PathBuf,
    pub venv_python: PathBuf,
    pub worker_lock_sha256: String,
    pub worker_project_sha256: String,
}
#[derive(Debug, thiserror::Error)]
pub(crate) enum AiRuntimeError {
    #[error("AI worker provenance is invalid")]
    ProvenanceInvalid,
    #[error("AI worker environment is not locked")]
    EnvironmentInvalid,
    #[error("AI worker executable is missing")]
    PythonMissing,
    #[error("AI worker import smoke test failed")]
    ImportFailed,
}
pub(crate) fn ai_worker_environment(application_home: &Path) -> Vec<(OsString, OsString)>;
pub(crate) fn ai_worker_sync_spec(layout: &RuntimeLayout, application_home: &Path) -> ProcessSpec;
pub(crate) fn verify_ai_runtime(layout: &AiRuntimeLayout) -> Result<(), AiRuntimeError>;

impl GeneralWorkerClient {
    pub(crate) async fn spawn_probe(
        runtime: &AiRuntimeLayout,
        connection: GeneralProbeConnection,
        workspace: &Path,
        limits: WorkerLimits,
        cancellation: CancellationToken,
    ) -> Result<Self, ProviderFailure>;
    pub(crate) async fn spawn(
        runtime: &AiRuntimeLayout,
        connection: GeneralWorkerConnection,
        workspace: &Path,
        limits: WorkerLimits,
        cancellation: CancellationToken,
    ) -> Result<Self, ProviderFailure>;
}

// Local integration-test harness methods, defined on the existing RuntimeHarness
// in src-tauri/tests/runtime_readiness.rs; they call only public Host readiness APIs.
impl RuntimeHarness {
    pub fn stage_ai_worker_project(&self) -> Result<(), Box<dyn std::error::Error>>;
    pub fn flip_ai_worker_project_byte(&self, relative_path: &str) -> Result<(), Box<dyn std::error::Error>>;
    pub fn replace_ai_worker_lock_version(&self, package: &str, version: &str) -> Result<(), Box<dyn std::error::Error>>;
    pub fn mark_ai_worker_venv_editable(&self) -> Result<(), Box<dyn std::error::Error>>;
    pub fn link_ai_worker_project_file(&self, relative_path: &str) -> Result<(), Box<dyn std::error::Error>>;
    pub fn remove_ai_worker_python(&self) -> Result<(), Box<dyn std::error::Error>>;
}
```

`ai_worker_environment` uses `env_clear` and supplies only `SystemRoot`, `WINDIR`, staging `TEMP`/`TMP`/`TMPDIR`, `UV_CACHE_DIR`, `UV_PROJECT_ENVIRONMENT`, `UV_PYTHON_INSTALL_DIR`, and `UV_NO_CONFIG=1`. It supplies no HOME/USERPROFILE, proxy, provider key, PYTHONPATH, user-site, netrc, cloud, CA, or telemetry setting. `ai_worker_sync_spec` uses the exact Task 3 `uv sync` command into `<Application Home>\runtimes\ai\venv`. `verify_ai_runtime` rejects edits, links, wrong package versions, missing venv python, or failed `python -I -m quantix_ai_worker --version`. `spawn_probe` accepts only a CAS-checked pre-activation draft projection; `spawn` accepts only a complete active projection. Both verify provenance/an empty workspace, launch the exact venv Python, write one zeroizing initialize frame, and hold no vault/installation lock.

- [ ] Add RED tests for changed source/lock/license/provenance, editable marker, wrong package version, linked file, missing worker, and import smoke failure. The named local `RuntimeHarness` operations above mutate only their test-owned temporary tree and observe production behavior through public Host readiness APIs; each validates a literal relative path against its fixture root before mutation. Keep production-private helper tests inside `runtime_readiness.rs`'s source module.

  ```rust
  #[tokio::test]
  async fn readiness_rejects_a_changed_ai_worker_lock() {
      let harness = RuntimeHarness::prepared_ai_worker().await;
      harness.replace_ai_worker_lock_version("openai", "0.0.0").unwrap();
      assert_eq!(harness.host.inspect_runtime_readiness().await.state, RuntimeReadinessState::RepairRequired);
  }
  ```

  Expected RED failure: `RuntimeHarness::prepared_ai_worker` and AI worker provenance validation do not exist.
- [ ] Keep Slice 1's exact ignore/negation ownership rule unchanged. Extend `prepare-runtime.mjs` so the already-committed provenance hashes every allowed AI input and license inventory, then prove it remains tracked and is staged.

  ```powershell
  git ls-files --error-unmatch src-tauri/runtime/runtime-provenance.json
  if ($LASTEXITCODE -ne 0) { throw "runtime provenance is not tracked" }
  npm run prepare:runtime
  git add src-tauri/runtime/runtime-provenance.json
  git ls-files --error-unmatch src-tauri/runtime/runtime-provenance.json
  if ($LASTEXITCODE -ne 0) { throw "runtime provenance is not tracked" }
  git diff --cached --name-only -- src-tauri/runtime/runtime-provenance.json
  ```

  The last command must print the provenance path.
- [ ] Run RED then GREEN:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --test runtime_readiness --features runtime-fixture ai_worker -- --nocapture
  cargo test --manifest-path src-tauri/Cargo.toml --lib runtime_readiness::tests --features runtime-fixture
  node scripts/run-ai-worker-tests.mjs
  ```

  Bundle only uv, OCR allowlisted inputs, AI `pyproject.toml`, lock, downloads manifest, license inventory, `quantix_ai_worker/**`, and provenance. Exclude tests, `.venv`, `.dev`, caches, downloads, and every unlisted file. Task 8 adds the versioned pricing snapshot and re-runs the same allowlist/provenance validation.
- [ ] Implement the readiness minimum:

  ```rust
  let output = supervisor.run(ProcessSpec {
      executable: layout.venv_python.clone(),
      arguments: vec![OsString::from("-I"), OsString::from("-m"), OsString::from("quantix_ai_worker"), OsString::from("--version")],
      current_directory: Some(application_home.join("staging")),
      environment: ai_worker_environment(application_home), inherit_environment: false,
      stdin: Vec::new(), timeout: VERSION_TIMEOUT, stdout_limit: PROBE_OUTPUT_LIMIT, stderr_limit: PROBE_OUTPUT_LIMIT,
  }, CancellationToken::new()).await.map_err(|_| AiRuntimeError::ImportFailed)?;
  ```
- [ ] Commit:

  ```powershell
  git add src-tauri/src/ai/runtime.rs src-tauri/src/ai/mod.rs src-tauri/src/ai/worker.rs scripts/prepare-runtime.mjs src-tauri/runtime/runtime-provenance.json src-tauri/src/runtime_readiness.rs src-tauri/tests/runtime_readiness.rs src-tauri/tests/support/runtime_fixture.rs src-tauri/tauri.conf.json
  git commit -m "feat: provision verified general worker runtime"
  ```

  Confirm the main dev server remains running.

### Task 5: Implement the connect-first compatible endpoint guard

**Files:**

- Create: `src-tauri/runtime/ai/quantix_ai_worker/guarded_transport.py`
- Create: `src-tauri/runtime/ai/tests/test_guarded_transport.py`

**Interfaces:**

```python
@dataclass(frozen=True)
class EndpointPolicy:
    base_url: httpx.URL
    destination_class: Literal["public", "private", "loopback"]
    connect_timeout_seconds: float
    write_timeout_seconds: float
    read_timeout_seconds: float
    overall_timeout_seconds: float
    compressed_response_bytes: int
    decompressed_response_bytes: int

class GuardedConnection(Protocol):
    async def write_http_request(self, request: httpx.Request) -> None: ...
    async def read_http_response(self, limits: EndpointPolicy) -> httpx.Response: ...
    async def aclose(self) -> None: ...

class GuardedTcpConnection(Protocol):
    @property
    def peer_address(self) -> ipaddress.IPv4Address | ipaddress.IPv6Address: ...
    async def start_tls(self, server_hostname: str) -> GuardedConnection: ...
    async def use_loopback_plaintext(self) -> GuardedConnection: ...
    async def aclose(self) -> None: ...

class GuardedNetworkBackend(Protocol):
    async def resolve_all(self, host: str, port: int) -> tuple[ipaddress.IPv4Address | ipaddress.IPv6Address, ...]: ...
    async def connect_tcp(self, address: ipaddress.IPv4Address | ipaddress.IPv6Address, port: int, policy: EndpointPolicy) -> GuardedTcpConnection: ...

def classify_address(address: ipaddress.IPv4Address | ipaddress.IPv6Address) -> Literal["public", "private", "loopback", "forbidden"]: ...
def validate_resolution(addresses: tuple[ipaddress.IPv4Address | ipaddress.IPv6Address, ...], expected: str) -> tuple[ipaddress.IPv4Address | ipaddress.IPv6Address, ...]: ...
def join_endpoint_path(base_url: URL, request_path: str) -> URL: ...
class QuantixGuardedTransport(httpx.AsyncBaseTransport):
    @classmethod
    def for_endpoint(cls, policy: EndpointPolicy) -> "QuantixGuardedTransport": ...
    async def handle_async_request(self, request: httpx.Request) -> httpx.Response: ...

@dataclass
class FakeGuardedNetworkBackend:
    resolved: tuple[ipaddress.IPv4Address | ipaddress.IPv6Address, ...]
    connected_peer: ipaddress.IPv4Address | ipaddress.IPv6Address
    bytes_written: int = 0
    tls_started: bool = False
    async def resolve_all(self, host: str, port: int) -> tuple[ipaddress.IPv4Address | ipaddress.IPv6Address, ...]: ...
    async def connect_tcp(self, address: ipaddress.IPv4Address | ipaddress.IPv6Address, port: int, policy: EndpointPolicy) -> GuardedTcpConnection: ...

class AsyncioGuardedNetworkBackend:
    @classmethod
    def create(cls) -> "AsyncioGuardedNetworkBackend": ...
    async def resolve_all(self, host: str, port: int) -> tuple[ipaddress.IPv4Address | ipaddress.IPv6Address, ...]: ...
    async def connect_tcp(self, address: ipaddress.IPv4Address | ipaddress.IPv6Address, port: int, policy: EndpointPolicy) -> GuardedTcpConnection: ...

```

`classify_address` rejects unspecified, multicast, broadcast, link-local, documentation, carrier-grade NAT, metadata, IPv4-mapped IPv6, NAT64/6to4, reserved, and non-global addresses except exact configured literal loopback HTTP. `validate_resolution` rejects no answers, mixed classes, forbidden answers, and class drift. `join_endpoint_path` preserves the configured path prefix and rejects authority/query/fragment/dot-segment/absolute replacement.

`AsyncioGuardedNetworkBackend.create` owns no ambient proxy/TLS context. `resolve_all` calls `asyncio.get_running_loop().getaddrinfo(host, port, type=socket.SOCK_STREAM)`, parses only literal returned IPs, deduplicates and canonical-sorts them. `connect_tcp` calls `asyncio.open_connection(host=str(address), port=port, ssl=None, server_hostname=None)` under `connect_timeout_seconds`, obtains the socket peer with `writer.get_extra_info("peername")`, and creates a `GuardedTcpConnection`. `start_tls` calls `writer.start_tls(ssl.create_default_context(), server_hostname=original_hostname, ssl_handshake_timeout=connect_timeout_seconds)` and verifies the TLS hostname; `use_loopback_plaintext` rejects every nonliteral-loopback policy. `QuantixGuardedTransport.for_endpoint` creates this production backend; tests inject only `FakeGuardedNetworkBackend` into the ordinary constructor.

`handle_async_request` must: validate URL/no redirect; resolve all; validate all; canonical-sort the approved set; make exactly one TCP attempt to its first address; validate `tcp.peer_address` against that approved class; only then call `start_tls(original_hostname)` for HTTPS or `use_loopback_plaintext()` for an exact literal-loopback HTTP origin; then write/read under the policy and always close. It sends no TLS or HTTP byte before connected-peer validation and performs no address fallback. HTTPS preserves SNI/hostname validation. Redirects fail rather than follow. Provider factories enforce `trust_env=False` in Task 6.

- [ ] Write RED tests using `FakeGuardedNetworkBackend`, recording resolve/connect/peer/bytes written: prefix join, literal loopback HTTP, public HTTPS, private/metadata/mapped/mixed DNS rejection, mismatched connected peer with `bytes_written == 0`, redirect denial, SNI preservation, absent proxy/netrc/ADC, all timeouts, and compressed/decompressed caps.

  ```python
  class GuardedTransportTests(unittest.IsolatedAsyncioTestCase):
      async def test_peer_drift_fails_before_tls_or_http_write(self) -> None:
          backend = FakeGuardedNetworkBackend(
              resolved=(ipaddress.ip_address("1.1.1.1"),), connected_peer=ipaddress.ip_address("10.0.0.8"),
          )
          transport = QuantixGuardedTransport(EndpointPolicy(
              base_url=httpx.URL("https://api.example/v1"), destination_class="public",
              connect_timeout_seconds=1, write_timeout_seconds=1, read_timeout_seconds=1,
              overall_timeout_seconds=2, compressed_response_bytes=1024, decompressed_response_bytes=1024,
          ), backend)
          with self.assertRaisesRegex(GuardedTransportFailure, "connected peer"):
              await transport.handle_async_request(httpx.Request("POST", "https://api.example/v1/models"))
          self.assertEqual(backend.bytes_written, 0)
          self.assertFalse(backend.tls_started)
  ```

  Expected RED failure: `QuantixGuardedTransport` has no connect-first implementation and the fake observes an HTTP write.
- [ ] Run RED: `node scripts/run-ai-worker-tests.mjs`.
- [ ] Implement only guard helpers and transport; no provider factory in this task. Run GREEN and commit:

  ```python
  approved = validate_resolution(await self._backend.resolve_all(host, port), self._policy.destination_class)
  tcp = await self._backend.connect_tcp(approved[0], port, self._policy)
  if classify_address(tcp.peer_address) != self._policy.destination_class:
      await tcp.aclose()
      raise GuardedTransportFailure("connected peer destination class changed")
  connection = await tcp.start_tls(host) if request.url.scheme == "https" else await tcp.use_loopback_plaintext()
  ```

  ```powershell
  node scripts/run-ai-worker-tests.mjs
  git add src-tauri/runtime/ai/quantix_ai_worker/guarded_transport.py src-tauri/runtime/ai/tests/test_guarded_transport.py
  git commit -m "feat: guard compatible connections before HTTP writes"
  ```

  Confirm the main dev server remains running.

### Task 6: Implement six closed official-SDK factories

**Files:**

- Create: `src-tauri/runtime/ai/quantix_ai_worker/model_factory.py`
- Create: `src-tauri/runtime/ai/tests/test_model_factory.py`
- Modify: `src-tauri/runtime/ai/quantix_ai_worker/protocol.py`

**Interfaces:**

```python
def build_model(connection: WorkerConnection, transport: QuantixGuardedTransport) -> Model: ...
def build_openai_client(connection: WorkerConnection, transport: QuantixGuardedTransport) -> AsyncOpenAI: ...
def build_anthropic_client(connection: WorkerConnection, transport: QuantixGuardedTransport) -> AsyncAnthropic: ...
def build_google_client(connection: WorkerConnection, transport: QuantixGuardedTransport) -> genai.Client: ...
def build_xai_client(connection: WorkerConnection) -> xai_sdk.AsyncClient: ...
```

`build_model` is exhaustive: `OpenAIResponsesModel`, `OpenAIChatModel`, `AnthropicModel` direct/compatible, `GoogleModel`, and `XaiModel`. It pattern-matches the exact `DirectProviderCredential` variant named in Task 2 and rejects a missing/cross-provider variant before constructing a client. Compatible OpenAI/Anthropic clients receive `httpx.AsyncClient(transport=guard, trust_env=False, follow_redirects=False)`, explicit prefix-preserved base URL, bounded header/query additions, and exactly one auth placement. Direct clients reject compatible additions but always consume their required direct credential.

- [ ] Write RED tests for every valid route's model class, API surface, exact model/reasoning, base URL, one request, redirect denial, no fallback, and all invalid cross-pairs. Assert bearer adds only Authorization; named-key adds only the configured key header; header/query values survive initialize without duplicates.

  ```python
  class ModelFactoryTests(unittest.IsolatedAsyncioTestCase):
      async def test_factory_uses_one_exact_sdk_surface_for_all_six_routes(self) -> None:
          for connection, model_type in six_worker_connections():
              transport = CountingTransport(one_terminal_response(connection.model_id))
              model = build_model(connection, transport)
              self.assertIsInstance(model, model_type)
              await run_one_probe_request(model, connection.model_id, connection.reasoning)
              self.assertEqual(transport.request_count, 1)
              self.assertTrue(transport.last_request.url.path.startswith(connection.base_url.path if connection.base_url else "/"))
  ```

  `CountingTransport` is a `httpx.AsyncBaseTransport` test double in `tests/fakes.py`; `six_worker_connections()` returns one fully valid connection and expected Pydantic model class per route, all with fixture-only credentials.

  Expected RED failure: `build_model` is absent and no route can prove a single exact request.
- [ ] Run RED: `node scripts/run-ai-worker-tests.mjs`.
- [ ] Implement retry/ambient-state denial: OpenAI/Anthropic `max_retries=0`; Google one total attempt plus guard; xAI `channel_options=[("grpc.enable_retries", 0)]`, empty retry service configuration, and constructor timeout. Assert injected credentials rather than environment lookup.

  ```python
  match connection.route:
      case "direct_openai_responses":
          return OpenAIResponsesModel(connection.model_id, provider=OpenAIProvider(openai_client=build_openai_client(connection, transport)))
      case "openai_compatible_chat":
          return OpenAIChatModel(connection.model_id, provider=OpenAIProvider(openai_client=build_openai_client(connection, transport)))
      case "direct_anthropic_messages" | "anthropic_compatible_messages":
          return AnthropicModel(connection.model_id, provider=AnthropicProvider(anthropic_client=build_anthropic_client(connection, transport)))
      case "gemini_developer_api":
          return GoogleModel(connection.model_id, provider=GoogleProvider(client=build_google_client(connection, transport)))
      case "xai_native":
          return XaiModel(connection.model_id, provider=XaiProvider(xai_client=build_xai_client(connection)))
      case _:
          raise ProtocolFailure("unsupported provider route")
  ```
- [ ] Run GREEN and commit:

  ```powershell
  node scripts/run-ai-worker-tests.mjs
  git add src-tauri/runtime/ai/quantix_ai_worker/model_factory.py src-tauri/runtime/ai/quantix_ai_worker/protocol.py src-tauri/runtime/ai/tests/test_model_factory.py
  git commit -m "feat: construct pinned official provider SDK clients"
  ```

  Confirm the main dev server remains running.

### Task 7: Implement deferred Host tools, async callbacks, usage, and cancellation

**Files:**

- Create: `src-tauri/runtime/ai/quantix_ai_worker/general_adapter.py`
- Create: `src-tauri/runtime/ai/quantix_ai_worker/host_bridge.py`
- Create: `src-tauri/runtime/ai/tests/test_general_adapter.py`
- Modify: `src-tauri/src/ai/worker.rs`
- Modify: `src-tauri/src/agent_runtime/permissions.rs`

**Interfaces:**

```rust
pub(crate) type WorkerFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub(crate) trait WorkerHostCallbacks: Send {
    fn persist_event<'a>(&'a mut self, event: WorkerEvent, usage: ProviderUsage)
        -> WorkerFuture<'a, Result<CommittedProviderEventReceipt, ProviderFailure>>;
    fn resolve_tool<'a>(&'a mut self, call: WorkerToolCall, usage: ProviderUsage)
        -> WorkerFuture<'a, Result<CommittedToolReceipt, ProviderFailure>>;
    fn is_cancelled(&self) -> bool;
}
impl GeneralWorkerClient {
    pub(crate) async fn run_turn(&mut self, request: WorkerTurnRequest,
        callbacks: &mut dyn WorkerHostCallbacks) -> ProviderExecution;
}
```

`route_for_selection` exhaustively maps four direct provider kinds and two compatible methods; there is no catch-all. `validate_route_reasoning_pair` requires exact connection revision, endpoint fingerprint, model, canonical reasoning selection, capability hash, and tested probe evidence. It accepts `Unsupported` only where the selected model's evidence says unsupported and accepts `Effort { id }` only for that exact tested effort; it never promotes a sibling effort or provider-default metadata.

Both futures are awaited. `persist_event` returns only a committed event receipt, and the adapter calls `verify_for(request.run_id, expected_sequence)` before accepting it. `resolve_tool` reserves idempotency/quota, authorizes, executes, and persists result/denial plus cumulative usage in one Host transaction before returning its private committed tool receipt; the adapter verifies the same run ID, call ID, and Host-derived idempotency key before deriving and writing the correlated JSONL tool response. The worker receives the JSONL response, never either receipt. Therefore Python cannot make the next model request before Host persistence succeeds, and a receipt from another run/call cannot continue this operation.

- [ ] Write RED tests for text/structured output, strict schemas, approval/denial, malformed/duplicate calls, parallel proposals, 32 rounds, cumulative continuations, exhaustion, cancellation, accepted-stream indeterminate failure, mismatched run/call/idempotency receipts, and a persistence future held pending. Assert no continuation request before that future resolves.
  Put callback/state-machine tests in `src-tauri/src/ai/worker.rs`'s `#[cfg(test)] mod tests`. The integration test exercises the same behavior only through public `QuantixHost` run commands and the `tests/support` fixture; it never imports `WorkerHostCallbacks` or another `pub(crate)` type.

  ```rust
  #[tokio::test]
  async fn worker_waits_for_a_matching_committed_tool_receipt() {
      let (mut callbacks, release_commit) = pending_commit_callbacks("run-a", "call-a", "idem-a");
      let mut client = fixture_tool_worker("call-a");
      let task = tokio::spawn(async move { client.run_turn(fixture_turn("run-a"), &mut callbacks).await });
      assert_eq!(fixture_provider_request_count(), 1);
      release_commit.send(committed_tool_receipt("run-a", "call-a", "idem-a")).unwrap();
      let execution = task.await.unwrap();
      assert_eq!(execution.usage.request_count, 2);
  }
  ```

  `pending_commit_callbacks()` returns a callback implementation whose `resolve_tool` future waits on the paired one-shot sender; `fixture_tool_worker()` emits one `tool_call` then records every outbound Host frame; `fixture_provider_request_count()` reads that fixture's atomic request counter.

  Expected RED failure: the callback currently cannot await or verify a committed receipt before issuing the continuation.
- [ ] Run RED:

  ```powershell
  node scripts/run-ai-worker-tests.mjs
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::worker::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test ai_worker_contract --features runtime-fixture
  ```

- [ ] Implement one Agent per operation: `instrument=False`, zero retries, `ExternalToolset` only, Host schema, and `UsageLimits` no greater than Host budgets. `host_bridge.py` converts correlated JSONL outcomes to `DeferredToolResults` while retaining cumulative `RunUsage`, limits, and validated history.

  ```rust
  let receipt = callbacks.resolve_tool(call.clone(), usage.clone()).await?;
  receipt.verify_for(&request.run_id, &call.call_id, &idempotency_key)?;
  let response = match receipt.resolution() {
      WorkerToolResolution::Result { call_id, result } => WorkerInputFrame::ToolResult { request_id: request.request_id.clone(), call_id: call_id.clone(), result: result.clone() },
      WorkerToolResolution::Denied { call_id, reason } => WorkerInputFrame::ToolDenied { request_id: request.request_id.clone(), call_id: call_id.clone(), reason_code: reason.as_str().into() },
  };
  self.write_host_frame(response).await?;
  ```
- [ ] Run GREEN and commit; confirm dev server remains running.

  ```powershell
  node scripts/run-ai-worker-tests.mjs
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::worker::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test ai_worker_contract --features runtime-fixture
  git add src-tauri/runtime/ai/quantix_ai_worker/general_adapter.py src-tauri/runtime/ai/quantix_ai_worker/host_bridge.py src-tauri/runtime/ai/tests/test_general_adapter.py src-tauri/src/ai/worker.rs src-tauri/src/agent_runtime/permissions.rs
  git commit -m "feat: defer provider tools through async Host authority"
  ```

### Task 8: Add discovery, qualification, and pricing evidence

**Files:**

- Create: `src-tauri/runtime/ai/quantix_ai_worker/probe.py`
- Create: `src-tauri/runtime/ai/tests/test_probe.py`
- Create: `src-tauri/runtime/ai/pricing-snapshot.v1.json`
- Create: `src-tauri/src/ai/pricing.rs`
- Modify: `src-tauri/src/ai/mod.rs`
- Modify: `src-tauri/src/bin/export_bindings.rs`
- Create by generation: `src/bindings/GuardedPeerEvidence.ts`
- Create: `src-tauri/tests/ai_pricing.rs`
- Modify: `src-tauri/src/ai/contract.rs`
- Modify: `src-tauri/src/ai/connections.rs`
- Modify: `src-tauri/src/agent_runtime/codex_app_server.rs`
- Modify: `src-tauri/src/setup.rs`
- Modify: `src-tauri/src/runtime_readiness.rs`
- Modify: `scripts/prepare-runtime.mjs`
- Modify: `src-tauri/runtime/runtime-provenance.json`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/tests/ai_connection_repository.rs`
- Modify: `src-tauri/tests/codex_qualification.rs`
- Modify: `src-tauri/tests/runtime_readiness.rs`
- Modify: `src-tauri/tests/safe_updates.rs`
- Modify: `src-tauri/tests/quantix_setup.rs`

**Interfaces:**

```rust
pub(crate) struct AiPricingSnapshot {
    pub schema_version: u32,
    pub snapshot_sha256: String,
    pub source_url: String,
    pub source_retrieved_at: String,
    pub effective_at: String,
    pub expires_at: String,
    pub currency: String,
    pub model_id: String,
    pub input_microunits_per_token: Option<u64>,
    pub cached_input_microunits_per_token: Option<u64>,
    pub output_microunits_per_token: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GuardedPeerEvidence {
    pub destination_class: AiNetworkDestinationClass,
    pub resolved_addresses_sha256: String,
    pub connected_address_sha256: String,
    pub tls_hostname_verified: bool,
    pub redirects_followed: bool,
}
pub struct AiProbeEvidence {
    pub connection_id: AiConnectionId,
    pub execution_revision: AiConnectionRevision,
    pub provider: AiProviderKind,
    pub endpoint_fingerprint: String,
    pub adapter_version: String,
    pub destination_class: AiNetworkDestinationClass,
    pub models: Vec<AiModelView>,
    pub tested_model_id: String,
    pub tested_reasoning: AiReasoningSelection,
    pub observed_at: String,
    pub guarded_peer_evidence: Option<GuardedPeerEvidence>,
}
pub(crate) struct CompatiblePricingApproval {
    pub connection_id: AiConnectionId,
    pub execution_revision: AiConnectionRevision,
    pub model_id: String,
    pub snapshot_sha256: String,
    pub source_url: String,
    pub effective_at: String,
    pub expires_at: String,
    pub currency: String,
    pub input_microunits_per_token: u64,
    pub cached_input_microunits_per_token: Option<u64>,
    pub output_microunits_per_token: u64,
    pub approved_at: String,
}
pub(crate) fn pricing_preflight(active: &ActiveAiConfiguration,
    budget: &AgentResourceBudget, pricing: Option<&AiPricingSnapshot>) -> Result<(), ProviderFailure>;
```

`pricing-snapshot.v1.json` is a committed, shipped, version-1 array of known-provider model prices. Every record has source URL, retrieval timestamp, effective timestamp, expiry timestamp, ISO currency, and all applicable token-unit prices; provenance hashes the whole file. It is stale at or after `expires_at`; an invalid source/effective/expiry ordering or a missing model price is unknown. The installation SQLite schema owns `ai_pricing_approvals`, keyed by `(connection_id, execution_revision, model_id)`, and stores the complete canonical `CompatiblePricingApproval` JSON—including source/effective/expiry facts—plus snapshot hash/approved time. It holds no credentials and can reconstruct the exact preflight snapshot after restart.

```sql
CREATE TABLE ai_pricing_approvals (
  connection_id TEXT NOT NULL,
  execution_revision INTEGER NOT NULL CHECK (execution_revision > 0),
  model_id TEXT NOT NULL CHECK (length(model_id) BETWEEN 1 AND 200),
  approval_json TEXT NOT NULL CHECK (json_valid(approval_json)),
  snapshot_sha256 TEXT NOT NULL CHECK (length(snapshot_sha256) = 64),
  approved_at TEXT NOT NULL,
  PRIMARY KEY (connection_id, execution_revision, model_id)
) STRICT;
```

Replacing a compatible endpoint or credential placement advances its execution revision, so the old row cannot authorize the replacement. Choosing another model requires a separate row because `model_id` is part of the primary key and snapshot hash. The Task 9 `approve_compatible_pricing` command is the only renderer write path; it validates the exact tested model plus the same source/effective/expiry/currency/unit-price constraints before inserting canonical JSON.

This new installation table changes the fresh-installation schema exactly once: change `INSTALLATION_SCHEMA_VERSION` and the literal installation-table constraint from `25` to `26`, update every exact schema fixture/assertion in `runtime_readiness.rs`, `safe_updates.rs`, and `quantix_setup.rs`, and retain every schema-25 table/index/trigger unchanged. Per repository policy, there is no version-25 compatibility migration or fallback.

`GuardedPeerEvidence` is a public/exportable, secret-free evidence DTO attached as `Some` only to compatible-endpoint probe evidence and immutable Agent Run diagnostics; address values are SHA-256 rather than raw internal endpoint IPs. It must show destination class, resolution and connected-peer fingerprints, TLS verification for HTTPS, and `redirects_followed == false`. Direct official-SDK routes truthfully store `None` because they do not use the compatible-endpoint guard; their ordinary endpoint/model/request evidence remains separate. `pricing_preflight` permits absent/stale/unknown pricing only if no monetary ceiling exists. With a monetary ceiling, it rejects locally before worker spawn and never treats unknown pricing as zero.

- [ ] Write RED tests for the shipped file's schema/hash/source/effective/expiry/currency validation, exact stale boundary, missing model price, 500 models, duplicate/empty/incomplete pages, compatible models 404/manual ID only, auth/malformed/model failure, selected streaming/strict output/no-op tool/cancel/usage/request-ID/context, reported-identity reroute, selected reasoning only, compatible approval revision invalidation, and guarded-peer evidence. Ensure timestamp/display/order/secrets/raw response do not affect semantic catalogue hash, while pricing timestamps do affect pricing-snapshot identity.
  Put `pricing_preflight`, normalization, and hash tests in `src-tauri/src/ai/pricing.rs` and `src-tauri/src/ai/contract.rs` under `#[cfg(test)]`. `src-tauri/tests/ai_pricing.rs` uses only public connection/activation/run commands on `QuantixHost` and asserts the resulting public readiness/failure view; it never imports a `pub(crate)` helper.

  ```rust
  #[test]
  fn monetary_preflight_rejects_a_snapshot_at_its_expiry_boundary() {
      let snapshot = fixture_pricing("2026-08-25T00:00:00Z", "2026-08-26T00:00:00Z");
      let budget = fixture_budget(Some(MonetaryBudget { currency: "USD".into(), maximum_microunits: 1 }));
      assert!(pricing_preflight_at(&fixture_active(), &budget, Some(&snapshot), "2026-08-26T00:00:00Z").is_err());
  }
  ```

  ```python
  class ProbeTests(unittest.IsolatedAsyncioTestCase):
      async def test_compatible_models_404_runs_only_the_explicit_model_probe(self) -> None:
          provider = FakeCompatibleProvider(models_status=404, accepted_model="local-7")
          result = await probe_selected_tuple(provider, model_id="local-7", reasoning=Unsupported())
          self.assertEqual(result.models, ["local-7"])
          self.assertEqual(provider.requested_models, ["local-7"])
  ```

  `fixture_pricing(effective_at, expires_at)` returns a valid USD snapshot for `fixture_active().model_id`; `fixture_budget` supplies positive token/time/tool limits; `FakeCompatibleProvider` records each selected model request without network access.

  Expected RED failure: pricing/probe modules and the explicit-404 branch do not exist.
- [ ] Run RED:

  ```powershell
  node scripts/run-ai-worker-tests.mjs
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::pricing::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::contract::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test ai_pricing --test ai_connection_repository --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test runtime_readiness --test safe_updates --test quantix_setup --features runtime-fixture
  ```

- [ ] Implement `load_shipped_pricing_snapshot`, `pricing_preflight`, `record_compatible_pricing_approval`, `discover_models`, `probe_selected_tuple`, and `normalize_probe_result`. Add `pricing-snapshot.v1.json` to the exact runtime allowlist, run `npm run prepare:runtime`, and stage the resulting committed provenance before tests. Only explicit compatible 404/unsupported listing enables manual model; all other discovery/probe failure fails closed. Store exact pricing and guarded-peer evidence in the immutable run binding.

  ```rust
  fn pricing_preflight_at(active: &ActiveAiConfiguration, budget: &AgentResourceBudget,
      pricing: Option<&AiPricingSnapshot>, now: &str) -> Result<(), ProviderFailure> {
      let Some(monetary) = &budget.monetary else { return Ok(()) };
      let snapshot = pricing.filter(|item| item.currency == monetary.currency && item.expires_at.as_str() > now)
          .ok_or_else(|| ProviderFailure::new(ProviderFailureCategory::RequestBudgetExceeded, false,
              "Approve current pricing before using a monetary budget.", None))?;
      (snapshot.snapshot_sha256 == active.pricing_identity.as_ref().map(|v| v.snapshot_sha256.as_str()).unwrap_or_default())
          .then_some(()).ok_or_else(|| protocol_failure(false))
  }
  ```
- [ ] Run GREEN and commit; confirm dev server remains running.

  ```powershell
  node scripts/run-ai-worker-tests.mjs
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::pricing::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::contract::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test ai_pricing --test ai_connection_repository --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test runtime_readiness --test safe_updates --test quantix_setup --features runtime-fixture
  npm run bindings:generate
  git add src-tauri/runtime/ai/quantix_ai_worker/probe.py src-tauri/runtime/ai/tests/test_probe.py src-tauri/runtime/ai/pricing-snapshot.v1.json src-tauri/src/ai/pricing.rs src-tauri/src/ai/mod.rs src-tauri/src/bin/export_bindings.rs src/bindings/GuardedPeerEvidence.ts src-tauri/tests/ai_pricing.rs src-tauri/src/ai/contract.rs src-tauri/src/ai/connections.rs src-tauri/src/agent_runtime/codex_app_server.rs src-tauri/src/setup.rs src-tauri/src/runtime_readiness.rs scripts/prepare-runtime.mjs src-tauri/runtime/runtime-provenance.json src-tauri/tauri.conf.json src-tauri/tests/ai_connection_repository.rs src-tauri/tests/codex_qualification.rs src-tauri/tests/runtime_readiness.rs src-tauri/tests/safe_updates.rs src-tauri/tests/quantix_setup.rs
  git commit -m "feat: qualify selected provider tuples and pricing"
  ```

### Task 9: Expose six-route connection lifecycle in Settings, IPC, and generated bindings

**Files:**

- Modify: `src-tauri/src/ai/contract.rs`
- Modify: `src-tauri/src/ai/connections.rs`
- Modify: `src-tauri/src/ai/vault.rs`
- Modify: `src-tauri/src/ai/pricing.rs`
- Modify: `src-tauri/src/setup.rs`
- Modify: `src-tauri/src/host.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/bin/export_bindings.rs`
- Modify: `src-tauri/tests/ai_connection_repository.rs`
- Modify: `src-tauri/tests/agent_runtime.rs`
- Modify: `src/ApplicationSettings.tsx`
- Modify: `src/ApplicationSettings.test.tsx`
- Modify: `src/applicationAiSelectionReadiness.ts`
- Create: `src/applicationAiSelectionReadiness.test.ts`
- Modify: `src/quantixHost.ts`
- Regenerate: `src/bindings/AiConnectionView.ts`
- Regenerate: `src/bindings/ApplicationAiSettingsView.ts`
- Regenerate: `src/bindings/CreateAiConnectionCommand.ts`
- Regenerate: `src/bindings/UpdateAiConnectionCommand.ts`
- Create by generation: `src/bindings/DiscoverAiModelsCommand.ts`
- Create by generation: `src/bindings/DiscoverAiModelsResult.ts`
- Regenerate: `src/bindings/SetActiveAiConfigurationCommand.ts`
- Regenerate: `src/bindings/TestAiConnectionCommand.ts`
- Regenerate: `src/bindings/ApproveCompatiblePricingCommand.ts`
- Regenerate: `src/bindings/ActiveAiConfiguration.ts`

**Interfaces:**

**IPC commands:**

```rust
#[tauri::command]
fn create_ai_connection(host: State<QuantixHost>, command: CreateAiConnectionCommand)
    -> Result<AiConnectionView, AiConnectionError>;
#[tauri::command]
fn update_ai_connection(host: State<QuantixHost>, command: UpdateAiConnectionCommand)
    -> Result<AiConnectionView, AiConnectionError>;
#[tauri::command]
fn delete_ai_connection(host: State<QuantixHost>, command: DeleteAiConnectionCommand)
    -> Result<(), AiConnectionError>;
#[tauri::command]
async fn discover_ai_models(host: State<QuantixHost>, command: DiscoverAiModelsCommand)
    -> Result<DiscoverAiModelsResult, AiConnectionError>;
#[tauri::command]
async fn test_ai_connection(host: State<QuantixHost>, command: TestAiConnectionCommand)
    -> Result<AiConnectionView, AiConnectionError>;
#[tauri::command]
fn approve_compatible_pricing(host: State<QuantixHost>, command: ApproveCompatiblePricingCommand)
    -> Result<AiConnectionView, AiConnectionError>;
#[tauri::command]
fn set_active_ai_configuration(host: State<QuantixHost>, command: SetActiveAiConfigurationCommand)
    -> Result<ApplicationAiSettingsView, AiConnectionError>;
#[tauri::command]
fn inspect_application_ai_settings(host: State<QuantixHost>)
    -> Result<ApplicationAiSettingsView, AiConnectionError>;
```

```rust
pub struct DiscoverAiModelsCommand {
    pub connection_id: String,
    pub expected_execution_revision: u64,
    pub disclosed_billable_discovery_accepted: bool,
}
pub struct DiscoverAiModelsResult {
    pub connection_id: String,
    pub execution_revision: u64,
    pub models: Vec<AiModelView>,
    pub catalogue_sha256: String,
    pub observed_at: String,
}
pub struct TestAiConnectionCommand {
    pub connection_id: String,
    pub expected_execution_revision: u64,
    pub selected_model_id: String,
    pub selected_reasoning: AiReasoningSelection,
}
pub struct ApproveCompatiblePricingCommand {
    pub connection_id: String,
    pub expected_execution_revision: u64,
    pub selected_model_id: String,
    pub currency: String,
    pub input_microunits_per_token: u64,
    pub cached_input_microunits_per_token: Option<u64>,
    pub output_microunits_per_token: u64,
    pub source_url: String,
    pub effective_at: String,
    pub expires_at: String,
}
```

`DiscoverAiModelsCommand` creates a `GeneralDraftOperation::Discover` projection and uses `spawn_probe` plus `run_discovery`; it returns at most 500 bounded non-secret model/reasoning options and never activates. `TestAiConnectionCommand` creates a separate `GeneralDraftOperation::Probe` projection for one exact discovered model/reasoning, runs `spawn_probe`/`run_probe`, and records evidence but never activates it. Both CAS the connection revision and use disclosed request budgets. `SetActiveAiConfigurationCommand` accepts only the canonical `ActiveAiConfiguration` fields: connection ID, execution revision, canonical `AiProviderKind`, endpoint fingerprint, selected model/reasoning, adapter version, catalogue hash, destination class, capabilities, data destination, and optional approved pricing snapshot hash. It requires fresh matching probe evidence. A missing/stale price does not block token/tool/time-only activation, but `pricing_preflight` blocks every later run that declares a monetary ceiling. No renderer command accepts or returns a credential, custom header/query value, or raw peer address.

The Settings flow is: create direct or compatible connection; enter compatible endpoint/header/query/auth placement only into write-only controls; press **Discover models** and accept its disclosed possibly billable request; choose one returned model/reasoning; press **Test connection** and accept the bounded qualification; optionally enter and approve compatible pricing to enable monetary ceilings; choose **Activate** only after matching evidence. Without approved fresh pricing, Settings states that monetary budgets are unavailable while token/tool/time limits remain usable. It never auto-activates after create/discover/test, never selects a provider/model/reasoning default, and displays guarded peer evidence only as class/hashed proof.

- [ ] Write RED Rust source-module tests for all six routes' create/update/delete/discover/test/approve/activate transitions, discovery/probe with no active configuration, stale CAS rejection, discover/test-not-active behavior, 500-model bound, optional pricing activation plus monetary-run preflight rejection, canonical provider-kind pairing, complete active-selection equality, no secret serialization, and a failed probe leaving prior activation unchanged.

  ```rust
  #[tokio::test]
  async fn discovery_and_test_never_activate_a_connection() {
      let fixture = settings_fixture(GeneralProviderRoute::DirectOpenAiResponses);
      let created = fixture.create_direct_connection().unwrap();
      fixture.discover(DiscoverAiModelsCommand {
          connection_id: created.connection_id.clone(), expected_execution_revision: created.execution_revision,
          disclosed_billable_discovery_accepted: true,
      }).await.unwrap();
      fixture.test_connection(TestAiConnectionCommand::for_model(&created, "gpt-test", AiReasoningSelection::Unsupported)).await.unwrap();
      assert!(fixture.inspect().unwrap().active_configuration.is_none());
  }
  ```

  `settings_fixture(route)` creates an installation SQLite/vault pair under a temp directory and injects the route's committed worker transcript; its direct helper returns a credential-bearing command whose `Debug` and rendered projection are redacted.

  Expected RED failure: no Host-owned discovery/test commands exist and the Settings projection cannot distinguish tested from active.
- [ ] Write RED renderer tests for direct OpenAI/Anthropic/Gemini/xAI and both compatible forms: required fields, named-key versus bearer UI, header/query bounded validation, disclosed test warning, probe state, incompatible evidence, explicit pricing approval, explicit activation, redacted view, and no automatic selection. Test `applicationAiSelectionReadiness` against every active-configuration field rather than the old ChatGPT singleton.

  ```tsx
  it("requires a separate Activate click after a successful compatible probe", async () => {
    host.discoverAiModels.mockResolvedValue(discoveredCompatibleView());
    host.testAiConnection.mockResolvedValue(testedCompatibleView());
    renderSettings();
    await userEvent.click(screen.getByRole("button", { name: "Test connection" }));
    expect(host.setActiveAiConfiguration).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Activate" }));
    expect(host.setActiveAiConfiguration).toHaveBeenCalledTimes(1);
  });
  ```

  Expected RED failure: the renderer has neither discovery/probe controls nor the explicit Activate action.
- [ ] Run RED:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::connections::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_repository --features runtime-fixture
  npm run test:renderer -- ApplicationSettings applicationAiSelectionReadiness
  ```

- [ ] Implement Host ownership, commands, bindings, `src/quantixHost.ts` invocations, and Settings state machine. Register all commands in `generate_handler!`; remove old ChatGPT-only selection controls rather than keeping compatibility controls.

  ```ts
  export const discoverAiModels = (command: DiscoverAiModelsCommand) =>
    invoke<DiscoverAiModelsResult>("discover_ai_models", { command });
  export const testAiConnection = (command: TestAiConnectionCommand) =>
    invoke<AiConnectionView>("test_ai_connection", { command });
  export const setActiveAiConfiguration = (command: SetActiveAiConfigurationCommand) =>
    invoke<ApplicationAiSettingsView>("set_active_ai_configuration", { command });
  ```
- [ ] Run GREEN, regenerate bindings through `npm test`, and commit:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::connections::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_repository --features runtime-fixture
  npm run test:renderer -- ApplicationSettings applicationAiSelectionReadiness
  npm test
  git add src-tauri/src/ai/contract.rs src-tauri/src/ai/connections.rs src-tauri/src/ai/vault.rs src-tauri/src/ai/pricing.rs src-tauri/src/setup.rs src-tauri/src/host.rs src-tauri/src/lib.rs src-tauri/src/bin/export_bindings.rs src-tauri/tests/ai_connection_repository.rs src-tauri/tests/agent_runtime.rs src/ApplicationSettings.tsx src/ApplicationSettings.test.tsx src/applicationAiSelectionReadiness.ts src/applicationAiSelectionReadiness.test.ts src/quantixHost.ts src/bindings
  git commit -m "feat: manage and activate six general provider routes"
  ```

  Confirm the main dev server remains running.

### Task 10: Pass the deterministic pre-dispatch checkpoint

**Files:** None. This is the deterministic gate between connection/runtime construction and Agent Run dispatch; do not add a live credential, Test Project, acceptance Application Home, or release candidate.

**Interfaces:**

- Consumes Tasks 0-9 and produces a reviewed, credential-free pre-dispatch checkpoint; it creates no production API.

- [ ] Run the complete pre-dispatch source and public-boundary suite:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::worker::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::connections::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --lib ai::pricing::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test ai_worker_contract --test ai_connection_repository --test ai_pricing --test runtime_readiness --features runtime-fixture
  node scripts/run-ai-worker-tests.mjs
  npm run test:renderer -- ApplicationSettings applicationAiSelectionReadiness
  ```

- [ ] Have an independent reviewer confirm that the six Settings routes can be created/tested/explicitly activated only with matching evidence, pricing, and guarded-peer proof; that no Agent Run dispatch has been added yet; and that all test fixtures remain credential-free.
- [ ] Record the deterministic result in the task review/commit message. Confirm the main dev server remains running.

### Task 11: Dispatch immutable Agent Runs through the worker

**Files:**

- Modify: `src-tauri/src/ai/runtime.rs`
- Modify: `src-tauri/src/ai/worker.rs`
- Modify: `src-tauri/src/agent_runtime.rs`
- Modify: `src-tauri/src/agent_runtime/permissions.rs`
- Modify: `src-tauri/src/tender_store.rs`
- Modify: `src-tauri/src/tender_store/agent_records.rs`
- Modify: `src-tauri/src/runtime_readiness.rs`
- Modify: `src-tauri/tests/agent_runtime.rs`
- Modify: `src-tauri/tests/ai_connection_repository.rs`
- Regenerate: `src/bindings/AgentResourceBudget.ts`
- Regenerate: `src/bindings/ProviderUsage.ts`
- Regenerate: `src/bindings/ActiveAiConfiguration.ts`

**Interfaces:**

```rust
impl QuantixHost {
    pub(crate) async fn execute_general_provider_turn(
        &self,
        run_id: &str,
        active: &ActiveAiConfiguration,
        request: WorkerTurnRequest,
        cancellation: CancellationToken,
    ) -> ProviderExecution;
}
```

`execute_general_provider_turn` revalidates the Task 2 vault projection, uses Task 4's single `GeneralWorkerClient::spawn`, and verifies `request.run_id == run_id` before any process starts. It sends shutdown then kills/reaps on terminal/cancel/error. Agent runtime routes every non-Codex selection exclusively to it and implements the awaited Task 7 Host callbacks using the Task 0 committed receipts. It consumes the already-expanded explicit token/tool/monetary budget and usage types from Task 0; bindings are generated only by tests.

- [ ] Write RED tests for one process/one request in all six routes, immutable selection/pricing capture, monetary preflight, persistence before continuation, reroute, cancellation/late/Drop cleanup, vault race, output caps, and no sanctioned Tender/database/index path. Re-run FastEmbed/sqlite-vec parsing/search regression.

  ```rust
  #[tokio::test]
  async fn general_turn_uses_its_immutable_selected_route_once() {
      let fixture = general_turn_fixture(GeneralProviderRoute::GeminiDeveloperApi);
      let run = fixture.prepare_run_with_active_selection().await;
      let result = fixture.host.execute_general_provider_turn(&run.id, &run.active, run.request, CancellationToken::new()).await;
      assert_eq!(fixture.worker_process_count(), 1);
      assert_eq!(fixture.provider_request_count(), 1);
      assert_eq!(result.usage.reported_model_id.as_deref(), Some("gemini-test"));
      assert_eq!(fixture.persisted_run(&run.id).pricing_identity, run.active.pricing_identity);
  }
  ```

  `general_turn_fixture(route)` prepares a temporary Tender, immutable active binding, fake provider endpoint, and empty workspace; its process/request counters are incremented only by the supervised worker fixture.

  Expected RED failure: non-Codex selections are not dispatched through `execute_general_provider_turn`.
- [ ] Run RED:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib agent_runtime::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_repository --test agent_runtime --features runtime-fixture
  ```
- [ ] Implement dispatch, no legacy/default/fallback branch, then GREEN/full gate/commit:

  ```rust
  match active.provider {
      AiProviderKind::Codex => unreachable!("the Slice 1 Codex lane dispatches separately"),
      AiProviderKind::OpenAi | AiProviderKind::Anthropic | AiProviderKind::GoogleGemini |
      AiProviderKind::XAi | AiProviderKind::OpenAiCompatible | AiProviderKind::AnthropicCompatible => {
          self.execute_general_provider_turn(run_id, active, request, cancellation).await
      }
  }
  ```

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib agent_runtime::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test ai_connection_repository --test agent_runtime --features runtime-fixture
  npm run format:check
  npm run check
  npm test
  git add src-tauri/src/ai src-tauri/src/agent_runtime.rs src-tauri/src/agent_runtime/permissions.rs src-tauri/src/tender_store.rs src-tauri/src/tender_store/agent_records.rs src-tauri/src/runtime_readiness.rs src-tauri/tests/agent_runtime.rs src-tauri/tests/ai_connection_repository.rs src/bindings
  git commit -m "feat: dispatch general provider runs through pinned SDK worker"
  ```

  Confirm the main dev server remains running.

### Task 12: Add deterministic post-dispatch evidence and pass the Pre-Integration Gate

**Files:**

- Modify: `src-tauri/src/acceptance.rs`
- Modify: `src-tauri/src/bin/export_bindings.rs`
- Modify: `src-tauri/tests/release_configuration.rs`
- Modify: `src-tauri/tests/agent_runtime.rs`
- Create by generation: `src/bindings/AiRuntimeEvidence.ts`
- Create by generation: `src/bindings/GeneralProviderRuntimeEvidence.ts`

**Interfaces:**

```rust
pub enum AiRuntimeEvidence {
    CodexManaged(CodexManagedRuntimeEvidence),
    GeneralProvider(GeneralProviderRuntimeEvidence),
}
pub struct GeneralProviderRuntimeEvidence {
    pub route: String,
    pub connection_id: String,
    pub execution_revision: u64,
    pub destination_fingerprint: String,
    pub selected_model_id: String,
    pub selected_reasoning: AiReasoningSelection,
    pub reported_model_id: String,
    pub capability_sha256: String,
    pub pricing_sha256: Option<String>,
    pub python_version: String,
    pub worker_lock_sha256: String,
    pub pydantic_ai_version: String,
    pub provider_sdk_name: String,
    pub provider_sdk_version: String,
    pub request_count: u32,
    pub automatic_retry_count: u32,
    pub guarded_peer_evidence: Option<GuardedPeerEvidence>,
}
```

The existing `RecordLiveQualificationRunCommand` remains the only CLI input and contains only opt-in, candidate paths, and deterministic-record hash. The chosen connection and credential stay in the DPAPI vault under the acceptance Application Home and never enter the command/evidence JSON.

- [ ] Write RED acceptance tests that require exact immutable selection, destination, reported identity, lock/SDK versions, one request, zero automatic retries, budget/loop outcome, and no credential/header/query value. A direct-route fixture must serialize `guarded_peer_evidence: None`; each compatible-route fixture must require matching `Some(GuardedPeerEvidence)` and reject missing/mismatched peer proof.

  ```rust
  #[test]
  fn compatible_live_evidence_requires_the_committed_guarded_peer_fact() {
      let mut evidence = fixture_general_provider_evidence(GeneralProviderRoute::OpenAiCompatibleChat);
      evidence.guarded_peer_evidence = None;
      assert!(validate_general_provider_evidence(&evidence).is_err());
      evidence.guarded_peer_evidence = Some(fixture_guarded_peer_evidence());
      assert!(validate_general_provider_evidence(&evidence).is_ok());
  }
  ```

  `fixture_general_provider_evidence(route)` returns committed, secret-free fixture facts; `fixture_guarded_peer_evidence()` uses hashes of fixed public test addresses and `redirects_followed = false`.

  Expected RED failure: the general-provider evidence variant and validation routine do not exist.
- [ ] Run RED:

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib acceptance::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test release_configuration --test agent_runtime --features runtime-fixture general_provider_live_evidence -- --nocapture
  ```

- [ ] Implement the `AiRuntimeEvidence` tagged union, populate its general branch only from committed Agent Run/provider events, register both DTOs with the owned binding exporter, and run GREEN plus generation:

  ```rust
  let evidence = AiRuntimeEvidence::GeneralProvider(GeneralProviderRuntimeEvidence {
      route: route.as_str().into(), connection_id: active.connection_id.to_string(),
      execution_revision: active.execution_revision.get(), destination_fingerprint: active.endpoint_fingerprint.clone(),
      selected_model_id: active.model_id.clone(), selected_reasoning: active.reasoning.clone(),
      reported_model_id: usage.reported_model_id.clone().ok_or_else(|| protocol_failure(false))?,
      capability_sha256: active.capability_sha256.clone(), pricing_sha256: active.pricing_identity.as_ref().map(|v| v.snapshot_sha256.clone()),
      python_version, worker_lock_sha256, pydantic_ai_version, provider_sdk_name, provider_sdk_version,
      request_count: usage.request_count, automatic_retry_count: 0, guarded_peer_evidence,
  });
  ```

  ```powershell
  cargo test --manifest-path src-tauri/Cargo.toml --lib acceptance::tests --features runtime-fixture
  cargo test --manifest-path src-tauri/Cargo.toml --test release_configuration --test agent_runtime --features runtime-fixture general_provider_live_evidence
  npm test
  git add src-tauri/src/acceptance.rs src-tauri/src/bin/export_bindings.rs src-tauri/tests/release_configuration.rs src-tauri/tests/agent_runtime.rs src/bindings
  git commit -m "test: record general SDK worker qualification"
  ```

- [ ] This is the final pre-integration gate. Run `npm run verify`, obtain independent spec/code review for Tasks 0-12, and confirm deterministic/fixture evidence plus generated bindings are committed. It must not use a live credential, Test Project, or release candidate. Confirm the main dev server remains running.

### Task 13: Run the Post-Integration interactive live qualification against the normal Application Home

**Files:** None. This is an opted-in acceptance operation against the code and evidence committed by Task 12; do not edit source or build production artifacts.

**Interfaces:**

- Consumes the reviewed slice integrated on `main`, the still-running main dev app, one separately installed unchanged candidate, and the candidate's normal Application Home.
- Produces one dev smoke run and one opt-in formal live qualification record; it creates no production API.

- [ ] **Step 1: Run the post-integration smoke path in the existing main dev app.**

  Confirm the already-running main dev app rebuilt the integrated revision. In its Settings, create one opted-in direct-provider API-key connection through the Task 9 write-only UI, run the disclosed probe, explicitly activate its exact model/reasoning, import `C:\Users\kareem\Desktop\Test Project`, and complete one governed Agent Run. Verify one worker process, one provider request, exact immutable selection/pricing evidence, and no fallback. Leave the main dev app/server running.

- [ ] **Step 2: Configure the formal candidate in a separate sanitized account/VM.**

  Use a sanitized Windows account/VM whose profile has no unrelated files. Install and launch the unchanged release-stage candidate normally, then establish its normal Application Home:

  ```powershell
  $acceptanceRoot = 'C:\QuantixAcceptance\general-sdk-worker'
  $acceptanceHome = Join-Path $env:USERPROFILE '.quantix'
  $candidateRoot = Join-Path $acceptanceRoot 'candidate'
  [IO.Directory]::CreateDirectory($acceptanceRoot) | Out-Null
  if (-not [IO.Path]::GetFullPath($acceptanceHome).Equals(
      [IO.Path]::GetFullPath((Join-Path $env:USERPROFILE '.quantix')),
      [StringComparison]::OrdinalIgnoreCase
  )) { throw 'acceptance must use the normal Quantix Application Home' }
  ```

  In candidate Settings, enter the API key through the write-only control, run the disclosed probe, explicitly activate its exact model/reasoning, import the small Test Project copy, and complete one Agent Run. Verify the credential exists only at `$acceptanceHome\ai-connections.vault`, then close the candidate before CLI acceptance. Do not set `QUANTIX_APPLICATION_HOME`, copy a vault from the dev profile, or start a second debug/Tauri process. The API key must not appear in PowerShell, command JSON, argv, logs, diagnostics, or evidence. The original main dev server remains running in its original environment.

- [ ] **Step 3: Create and aggregate the formal deterministic record in that same home.**

  Run from the integrated repository in the sanitized account:

  ```powershell
  $sourceRevision = (& git rev-parse HEAD).Trim()
  $deterministicCommandPath = Join-Path $acceptanceRoot 'general-provider-deterministic.json'
  $deterministicCommand = [ordered]@{
      source_revision = $sourceRevision
      application_artifact_path = (Join-Path $candidateRoot 'quantix.exe')
      application_resource_directory_path = (Join-Path $candidateRoot 'resources')
      dependency_lock_path = (Resolve-Path -LiteralPath 'src-tauri\runtime\ai\uv.lock').Path
  }
  [IO.File]::WriteAllText(
      $deterministicCommandPath,
      ($deterministicCommand | ConvertTo-Json -Compress),
      [Text.UTF8Encoding]::new($false)
  )
  npm run acceptance:deterministic -- $acceptanceHome $deterministicCommandPath
  if ($LASTEXITCODE -ne 0) { throw 'general-provider deterministic acceptance failed' }
  $aggregateOutput = & npm run acceptance:aggregate -- $acceptanceHome $sourceRevision 2>&1
  $aggregateOutput | ForEach-Object { Write-Host $_ }
  if ($LASTEXITCODE -ne 0) { throw 'general-provider deterministic aggregation failed' }
  $aggregateJson = $aggregateOutput | Where-Object { $_ -match '^\{' } | Select-Object -Last 1 | ConvertFrom-Json
  $recordHash = [string]$aggregateJson.manifest_sha256
  if ($recordHash -notmatch '^[0-9a-f]{64}$') { throw 'invalid deterministic record hash' }
  [IO.File]::WriteAllText(
      (Join-Path $acceptanceRoot 'deterministic-record-sha256.txt'),
      $recordHash,
      [Text.UTF8Encoding]::new($false)
  )
  ```

- [ ] **Step 4: Record one live run against that exact candidate home.**

  Against the unchanged candidate and the same `$acceptanceHome`/vault configured in Step 2, create the non-secret command and record one live run:

  ```powershell
  $recordHash = (Get-Content -LiteralPath (Join-Path $acceptanceRoot 'deterministic-record-sha256.txt') -Raw).Trim().ToLowerInvariant()
  if ($recordHash -notmatch '^[0-9a-f]{64}$') { throw 'invalid deterministic record hash' }
  if (-not (Test-Path -LiteralPath (Join-Path $acceptanceHome 'ai-connections.vault') -PathType Leaf)) {
      throw 'Settings did not create the credential vault in the acceptance Application Home'
  }
  $commandPath = Join-Path $acceptanceRoot 'general-provider-live.json'
  $command = [ordered]@{
      opted_in = $true
      application_artifact_path = (Join-Path $candidateRoot 'quantix.exe')
      application_resource_directory_path = (Join-Path $candidateRoot 'resources')
      application_uninstaller_path = (Join-Path $candidateRoot 'uninstall.exe')
      deterministic_acceptance_record_sha256 = $recordHash
  }
  [IO.File]::WriteAllText($commandPath, ($command | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
  npm run acceptance:live -- $acceptanceHome $commandPath
  if ($LASTEXITCODE -ne 0) { throw 'general-provider live acceptance failed' }
  ```

  The returned record must contain `AiRuntimeEvidence::GeneralProvider`, the exact selected tuple and SDK lock identity, one request, zero automatic retries, and no secret. Do not run a production build in this task; leave the development server running.

## Plan Completion Gate

Slice 2 completes only when the Task 12 Pre-Integration Gate and all six-route deterministic route-pairing, exact-request-count, guarded-peer-before-write, JSONL, retry-disabled, vault-projection, awaited Host-persistence, cancellation, pricing, Settings/IPC activation, and cleanup tests pass; one explicitly opted-in direct-key route passes Task 13 using the formal candidate's same normal Application Home and vault; `npm run verify` exits zero; generated bindings are committed; committed runtime provenance is not ignored; no direct connector/default/fallback remains; and the existing main dev server remained running for the full slice. An independent reviewer must find no Critical or Important issue.
