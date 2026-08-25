# Layer 1B Supervised AI Workers Implementation Plan

> **Superseded — do not execute.** ADR 0018 and the
> [SDK-first runtime design](../specs/2026-08-25-sdk-first-ai-runtime-cutover-design.md)
> replace its Codex authentication, home lifecycle, MCP, and worker assumptions.
> A new plan will be written after the revised design is approved.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and package the two replaceable provider workers behind the Layer 1A semantic contract: direct Codex app-server for account execution and a Pydantic AI worker for direct-key and compatible endpoints.

**Architecture:** The Rust Host directly supervises Codex app-server JSON-RPC and separately supervises a strict Quantix JSONL Python worker. Both normalize into `ai::contract`, expose only Host-declared tools, disable their own retry/fallback authority, and are tested with deterministic fixtures before any visible product cutover.

**Tech Stack:** Tokio, ProcessKit, Serde, pinned official Codex Windows runtime 0.149.1, uv 0.12.2, Python 3.12.13, Pydantic AI 2.33.0, provider SDKs locked by uv, Python `unittest`.

**Spec:** `docs/superpowers/specs/2026-08-24-layer-1-ai-connection-foundation-design.md`, parent plan `docs/superpowers/plans/2026-08-24-layer-1-ai-connection-foundation.md`, and Layer 1A contract.

## Global Constraints

- Follow every parent-plan constraint. The current renderer and Agent Run dispatch remain unchanged in this subplan.
- No live account or API key appears in deterministic tests. All providers are simulated by local fixtures or Pydantic `TestModel`/`FunctionModel`.
- Worker secrets travel only in private stdio payloads; never argv, environment, stderr, test snapshots, or normalized output.
- The uv-locked Python interpreter and dependency set are trusted worker code. ProcessKit provides lifecycle containment only; it does not make a compromised dependency safe.
- Never instantiate Pydantic `FallbackModel`. Set Pydantic tool/output retries and every general-provider SDK transport retry to zero. Accept only the pinned Codex built-in provider's documented fixed bounded recovery; add no Host retry around Codex.
- Do not enable provider-native web search, code execution, MCP, file access, computer use, or other native tools in Layer 1.
- Codex shell, unified exec, web search, image view, apps/plugins, memories, multi-agent, hooks, update checks, and feedback are disabled where supported. Quantix executes only exact dynamic tools declared by the Host; any surfaced built-in action (including apply-patch/file behavior that app-server cannot fully hide) is a security failure and terminates the turn.
- A worker's advertised capability is `supported` only after the exact selected model passes the bounded probe. Unknown remains unknown.
- The Host accepts no rerouted/substituted model. A reroute event is a normalized `ModelRerouted` failure.
- Codex `dynamicTools` is experimental. Pin it to the exact app-server protocol fixture and fail the tools capability closed if that request/response contract changes.

---

### Task 1: Promote conversation supervision to a production primitive

**Files:**

- Modify: `src-tauri/src/process_supervisor.rs`
- Modify: `src-tauri/tests/runtime_readiness.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`

**Interfaces:**

- Production `ProcessSupervisor::start_conversation`.
- Bounded `SupervisedConversation::{begin_operation, write_frame, read_frame, terminate}`.
- One independent lifetime budget plus per-operation stdin/stdout/stderr budgets.

```rust
pub async fn start_conversation(
    &self,
    spec: ProcessSpec,
    policy: ConversationPolicy,
    operation_cancellation: CancellationToken,
) -> Result<SupervisedConversation, ProcessError>;

pub struct ConversationPolicy {
    pub max_memory_bytes: u64,
    pub max_processes: u32,
    pub lifetime: Duration,
    pub max_frames: u32,
    pub max_total_stdout_bytes: usize,
}
```

- [ ] **Write the first red cancellation/Drop test**

```rust
#[tokio::test]
async fn cancelled_operation_can_send_control_then_drop_kills_tree() {
    let (mut conversation, trace) = fixture_conversation("wait-for-cancel").await;
    conversation.operation_cancellation().cancel();
    conversation.write_control_frame(br#"{"type":"cancel"}"#).await.unwrap();
    drop(conversation);
    trace.assert_received("cancel");
    trace.assert_no_live_descendants();
}
```

- [ ] **Run it and verify red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib process_supervisor::tests::cancelled_operation_can_send_control_then_drop_kills_tree`

Expected: FAIL because `write_control_frame`/the Drop guard do not exist.

- [ ] **Add the minimal lifetime/control-write state**

```rust
impl SupervisedConversation {
    pub async fn write_control_frame(&mut self, frame: &[u8]) -> Result<(), ProcessError> {
        self.ensure_lifetime_and_frame_budget(frame)?;
        timeout_at(self.control_deadline, self.stdin_mut()?.write_all(frame))
            .await
            .map_err(|_| ProcessError::TimedOut)??;
        self.stdin_mut()?.write_all(b"\n").await.map_err(|_| ProcessError::ObservationFailed)
    }
}

impl Drop for SupervisedConversation {
    fn drop(&mut self) {
        self.child.terminate_group_now();
    }
}
```

- [ ] Write failing tests for newline framing, split reads, maximum frame size, total output limit, per-operation reset, timeout, cancellation, clean shutdown, crash, and stderr overflow.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml --lib process_supervisor::tests`; expect failures because the production framing/lifecycle methods do not exist.
- [ ] Move the current `SupervisedConversation` implementation out of `#[cfg(any(test, feature = "runtime-fixture"))]` without weakening its existing Windows Job Object containment.
- [ ] Add `write_frame` and `read_frame` wrappers that enforce UTF-8 JSONL, reject embedded newline framing attacks, and cap one frame at 1 MiB.
- [ ] Allow `begin_operation` only when there is no buffered partial frame, unresolved server/tool request, terminal state, or prior limit breach. Lifetime counters never reset; a breached conversation cannot begin another operation.
- [ ] Keep `inherit_environment: false` as the provider-worker default. Add an explicit environment allowlist helper instead of copying the parent environment.
- [ ] Add a `Drop`/early-return kill guard so every error path terminates and reaps the complete process group. Enforce global process lifetime, cumulative bytes, frame count, operation count, and per-operation budgets in addition to Agent Run budgets.
- [ ] Enable ProcessKit's `limits` feature. Create Windows groups with a 1 GiB `max_memory`; use `max_processes(2)` for the Python worker and `max_processes(4)` for Codex. Read `limit_evidence` after creation and fail before secret transfer unless memory/process limits are enforced.
- [ ] Add executable Windows fixtures that exceed memory and active-process limits; assert Job enforcement, typed failure, and complete cleanup. Timeout/output limits remain independently tested.
- [ ] Add a bounded `write_control_frame` path governed by the supervisor lifetime/grace deadline rather than the already-cancelled operation token, so `cancel`, `turn/interrupt`, and `shutdown` can still be sent. After a fixed five-second terminal grace period, terminate and reap the complete ProcessKit group.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml --lib process_supervisor::tests`; expect all supervisor tests to pass.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml --test runtime_readiness --features runtime-fixture`; expect success.
- [ ] Commit as `feat: supervise production provider conversations`.

---

### Task 2: Define the strict general-worker JSONL protocol and fixture

**Files:**

- Create: `src-tauri/src/ai/worker.rs`
- Create: `src-tauri/tests/ai_worker_contract.rs`
- Create: `src-tauri/tests/support/ai_worker_fixture.rs`
- Create: `src-tauri/tests/fixtures/ai_worker/happy.jsonl`
- Create: `src-tauri/tests/fixtures/ai_worker/tool-roundtrip.jsonl`
- Create: `src-tauri/tests/fixtures/ai_worker/malformed-sequence.jsonl`
- Create: `src-tauri/tests/fixtures/ai_worker/model-reroute.jsonl`
- Modify: `src-tauri/src/ai/mod.rs`
- Modify: `src-tauri/Cargo.toml`

**Wire shape:** The `probe` and `turn_start` lines below are alternatives; one worker operation sends exactly one of them.

```json
{"protocol_version":1,"request_id":"0123456789abcdef0123456789abcdef","type":"initialize","connection":{}}
{"protocol_version":1,"request_id":"0123456789abcdef0123456789abcdef","type":"probe","probe":{}}
{"protocol_version":1,"request_id":"0123456789abcdef0123456789abcdef","type":"turn_start","turn":{}}
{"protocol_version":1,"request_id":"0123456789abcdef0123456789abcdef","type":"tool_result","call_id":"call_01","result":{}}
{"protocol_version":1,"request_id":"0123456789abcdef0123456789abcdef","type":"cancel"}
{"protocol_version":1,"request_id":"0123456789abcdef0123456789abcdef","type":"shutdown"}
```

`initialize` is the mandatory handshake. A worker's `ready` frame has `sequence: 0`; every later worker frame increments by exactly one for that request. Worker frames use only `ready`, `event`, `tool_call`, `probe_result`, `terminal`, or `failure`. Both `terminal` and `failure` are final kinds; exactly one of them must end the operation.

**Interfaces:**

- Consumes: production `SupervisedConversation`, `AiRuntimeRequest`, and exact connection secret snapshot.
- Produces: `GeneralWorkerClient::probe`, `GeneralWorkerClient::run_turn`, and `GeneralWorkerClient::cancel`, returning only Layer 1A semantic events/results.

- [ ] **Write the red sequence/final-state test**

```rust
#[tokio::test]
async fn worker_rejects_gap_and_second_final_frame() {
    let mut client = fixture_client("malformed-sequence").await;
    let error = client.run_turn(fake_turn()).await.unwrap_err();
    assert_eq!(error.category, AiRuntimeFailureCategory::ProtocolDrift);
    assert_eq!(client.executed_tool_count(), 0);
}
```

- [ ] **Run it and verify red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ai_worker_contract worker_rejects_gap_and_second_final_frame --features runtime-fixture`

Expected: FAIL because `GeneralWorkerClient` and the fixture do not exist.

- [ ] **Implement the protocol state machine skeleton**

```rust
enum WorkerState { AwaitingReady, Running { next_sequence: u64 }, Final }

fn accept_frame(state: &mut WorkerState, frame: WorkerFrame) -> Result<Option<AiRuntimeResult>, AiRuntimeFailure> {
    match (state, frame) {
        (WorkerState::AwaitingReady, WorkerFrame::Ready { sequence: 0, .. }) => {
            *state = WorkerState::Running { next_sequence: 1 };
            Ok(None)
        }
        (WorkerState::Running { next_sequence }, frame) if frame.sequence() == *next_sequence => {
            *next_sequence += 1;
            if frame.is_final() { *state = WorkerState::Final; }
            frame.into_semantic_result()
        }
        _ => Err(AiRuntimeFailure::protocol_drift()),
    }
}
```

- [ ] Register a `quantix-ai-worker-fixture` binary behind `runtime-fixture` and make it replay only named committed scenarios.
- [ ] Write failing contract tests for handshake version mismatch, request ID mismatch, unknown field/type, frame overflow, duplicate/gapped/out-of-order sequence, tool result for an unknown call, terminal exactly once, event after terminal, and secret-shaped output fields.
- [ ] Implement strict Serde tagged unions with `deny_unknown_fields` and convert only validated frames into `AiRuntimeEvent`/`AiRuntimeResult`.
- [ ] Serialize secret-bearing initialize/login frames into `Zeroizing<Vec<u8>>`, write them once, then zero the caller buffer immediately. The conversation object must not retain sent-frame history.
- [ ] Ensure the client recognizes authentication, rate limit, quota, capability, invalid output, cancellation, timeout, crash, protocol, and indeterminate failures without parsing raw provider prose.
- [ ] Add a test that a failed operation spawns or contacts no fallback worker.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml --test ai_worker_contract --features runtime-fixture`; expect all fixture contract tests to pass.
- [ ] Commit as `feat: define the supervised ai worker protocol`.

---

### Task 3: Create the locked Python worker and matching protocol tests

**Files:**

- Create: `src-tauri/runtime/ai/pyproject.toml`
- Create: `src-tauri/runtime/ai/uv.lock`
- Create generated: `src-tauri/runtime/ai/THIRD_PARTY_LICENSES.json`
- Create: `src-tauri/runtime/ai/quantix_ai_worker/__init__.py`
- Create: `src-tauri/runtime/ai/quantix_ai_worker/__main__.py`
- Create: `src-tauri/runtime/ai/quantix_ai_worker/protocol.py`
- Create: `src-tauri/runtime/ai/tests/__init__.py`
- Create: `src-tauri/runtime/ai/tests/fakes.py`
- Create: `src-tauri/runtime/ai/tests/test_protocol.py`
- Create: `scripts/run-ai-worker-tests.mjs`
- Modify: `package.json`

**Pinned project:**

```toml
[project]
name = "quantix-ai-worker"
version = "0.0.0"
requires-python = "==3.12.13"
dependencies = [
  "pydantic-ai-slim[anthropic,google,openai,xai]==2.33.0",
]

[build-system]
requires = ["uv_build==0.12.2"]
build-backend = "uv_build"

[tool.uv.build-backend]
module-root = ""
module-name = "quantix_ai_worker"
```

**Interfaces:**

- Consumes: version-1 Host JSONL frames on stdin.
- Produces: strict `WorkerFrame` JSONL on stdout; stderr is non-protocol and always redacted/discarded by the Host.

- [ ] **Write the red Python protocol-model test**

```python
class ProtocolTests(unittest.TestCase):
    def test_secret_input_cannot_validate_as_output(self) -> None:
        request = InitializeFrame.model_validate({
            "protocol_version": 1,
            "request_id": "0" * 32,
            "type": "initialize",
            "connection": {"api_key": "secret-value"},
        })
        self.assertEqual(request.connection.api_key.get_secret_value(), "secret-value")
        with self.assertRaises(ValidationError):
            WorkerFrameAdapter.validate_python(request.model_dump())
```

- [ ] **Run the Python test and confirm red**

Run through `node scripts/run-ai-worker-tests.mjs`.

Expected: FAIL because the installable worker/protocol models do not exist.

- [ ] **Implement the strict base models and entrypoint**

```python
class StrictFrame(BaseModel):
    model_config = ConfigDict(extra="forbid", hide_input_in_errors=True)
    protocol_version: Literal[1]
    request_id: Annotated[str, StringConstraints(pattern=r"^[0-9a-f]{32}$")]

class ReadyFrame(StrictFrame):
    type: Literal["ready"] = "ready"
    sequence: Literal[0] = 0

def emit(frame: WorkerOutputFrame) -> None:
    sys.stdout.write(frame.model_dump_json() + "\n")
    sys.stdout.flush()
```

- [ ] Run `npm run prepare:runtime`, then write protocol tests and run `node scripts/run-ai-worker-tests.mjs`; expect import/module failures before the worker exists.
- [ ] Implement strict Pydantic protocol models mirroring the Rust tagged unions. Set `extra="forbid"`, `hide_input_in_errors=True`, byte/count limits, and `SecretStr`/explicit secret input models that cannot appear in any output union.
- [ ] Make `__main__.py` reserve stdout for JSONL frames and configure Python/provider logging to a redacted bounded stderr sink. A bare exception emits a typed failure, never `repr(request)`.
- [ ] After constructing provider clients, release Python request-model credential/header/query references as early as practical; do not claim reliable Python string zeroization. Terminate the run-scoped process at operation end so no reusable secret-bearing worker state survives.
- [ ] Install the worker package non-editably with frozen locked resolution into `.dev/runtime-provisioning/ai-worker-venv` for development/tests and `~/.quantix/runtimes/ai/venv` in the installed app. Launch the exact venv's `python.exe -I -m quantix_ai_worker`; isolated mode must ignore user site packages, `PYTHONPATH`, and the current directory. Fail if `uv.lock` changes.
- [ ] In `scripts/run-ai-worker-tests.mjs`, set `cwd` to the absolute `src-tauri/runtime/ai`, set `UV_PROJECT_ENVIRONMENT` to the absolute `.dev/runtime-provisioning/ai-worker-venv`, and run staged uv with `run --locked --no-editable python -I -m unittest discover -s tests -t .`. Production `ProcessSpec.current_directory` is the exact empty Agent Run workspace, not the worker source tree.
- [ ] Generate `src-tauri/runtime/ai/THIRD_PARTY_LICENSES.json` from the exact uv lock, verify distribution compatibility, and bind its hash into runtime provenance. Unknown or incompatible license metadata blocks packaging.
- [ ] Add exact script `"test:ai-worker": "npm run prepare:runtime && node scripts/run-ai-worker-tests.mjs"` and include `npm run test:ai-worker` once in `npm test` before Rust tests. A clean checkout's documented `npm test` must prepare ignored runtime tools itself.
- [ ] Run `npm run test:ai-worker`; expect all protocol tests to pass.
- [ ] Run `src-tauri/runtime/bin/uv.exe lock --project src-tauri/runtime/ai`, stage the intentional lock, then run `src-tauri/runtime/bin/uv.exe lock --check --project src-tauri/runtime/ai` and `git diff --exit-code -- src-tauri/runtime/ai/uv.lock`; expect no second change.
- [ ] Commit as `feat: add the locked general ai worker`.

---

### Task 4: Implement general provider factories with hidden retries disabled

**Files:**

- Create: `src-tauri/runtime/ai/quantix_ai_worker/model_factory.py`
- Create: `src-tauri/runtime/ai/quantix_ai_worker/general_adapter.py`
- Create: `src-tauri/runtime/ai/quantix_ai_worker/host_bridge.py`
- Create: `src-tauri/runtime/ai/tests/test_general_adapter.py`

**Provider mapping:**

```python
# Direct OpenAI
OpenAIResponsesModel(model_id, provider=OpenAIProvider(openai_client=client))

# Custom OpenAI-compatible
OpenAIChatModel(model_id, provider=OpenAIProvider(openai_client=client))

# Direct/custom Anthropic
AnthropicModel(model_id, provider=AnthropicProvider(anthropic_client=client))

# Gemini / xAI
GoogleModel(model_id, provider=GoogleProvider(api_key=secret))
XaiModel(model_id, provider=XaiProvider(xai_client=xai_client))
```

**Interfaces:**

- Consumes: `ConnectionRuntimeSecret`, canonical endpoint policy, exact model/reasoning, Host tool definitions, and Host budget.
- Produces: `build_model(connection) -> Model`, `run_general_turn(request, bridge) -> AsyncIterator[WorkerOutputFrame]`, and no persistent provider state.

- [ ] **Write the red seven-route factory/request-count test**

```python
class ModelFactoryTests(unittest.IsolatedAsyncioTestCase):
    async def test_every_route_uses_exact_model_and_one_request(self) -> None:
        for case in seven_route_cases():
            transport = CountingTransport(case.response)
            model = build_model(case.connection.with_transport(transport))
            await run_probe_request(model, case.model_id, case.reasoning)
            self.assertEqual(transport.request_count, 1, case.name)
            self.assertEqual(transport.last_model_id, case.model_id)
            self.assertFalse(transport.followed_redirect)
```

- [ ] **Run it and verify red**

Run: `node scripts/run-ai-worker-tests.mjs`

Expected: FAIL at `ModelFactoryTests.test_every_route_uses_exact_model_and_one_request` because factories/adapters are missing.

- [ ] **Implement the closed factory dispatcher**

```python
def build_model(connection: RuntimeConnection) -> Model:
    match connection.provider:
        case "open_ai":
            return OpenAIResponsesModel(connection.model_id, provider=openai_provider(connection))
        case "open_ai_compatible":
            return OpenAIChatModel(connection.model_id, provider=openai_provider(connection))
        case "anthropic" | "anthropic_compatible":
            return AnthropicModel(connection.model_id, provider=anthropic_provider(connection))
        case "google_gemini":
            return GoogleModel(connection.model_id, provider=google_provider(connection))
        case "x_ai":
            return XaiModel(connection.model_id, provider=xai_provider(connection))
        case _:
            raise WorkerFailure.incompatible()
```

- [ ] Write failing model-factory tests for all four direct providers and both compatible protocols. Assert exact model/base URL/auth/header/query configuration and that no factory has a default model.
- [ ] For compatible named-key mode, strip the SDK's standard authentication header before transport and send only the validated dedicated header. Tests must prove there is no placeholder `Authorization` or second `x-api-key`; bearer mode sends only standard authorization.
- [ ] Create OpenAI and Anthropic clients with `max_retries=0`; configure Google with `HttpRetryOptions(attempts=1)` and an injected `trust_env=False` HTTPX2 client; create xAI with `AsyncClient(timeout=host_operation_timeout_seconds, channel_options=[("grpc.enable_retries", 0)])` in the cleared environment because `ModelSettings.timeout` does not control xAI. Never use credential environment variables, proxy variables, `.netrc`, or ambient cloud credentials.
- [ ] Configure custom HTTP clients with `trust_env=False`, redirects disabled, TLS/hostname verification enabled, bounded connect/read/write/pool timeouts, bounded decompression/response bytes, and redacted exception mapping. The Host discards raw worker stderr after in-memory classification and may retain only category, byte count, SHA-256, and correlation ID.
- [ ] Revalidate endpoint resolution immediately before each connection in the worker. Reject unspecified, multicast, broadcast, link-local/metadata, IPv4-mapped, mixed public/private result sets, and destination-class drift from the successful probe; HTTP must connect to the exact literal loopback address from the approved URL.
- [ ] Use HTTPX transports with transport retries explicitly zero. Preserve the configured URL path prefix when joining `/models`, Responses/Chat, or Messages routes, and apply the same guarded transport to discovery, probes, and turns.
- [ ] Implement a Pydantic `Agent` per Host operation with `ExternalToolset`, runtime `ToolDefinition` JSON Schemas, zero tool/output retries, no native tools, and defense-in-depth `UsageLimits` copied from the smaller Host budget. Pydantic AI 2.33.0 has no `instrument` constructor argument: set `agent.instrument = False` after construction and assert it in tests.
- [ ] Build that `Agent` from the exact Provider Instruction Bundle and runtime output schema in the Host request. The worker contains no default persona, Tender role, model, reasoning, or durable instructions.
- [ ] Invoke `run_stream_events()` as an async context manager and stream normalized text, tool, usage, and terminal events. Do not expose hidden reasoning or raw provider events.
- [ ] Set `output_type=[declared_output_type, DeferredToolRequests]`. When a deferred request returns, emit exact Host tool calls, wait for matching results, then continue with `message_history=result.all_messages()` and `deferred_tool_results=DeferredToolResults(calls=resolved_calls, approvals=resolved_approvals)`; use `ModelMessagesTypeAdapter` only to validate/serialize checkpoints.
- [ ] Carry one cumulative `RunUsage` and the remaining `UsageLimits` into every deferred continuation so request/token/tool ceilings never reset. Add a multi-continuation test that exhausts the shared budget and emits one terminal budget failure.
- [ ] Make cancellation trigger Pydantic cancellation and emit one interrupted terminal; incomplete uncertain side effects become indeterminate.
- [ ] Use Pydantic `TestModel`/`FunctionModel` and local fake SDK clients to test text, streaming, structured output, parallel tool proposals, tool denial, malformed tool arguments, usage, cancellation, and provider exceptions.
- [ ] Run `npm run test:ai-worker`; expect all provider tests to pass.
- [ ] Commit as `feat: normalize direct and compatible ai providers`.

---

### Task 5: Implement model discovery and bounded capability probes

**Files:**

- Create: `src-tauri/runtime/ai/quantix_ai_worker/probe.py`
- Create: `src-tauri/runtime/ai/tests/test_probe.py`
- Modify: `src-tauri/runtime/ai/quantix_ai_worker/general_adapter.py`
- Create: `src-tauri/src/ai/probe.rs`
- Modify: `src-tauri/src/ai/worker.rs`
- Modify: `src-tauri/tests/ai_worker_contract.rs`

**Discovery sources:**

- OpenAI 3.3.1: `async for model in client.models.list()`.
- Anthropic 1.0.0: `async for model in client.models.list()`.
- Google Gen AI 2.19.0: `pager = await client.aio.models.list()`, then `async for model in pager`.
- xAI SDK 1.19.0: `models = await client.models.list_language_models()`.
- Compatible endpoint matching model-list route when implemented; otherwise the explicit user-supplied model ID.

**Interfaces:**

- Consumes: exact provider client and `ProbeRequest { connection_id, execution_revision, model_id, reasoning }`.
- Produces: bounded `AiProbeEvidence` with deterministic semantic hash and separate `observed_at`.

- [ ] **Write the red missing-model-list/manual-model test**

```python
class ProbeTests(unittest.IsolatedAsyncioTestCase):
    async def test_compatible_404_uses_only_explicit_model_probe(self) -> None:
        provider = FakeCompatibleProvider(models_status=404, accepted_model="local-model-7")
        result = await probe_connection(provider, ProbeRequest(model_id="local-model-7", reasoning="unsupported"))
        self.assertEqual(result.models, ["local-model-7"])
        self.assertEqual(provider.turn_models, ["local-model-7"])
```

- [ ] **Run it and confirm red**

Run: `node scripts/run-ai-worker-tests.mjs`

Expected: FAIL at the named probe test because `probe_connection` is missing.

- [ ] **Implement bounded discovery/probe dispatch**

```python
async def probe_connection(provider: ProviderAdapter, request: ProbeRequest) -> ProbeResult:
    models = await provider.discover_models(limit=500)
    if models.unsupported:
        models = ModelCatalogue.explicit(request.model_id)
    models.require(request.model_id)
    with anyio.fail_after(180):
        return await provider.probe_exact(
            model_id=request.model_id,
            reasoning=request.reasoning,
            max_requests=6,
            max_output_tokens=1_024,
        )
```

- [ ] Write failing tests for paginated catalogues, duplicate IDs, empty pages, incomplete pagination, missing custom `/models`, and explicit custom model probes.
- [ ] Normalize model ID and display metadata only. Discard provider `default`/recommended flags and never sort one model into an implied recommendation.
- [ ] Implement a bounded probe plan for text streaming, strict structured output, one no-op external tool call, the exact Engineer-selected reasoning option, and optional image input. Record `unknown` instead of guessing any untested capability or reasoning effort.
- [ ] Source candidate reasoning options only from current adapter/provider metadata. Mark only the tested model/option pair supported; never promote sibling options by inference.
- [ ] Cap total probe model calls/tokens/time and label the returned evidence as a possibly billable Engineer-triggered test.
- [ ] Canonicalize the exact provider, endpoint fingerprint, model, reasoning selection, adapter version, and semantic test outcomes into the catalogue hash. Store `observed_at` separately; timestamps, display labels, ordering noise, secrets, and raw responses never enter the deterministic hash.
- [ ] Record both requested and provider-reported model identity when the API exposes it. Activation pins the reported identity and later drift fails as reroute; providers that expose no response model identity record reroute detection as `unknown` rather than claiming detection.
- [ ] Map a 404/unsupported model-list route on a compatible endpoint to manual-model mode only; authentication errors, model rejection, malformed responses, or failed required probes remain failures.
- [ ] Run `npm run test:ai-worker` and `cargo test --manifest-path src-tauri/Cargo.toml --test ai_worker_contract --features runtime-fixture`; expect success.
- [ ] Commit as `feat: probe exact provider capabilities`.

---

### Task 6: Stage and verify the official Codex app-server runtime

**Files:**

- Modify: `scripts/prepare-runtime.mjs`
- Create generated: `src-tauri/runtime/THIRD_PARTY_NOTICES.txt`
- Create: `src-tauri/src/ai/runtime.rs`
- Modify: `src-tauri/src/ai/mod.rs`
- Modify: `src-tauri/src/runtime_readiness.rs`
- Modify: `src-tauri/tests/runtime_readiness.rs`
- Replace generated: `src-tauri/tests/fixtures/codex_app_server_protocol.schemas.json`
- Modify: `src-tauri/tauri.conf.json`

**Pinned artifact:**

```text
Package: @openai/codex version 0.149.1-win32-x64
Tarball: https://registry.npmjs.org/@openai/codex/-/codex-0.149.1-win32-x64.tgz
Integrity: sha512-G3QXGAg7nyyhqOeooAMUekBCeHd8a1QByhKcVAFyzNBaI06t6Ft7nsF+1SzFS0spuIdU4YyMi5YD26ukADBQUQ==
Archive SHA-256: 513bde2e7a1fe31e9b7ab2c9ec1dc87e54eb93d3adc5ae579452a7f0c09e9ed2
Executable: package/vendor/x86_64-pc-windows-msvc/bin/codex.exe
```

**Interfaces:**

- Consumes: verified npm/uv artifacts and the bundled runtime source tree.
- Produces: `AiRuntimeLayout`, provenance schema 4, exact prepared Codex vendor tree, installed Python venv, generated experimental protocol schema, and readiness projection.

- [ ] **Write the red provenance-drift test**

```rust
#[tokio::test]
async fn ai_runtime_rejects_codex_or_worker_hash_drift() {
    let fixture = RuntimeFixture::prepared_ai_runtime();
    fixture.flip_one_byte("runtimes/codex/0.149.1/vendor/bin/codex.exe");
    assert_eq!(fixture.host.inspect_runtime_readiness().await.state, RuntimeReadinessState::Blocked);
    assert_eq!(fixture.host.inspect_runtime_readiness().await.issue, Some(RuntimeReadinessIssue::ProvenanceMismatch));
}
```

- [ ] **Run it and confirm red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test runtime_readiness ai_runtime_rejects_codex_or_worker_hash_drift --features runtime-fixture`

Expected: FAIL because AI runtime provenance/readiness does not exist.

- [ ] **Implement the layout/provenance boundary**

```rust
pub struct AiRuntimeLayout {
    pub codex_executable: PathBuf,
    pub python_executable: PathBuf,
    pub worker_project: PathBuf,
    pub notices: PathBuf,
}

impl AiRuntimeLayout {
    pub fn under(application_home: &Path) -> Self {
        Self {
            codex_executable: application_home.join("runtimes/codex/0.149.1/vendor/bin/codex.exe"),
            python_executable: application_home.join("runtimes/ai/venv/Scripts/python.exe"),
            worker_project: application_home.join("runtimes/ai/project"),
            notices: application_home.join("runtimes/THIRD_PARTY_NOTICES.txt"),
        }
    }
}
```

- [ ] Write failing readiness tests for wrong Codex version/hash, modified worker source/lock, missing package, corrupt environment, smoke timeout, and interrupted preparation.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml --test runtime_readiness --features runtime-fixture`; expect failures because Codex/AI provenance and readiness activities do not exist.
- [ ] Replace the current legacy-Codex deletion logic with verified download/cache/extract/stage logic. Verify npm SHA-512 integrity before extraction and record the staged executable SHA-256.
- [ ] Stage the exact published relative vendor tree: `bin/codex.exe`, `bin/codex-code-mode-host.exe`, `codex-resources/codex-command-runner.exe`, `codex-resources/codex-windows-sandbox-setup.exe`, `codex-path/rg.exe`, and `codex-package.json`. Hash every file and reject extra/link/type/path drift.
- [ ] Bump runtime provenance schema and include every Codex vendor-tree hash plus every `src-tauri/runtime/ai` project file hash. Reject links and unlisted files.
- [ ] Generate `src-tauri/runtime/THIRD_PARTY_NOTICES.txt` with the complete required Codex and locked Python distribution license/NOTICE texts, then record its hash with the exact npm identity, upstream source tag, archive hashes, executable hash, and preparation-script hash in provenance. Metadata alone is not distribution compliance.
- [ ] Add readiness activities for the Codex vendor tree, isolated AI environment, Python worker import, and deterministic worker smoke test. Run one complete app-server turn against a local fake upstream to prove the staged relative tree and disabled-tool mode before any live account test.
- [ ] Prepare `~/.quantix/runtimes/ai` from the checked-in uv lock without adding packages to the OCR environment.
- [ ] Replace the broad `runtime` Tauri resource with an explicit allowlist for staged `uv.exe`, the six approved Codex vendor-tree files, `THIRD_PARTY_NOTICES.txt`, OCR project/runtime inputs, AI `pyproject.toml`/`uv.lock`/`THIRD_PARTY_LICENSES.json`, and `quantix_ai_worker/**`. Exclude Python tests, caches, `.venv`, `.dev`, downloads, and unlisted files; do not add a Tauri sidecar declaration.
- [ ] Probe `codex --version`, send `initialize`, receive success, send `initialized`, then close stdin and await/terminate the process. App-server has no shutdown RPC. Do not perform login or a live model call.
- [ ] Resolve and validate `.dev/runtime-provisioning/codex-schema-0.149.1`, then run `codex app-server generate-json-schema --experimental --out .dev/runtime-provisioning/codex-schema-0.149.1`; canonically bundle the complete experimental schema, atomically replace the checked-in fixture, record its SHA-256 in provenance, and fail tests when runtime/schema hashes drift. The `--experimental` flag is mandatory because the stable schema omits `dynamicTools`.
- [ ] Run `npm run prepare:runtime` twice; the second run must be idempotent and produce no tracked diff other than an intentional provenance update.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml --test runtime_readiness --features runtime-fixture`; expect success.
- [ ] Commit as `build: package pinned ai provider runtimes`.

---

### Task 7: Implement the strict Codex app-server adapter

**Files:**

- Create: `src-tauri/src/ai/codex_worker.rs`
- Create: `src-tauri/tests/fixtures/ai_worker/codex-happy.jsonl`
- Create: `src-tauri/tests/fixtures/ai_worker/codex-tool-roundtrip.jsonl`
- Create: `src-tauri/tests/fixtures/ai_worker/codex-refresh.jsonl`
- Create: `src-tauri/tests/fixtures/ai_worker/codex-reroute.jsonl`
- Modify: `src-tauri/src/ai/mod.rs`
- Modify: `src-tauri/src/ai/worker.rs`
- Modify: `src-tauri/tests/support/ai_worker_fixture.rs`
- Modify: `src-tauri/tests/ai_worker_contract.rs`

**Initialize policy:**

```json
{
  "clientInfo": {"name":"quantix","title":"Quantix","version":"0.1.0"},
  "capabilities": {"experimentalApi":true}
}
```

Each operation gets a unique disposable `CODEX_HOME`. Its root thread requests `ephemeral: true`, the exact selected model, a read-only empty run workspace, approval policy `never`, and the Host-declared dynamic tools. Quantix executes only those dynamic tools and fails the turn if any built-in action surfaces.

Codex 0.149.1 cannot express a restricted readable-root list. Use its exact
read-only schema and keep the account adapter private/non-shippable:

```rust
let run_workspace_wire = run_workspace.to_string_lossy().into_owned();
serde_json::json!({
    "cwd": &run_workspace_wire,
    "sandboxPolicy": {
        "type": "readOnly",
        "networkAccess": false,
    },
})
```

This policy is read-only but permits broad host reads if a built-in read path is
reached. The empty staged workspace, disabled native tools, fail-on-built-in event
rule, and written-approval release gate reduce private-prototype risk; they do not
constitute a production filesystem sandbox.

The isolated Codex configuration is exact and must be confirmed with `config/read`:

```toml
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
browser_use_full_cdp_access = false
code_mode_host = false
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
in_app_chat = false
in_app_dictation = false
in_app_updates = false
plugin_sharing = false
remote_plugin = false
secret_auth_storage = false
skill_mcp_dependency_install = false
skill_search = false
tool_call_mcp_elicitation = false
tool_suggest = false
unbounded_connection_retries = false
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

**Interfaces:**

- Consumes: `AiRuntimeRequest`, zeroizing account token snapshot/refresh callback, `SupervisedConversation`, generated 0.149.1 experimental schema, and exact Agent Run workspace.
- Produces: `CodexWorker::probe`, `CodexWorker::run_turn`, and `CodexWorker::cancel`, mapped to final `AiRuntime*` types.

- [ ] **Write the red unknown-built-in/reroute test**

```rust
#[tokio::test]
async fn codex_builtin_action_or_reroute_is_terminal_failure() {
    for scenario in ["codex-built-in-file", "codex-reroute"] {
        let worker = fixture_codex_worker(scenario).await;
        let failure = worker.run_turn(fake_codex_turn()).await.unwrap_err();
        assert!(matches!(failure.category,
            AiRuntimeFailureCategory::PermissionDenied | AiRuntimeFailureCategory::ModelRerouted));
        assert_eq!(worker.host_tool_execution_count(), 0);
    }
}
```

- [ ] **Run it and confirm red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ai_worker_contract codex_builtin_action_or_reroute_is_terminal_failure --features runtime-fixture`

Expected: FAIL because `CodexWorker`/fixture handling is missing.

- [ ] **Implement initialize plus fail-closed request dispatch**

```rust
match message {
    ServerMessage::Request(request) if request.method == "item/tool/call" => {
        self.resolve_dynamic_tool_once(request).await
    }
    ServerMessage::Request(request) if request.method == "account/chatgptAuthTokens/refresh" => {
        self.refresh_same_account_once(request).await
    }
    ServerMessage::Notification(notification) if notification.method == "model/rerouted" => {
        self.interrupt().await?;
        Err(AiRuntimeFailure::model_rerouted(notification))
    }
    ServerMessage::Request(request) => {
        self.reject_server_request(&request).await?;
        self.interrupt().await?;
        Err(AiRuntimeFailure::permission_denied())
    }
    other => self.normalize_event(other).await,
}
```

- [ ] Write failing fixture tests for initialize/initialized ordering, host-managed token login, token refresh server request, paginated model listing, streamed turn, dynamic tool call/result, usage, interruption, unknown server request, built-in command/file event, reroute, and terminal status.
- [ ] Start app-server with a unique operation-owned credential-free `CODEX_HOME`, cleared environment, fixed configuration overrides disabling shell/unified exec/web/images/apps/plugins/memory/multi-agent/hooks/update/feedback, and no inherited user Codex configuration.
- [ ] Do not write `[model_providers.openai]`: `openai` is a reserved built-in ID and app-server rejects overrides. Record the pinned adapter's four request retries and five interrupted-stream retries as an explicit bounded limitation, add no Host-level Codex retry, and keep retry-related usage unknown when app-server does not report it.
- [ ] Verify OTEL logs/traces/metrics exporters are `none`, prompt logging is false, fast mode and skill dependency installation are off, and both top-level/tool web search controls are disabled. Any `configWarning` makes the adapter incompatible.
- [ ] Before every probe/turn, send `account/login/start` with `type: chatgptAuthTokens`, the in-memory access token, account ID, and plan type. Handle `account/chatgptAuthTokens/refresh` only through a Host callback that refreshes and commits the vault before returning new tokens.
- [ ] Refresh uses compare-and-swap on the expected semantic revision, account ID, and credential generation. Return the new token to app-server only after the encrypted vault commit succeeds; concurrent disconnect/relogin makes the operation fail authentication.
- [ ] Permit at most one `chatgptAuthTokens` refresh request and the app-server's immediate continuation of the same accepted upstream request per operation. Count it in the exact Host budget; a second refresh request fails authentication. This is not a general retry and cannot change provider/model/turn identity.
- [ ] Build login/refresh response frames in zeroizing buffers and never include them in protocol diagnostics, tracing fields, fixture snapshots, or failure summaries.
- [ ] Assert no `auth.json` or keyring configuration is created in isolated Codex state after login, refresh, turn, interruption, or shutdown.
- [ ] Page `model/list` to completion. A non-null cursor that cannot be consumed is a protocol failure. Discard the provider's default flag from Quantix views.
- [ ] Require `thread/start` to return `ephemeral: true` and an empty `instructionSources` array. Any loaded user/workspace instruction file or persisted root thread is a security failure.
- [ ] Resolve the exact empty Agent Run workspace into `cwd` and send only `{type: "readOnly", networkAccess: false}` as the 0.149.1-supported sandbox policy. Add a test that invalid restricted-root fields are rejected, record broad-read capability as an account-adapter release blocker, and never claim outside reads are denied.
- [ ] Supply the exact Quantix Provider Instruction Bundle through `baseInstructions` and `developerInstructions`, and the exact Host output schema on the turn. Do not rely on Codex's coding persona, repository instructions, or provider thread defaults for Tender behavior or authority.
- [ ] Reject every unknown server request by default with the documented method-specific denial or JSON-RPC error before termination. Treat any command execution, apply-patch/file change, native web search, MCP, app, collaboration, or permission request/event as a security failure and interrupt the turn. Add a pinned-runtime contract test that inventories observed built-in tool behavior in the empty read-only workspace.
- [ ] Map dynamic tool calls only after matching exact Host tool name/schema/call ID. Track call IDs for the full turn; a duplicate returns the recorded idempotent result or denial and never executes twice. Host denial/result returns through the documented tool response.
- [ ] On `model/rerouted`, interrupt and return a typed reroute failure containing only the selected and observed model IDs.
- [ ] Send `turn/interrupt` on cancellation, wait up to five seconds for terminal `interrupted`, then terminate the process group.
- [ ] Delete any unexpectedly persisted thread, shut down app-server, and remove the unique Codex home on every terminal, error, cancellation, crash, or indeterminate exit. Startup sweeps only exact abandoned operation-owned homes. Never quarantine, back up, or reuse raw Codex state; the separate Agent Run workspace carries the indeterminate evidence.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml --test ai_worker_contract --features runtime-fixture`; expect all general and Codex contract tests to pass.
- [ ] Commit as `feat: add the controlled codex app server worker`.

---

### Task 8: Verify and review Layer 1B

- [ ] Run `npm run test:ai-worker`; expect success without network credentials.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml --test ai_worker_contract --features runtime-fixture`; expect success.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml --test runtime_readiness --features runtime-fixture`; expect success.
- [ ] Run `npm run format:check`, `npm run check`, and `npm test`; expect success.
- [ ] Run `git diff --check`, `npm run prepare:runtime`, and `src-tauri/runtime/bin/uv.exe lock --check --project src-tauri/runtime/ai`; expect no unintended diff.
- [ ] Use `superpowers:requesting-code-review`. Require explicit review of fail-closed server requests, process-tree cleanup, secret transport, retry disabling, redirects, model reroutes, probe billing bounds, and runtime provenance.
- [ ] Apply valid findings, rerun `npm run verify`, and commit fixes separately.
- [ ] Confirm the existing visible ChatGPT flow is still the mounted product path; Layer 1B must not expose the new workers before the atomic cutover.
