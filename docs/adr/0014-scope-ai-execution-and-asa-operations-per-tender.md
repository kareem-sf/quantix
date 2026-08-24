---
status: accepted
---

# Scope AI execution and ASA operations per Tender

ADR 0017 supersedes only the AI-selection scope in this decision. Connection and
model selection is now one optional global Active AI Configuration resolved and
immutably captured when an Agent Run is created. Tender-scoped work, Host
permission and persistence authority, immutable run binding, operational
isolation, and fail-closed execution remain accepted.

Quantix is a Tender-centric Agentic Super App. Every Tender owns one independent Tendering Manager Agent, AI Execution Selection, Tender Office Conversation, work state, files, Agent Runs, and operational health. The native Host remains the workflow, permission, evidence, persistence, repair, and Engineer-in-the-Loop authority. Quantix does not add a global Manager, cross-Tender memory, automatic model routing, silent fallback, or a second agent framework.

This decision supersedes only ADR 0012's application-wide runtime-selection consequence and ADR 0013's selected-Tender-header menu placement. Their Provider Contract, credential isolation, fail-closed capability validation, immutable per-Run capture, safe-terminal retention boundaries, Trash, purge, and Deletion Receipt consequences continue to apply.

## Consequences

- Application Settings stores the non-secret default AI Execution Selection for future Tenders. A new Tender copies that default once; later Application Settings changes never rewrite an existing Tender.
- Each Tender Store owns an optional AI Execution Selection with an optimistic revision. A Tender without a complete live selection remains available for local work while AI-required work is `Waiting for AI Provider`.
- Every new Agent Run validates and immutably captures the selected Tender's exact Provider Connection, provider-qualified model, reasoning setting, catalogue provenance, and adapter/runtime version. Existing and queued runs remain pinned.
- Quantix never silently substitutes a provider, model, or reasoning setting. Changing a Tender's selection is an attributable Tendering Engineer decision and affects only future Runs in that Tender.
- Commands are serialized within one Tender. Different Tenders may intake and execute concurrently; shared resource capacity queues excess work instead of rejecting it because another Tender is active.
- Tender rows expose Rename, Archive, and Move to Trash through one Quantix menu available from the row ellipsis, keyboard menu key, and secondary click. The Host supplies availability and the exact disabled reason.
- Permanent Tender Deletion remains available only in `Archived & Trash` after consequence review and explicit confirmation.
- Quantix Doctor diagnosis is automatic and redacted. Repairs are closed typed Host actions and run only after the Tendering Engineer reviews their impact and commands them; external outages truthfully report that no local repair is available.
- The Manager composer exposes governed Tools & Context plus the Tender's provider, model, and reasoning selection. It does not expose a generic full-access mode, arbitrary tools, shell execution, or Agent-owned permission expansion.

## Evidence

- [Manager-led workspace specification](../product/agentic-tender-workspace.md)
- [Manager-led orchestration decision](./0011-center-tenders-on-manager-agent-orchestration.md)
- [Provider-neutral AI decision](./0012-connect-provider-neutral-ai-without-silent-fallback.md)
- [Tender retention decision](./0013-separate-archive-trash-and-permanent-tender-deletion.md)
