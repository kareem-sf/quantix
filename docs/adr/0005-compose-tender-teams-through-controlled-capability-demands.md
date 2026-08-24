# Compose Tender teams through controlled Capability Demands

The fixed Bootstrap Team in this ADR is superseded by the
[controlled modular agent-platform design](../superpowers/specs/2026-08-24-controlled-modular-agent-platform-design.md),
but it remains the implementation fact until Layer 2 replaces it. Layer 1 changes
AI connection infrastructure only and must not create a compatibility roster.
The capability, separation-of-duty, review, and Host-approval principles below
remain accepted.

Quantix v0 uses a host-controlled Team Composer to map exact Project Fingerprint, Capability Catalogue, and policy versions into classified Capability Demands and a versioned Work Plan Proposal. Deterministic rules own Policy-required and Tender-required coverage, Codex may propose evidence-linked Risk-recommended specialists and operational profile details, and only the Engineer User may approve the resulting team; this preserves a genuinely project-specific Tender Office without allowing an AI agent to invent expertise, permissions, reviewers, or approval authority.

## Consequences

- Creating a Tender activates only the restricted Bootstrap Team: Tender Office Coordinator, Document Controller, Tender Analyst, and Independent Reviewer. After a Proceed Bid Decision, Work Plan Approval authorizes an explicit Active Production transition that activates exactly those core profiles plus the mandatory Cost Estimator and every approved conditional specialist; approval alone does not start production work.
- Each Tender-scoped Agent Profile represents one coherent professional role with operational traits, an exact immutable version, and one persistent provider thread. Compatible Capabilities may be combined, but independence, permission, qualification, or workload boundaries force separate profiles.
- Team Composer may specialize approved Capability Catalogue components into project-specific roles. An unsupported need becomes a visible Capability Gap and blocks affected work instead of producing a fictional expert.
- Every Tender Task has one accountable Agent Profile version, an evidence or policy trigger, exact inputs, dependencies, permissions, budget, output contract, and review policy. Contributors work through registered child tasks rather than shared mutable work.
- Authors and reviewers always use separate profiles and threads. Specialist outputs require reviewers with matching approved Capabilities; reviewers cannot edit targets, close their own findings, or approve.
- A successful production author run publishes an immutable Artifact Version only after the Host validates its output contract and exact Evidence references. Review runs bind that exact Artifact Version and payload digest together with the reviewer Profile Version, Capability, scope, criteria, inputs, result, and individually attributable findings.
- Critical and Major findings block integration. Critical findings are never waivable; a Major finding may receive an exact attributable Engineer exception only when the approved Work Plan binds `Engineer Exception Allowed` (currently limited to document-control and cost-estimation review), while other reviewed work binds `Remediation Required`. Minor findings remain disclosed. Reviewers report findings and remediation verification, while Host policy owns canonical finding dispositions so a reviewer cannot directly close its own finding.
- Remediation is new author work: it creates a successor Artifact Version linked to the prior negative Review and is independently reviewed again. Reviewed bytes, findings, Reviews, and dispositions are immutable.
- `Ready for Integration` is an explicit exact-version record created only after output validation, Evidence verification, dependency readiness, required Reviews, finding dispositions, and the Bid Decision, Work Plan, and Production approval gates all pass.
- The Engineer User may revise a Work Plan Proposal within Quantix invariants. Routine scheduling remains with the Tender Office Coordinator, while material membership, profile, permission, budget, responsibility, milestone, or review changes require a versioned Work Plan Amendment and EITL approval.
- Agent Profiles follow `Proposed -> Active -> Suspended -> Retired`, remain scoped to one Tender, and never carry provider-thread memory across Tenders. Only approved Role Archetypes, Capability definitions, playbooks, templates, and registered company knowledge are reusable.

## Evidence

- [Decision ticket](https://github.com/kareem-sf/quantix/issues/9)
- [Independent production review ticket](https://github.com/kareem-sf/quantix/issues/40)
- [Agent Profile runtime boundary](./0004-run-agent-profiles-through-host-controlled-codex-threads.md)
