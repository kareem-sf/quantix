# Keep chats outside the Tender system of record

Quantix stores canonical current domain records and immutable registered versions in a Tender Store under `~/.quantix`, with append-only, tamper-evident Audit Events recording their history. Chats, Codex threads, Agent Runs, sandboxes, and temporary files remain inputs or traces; we chose direct current records with retained revisions over chat-derived state or full event sourcing so the Tender remains simple to query while every material change stays attributable.

## Consequences

- Source Artifact Versions and Artifact Versions are immutable, hashed, and selected explicitly; no review, decision, approval, or package may target an ambiguous latest file.
- Schema-valid agent output enters as Proposed rather than verified truth. Material content must be Verified or represented by an EITL-approved Assumption before it can support an approval.
- Working Artifacts have no canonical standing until their Tender Task output contract validates and Quantix registers an Artifact Version with complete provenance.
- Reviews, Review Findings, Tender Decisions, and Approval Records bind to exact revisions and versions. Their history is preserved instead of edited in place.
- Typed Provenance Links calculate targeted invalidation: changed dependencies mark affected work Stale, reopen affected Tender Tasks, expire affected Reviews, and invalidate affected approvals without discarding unaffected work.
- Successful state changes atomically update current canonical records and append their Audit Events. Denied and failed attempts are audited without changing canonical state.
