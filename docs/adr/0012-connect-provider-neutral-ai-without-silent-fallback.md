---
status: superseded
superseded_by:
  - 0017-run-multiple-ai-connections-through-one-active-configuration.md
  - ../superpowers/specs/2026-08-24-layer-1-ai-connection-foundation-design.md
---

# Superseded: Connect provider-neutral AI without silent fallback

This ADR recorded an earlier multi-provider connection decision. It is not the
current connection catalogue, credential model, or runtime contract and must not
be used as a compatibility specification.

The current decision is
[ADR 0017](./0017-run-multiple-ai-connections-through-one-active-configuration.md),
refined by the
[Layer 1 AI connection foundation design](../superpowers/specs/2026-08-24-layer-1-ai-connection-foundation-design.md).
Several connections may be saved, but only one explicit global Active AI
Configuration may be selected and there is no fallback.

The enduring safety principles are unchanged: Quantix owns Tender workflow,
permissions, evidence, audit, validation, Engineer-in-the-Loop approvals, and
canonical Tender state. The connected AI service receives only bounded work and
cannot exercise those authorities.

## Historical note

This document remains only to preserve the decision trail. Its former provider
catalogue, authentication, credential, and routing details are obsolete. Its
no-silent-fallback, fail-closed validation, immutable per-run capture, and
Host-authority principles remain binding through ADR 0017.
