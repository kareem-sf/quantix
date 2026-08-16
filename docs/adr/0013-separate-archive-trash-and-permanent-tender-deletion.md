---
status: accepted
---

# Separate Archive, Trash, and Permanent Tender Deletion

Quantix preserves three distinct Tender lifecycle operations: Archive places a safely terminal Tender into reversible read-only use; Delete moves a safely terminal Tender Store into recoverable Tender Trash; Permanent Tender Deletion is a separate irreversible Tendering Engineer decision available only from Tender Trash. Permanent Tender Deletion removes every identifiable Tender-associated copy controlled by Quantix, retains only a minimal Deletion Receipt, and completes local deletion without waiting for provider-side thread cleanup.

This decision revises ADR 0009's whole-Tender retention and purge consequence. Its verified-store, atomic publication, recovery, integrity, audit, and Engineer-in-the-Loop controls continue to apply.

## Consequences

- Archive, Delete, Restore, and Permanent Tender Deletion are unavailable while a Tender has active or protected work. The Host, not the renderer, proves the exact safe terminal boundary.
- Archive does not move or duplicate the Tender Store. It creates an attributable reversible read-only state, removes the Tender from the active list, and permits ordinary Manager, Work, and Files inspection with a clear Archived status and Restore action.
- Delete is recoverable. It reuses the existing verified move into Tender Trash, and restoration remains a separate attributable decision. Tender Trash has no automatic retention deadline or automatic purge.
- Permanent Tender Deletion is available only from Tender Trash after an exact consequence review and explicit Tendering Engineer confirmation. The ordinary Delete action never performs it directly.
- Permanent Tender Deletion removes the Tender Store and every identifiable Tender-associated backup, Portable Tender Archive, Delivery Export, Agent Run Workspace, staging item, quarantine item, and Tender-specific log controlled by Quantix. It does not claim erasure of original source packages, exports copied outside Quantix control, recipient copies, operating-system or third-party backups, or other external copies.
- Provider Threads are externally managed and known only by opaque references. Quantix requests their deletion and discards local Tender content immediately. If deletion cannot be confirmed, the Deletion Receipt records Provider Cleanup Pending and Quantix retries without retaining Tender content or changing the completed local-deletion result.
- A Deletion Receipt retains only the minimum installation-level facts needed to prove the attributable decision, local completion, erased-copy classes, and provider-cleanup status. It contains no confidential Tender content and is not a recoverable tombstone.
- Removing a Tender never deletes an application-wide Provider Credential. Disconnecting or revoking a Provider Connection is a separate Application Settings operation.
- The minimalist workspace exposes Archive and Delete through the selected Tender's existing menu and exposes Archived Tenders, Tender Trash, restoration, Permanent Tender Deletion, and Deletion Receipts through one `Archived & Trash` sidebar destination. No fourth Tender tab or duplicate retention workflow is introduced.

## Evidence

- [Manager-led workspace specification](../product/agentic-tender-workspace.md)
- [Local Host and Tender Store decision](./0009-run-one-local-host-over-self-contained-tender-stores.md)
