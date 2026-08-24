# Layer 1 AI Connection Foundation Design

## 1. Outcome

Layer 1 gives a non-developer Windows Engineer one integrated place to save several
AI connections, test an exact model, and explicitly choose zero or one global
Active AI Configuration. Quantix supplies no provider, model, reasoning, or
fallback default.

The supported connection methods are:

1. ChatGPT/Codex account login, private-development only until written OpenAI
   approval permits the Quantix client/integration;
2. direct OpenAI, Anthropic, Google Gemini, or xAI provider key;
3. a custom OpenAI Chat Completions-compatible HTTPS endpoint; and
4. a custom Anthropic Messages-compatible HTTPS endpoint.

Exact literal `127.0.0.1` and `[::1]` HTTP endpoints are allowed for local compatible
services. Unauthenticated compatible endpoints are outside Layer 1.

## 2. Scope Boundary

Layer 1 owns connection persistence, provider/model testing, one active global
selection, normalized Provider Turns, immutable run binding, failure/recovery, and
Settings UX.

It does not add dynamic employee creation, a permanent employee roster, memory,
RAG, web/deep research, MCP catalogue, plugins, Tool Workshop, recursive spawning,
or adaptive improvement. Those remain later approved layers. Existing Tender work
continues to use the current team model until Layer 2 replaces it without a
compatibility roster.

## 3. Host and Worker Boundary

The Rust Host remains the only authority for:

- connection and active-configuration state;
- credentials and token refresh persistence;
- Agent Run creation and immutable binding;
- tool schemas, permission grants, approvals, execution, and idempotency;
- model/tool/token/time/output budgets;
- candidate validation and canonical Tender writes;
- cancellation, recovery, diagnostics, and audit.

Providers supply intelligence only.

### 3.1 Account-backed Codex

The Rust Host drives official Codex app-server 0.149.1 directly over supervised
stdio JSON-RPC. OpenAI recommends app-server when an agent is embedded inside a
product and the product needs lifecycle, streaming, interruption, tools, and
approval handling.

Quantix uses the app-server's host-managed `chatgptAuthTokens` and client-executed
`dynamicTools` seams. The pinned experimental schema labels the former unstable and
internal-only. Therefore:

- both seams are pinned to exact generated experimental schema and contract tests;
- they may be used only in private development;
- public/commercial activation requires written OpenAI approval;
- drift makes the account adapter incompatible;
- Quantix never constructs or calls a private ChatGPT execution URL; and
- failure never invokes the removed private adapter or another provider.

The Host owns browser/device login and refresh through maintained OpenID Connect
4.0.1 plus the pinned Codex auth parameters. Browser login uses PKCE, state, nonce,
issuer discovery/JWKS, signature, audience, expiry, and exact account validation.
Device code remains an explicit beta fallback. Refresh must preserve account ID,
plan, and residency.

Codex runs in a unique disposable credential-free `CODEX_HOME` and requests an
ephemeral thread. It receives an exact Quantix instruction bundle and output schema.
Shell, exec, web, image, app/plugin, memory, multi-agent, skill installation,
telemetry, feedback, analytics, update, and other native surfaces are disabled when
the pinned runtime supports doing so. Dynamic tools are additive, so Quantix does
not claim built-in tools are absent: any built-in command, patch, file, web, MCP,
app, permission, or collaboration request/event is a security failure and
terminates the turn. Quantix executes only Host-declared dynamic tools.

The pinned read-only sandbox schema cannot restrict readable roots and therefore is
not a production filesystem-isolation guarantee. Quantix supplies an empty staged
workspace and detects built-in actions, but account mode remains non-shippable until
the approved integration also satisfies the required filesystem boundary.

The reserved built-in Codex account provider rejects retry overrides. Quantix
therefore accepts the pinned runtime's fixed bounded transport recovery—four request
retries and five interrupted-stream retries—as an explicit adapter limitation. It
adds no outer Codex retry and records missing internal retry usage as unknown. One
same-request authentication continuation is allowed after a successfully persisted
same-account token refresh; a second refresh request fails authentication.

### 3.2 General Providers

A disposable Python 3.12.13 worker uses
`pydantic-ai-slim[anthropic,google,openai,xai]==2.33.0`:

- direct OpenAI uses `OpenAIResponsesModel`;
- custom OpenAI-compatible uses `OpenAIChatModel`;
- direct/custom Anthropic uses `AnthropicModel`;
- Gemini uses `GoogleModel`; and
- xAI uses `XaiModel`.

Pydantic AI is only the model-facing stream/structured-output/external-tool loop.
It is not Quantix's orchestrator, memory, workflow, approval, retry, fallback, or
durability layer. External tools defer to the Host. Every continuation shares one
cumulative usage object and remaining budget.

All general-provider/Pydantic retries are disabled. The Host may authorize one
bounded transient retry on the same connection revision/model when no uncertain
side effect exists. `FallbackModel` is forbidden.

The worker is installed non-editably in an isolated uv-locked venv and launched as
the exact venv `python.exe -I -m quantix_ai_worker`, with environment inheritance,
proxies, `.netrc`, user-site packages, current-directory imports, telemetry, and raw
stderr persistence disabled.

## 4. Connection and Active Configuration Model

A connection has:

- a random 128-bit ID;
- method and provider kind;
- display name;
- enabled state;
- semantic `execution_revision`;
- secret-only `credential_generation`;
- provider/account or compatible endpoint identity;
- encrypted credential/header/query material;
- discovered model metadata; and
- exact probe evidence.

Renaming changes no execution revision. Provider, endpoint, credential placement,
custom header/query, different account, or user key replacement increments the
execution revision and clears probe evidence. A verified same-account OAuth token
rotation increments only credential generation and preserves the active semantic
revision/probe.

An Active AI Configuration pins:

- connection ID and execution revision;
- provider and data-destination fingerprint;
- requested and reported model identity when available;
- explicit reasoning selection or proven unsupported state;
- adapter/runtime version;
- deterministic capability-catalogue hash; and
- activation time and attributable Engineer action.

Probe time is stored outside the semantic catalogue hash. No test action activates
a connection. Adapter or catalogue drift makes the active reference stale and asks
the Engineer to retest/reselect.

Disabling affects future runs and lets a running bounded turn reach terminal.
Disconnect cancels nonterminal operations, invalidates pending approvals, rejects
late tool results, and removes the credential. Deleting an active connection or one
referenced by a nonterminal run is rejected.

## 5. Credential Vault

All complete connection configuration and persistent secret material at rest lives
in `~/.quantix/ai-connections.vault`. The complete versioned JSON payload is
encrypted with current-user DPAPI. Quantix never uses machine-scoped DPAPI,
Credential Manager, a keyring, `auth.json`, an application password, or portable
credential export.

During connection entry, the WebView and Tauri IPC temporarily hold the newly typed
secret. JavaScript zeroization is not claimed. The secret is never returned,
persisted in renderer state/storage, logged, or rendered again. During execution,
only the Host and assigned worker receive the stored credential in memory/private
stdio.

Vault publication uses:

- connection/account-auth gate;
- in-process mutex;
- persistent cross-process file lock;
- handle-based no-follow validation;
- same-directory create-new staging;
- write, flush, and `sync_all`;
- `ReplaceFileW` with supported flags for an existing target;
- `MoveFileExW(MOVEFILE_WRITE_THROUGH)` for first publication; and
- reopen/hash/revision reconciliation after ambiguous publication errors.

Corrupt or unsupported ciphertext never projects as empty. An exact-confirmation
Host recovery action may clear the active reference, remove only the validated
vault, record redacted hash/size evidence, and create a new encrypted empty vault.
The Engineer must reconnect.

The fixed multi-store order is connection/account-auth gate → in-process vault
mutex → cross-process vault lock → installation SQLite → Tender SQLite. Every read
and write obeys it. Store locks are released before a worker starts.

## 6. Compatible Endpoint Network Policy

Compatible base URLs reject userinfo, fragments, embedded query strings, path
escape, and ordinary transport/authentication headers. The configured path prefix
is preserved when routes are joined.

The guarded transport:

- allows HTTP only for literal `127.0.0.1` or `[::1]` and verifies the connected
  address remains loopback;
- requires TLS and hostname validation otherwise;
- disables redirects, proxies, `.netrc`, and transport retries;
- rejects unspecified, multicast, broadcast, link-local/metadata, IPv4-mapped, and
  mixed public/private DNS result sets;
- rejects destination-class drift from successful probe to turn;
- bounds compressed/decompressed response bytes and every timeout; and
- redacts URL query/header values and raw response bodies.

Compatible authentication is either standard bearer authorization or one validated
dedicated API-key header. Named-key mode strips the SDK's normal auth header so only
the selected header is sent.

## 7. Model Discovery and Probe

Direct providers use their official model-list APIs. Compatible endpoints attempt
the matching list route; a missing list route is acceptable only when the Engineer
entered a model ID and that exact model passes the probe.

Quantix never uses provider default/recommended flags. It tests the exact selected
model/reasoning pair under a disclosed, bounded, possibly billable probe. Capability
states are supported, unsupported, or unknown. Structured output also records its
mode: native JSON Schema, tool, prompted, unsupported, or unknown.

Requested and reported model identities are compared when the provider exposes the
latter. A later mismatch fails as reroute. If identity is not reported, reroute
detection remains unknown rather than falsely guaranteed.

## 8. Runtime Contract and Budgets

The general worker uses strict version-1 JSONL with one initialize handshake, one
probe or turn, correlated tool results, optional cancel, final shutdown, monotonic
sequence, and exactly one terminal frame. Codex keeps its generated app-server
JSON-RPC protocol; the Host maps it into the same semantic types.

Every operation enforces frame, cumulative bytes, process lifetime, model request,
token, tool-round, and output bounds. The existing 32-tool-round hard ceiling
remains. A cancellation control path stays usable after the operation token fires;
then ProcessKit terminates the complete Windows Job Object after a five-second
grace period. Job Object containment proves descendant cleanup only, not filesystem
or network sandboxing.

## 9. Persistence and Schema

Installation schema 26 stores general preferences and the optional non-secret
active reference. Provider connection rows are removed because the vault owns the
complete records.

Tender schema 46 removes mutable per-Tender AI selection. The immutable
`agent_run_provider_bindings` record remains. The global configuration is resolved
only when an Agent Run is created; queued work has no pre-run binding. The Host
holds the fixed store order through Agent Run plus binding commit, releases locks,
then starts the worker.

There is no migration or compatibility deserializer. Known schema-25/45
development data is classified as unsupported, preserved byte-for-byte, and never
misreported as corruption. Legacy plaintext `auth.json` is detected and removed
only through an explicit exact-path decision. No Tender data is silently purged.

Backups and archives exclude the vault, lock/temp files, runtimes/venvs, raw worker
state, and every Codex home. Historical credential-free run bindings remain.

## 10. Settings Experience

The visible flow is:

**AI Settings → Connections → Add or sign in → Load models → Test exact model → Set
Active**.

The UI supports create, rename/edit, credential replacement, discovery, test,
enable/disable, disconnect, guarded delete, clear active, and corrupt-vault reset.
It never preselects the first connection/model/reasoning option. It discloses before
probe that a small request may consume provider usage.

Secrets use write-only password fields and are cleared best-effort on every settle,
cancel, close, navigation, and unmount. Existing credentials and custom values are
never refilled. Device verification code exists only during the initiating login
flow and is never persisted.

## 11. Failure and Recovery

Authentication, quota, capability, worker crash, timeout, cancellation, protocol
drift, model reroute, invalid output, vault failure, and indeterminate outcome are
typed and redacted. No failure selects another provider/model or starts an
unattributed turn.

The exact Codex home is deleted on every exit and swept if abandoned. An
indeterminate Agent Run preserves its separate run workspace/evidence, not raw
provider state. Corrupt-vault reset and legacy-auth removal are explicit destructive
actions. Missing usage remains unknown.

## 12. Acceptance

Deterministic tests cover seven concrete routes: Codex account, OpenAI, Anthropic,
Gemini, xAI, OpenAI-compatible, and Anthropic-compatible. Required evidence covers:

- zero factory defaults and no fallback;
- exact revision/credential-generation semantics;
- run A/new run B pinning;
- DPAPI corruption, atomicity, locking, DACLs, and recovery;
- post-submission secret absence in every returned/persisted/exported form;
- redirects/proxies/DNS/headers/retries and request counts;
- worker crash/cancel/Drop/descendant cleanup;
- duplicate tool-call idempotency;
- pinned experimental Codex schema/config/tool behavior; and
- backup/diagnostic/release evidence.

Live provider checks remain opt-in. Account-backed public release remains blocked
without written OpenAI approval. Production builds remain release-stage only.

## 13. Threat Boundary

DPAPI protects credentials at rest from offline access by another ordinary Windows
account or copied backup. It does not protect against pagefile/hibernation capture,
operating-system crash dumps, an administrator, kernel compromise, or a malicious
process already running as the same Windows user. Full-disk encryption and OS
hardening remain organizational responsibilities.

## 14. Primary Sources

- [OpenAI: Codex as a platform](https://developers.openai.com/blog/codex-as-a-platform)
- [OpenAI Codex app-server](https://learn.chatgpt.com/docs/app-server)
- [OpenAI Codex authentication](https://learn.chatgpt.com/docs/auth)
- [Pydantic AI providers](https://pydantic.dev/docs/ai/models/overview/)
- [Pydantic AI deferred tools](https://pydantic.dev/docs/ai/tools-toolsets/deferred-tools/)
- [Microsoft CryptProtectData](https://learn.microsoft.com/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)
- [Microsoft ReplaceFileW](https://learn.microsoft.com/windows/win32/api/winbase/nf-winbase-replacefilew)
