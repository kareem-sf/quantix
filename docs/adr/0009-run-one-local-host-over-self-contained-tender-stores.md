# Run one local Quantix Host over self-contained Tender Stores

Status: accepted; desktop runtime revised on 2026-08-07 by explicit Engineer User decision.

The former Codex authentication, credential, thread, and execution consequences
in this decision are superseded by ADR 0016 and the [approved
beginner-connection design](../superpowers/specs/2026-08-22-codex-only-beginner-connection-design.md).
The whole-Tender retention and permanent-purge consequences are revised by ADR
0013. Its remaining desktop Host, storage, local-tool process-supervision,
recovery, update, and release-qualification consequences remain accepted.

Quantix v0 is a Tauri 2 desktop application with one React/TypeScript renderer and one genuine Rust Quantix Host in Tauri's Core process. There is no Electron implementation, Node Host sidecar, local web server, runtime selector, or shared shell abstraction. The Rust Host is the only writer for the Engineer User's `~/.quantix` application home and owns domain commands, EITL enforcement, persistence, process supervision, recovery, and updates.

The earlier Electron/TypeScript Host choice was reconsidered through primary-source research. That research found Tauri/Rust feasible but warned that it adds Rust-to-TypeScript binding coordination, explicit process-tree supervision, and more Quantix-owned transition code. The Engineer User explicitly accepted those costs, cancelled the packaged comparison, and selected Tauri/Rust for the durable cross-platform architecture. This is a product-owner constraint, not a claim that Tauri won an unperformed benchmark.

## Consequences

- Tauri's Core process is the trusted Host. The operating-system WebView loads bundled local UI only, uses a restrictive Content Security Policy, and receives no filesystem, SQL, generic shell, credential, or updater authority. A small capability manifest permits only named Quantix commands for the main WebView.
- Renderer commands are domain-shaped rather than crate-shaped. Strict Serde DTOs deny unknown fields, `garde` enforces Safety Limits, and `ts-rs` generates committed TypeScript declarations. The renderer never learns database paths, object-store keys, process arguments, audit ordering, or updater primitives.
- Windows 11 x64, macOS 14+ Apple Silicon, and Ubuntu 24.04 x64 are the supported desktop targets. Tauri CLI produces native platform artifacts from native CI runners. Platform-WebView differences, signing, updater behavior, keyboard operation, and native assistive technology must be verified on every supported target before release.
- `tauri-plugin-single-instance` establishes one application owner. A second launch activates the existing window instead of opening another writer for `~/.quantix`.
- First-run Quantix Setup creates and validates `~/.quantix`, initializes
  non-secret installation state, and uses a bundled, pinned `uv` sidecar to
  install the exact locked Docling runtime and prefetched models below that
  root. Confidentiality relies on private application-home permissions, safe
  canonical paths, atomic publication, and full-disk encryption where required.
  A ChatGPT connection later stores its Quantix-owned OAuth tokens in
  `~/.quantix/auth.json`. The signed application does not bundle, locate,
  launch, or supervise a Codex executable or app-server.
- The application home contains `installation.sqlite`; isolated `tenders/<tender-id>/` directories with `tender.sqlite`, content objects, run work, and staging; and application-wide Docling runtime/model, backup, export, log, and trash directories. The installation catalogue is rebuildable, while non-secret settings, backup records, runtime manifests, and deletion receipts are installation facts.
- Each Tender Store is a self-contained source of truth. A deep Tender Store Module hides `rusqlite`, content-addressed storage, archive handling, audit chaining, integrity verification, backup, restore, and recovery behind one domain Interface. The Host owns one writer connection per open Tender; blocking SQLite work runs away from the WebView/event loop.
- `rusqlite` uses one bundled SQLite build with foreign keys, WAL, a defined busy timeout and synchronous policy, integrity checks, runtime limits, and online backup. Quantix v0 uses one exact schema and fails closed on mismatch; it adds neither an ORM nor a general migration framework.
- Workflow state is represented by typed Rust domain states, events, guards, and pure transition functions adjacent to the owning commands. Persisted facts and Audit Events remain canonical. No workflow engine is added until an end-to-end implementation demonstrates repeated hierarchical behavior that a maintained dependency materially simplifies.
- Registered Source Artifact and Artifact Version bytes are streamed into a maintained SHA-256 content-addressed store. SQLite records digest, size, media type, provenance, and logical version; cache indexes are not domain state, and connected Tender Package paths are no longer required after intake.
- Publication order is invariant: finish and verify the content write; lock the Tender writer; begin one SQLite transaction; register the immutable version, advance its logical pointer, and append the Audit Event; commit; then return the domain result. A crash may leave an unreferenced valid object for reconciliation, but a committed record can never reference unfinished bytes.
- Audit Events form an immutable per-Tender sequence whose RFC 8785 canonical payload, preceding hash, and current hash are protected with SHA-256; database triggers reject update and deletion. Verified backups and archives anchor the event count and chain head. This is tamper-evident against selective mutation, not tamper-proof against an administrator controlling every local file.
- A deep Process Supervisor Module owns Tokio tasks, bounded stdio,
  cancellation, timeouts, stderr limits, and terminal process facts for Docling
  and other explicitly approved local tools. ProcessKit supplies the Windows Job
  Object used by private v0. The renderer receives no Tauri shell-plugin
  permission and cannot choose executables or arguments.
- Quantix supervises one disposable official Docling CLI process per parsing
  attempt. It receives staged inputs and candidate-output locations only and
  never writes SQLite or canonical objects. Quantix validates Docling output
  before publication. ChatGPT Provider Turns are direct HTTPS/SSE requests
  owned by the Rust Host, not work delegated to a local Codex process. Quantix
  does not create a Python daemon, localhost server, plugin runtime, or
  recursive process killer.
- ChatGPT authorization, subscription eligibility, and backend behavior are
  externally controlled. Private qualification may establish technical evidence,
  but public release remains blocked until applicable OpenAI terms and product
  authorization permit Quantix's subscription-backed integration. An explicit
  risk decision cannot waive contractual authorization.
- Startup reconciles persisted operations before accepting work. SQLite rolls
  back incomplete transactions, unpublished staging remains noncanonical,
  proven unreferenced objects may be cleaned, interrupted Docling work becomes
  failed or indeterminate, and only a provably exact accepted Provider Turn may
  resume. Commands, approvals, side effects, and provider turns are never
  silently replayed.
- Opening a Tender performs a quick database check and verifies its Audit Event chain. Relevant full integrity and digest verification precedes Tender Backup, Portable Tender Archive creation, Tender Recovery, permanent purge, and Submission Package release. Any failure puts the Tender into read-only Recovery Required and blocks canonical work until EITL-controlled recovery or purge.
- `rusqlite` online backup and the maintained Rust `zip` implementation create verified ZIP64 Tender Backups and Portable Tender Archives. Archives carry a versioned canonical manifest, database digest, referenced content digests and sizes, Audit Event count, and chain head. Restore verifies in staging before atomic publication, blocks identity collisions, and never merges or overwrites.
- Archive is a reversible read-only Tender state. Approved deletion moves the complete store into Quantix Trash; permanent purge is a separate EITL decision and leaves a minimal installation-level Deletion Receipt. Canonical records, revisions, Audit Events, backups, and quarantined failures are not silently pruned.
- Live Tender confidentiality relies on the signed-in operating-system account,
  restrictive local permissions, safe canonical paths, and atomic publication.
  Full-disk encryption is an operating-system or organizational responsibility,
  not a Quantix runtime requirement. Portable Tender Archives remain encrypted
  when leaving that protected store. Quantix does not add local encryption whose
  automatically available key would not protect against the same signed-in user.
- Every intake, archive, parser job, Agent Run, output, IPC message, and storage operation has explicit non-overridable limits for bytes, entries, expansion, nesting, time, memory, output, and free disk. Exceeding a limit publishes no partial state and is audited; EITL cannot convert a safety violation into permission.
- A deep Update Module wraps `tauri-plugin-updater`. Updates require a valid signature, explicit Engineer approval, no active irreversible work, child-process quiescence, compatible runtime manifests, and a verified backup when stored data may be affected. The updater's generic commands are not exposed directly to the renderer.
- The minimum intended implementation stack is Tauri 2, React, TypeScript, Vite, Tokio, ProcessKit, `rusqlite`, `cacache`, Rust `zip`, Serde, `serde_json`, `serde_json_canonicalizer`, `sha2`, `garde`, `ts-rs`, `jsonschema`, `tracing`, `tempfile`, `thiserror`, pinned `uv`, and Docling. Exact versions and features belong in lockfiles and are checked against current documentation before implementation.

Implementation correction (2026-08-07): the first implementation used `process-wrap` 9.1.0, but an executable Windows host-death test proved that its Job Object was not configured with kill-on-close when combined with its `KillOnDrop` wrapper. ProcessKit 3.2.0 replaced only that containment seam after the equivalent descendant-lock test passed. The Host-owned supervisor boundary and the rest of this decision are unchanged.

Qualification boundary (2026-08-07): ticket #27 proves descendant containment and abrupt Host-death cleanup only on the current private-v0 platform, Windows 11 x64. ProcessKit 3.2.0 documents Linux parent-death handling for the direct child and no equivalent macOS support; it does not by itself prove whole-tree abrupt cleanup on those targets. Platform-native whole-tree containment and enforceable per-job CPU, memory, process-count, and disk limits remain mandatory public-release acceptance work under ADR 0010. The current slice claims bounded stdio, stdin, time, cancellation, and Windows descendant lifetime—not those deferred platform and resource controls.

## Rejected alternatives

- Electron with a TypeScript Host is removed from the architecture. Its provisional research advantage in integration cost was consciously traded for a Rust-owned Tauri Core, declarative capabilities, native binaries without Electron ABI rebuilding, and one long-term cross-platform desktop direction.
- Tauri with a Node Host sidecar is rejected because it would make Rust shallow glue and introduce another privileged process and protocol.
- A renderer-accessible SQL plugin, shell plugin, filesystem plugin, or updater Interface is rejected because it enlarges the least-trusted Interface.
- A local HTTP/WebSocket application server, persistent Python daemon, microservices, shared database, runtime selector, and dual Electron/Tauri implementation are rejected as unnecessary seams.

## Evidence

- [Original local runtime and storage decision](https://github.com/kareem-sf/quantix/issues/13)
- [Runtime re-evaluation map](https://github.com/kareem-sf/quantix/issues/16)
- [Desktop security and distribution research](../research/electron-tauri-security-distribution.md)
- [Smallest mature Rust Host stack](../research/rust-host-stack.md)
- [Codex and Docling desktop Host research (historical)](../research/codex-docling-desktop-hosts.md)
- [Tender Store source-of-truth decision](./0002-keep-chats-outside-the-tender-system-of-record.md)
- [Superseded Codex thread runtime decision](./0004-run-agent-profiles-through-host-controlled-codex-threads.md)
- [Host-owned permission decision](./0006-enforce-agent-access-through-host-owned-run-grants.md)
- [Current ChatGPT connection decision](./0016-connect-chatgpt-through-quantix-owned-oauth.md)
- [Layered product acceptance](./0010-qualify-v0-through-layered-product-acceptance.md)
- [Agent-framework selection research](../research/agent-framework-selection.md)
- [Tauri process model](https://v2.tauri.app/concept/process-model/)
- [Tauri capabilities](https://v2.tauri.app/security/capabilities/)
- [Tauri sidecars](https://v2.tauri.app/develop/sidecar/)
- [Tauri updater](https://v2.tauri.app/plugin/updater/)
- [SQLite write-ahead logging](https://www.sqlite.org/wal.html)
- [SQLite online backup](https://www.sqlite.org/backup.html)
- [Docling CLI](https://docling-project.github.io/docling/reference/cli/)
