---
status: superseded
superseded_by:
  - 0016-connect-chatgpt-through-quantix-owned-oauth.md
  - ../superpowers/specs/2026-08-22-codex-only-beginner-connection-design.md
---

# Superseded: Connect provider-neutral AI without silent fallback

This ADR recorded an earlier multi-provider connection decision. It is no
longer a statement of Quantix capability and must not be used to justify an
additional provider, an API-key connection, credential-vault storage, provider
routing, or a fallback chain.

Quantix now has exactly one AI connection: the Engineer's ChatGPT account. The
current connection and execution decision is [ADR 0016](./0016-connect-chatgpt-through-quantix-owned-oauth.md), refined by the [approved beginner-connection design](../superpowers/specs/2026-08-22-codex-only-beginner-connection-design.md).

The enduring safety principles are unchanged: Quantix owns Tender workflow,
permissions, evidence, audit, validation, Engineer-in-the-Loop approvals, and
canonical Tender state. The connected AI service receives only bounded work and
cannot exercise those authorities.

## Historical note

This document remains only to preserve the decision trail. Its former provider
catalogue, authentication, credential, and routing consequences are obsolete.
