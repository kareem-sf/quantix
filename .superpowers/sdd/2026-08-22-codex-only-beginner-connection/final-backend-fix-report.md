# Final backend fix report

## Status and commits

- Backend implementation commit: `44e3011d99317bf9afe9bcad18b41971952eef21` (`fix: harden chatgpt connection lifecycle`)
- Implementation base: `f6bb0aea97ab92aea9ff493a315f717ebee393ef`
- Scope: seven owned Rust modules under `src-tauri/src`; no renderer, generated binding, or approved design/plan changes are included.

## Design

### Connection and settings serialization

- One connection-mutation boundary now covers the authentication read/refresh decision and the SQLite settings projection written by application-settings refresh.
- Disconnect snapshots the active login state without retaining its mutex, then holds the connection-mutation boundary across cancellation/revocation, authentication removal, and the disconnected SQLite projection.
- Internal unlocked helpers let composite operations retain a single boundary without recursively locking it. The lock order is connection mutation before SQLite, while the login-state mutex is never retained across that boundary.
- Refresh, login persistence, and disconnect therefore have one deterministic ordering and cannot finish by persisting an older ready or absent projection after a later serialized mutation.

### Approval invalidation

- Live connection persistence updates connection, selection, and approval in one SQLite transaction.
- An existing approval is retained only when the existing selection remains supported and `approval_matches` succeeds against the current provider, account fingerprint, data destination, model, and reasoning configuration.
- A mismatch clears approval; live refresh never creates or silently grants approval.

### Callback state and browser copy

- Callback state is checked before every terminal outcome, including a provider-reported error. Missing or incorrect state produces a fixed safe page for that request but leaves the active callback attempt listening.
- Provider details are ignored when rendering failures. Every browser failure reason is selected from fixed novice ChatGPT sign-in copy that excludes protocol terminology, provider-response details, ports, process identifiers, and secrets.
- The primary browser sign-in success path and redaction behavior remain intact.

### Shared device deadline

- A single 15-minute deadline is created before device initiation and threaded through initiation, polling, exchange, mutation-lock acquisition, and the final persistence checks.
- Initiation, polling, and exchange request timeouts are capped by the remaining budget. Explicit pending `403`/`404` handling and `interval + 3 seconds` polling remain, bounded by that same deadline.
- Cancellation is observed after polling and again after exchange before persistence. Waiting to claim the persistence mutation boundary is also deadline-bounded.

## Files

- `src-tauri/src/application_settings.rs`
- `src-tauri/src/chatgpt_login.rs`
- `src-tauri/src/chatgpt_oauth/callback_server.rs`
- `src-tauri/src/chatgpt_oauth/device.rs`
- `src-tauri/src/chatgpt_oauth/mod.rs`
- `src-tauri/src/chatgpt_oauth/store.rs`
- `src-tauri/src/chatgpt_oauth/tokens.rs`

## Added static regression sources

- Deterministic settings interleavings cover ready-refresh versus disconnect and absent-refresh versus login persistence.
- Disconnect coverage blocks the SQLite transaction and demonstrates that the connection boundary remains held through the disconnected settings projection.
- Approval coverage invalidates on account fingerprint change and data-destination mismatch.
- Callback coverage exercises missing/wrong state before provider error, continuation after invalid state, later exact-state completion, and a terminology exclusion audit over all visible failure copy.
- Device coverage exercises initiation budget, full-flow initiation plus pending wait, `interval + 3 seconds`, exchange timeout capping, delayed exchange, cancellation after exchange, and bounded persistence-lock wait.

These test sources were deliberately not executed under the task constraint.

## Static evidence

- `rustfmt 1.9.0-stable (8bab26f4f6 2026-07-14)` formatted only the seven owned Rust files with edition 2021 and exited successfully. This provides parsing/formatting evidence, not compilation evidence.
- `git diff --check` and `git diff --cached --check` completed with no reported whitespace errors before the implementation commit.
- Read-only `rg` audits confirmed state validation precedes provider-error handling, the shared deadline is threaded through device stages, approval retention calls `approval_matches`, the requested regression source names are present, and obsolete callback/deadline symbols are absent.
- The implementation commit changes 7 files with 912 insertions and 186 deletions.
- No tests, Cargo checks, builds, Clippy, development servers, application commands, or generators were run. No runtime, compile, or test-pass claim is made.

## File SHA-256 hashes at the implementation commit

- `application_settings.rs`: `48D058DAED8715EC17C1D5437E7D4CE8A5E94998C4A5AB2A08D1F3ACAA36A533`
- `chatgpt_login.rs`: `E45C02A3F06828D2EB87324AE1E7F1B936A01EDCC072A7E762F2C52A07F61E2D`
- `chatgpt_oauth/callback_server.rs`: `1338111F5CF9C1A27AA26B80094046AC77FB15816DB76978432FCA321C2D836F`
- `chatgpt_oauth/device.rs`: `04CB7D0F399E40526319FDFA7E4D73AB6859821AABDBE3ED6067471542E4668F`
- `chatgpt_oauth/mod.rs`: `81F758ACA322D7624D67BC373815DCD62BACA11FB173F06373A0BD1AE3E413A8`
- `chatgpt_oauth/store.rs`: `72CD6A5A7F94DBB915B87C476D688D4B42F581B4D1350C52411786BD7630BC46`
- `chatgpt_oauth/tokens.rs`: `4381F1874C1528A3C91424CB0D91F5142D9AD9F47D7F77C155F2031B1AF79F0E`

## Concerns and limits

- The new regression sources are unexecuted and uncompiled by explicit instruction.
- Authentication-file and SQLite updates are serialized against competing in-process connection mutations, but they are separate durable stores and therefore are not crash-atomic as a pair.
- A final local filesystem synchronization that starts within the device budget can finish marginally after the deadline; all prior network waits, polling waits, cancellation checks, and mutation-lock acquisition are deadline-bounded.
- The pre-existing unstaged whitespace change in `docs/superpowers/specs/2026-08-22-codex-only-beginner-connection-design.md` was left untouched and excluded from the implementation commit.
