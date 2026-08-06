# Release only manifest-bound Submission Package versions

Quantix v0 treats a Submission Package as a stable logical object and releases only an exact immutable Submission Package Version whose canonical manifest binds the approved Coordinated Bid Baseline, Calculation Manifest, Tender-shaped sections, every included file, coverage, validation, review, and approval record. We chose this over mutable output folders, checklist-only sign-off, and agent readiness scores so the Tendering Manager approves a reproducible package and any changed byte or commitment invalidates the appropriate controls.

## Consequences

- Package Production may apply only controlled presentation transformations. A changed technical, commercial, contractual, programme, qualification, assumption, exclusion, or numeric commitment invalidates affected baseline approval.
- Every included file is an exact registered version. A versioned Submission Coverage Matrix accounts for every mandatory requirement, deliverable, addendum instruction, execution requirement, and required file; every material claim remains traceable to Verified Evidence, an approved Assumption, an approved Calculation Run, or an attributable Engineer User entry.
- A versioned Package Validation Policy owns release conditions. Deterministic validators decide supported checks, exact-hash Manual Verification covers permitted unsupported properties, and AI agents may raise risks but cannot pass checks.
- File, structure, rendering, cross-artifact, calculation, hidden-content, and information-boundary checks target one exact package. Package-wide checks rerun for every version; item results may be reused only when their file hash, check version, policy, and context are identical.
- An independent Final Review Plan controls qualified review and finding closure. Critical findings are never waivable, permitted Major findings require exact EITL Exception Approval, and disclosed Minor findings may remain only through Final Approval.
- Final Approval atomically rechecks identity, deadline, addenda, staleness, hashes, validation, independent review, and exceptions before freezing the exact package and manifest as Ready for Submission. Later content changes revoke approval and require a new validation run and Final Review.
- v0 hashes raw file bytes with SHA-256. The Submission Manifest is canonical UTF-8 JSON hashed with SHA-256 while omitting its own digest field; that digest is the package root hash, and a single-file Release Copy receives a separate SHA-256 digest.
- Release Copies are Tender-shaped exports verified against the frozen manifest. Export, retry, or later external modification never changes the canonical package or represents external submission.

## Evidence

- [Decision ticket](https://github.com/kareem-sf/quantix/issues/11)
- [Tender lifecycle boundary](./0001-control-the-tender-lifecycle-with-eitl-gates.md)
- [Tender system of record](./0002-keep-chats-outside-the-tender-system-of-record.md)
- [Canonical calculation boundary](./0003-make-deterministic-calculations-canonical.md)
- [Agent access boundary](./0006-enforce-agent-access-through-host-owned-run-grants.md)
