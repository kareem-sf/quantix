---
status: superseded
superseded_on: 2026-08-22
superseded_by:
  - 0016-connect-chatgpt-through-quantix-owned-oauth.md
  - ../superpowers/specs/2026-08-22-codex-only-beginner-connection-design.md
---

# Superseded: Run Agent Profiles through host-controlled Codex threads

This ADR recorded the original app-server and externally stored thread design.
It is historical evidence only and does not describe current Quantix behavior.
Quantix no longer connects to an installed or bundled Codex app-server, relies
on another Codex installation's login, or creates external Codex threads for
Agent Profiles.

The current design is [ADR 0016](./0016-connect-chatgpt-through-quantix-owned-oauth.md)
and the [approved beginner-connection design](../superpowers/specs/2026-08-22-codex-only-beginner-connection-design.md):
the Rust Host owns one direct ChatGPT OAuth connection under Quantix Application
Home and sends bounded HTTPS/SSE Provider Turns. Quantix retains workflow,
permissions, evidence, validation, audit, and Engineer-in-the-Loop authority.

## Historical record

The former design used an installed Codex app-server over stdio, one persistent
external thread per active Agent Profile, and a single supervised app-server
process. Its disposable-thread boundary, host validation, typed-tool control,
and no-shared-sandbox lessons remain useful historical rationale. Its
authentication, process, thread, protocol-version, and release consequences are
fully superseded and must not be implemented as current product behavior.

The prototype result below is evidence of that former architecture, not a
claim about the current product.

## Historical evidence

- [Prototype and measured result](https://github.com/kareem-sf/quantix/blob/prototype/codex-multithread-tender-office/prototypes/codex-multithread/RESULTS.md)
- [Decision ticket](https://github.com/kareem-sf/quantix/issues/7)
- [Former Codex app-server documentation](https://developers.openai.com/codex/app-server/)
- [OpenAI authentication documentation](https://learn.chatgpt.com/docs/auth)
