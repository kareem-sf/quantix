# Run Agent Profiles through host-controlled Codex threads

Quantix v0 connects to the installed Codex app-server over stdio using the
Engineer User's existing ChatGPT-authenticated Codex login, with one persistent
Codex thread for each active Agent Profile in a Tender and only one locally
supervised app-server process at a time. Quantix supplies the versioned Agent
Profile instructions, exact registered Tender inputs, and a network-disabled
workspace-write sandbox scoped to that role; Quantix also owns scheduling,
task dependencies, structured output validation, Artifact registration, audit,
and every EITL decision and Approval Gate.

Codex threads and their sandbox files remain disposable working context rather
than the Tender system of record. A role's final structured output crosses a
host-controlled output contract and enters the Tender Store as Proposed before
it may be reviewed, verified, or approved. Quantix uses narrow typed tools and
a conservative scheduler instead of treating generated shell programs as an
artifact protocol or assuming an account-wide concurrency guarantee.

## Consequences

- First-time setup verifies a supported Codex CLI and ChatGPT login but never
  requests, copies, or stores an API key, session token, or account credential
  under `~/.quantix`.
- Each Agent Run records its provider thread reference, Agent Profile version,
  exact inputs, permissions, usage, outcome, and registered outputs without
  making the external thread canonical.
- Role turns may overlap, but Quantix controls when tasks and tool-bearing work
  start, supplies exact upstream Artifact Versions to downstream roles, and
  exposes rate-limit, interruption, retry, and failure states visibly.
- Cross-role workspace writes are denied. Inter-role handoff occurs only after
  Quantix validates and registers an output, never through a shared mutable
  sandbox or chat history.
- Quantix can interrupt an active turn and resume its persistent thread after
  replacing app-server; resumed work is still subject to current task inputs,
  permissions, staleness checks, and EITL controls.
- v0 supports one explicitly validated Codex CLI/app-server protocol version at
  a time and fails visibly on an unsupported version instead of maintaining
  compatibility shims.
- The validated prototype demonstrates this boundary on Windows with
  `codex-cli 0.146.1` and a ChatGPT Pro account: both producer turns overlapped,
  cross-workspace writes were blocked, a host-gated independent review ran,
  interruption and process-restart resumption succeeded, and all temporary
  threads were archived.
- This decision authorizes a private, engineer-operated v0 only. OpenAI does
  not provide an account-wide concurrency SLA, app-server remains experimental,
  and public paid distribution must not rely on ChatGPT subscription usage
  without explicit written confirmation from OpenAI.

## Evidence

- [Prototype and measured result](https://github.com/kareem-sf/quantix/blob/prototype/codex-multithread-tender-office/prototypes/codex-multithread/RESULTS.md)
- [Decision ticket](https://github.com/kareem-sf/quantix/issues/7)
- [Official Codex app-server documentation](https://developers.openai.com/codex/app-server/)
- [Official Codex authentication documentation](https://developers.openai.com/codex/auth/)
