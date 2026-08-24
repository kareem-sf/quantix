---
status: accepted
---

# Run Agent Profiles through host-controlled Codex threads

The Codex runtime consequences in this ADR are superseded by
[ADR 0017](./0017-run-multiple-ai-connections-through-one-active-configuration.md#account-backed-codex).
Quantix now drives one pinned official Codex app-server per bounded operation,
passes Host-owned vault-backed tokens only through the privately prototyped
`chatgptAuthTokens` seam, and uses an ephemeral thread. It does not rely on
another Codex installation, Codex-owned persistent login, or a persistent thread
per Agent Profile.

The Host-controlled principles remain accepted: Quantix owns permission,
minimum-disclosure inputs, tool authorization, budgets, interruption, candidate
validation, evidence, audit, and canonical Tender state. Provider threads remain
noncanonical and cannot expand a run's authority.

## Historical record

The former design used an installed Codex app-server over stdio, one persistent
external thread per active Agent Profile, and a single supervised app-server
process. Its host validation, typed-tool control, and no-shared-sandbox lessons
remain binding rationale. Its authentication, process-lifetime,
persistent-thread, and protocol-version consequences are superseded and must not
be implemented as current product behavior.

The prototype result below is evidence of that former architecture, not a
claim about the current product.

## Historical evidence

- [Prototype and measured result](https://github.com/kareem-sf/quantix/blob/prototype/codex-multithread-tender-office/prototypes/codex-multithread/RESULTS.md)
- [Decision ticket](https://github.com/kareem-sf/quantix/issues/7)
- [Codex app-server documentation](https://learn.chatgpt.com/docs/app-server)
- [OpenAI authentication documentation](https://learn.chatgpt.com/docs/auth)
