# Keep Codex behind a Quantix-owned AI Provider contract

Quantix v0 routes AI execution through one versioned, Quantix-owned semantic AI Provider Contract and one built-in Codex adapter using the Engineer User's Codex-managed ChatGPT subscription session. We chose this over exposing app-server throughout the domain or building speculative multi-provider machinery so Quantix retains workflow, permission, evidence, and EITL authority while provider protocols remain replaceable at one narrow boundary.

## Consequences

- v0 has no provider registry, picker, router, fallback chain, marketplace, dynamic loading, per-Agent provider selection, API-key UI, or BYOK path. A second provider must first prove a shared contract need before the boundary grows.
- Codex app-server supplies the model-facing agent loop, threads, turns, streamed events, provider control requests, and sandbox execution. Quantix adds no PydanticAI, Agents SDK, LangGraph, CrewAI, AutoGen, or other generic agent runtime in v0; the Rust Host owns the product-specific Team Composer, Tender Tasks, permissions, EITL, evidence, validation, and publication rather than duplicating those authorities in a second runtime.
- Codex owns browser login, credentials, refresh, configuration, and provider-managed thread storage. Quantix persists only credential-free connection state, opaque references, normalized events, Agent Runs, and audits under `~/.quantix`.
- The Codex adapter supports one generated app-server schema and one explicitly validated CLI/protocol version at a time. Startup fails visibly on incompatible authentication, versions, or mandatory capabilities instead of using compatibility shims or degraded authority.
- The generic surface is limited to inspecting readiness, establishing or resuming an Agent Profile thread, running one turn, interrupting it, archiving the thread, and shutting down the connection. Provider-specific RPC, model settings, history operations, and process details stay inside the adapter.
- One Agent Run maps to exactly one Provider Turn. Reconnection may reconcile that same turn, but a retry or repair that starts another turn creates a separate linked Agent Run; an outcome that cannot be established becomes Indeterminate and blocks the Tender Task.
- Every turn receives the current Provider Instruction Bundle, exact Data Views, output contract, Typed Tools, and Permission Grant-derived resource and sandbox constraints. Provider thread memory remains noncanonical, and a thread is replaced whenever its exact Agent Profile Version or exposure becomes incompatible.
- Quantix materializes the Agent Run Workspace and reauthorizes every correlated Provider Control Request. Arbitrary commands, unknown tools, network, access outside exact roots, provider-side approvals, and provider defaults cannot expand authority; duplicate side effects return the recorded idempotent result.
- Provider outputs remain candidates until independent host validation. The adapter normalizes ordered events, usage, rate limits, failures, interruption, and terminal results without retaining raw protocol traffic, streamed deltas, credentials, or hidden reasoning as canonical history.
- Automatic recovery is limited to idempotent transport work before turn acceptance or reconciliation of the same accepted turn. Revocation or shutdown interrupts active work, quarantines partial output, and stops the supervised app-server; v0 runs no hidden background Tender Office.
- Missing usage and capacity observations remain unknown. Quantix owns conservative scheduling and never invents subscription cost, purchases capacity, consumes reset credits, changes authentication mode, or silently falls back to another provider.

## Evidence

- [Decision ticket](https://github.com/kareem-sf/quantix/issues/12)
- [Codex thread runtime boundary](./0004-run-agent-profiles-through-host-controlled-codex-threads.md)
- [Host-owned permission boundary](./0006-enforce-agent-access-through-host-owned-run-grants.md)
- [Agent-framework selection research](../research/agent-framework-selection.md)
- [Official Codex app-server documentation](https://developers.openai.com/codex/app-server/)
- [Official Codex authentication documentation](https://developers.openai.com/codex/auth/)
