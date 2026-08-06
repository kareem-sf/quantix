# Enforce agent access through host-owned run grants

Quantix v0 enforces a versioned default-deny Permission Policy outside Codex and issues one immutable, short-lived Permission Grant per Agent Run from the intersection of the approved Capability, Agent Profile Version, Work Plan, Tender Task, exact data, and current policy. We chose host-materialized minimum-disclosure Data Views, fresh isolated workspaces, typed allowlisted tools, and disabled agent networking over prompt permissions, broad role sessions, persistent shared filesystems, or unrestricted shells so untrusted Tender content and model behavior cannot expand authority.

## Consequences

- Data Scope and Data Classification are independent. Tender Internal and Sensitive data remain compartmentalized, while Secret credentials, tokens, keys, signing material, and portal secrets are never model-visible or logged.
- Agents receive exact read-only inputs in a private Agent Run Workspace, may create only disposable Working Artifacts and staged outputs, and never access the Tender Store, original package location, user filesystem, repository, or another workspace directly.
- Every Typed Tool declares schemas, required scopes and classifications, side-effect class, version, resource limits, and audit behavior. Production roles receive no unrestricted shell, executable discovery, package installation, arbitrary MCP tool, application control, or direct network access; Codex provider transport is separate host infrastructure.
- Agents cannot perform canonical mutations. Quantix validates proposed records and artifacts, applies workflow rules, and registers accepted versions itself.
- Access expansion creates a blocked Access Request. A one-run expansion within the approved ceiling requires expiring EITL Access Approval; recurring or ceiling-changing access requires a Work Plan Amendment, and Secret data remains ineligible.
- Provider-thread exposure is irreversible and recorded in a cumulative Thread Exposure Set. Revocation stops active access immediately, and an incompatible next task archives the thread and starts a fresh one rather than pretending prior disclosure can be erased.
- Candidate outputs inherit the union of input scopes and highest classification. Declassification requires controlled redaction, provenance, independent verification, and any material EITL approval; agents cannot declassify their own work.
- Prohibited Actions cannot become agent permissions. Agent Runs capture grants, inputs, views, workspace manifests, tools, outputs, and usage, while append-only Audit Events record access decisions, denials, revocations, violations, classification changes, and cleanup without secrets or hidden reasoning.

## Evidence

- [Decision ticket](https://github.com/kareem-sf/quantix/issues/10)
- [Agent Profile runtime boundary](./0004-run-agent-profiles-through-host-controlled-codex-threads.md)
- [Controlled Team Composer boundary](./0005-compose-tender-teams-through-controlled-capability-demands.md)
