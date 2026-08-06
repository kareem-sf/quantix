# Control the Tender lifecycle with EITL Approval Gates

Quantix v0 controls each Tender through `Intake -> Fingerprinting -> Initial Compliance -> Bid Decision -> Tender Planning -> Active Production -> Integrated Review -> Package Production -> Final Review -> Ready for Submission`. Active Production runs project-specific specialist Workstreams in parallel, including mandatory Cost Estimating and cross-lifecycle Query and RFI Control; deterministic workflow rules coordinate their outputs, while the authenticated Engineer User acts as Tendering Manager and is the sole authority for every formal approval.

## Consequences

- The full Tender Office cannot begin production until the Engineer User chooses Proceed and approves the Work Plan.
- AI agents may analyze, draft, calculate, coordinate, review, and recommend, but cannot approve, infer approval, close their own findings, or communicate externally on their own.
- The Cost Estimating Workstream must produce an independently reviewed Priced Cost Baseline and an Approved Tender Price through separate EITL gates before Integrated Review can complete.
- The Query Register opens during Intake. Every External RFI requires EITL approval before issue, and every material response interpretation or unanswered-query treatment requires EITL approval before affected work can close.
- Integrated Review produces a versioned Coordinated Bid Baseline. Package Production may transform its presentation but cannot change its meaning without invalidating affected approvals and repeating review.
- Final Approval freezes the exact Submission Package version and its manifest. Any later content change revokes that approval and returns the package to Final Review; external submission remains outside v0.
- Addenda and other material changes trigger targeted Change Assessment and invalidate only affected tasks, artifacts, estimates, prices, decisions, and approvals. Local failures apply a Blocked status rather than moving the whole Tender to a generic Failed state.
