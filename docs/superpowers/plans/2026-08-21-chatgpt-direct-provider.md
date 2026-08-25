# Direct ChatGPT Provider Implementation Plan

> **Historical — do not execute.** This plan is superseded by `docs/superpowers/plans/2026-08-25-sdk-first-ai-runtime-plan-suite.md` and its Codex managed-runtime cutover. Quantix must not restore the private ChatGPT backend or custom OAuth path described below.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the bundled Codex CLI with a Quantix-owned ChatGPT OAuth connection and a direct executor against `https://chatgpt.com/backend-api/codex/responses`, deleting every trace of the Codex binary integration.

**Architecture:** New `chatgpt_oauth` host module owns login (loopback PKCE server on ports 1455→1457), token storage at `<application home>/auth.json`, and refresh. A new backend-client seam feeds a rewritten executor that preserves today's Provider Turn lifecycle (Permission Grants, Typed Tools, candidate validation, interruption semantics) with tool approvals enforced fully host-side. Built additively on an isolated branch, legacy deleted in the same landing; main never breaks.

**Tech Stack:** Rust (Tauri 2 host, reqwest/rustls, sha2, serde_json, getrandom), React 19 + TypeScript renderer, ts-rs bindings, vitest + cargo tests, `runtime-fixture` feature for deterministic lanes.

**Spec:** `docs/superpowers/specs/2026-08-21-chatgpt-direct-provider-design.md`

## Global Constraints

- Toolchain pins: Rust `1.97.1` (`rust-toolchain.toml`), Node `22.23.2` (`.node-version`). Never bump in this work.
- Every task ends green: `npm run verify` (identity lint → prettier+cargo fmt → tsc + `cargo clippy -D warnings --features runtime-fixture` → vitest + cargo tests). Final stage also `npm run build`.
- Generated TypeScript DTOs under `src/bindings/` are committed outputs; any task changing exported types runs `npm test` and includes regenerated declarations in the same commit (CI enforces `git diff --exit-code`).
- Allowed new dependency: `getrandom` only (CSPRNG). No web frameworks, no OAuth crates, no HTTP server crates — hand-rolled loopback responder per spec simplicity rule.
- Exact external constants (verbatim): issuer `https://auth.openai.com`; client_id `app_EMoamEEZ73f0CkXaXp7hrann`; scope `openid profile email offline_access`; callback paths `/auth/callback`; ports `1455` then `1457`; authorize extras `id_token_add_organizations=true`, `originator=quantix`; backend `https://chatgpt.com/backend-api/codex/responses`.
- Secrets discipline: tokens never enter Tender Stores, logs, diagnostics, or audit events. Error strings carry redacted detail only.
- Do not touch the maintainer's unrelated uncommitted WIP in the primary checkout; all work happens in the feature worktree.
- EITL approval semantics, Permission Grant fail-closed behavior, and audit-chain invariants are untouchable.

---

### Task 1: Isolated worktree + baseline green

**Files:** none created (environment setup)

**Interfaces:**
- Produces: worktree at `<repo-sibling>/quantix-feature` on branch `feature/chatgpt-direct-provider` from `main` tip (`760f8ad`), with `npm ci` done and `target/` primed.

- [ ] Step 1: `git worktree add ../quantix-feature -b feature/chatgpt-direct-provider` (run from primary checkout; sibling dir keeps paths short)
- [ ] Step 2: In the worktree: `npm ci`
- [ ] Step 3: Baseline gate: `npm run verify`
- Expected: PASS (committed main is CI-green). If anything fails, STOP and report — do not build on a red base.
- [ ] Step 4: Commit nothing (setup only)

### Task 2: Crypto helpers — base64url + PKCE + state

**Files:**
- Create: `src-tauri/src/chatgpt_oauth/mod.rs` (module root, re-exports)
- Create: `src-tauri/src/chatgpt_oauth/crypto.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod chatgpt_oauth;`)
- Modify: `src-tauri/Cargo.toml` (add `getrandom = "0.3"`)
- Test: inline `#[cfg(test)]` in `crypto.rs`

**Interfaces:**
- Produces:
  - `pub(crate) fn base64url_encode(data: &[u8]) -> String`
  - `pub(crate) fn base64url_decode(s: &str) -> Result<Vec<u8>, OauthCodecError>`
  - `pub(crate) struct PkceCodes { pub verifier: String, pub challenge: String }`
  - `pub(crate) fn generate_pkce() -> Result<PkceCodes, RandomError>` — 43 uniform random bytes from `[A-Za-z0-9-._~]`, challenge = base64url(SHA-256(verifier bytes))
  - `pub(crate) fn generate_state() -> Result<String, RandomError>` — base64url(32 random bytes)

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn base64url_roundtrips_and_avoids_padding() {
    let input = b"subjects?_";
    let encoded = super::base64url_encode(input);
    assert_eq!(encoded, "c3ViamVjdHM_Xw"); // no '+', '/', '='
    assert_eq!(super::base64url_decode(&encoded).unwrap(), input);
    // second remainder path (len % 4 == 3)
    assert_eq!(super::base64url_encode(b"subjects?__"), "c3ViamVjdHM_X18");
}

#[test]
fn pkce_verifier_matches_challenge() {
    let pkce = super::generate_pkce().unwrap();
    assert_eq!(pkce.verifier.len(), 43);
    let digest = Sha256::digest(pkce.verifier.as_bytes());
    assert_eq!(super::base64url_encode(&digest), pkce.challenge);
}

#[test]
fn state_is_unique_and_urlsafe() {
    let a = super::generate_state().unwrap();
    let b = super::generate_state().unwrap();
    assert_ne!(a, b);
    assert_eq!(a.len(), 43);
    assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~'));
}
```

- [ ] **Step 2: Run** `cargo test -p quantix --lib chatgpt_oauth` → expect FAIL (unresolved)
- [ ] **Step 3: Implement** using the existing `base64 = "0.22.1"` dependency (`engine::general_purpose::URL_SAFE_NO_PAD`, strict decode, `DecodeError` mapped to the codec error); `generate_pkce` fills 43 bytes via `getrandom::fill` then maps each byte modulo 64 through the 64-char subset `[A-Za-z0-9-_]` of the unreserved charset before hashing (hash the *charset-encoded verifier string*).
- [ ] **Step 4: Run** again → PASS
- [ ] **Step 5: Commit** `feat: add chatgpt oauth crypto primitives`

### Task 3: JWT claims parsing + identity extraction

**Files:**
- Create: `src-tauri/src/chatgpt_oauth/jwt.rs`
- Test: inline `#[cfg(test)]`

**Interfaces:**
- Consumes: `base64url_decode` from Task 2
- Produces:
  - `pub(crate) fn parse_jwt_claims(token: &str) -> Option<serde_json::Value>`
  - `pub(crate) struct ChatGptIdentity { pub account_id: String, pub plan_type: Option<String> }`
  - `pub(crate) fn extract_identity(token: &str) -> Option<ChatGptIdentity>` — precedence: root `chatgpt_account_id`, then claims["https://api.openai.com/auth"]["chatgpt_account_id"], then `organizations[0].id`; plan from nested auth claim `chatgpt_plan_type`.

- [ ] **Step 1: Write failing tests** using three synthetic JWTs built in-test (header/claims/signature where claims JSON contains each precedence variant); assert extraction picks root > nested > org, missing id → None, malformed base64 → None.
- [ ] **Step 2: Run** → FAIL
- [ ] **Step 3: Implement** split-on-'.' payload decode → `serde_json::from_slice`. No signature verification (token arrives over TLS directly from issuer; same trust stance as OpenCode/Codex CLI).
- [ ] **Step 4: Run** → PASS
- [ ] **Step 5: Commit** `feat: parse chatgpt token identity claims`

### Task 4: Authorize URL builder

**Files:**
- Create: `src-tauri/src/chatgpt_oauth/authorize.rs`
- Test: inline

**Interfaces:**
- Consumes: `PkceCodes`
- Produces: `pub(crate) fn build_authorize_url(redirect_uri: &str, pkce: &PkceCodes, state: &str) -> String`

Exact output shape (order-insensitive params, values percent-encoded):

```
https://auth.openai.com/oauth/authorize
    ?response_type=code
    &client_id=app_EMoamEEZ73f0CkXaXp7hrann
    &redirect_uri=<redirect_uri>
    &scope=openid%20profile%20email%20offline_access
    &code_challenge=<challenge>
    &code_challenge_method=S256
    &id_token_add_organizations=true
    &state=<state>
    &originator=quantix
```

- [ ] **Step 1: Failing test** asserting every parameter/value above for a fixed PKCE/state triple.
- [ ] **Step 2: Run** → FAIL
- [ ] **Step 3: Implement** with `serde_urlencoded`-style manual query assembly (percent-encode via existing `form_urlencoded`? — if not a direct dependency, hand-roll `fn qp(s:&str)` escaping only `% & = + # ?` and space→`%20`).
- [ ] **Step 4: Run** → PASS
- [ ] **Step 5: Commit** `feat: build chatgpt authorize url`

### Task 5: Token client — exchange + refresh

**Files:**
- Create: `src-tauri/src/chatgpt_oauth/tokens.rs`
- Test: inline, spawning a local `std::net::TcpListener` mock issuer on an OS-assigned port (deterministic offline)

**Interfaces:**
- Consumes: `PkceCodes`, `ChatGptIdentity`
- Produces:
  - `pub(crate) struct IssuedTokens { pub access_token: String, pub refresh_token: String, pub id_token: String, pub expires_in_secs: u64 }`
  - `pub(crate) struct TokenClient { issuer_base: String, http: reqwest::blocking::Client }`
  - `impl TokenClient { pub fn exchange_code(&self, code:&str, redirect_uri:&str, pkce:&PkceCodes) -> Result<IssuedTokens, TokenError>; pub fn refresh(&self, refresh_token:&str) -> Result<IssuedTokens, TokenError> }`
  - `TokenError` variants: `Provider{status, redacted_body_kind}` | `Transport(String)` — bodies are classified (`invalid_grant`, `invalid_client`, …) and never embedded verbatim.

Wire format (POST form, `application/x-www-form-urlencoded`):
- exchange: `grant_type=authorization_code`, `code`, `redirect_uri`, `client_id`, `code_verifier`
- refresh: `grant_type=refresh_token`, `refresh_token`, `client_id`

- [ ] **Step 1: Failing tests**: mock issuer asserts received form fields, replies canned JSON `{access_token, refresh_token, id_token, expires_in}` → parsed struct; 400 `{"error":"invalid_grant"}` → `TokenError::Provider` with kind `InvalidGrant`; connection-refused port → `TokenError::Transport`.
- [ ] **Step 2: Run** → FAIL
- [ ] **Step 3: Implement** with `reqwest::blocking` (host already depends on reqwest; blocking is fine inside `spawn_blocking`).
- [ ] **Step 4: Run** → PASS
- [ ] **Step 5: Commit** `feat: chatgpt token exchange and refresh client`

### Task 6: Loopback callback server with port fallback

**Files:**
- Create: `src-tauri/src/chatgpt_oauth/callback_server.rs`
- Test: inline (inject port candidates `[0]`/ephemeral for tests; production passes `[1455, 1457]`)

**Interfaces:**
- Consumes: `build_authorize_url`, `TokenClient`
- Produces:
  - `pub(crate) enum CallbackOutcome { Authorized(IssuedTokens), Cancelled, Failed(CallbackFailure) }`
  - `pub(crate) struct PortHolders { pub port_1455: Option<u32>, pub port_1457: Option<u32> }` (PIDs resolved via platform socket tables — Windows: `GetExtendedTcpTable` through existing `windows` crate; other platforms best-effort None)
  - `pub(crate) fn run_login(port_candidates: &[u16], open_browser: impl FnOnce(&str), issuer: &str) -> CallbackOutcome`

Behavior contract:
1. Bind first free candidate; both fail → resolve holder PIDs → `Failed(PortBlocked(holders))`.
2. Build URL with bound port, call `open_browser(url)`.
3. Serve GET requests until terminal outcome:
   - `/auth/callback` with `error` param → branded error page, `Failed(ProviderDenied(msg))`
   - `/auth/callback` missing/mismatched `state` vs pending → branded CSRF-error page, `Failed(StateMismatch)`
   - valid `code`+`state` → token exchange → branded success page → `Authorized(tokens)`
   - `/cancel` → plain-text ack, `Cancelled`
   - unknown path → 404 text
4. Hard timeout 5 min → `Failed(Timeout)`. Server closes on any terminal outcome.

Branded pages: single-file inline HTML/CSS using Quantix ink/vellum palette (`--qx-*` values copied from `src/quantixDesignSystem.css`), centered card, Quantix mark SVG inline, headings exactly:
- Success: "Authorization successful" / "Quantix is now connected to ChatGPT." / "You can close this window."
- Error variant mirrors structure with failure reason line.

- [ ] **Step 1: Failing tests** driving the server over real sockets: happy path (issue GET with code/state against ephemeral-port instance wired to mock issuer from Task 5), cancel path, state-mismatch path, error-param path, double-port-blocked path (bind both test ports beforehand, assert `PortBlocked` carries them).
- [ ] **Step 2: Run** → FAIL
- [ ] **Step 3: Implement** minimal HTTP/1.1 reader: read request line + headers until `\r\n\r\n`, ignore body; write response with `Content-Type: text/html; charset=utf-8` + `Connection: close`.
- [ ] **Step 4: Run** → PASS
- [ ] **Step 5: Commit** `feat: chatgpt oauth loopback login server`

### Task 7: Token store at Application Home

**Files:**
- Create: `src-tauri/src/chatgpt_oauth/store.rs`
- Test: inline (tempdir home)

**Interfaces:**
- Produces:
  - `pub(crate) struct StoredConnection { pub access_token: String, pub refresh_token: String, pub id_token: String, pub expires_at_ms: u64, pub account_id: String, pub plan_type: Option<String> }`
  - `pub(crate) enum LoadState { Connected(Box<StoredConnection>), Absent, Unusable }` (`Unusable` = corrupt/unparseable → treated as logged-out, never a crash)
  - `pub(crate) fn load(home: &Path) -> LoadState`
  - `pub(crate) fn save(home: &Path, conn: &StoredConnection) -> io::Result<()>` — write `auth.json.tmp` then rename (atomic), JSON shape `{"version":1,"access":…,"refresh":…,"id_token":…,"expires_at_ms":…,"account_id":…,"plan_type":…}`
  - `pub(crate) fn clear(home: &Path) -> io::Result<()>`
  - `pub(crate) fn needs_refresh(conn: &StoredConnection, now_ms: u64) -> bool` — true when ≤120 s remain

- [ ] **Step 1: Failing tests**: save→load roundtrip; clear removes file; truncated JSON → `Unusable`; expiry boundary math.
- [ ] **Step 2–4: RED→implement→GREEN** as usual.
- [ ] **Step 5: Commit** `feat: chatgpt token store in application home`

### Task 8: Host commands + settings surface wiring

**Files:**
- Modify: `src-tauri/src/host.rs` (QuantixHost methods)
- Modify: `src-tauri/src/lib.rs` (command wrappers + `generate_handler` additions)
- Modify: `src-tauri/src/application_settings.rs` (connection record gains `login_flow` state machine: Idle/AwaitingBrowser/Completed; reuse `CODEX_CONNECTION_ID`)
- Create/Modify bindings via `#[derive(TS)]` on new DTOs (`StartChatGptLoginResult`, `ChatGptConnectionStatus`) → `src/bin/export_bindings.rs` picks up automatically if following existing registration pattern
- Test: host-level tests with injected fake browser closure + mock issuer + ephemeral ports (pattern: existing `application_settings.rs` tests)

**Interfaces:**
- Produces (renderer-facing commands, snake_case payloads wrapped in `{ command }` like all existing commands):
  - `start_chatgpt_login` → `StartChatGptLoginResult { status: "awaiting_browser" }`; errors map to existing `TenderCommandError{code}` codes, new codes `oauth_port_blocked` (payload lists holders), `oauth_already_running`
  - `cancel_chatgpt_login` → idempotent ok
  - `disconnect_chatgpt` → clears store, returns ok
  - extended settings inspect includes `chatgpt: ChatGptConnectionStatus { state: "absent"|"connected"|"unusable", account_id?, plan_type?, expires_at_ms? }`
- Renderer consumes these in Task 9.

Flow inside `start_chatgpt_login`: reject if flow active; read store — already connected & fresh → return connected; else spawn thread running `run_login([1455,1457], opener, issuer)` where opener uses `std::process::Command`: windows `cmd /c start "" <url>`, macos `open <url>`, else `xdg-open <url>`; on outcome persist/clear store and advance state machine; emit progress through the existing diagnostics/status channel used by intake (reuse `emit` pattern already present in lib.rs).

- [ ] Steps: failing host test → implement wrappers (validate → `spawn_blocking` → host method, matching house style) → `npm test` regenerates bindings → PASS → commit `feat: quantix-owned chatgpt login commands`

### Task 9: Renderer settings UI for the new connection

**Files:**
- Modify: `src/ApplicationSettings.tsx` (Codex card → ChatGPT card: Connect button starts flow; awaiting spinner; success shows account_id + plan + expiry; Disconnect)
- Modify: `src/quantixHost.ts` (new wrappers `startChatGptLogin`, `cancelChatGptLogin`, `disconnectChatGpt`, typed against new bindings)
- Modify: `src/browserPreviewHost.ts` (mock the four commands so dev preview works)
- Test: extend `ApplicationSettings` tests (existing patterns mock invoke; assert exact command objects)

Copy rules: button "Connect ChatGPT"; awaiting copy "Finish signing in through your browser."; blocked-port copy lists holding PIDs verbatim from error payload.

- [ ] Steps: failing vitest → implement → PASS → `npx prettier --write` touched files → commit `feat: settings ui drives quantix-owned chatgpt login`

### Task 10: Backend contract spike (research only, no product code)

**Files:**
- Create: `docs/research/chatgpt-backend-contract.md`

Method: derive from OpenCode sources (`packages/opencode/src/plugin/openai/codex.ts`, `ws.ts`, `ws-pool.ts`, transform layer) + Codex CLI protocol docs already vendored at `src-tauri/runtime/schema/*.json` (request shapes overlap). NO live calls with user credentials by agents.

Must answer, each with source citation:
1. Request body: model, instructions, input item shapes, `tools` array (function tools w/ JSON schema), `store:false`, `include:["reasoning.encrypted_content"]`, `stream:true`, reasoning effort/verbosity fields.
2. SSE event inventory (`response.created`, `response.output_item.added`, `response.output_text.delta`, `response.function_call_arguments.delta|done`, `response.completed`, error events) and terminal detection.
3. Required headers beyond auth/account-id: `originator`, `session_id` (UUID per thread), `User-Agent`, `OpenAI-Beta: responses=experimental` (confirm presence/version).
4. Rate-limit/usage surfaces (headers vs `response.completed` usage block).
5. Model + reasoning catalogue: does any discoverable endpoint exist? If not, enumerate OpenCode's hardcoded GPT‑5 preset list as the seed for Quantix's versioned built-in catalogue.
6. WebSocket alternative in ws-pool: confirm whether REST+SSE suffices for v0 (expected yes).

Output ends with a GO/NO-GO for REST+SSE and the catalogue decision (built-in versioned list vs live).

- [ ] Step 1: research + write doc; Step 2: commit `docs: chatgpt backend contract spike`

### Task 11: Backend client seam

**Files:**
- Create: `src-tauri/src/agent_backend/client.rs` (trait + reqwest impl + SSE byte-stream type)
- Create: `src-tauri/src/agent_backend/fixture_client.rs` (`cfg(feature="runtime-fixture")`, scripted responses)
- Modify: `src-tauri/src/lib.rs` (`mod agent_backend;`)
- Test: inline; fixture scripts as test data under `src-tauri/tests/support/backend_scripts/*.sse` (bytes committed)

**Interfaces:**
- Produces:
  - `pub(crate) struct BackendRequest { pub model: String, pub instructions: String, pub input_items: Vec<serde_json::Value>, pub tools: Vec<serde_json::Value>, pub store: bool /* always false */, pub include_reasoning: bool /* always true */ }`
  - `pub(crate) enum StreamEvent { ItemAdded(Value), TextDelta(String), FunctionCallDelta{call_id:String, name:String, args_delta:String}, FunctionCallDone{call_id:String, name:String, arguments:String}, Completed{response_id:String, usage:UsageSnapshot}, Errored(RedactedFailure) }`
  - `pub(crate) trait ChatGptBackend: Send { fn create_response(&self, auth: &StoredConnection, req: &BackendRequest, on_event: &mut dyn FnMut(StreamEvent)) -> Result<TurnDisposition, BackendError>; }`
  - `TurnDisposition::{Completed, Interrupted, Failed}` aligning with existing Provider Turn outcomes.

Reqwest impl: POST SSE (`text/event-stream`), parse `data:` lines incrementally, honor rate-limit headers into `UsageSnapshot`. Fixture impl replays scripted `.sse` files, can simulate mid-stream abort + 401.

- [ ] Steps: failing fixture-driven parse tests (delta accumulation, tool-call completion, completed-with-usage, error event) → implement both clients → PASS → commit `feat: chatgpt backend client seam`

### Task 12: Turn executor over the seam

**Files:**
- Create: `src-tauri/src/agent_backend/turn_executor.rs`
- Modify: `src-tauri/src/agent_runtime/permissions.rs` (expose `authorize_tool_call(grant, tool_name, args_digest) -> Result<ToolReservation, AccessFailure>` extracted from control-request logic — pure refactor, existing tests keep passing)
- Test: inline + fixture scripts

**Interfaces:**
- Consumes: `ChatGptBackend`, `permissions.rs` reservations, existing `prepare_*_run` staging/workspace/grant rows, existing candidate-validation + `complete_agent_run` persistence
- Produces: `pub(crate) async fn execute_provider_turn(ctx: TurnContext<'_>) -> Result<ProviderTurnResult, ProviderFailure>` — same signature family as today's codex path so the cutover in Task 13 is a routing change.

Behavior: build `BackendRequest` from Provider Turn Request (instructions bundle → `instructions`, Data Views/input manifest → initial `input_items`, Typed Tools → `tools` with JSON schemas from grant); loop on `FunctionCallDone`: validate reservation → execute Typed Tool → append `{type:"function_call_output", call_id, output}` → issue a stateless follow-up request that replays the original input plus ID-stripped response items, encrypted reasoning, function calls and tool outputs under `store:false`; map dispositions to existing outcome enums; interruption = cooperative cancel flag checked between events (stream dropped → `Interrupted`; uncertainty → existing Indeterminate quarantine path); usage recorded per existing Provider Usage records.

- [ ] Steps: fixture-scripted tests: (a) straight text answer completes + artifacts validated; (b) one tool roundtrip executes grant-approved tool, denies out-of-grant call with audited denial event; (c) mid-stream abort → Interrupted; (d) 401 → refresh-once-then-`AuthenticationRequired`. Implement until GREEN. Commit `feat: host-executed chatgpt turns`

### Task 13: Cutover routing + readiness redefinition

**Files:**
- Modify: `src-tauri/src/agent_runtime.rs` (`execute_provider_turn` switch: Codex runs → new executor; delete codex actor spawn/supervision paths)
- Modify: `src-tauri/src/runtime_readiness.rs` (drop codex binary checks; uv+OCR checks remain)
- Modify: `src-tauri/src/application_settings.rs` (connection readiness = stored-token validity + plan eligibility (`chatgpt_subscription_is_supported` reused against `plan_type` claim); expired → auto-refresh attempt → else `AuthenticationRequired` surfaced through existing Waiting-for-AI-Provider semantics)
- Test: update all touched test modules; fixture lane covers ready/refresh-required/blocked

- [ ] Steps: reroute + simplify; full `cargo test --features runtime-fixture` green; commit `refactor: route provider turns through quantix-owned chatgpt backend`

### Task 14: Delete the Codex legacy (separation proof)

**Files:**
- Delete: `src-tauri/src/agent_runtime/codex_actor.rs`, `src-tauri/src/agent_runtime/codex_protocol.rs`, `src-tauri/src/agent_runtime/bootstrap_profile.rs` if solely codex-thread shaped (verify importers first)
- Modify: `scripts/prepare-runtime.mjs` (remove codex resolution/staging/schema-digest; keep uv + OCR provisioning + provenance manifest minus codex keys)
- Modify: `src-tauri/Cargo.toml` + root `package.json` (remove `@openai/codex` devDep, `smoke:codex` script)
- Modify: delete CODEX_HOME/env plumbing (`controlled_codex_environment`), codex readiness commands/DTOs, `runtime-provenance.json` codex fields, related fixtures/tests
- Verify gate: `git grep -nE "codex_actor|codex_protocol|CODEX_HOME|account/login|app-server|smoke:codex|@openai/codex"` → zero hits outside docs/adr history + this plan/spec.

- [ ] Steps: mechanical deletion, fix compile fallout, update/remove dependent tests, run gates, commit `chore!: remove bundled codex runtime entirely`

### Task 15: Governance docs

**Files:**
- Create: `docs/adr/0016-connect-chatgpt-through-quantix-owned-oauth.md` — supersedes ADR 0004 + ADR 0008; amends 0012/0014 (provider connection ownership, credential location in Application Home, catalogue decision from spike)
- Modify: `CONTEXT.md` — Provider Credential term: OAuth tokens for the ChatGPT connection reside at `<application home>/auth.json`; Provider Connection term loses "Codex-managed" phrasing; Capability Catalogue entry notes built-in versioned catalogue if that was the spike decision
- Modify: `AGENTS.md` — add standing rule: *"Always choose the simplest approach that achieves the same goal; reject extra layers, configuration, indirection, or abstraction that do not change behavior."*
- Modify: `README.md` architecture paragraph + remove `smoke:codex` mention; `CONTRIBUTING.md` unaffected
- [ ] Steps: write docs; link Evidence sections to spike doc + spec; commit `docs: govern the direct chatgpt provider`

### Task 16: Acceptance lanes rework

**Files:**
- Modify: `src-tauri/src/acceptance.rs` + `src/bin/product_acceptance.rs` — deterministic areas touching codex readiness/login switch to token-store fixtures; live/private/native/release modes qualify the Quantix-owned flow (bundled-codex login assertions removed)
- Modify: `fixtures/acceptance/v1/*` only if oracle fields referenced codex readiness (inspect first; extend rather than weaken oracle)
- Modify: `scripts/setup-windows-private-release.sh` — replace bundled-codex qualification steps with Quantix-login live steps (same five-clean-runs discipline, binary-hash stability unchanged)
- [ ] Steps: update, run `acceptance:deterministic` locally against a fixture home, commit `test: qualify direct chatgpt acceptance lanes`

### Task 17: Final gates + landing

- [ ] Step 1: `npm test` (bindings drift check) → commit any regenerated DTOs
- [ ] Step 2: `npm run verify`
- [ ] Step 3: `npm run build`
- [ ] Step 4: Separation proof: rerun Task 14 grep → clean
- [ ] Step 5: Merge to main with `--no-ff` (user-authorized). Pre-merge: confirm with user how to handle their unrelated uncommitted WIP in the primary checkout (park on a side branch vs commit-as-WIP) since several WIP files overlap paths we changed; never discard it.
