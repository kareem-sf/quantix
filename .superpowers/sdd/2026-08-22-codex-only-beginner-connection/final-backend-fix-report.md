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

## Independent-review follow-up

### Commit

`5eb57786e35fef7ae907b9e1cc7fdfff1450b75f` (`fix: serialize connection publication`)

### Design and scope

- Browser and device persistence now write `auth.json` and the matching ready provider/default-selection projection while retaining one connection-mutation boundary. A projection failure restores the previous authorization state; cancellation and the device deadline are checked before and after projection, and a post-projection cancellation also restores the prior settings projection.
- Selection update and confirmation now run their current-auth observation/refresh, projection, selection validation, SQLite transaction, approval creation or invalidation, and returned settings load inside one connection-mutation closure. Confirmation constructs its account fingerprint only from the revalidated connection held by that closure.
- Production subscription inspection and production provider-turn readiness now use one shared observe/refresh-and-project primitive. Provider success and failure projections occur before the connection boundary is released; the runtime-fixture provider path remains separate.
- Auth refresh is restored to its preceding state if the matching SQLite projection cannot be persisted. Lock order remains connection mutation before auth-file and SQLite operations. Login-state and tender-store mutexes are not retained while the connection boundary is held.

### Static regression sources

- Browser completion asserts connected auth, a ready matching provider, and the default model selection immediately after persistence.
- Projection failure asserts restoration of the prior authorized connection.
- Runtime failure projection versus later login asserts that the later serialized login leaves connected, ready settings instead of a split state.
- Deterministic update-versus-disconnect and confirmation-versus-account-replacement interleavings assert that stale selection or approval cannot publish after the later mutation.

### Static evidence and limitations

- `rustfmt --edition 2021` completed successfully on the five follow-up Rust files. This is parser/format evidence only.
- `git diff --check` and `git diff --cached --check` reported no whitespace errors, and read-only symbol audits confirmed both production runtime readiness call sites use `project_chatgpt_connection_readiness`; the removed separate runtime failure save and asynchronous selection-readiness helper are absent.
- No tests, Cargo checks, builds, Clippy, development servers, application commands, or generators were run. The new regression sources remain unexecuted and uncompiled by explicit instruction.
- A cancelled production runtime inspection can stop awaiting its blocking worker, but that worker still completes observation and its matching projection under one boundary. Cancellation therefore cannot leave a half-published runtime observation.
- Auth-file and SQLite writes remain separate durable stores and are not crash-atomic, although in-process publication, rollback, and competing mutations are serialized.

### Follow-up file SHA-256 hashes

- `agent_runtime.rs`: `ED43B06D2DCA7DCAADBA2EEF4688D0B18BB42AD37921E1D45EB8545B82F0A1D6`
- `application_settings.rs`: `7135B430F1156D1EAE245320BC9FC851CE10C9F7FA4A060D8C84B6989A1512F3`
- `chatgpt_login.rs`: `762CF28DF8A38718CF606964D308BC37DEA1851D692056860A5D8FF5815B0DCE`
- `chatgpt_oauth/mod.rs`: `93A3C5842C9C514702C79BFC8EEFF76311CCCF2C2FD48D726D3EF89E89FC6E49`
- `chatgpt_oauth/store.rs`: `1CAD2FC995134C981E86524345C041CFC407CF38532651F00E964D5133CD176B`

## Execution-authorization follow-up

### Commit

`2f9381d2b87dc5e751f115e096a3a23de9b906f8` (`fix: revalidate provider execution approval`)

### Security boundary

- Cached per-Tender readiness is no longer an execution authority. `require_current_tender_ai_selection` copies the Tender selection while holding the Tender-store lock, releases that lock, then refreshes/projects current auth and requires the exact global selection and account-bound approval inside the connection-mutation boundary.
- Production provider turns repeat this gate immediately before content handling: the same connection closure refreshes auth, publishes its matching settings projection, compares the prepared exact selection to the current global selection/approval, and returns the `StoredConnection` whose credentials the backend request will use. The connection mutex is released before content construction and provider network execution.
- A disconnect or account replacement that linearizes before either gate returns an authentication-required failure. If it occurs between Tender preparation and provider-turn startup, the second gate prevents content from being sent under the replacement account.
- Provider token refresh is forward-only. Application-settings projection and selection failures no longer restore a pre-refresh credential whose refresh token may have been consumed. The refreshed auth remains stored while the operation returns blocked; renderer identity matching and both execution gates remain false until its matching projection is published. Browser/device authorization-code rollback remains separate because that path did not consume the previous refresh credential.
- Lock ordering remains Tender snapshot/release, then connection mutation, then SQLite. No connection mutex is held while taking a Tender-store lock or executing a provider request.

### Static regression sources and evidence

- A real cached `Ready` Tender followed by account replacement must return `AiProviderRequired` from `require_current_tender_ai_selection`.
- A provider-turn selection approved for account A is rejected after auth changes to account B; the account-B projection also invalidates the old approval.
- A simulated rotated refresh credential remains authoritative when SQLite projection fails; the consumed old refresh token is never restored.
- `rustfmt --edition 2021` completed successfully on both modified Rust files. `git diff --check` and `git diff --cached --check` reported no whitespace errors, and read-only source audits confirmed production provider turn uses `project_approved_chatgpt_connection` and application-settings no longer calls `restore_unlocked`.
- No tests, Cargo checks, builds, Clippy, development servers, application commands, or generators were run. These regression sources remain unexecuted and uncompiled by explicit instruction.

### File SHA-256 hashes

- `agent_runtime.rs`: `E731C3252AFF4F2944F9FDE6EC27B7490E83096303E2EE705974D746CBD13648`
- `application_settings.rs`: `D5CF393DDC6674F2120433227BAD68077ACC570D103D416C8F4C42F5CCC4FDEF`

## Account-bound 401 retry follow-up

### Commit

`b458857eb6fad51c5b18fb3f057e78934146fedb` (`fix: bind provider retry to approved account`)

### Security boundary

- The backend turn executor no longer reloads whichever credential happens to be current after a 401. It receives a narrow refresh callback, passes the originally validated account identifier into it, and independently rejects any returned connection whose account identifier differs before a retry can be sent.
- The production callback checks that the stored connection still belongs to the expected account while holding the connection-mutation boundary. A replacement account is rejected before its refresh credential or Tender content can be used.
- A same-account refresh, its settings projection, and exact prepared-selection/account-bound approval revalidation occur inside that same boundary. The mutex is released before the single retry request, and no Tender-store lock or provider request occurs while it is held.
- Provider cancellation is checked before the blocking refresh and again before the retry. A cancellation or any account, selection, approval, projection, or refresh failure stops the turn without a second content request.
- The direct serialized-store refresh wrapper is now test-only; production 401 recovery must pass through the approval-aware application-settings composite.

### Static regression sources and evidence

- `account_replacement_between_401_and_retry_never_receives_tender_content` scripts an initial request with account A, a 401, and a refresh callback returning account B. It asserts authentication-required, one request body, and only account A's access token observed by the backend.
- `unauthorized_retry_rejects_replacement_account_before_refresh` stores account B after account A was approved and asserts the production composite rejects it without contacting the refresh issuer or consuming account B's refresh credential.
- The existing successful 401 source continues to cover one same-account refresh and one retry. Its fixture identity now carries the same account identifier as the approved connection so the account invariant is explicit.
- `rustfmt --edition 2021` completed successfully on the five modified Rust files. `git diff --check` and `git diff --cached --check` reported no whitespace errors. Read-only symbol audits confirmed production runtime supplies `refresh_approved_chatgpt_connection`, the executor compares returned and original account identifiers before the second backend call, and the unrestricted serialized refresh wrapper is absent from production code.
- No tests, Cargo checks, builds, Clippy, development servers, application commands, or generators were run. The regression sources remain unexecuted and uncompiled by explicit instruction.

### File SHA-256 hashes

- `agent_backend/turn_executor.rs`: `C09446D746C1B77A42BC5A5CEDC4A2899392BCEC59DA38653EE723EF13173E4B`
- `agent_runtime.rs`: `B9025664CF8A6583F806123870B38E9EE573B0DE5E08BD7367D968B55E62013B`
- `application_settings.rs`: `972A6922A3E06610B3F11E1257562CBFBA40ABF113F0562A319ED5C99E976296`
- `chatgpt_oauth/mod.rs`: `510C5897BFD0906D4BCA23CCFE23C140F5E6C45D51934BA73A5C87BAFD6D96CC`
- `chatgpt_oauth/store.rs`: `0A4D63177EFF01FEBA63B20A575CF7D7170C54197830F240064CE8DE67B93535`
