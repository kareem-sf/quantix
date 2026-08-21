# Adaptive Quantix and Doctor — Design

Date: 2026-08-21
Status: Approved (user-approved in session)

## Goal

Quantix remains usable when application contracts, Construction Project metadata, local
runtime state, provider state, or renderer surfaces change or fail. It prevents invalid
contract changes before release, isolates runtime failures to the affected Tender or
capability, automatically repairs only reversible non-canonical state, and gives the
Tendering Engineer one truthful action for every canonical recovery decision.

The design extends the existing Quantix Doctor, diagnostics, Tender recovery, and
startup reconciliation boundaries. It does not introduce a parallel health framework.

## Evidence from the latest production-shaped session

The Juhayna first-Tender run established the concrete failure pattern this design must
remove:

- The Juhayna Tender metadata remained intact at revision 1 in both the Tender Store and
  installation catalogue.
- The Tender Analyst output contract changed while its immutable system profile still
  declared version 3. An existing Tender Store contained the prior version-3 profile,
  so Quantix correctly rejected the silent mutation as `IntegrityFailed`.
- Advancing the profile to version 4 allowed a new immutable contract identity and
  unblocked later runs without rewriting version 3.
- Later Agent Runs failed independently through provider protocol, provenance
  validation, indeterminate cleanup, and Windows workspace-handle conditions. Several
  distinct causes were projected through generic failure summaries.
- The current uncommitted Doctor covers setup, document tools, AI readiness, signed
  updates, diagnostics health, and Tender integrity. It does not yet cover contract
  drift, Agent Run terminal facts, or renderer crashes.

This proves that the principal metadata incident was not arbitrary data corruption. It
was an undeclared immutable-contract change that reached an existing Tender Store.

## Product principles

1. **Strict boundaries, dynamic domain content.** IPC, security, authority, and
   persisted envelope contracts remain strict. Construction Project metadata is carried
   inside stable record-and-field collections so new field keys do not change those
   envelopes.
2. **Prevent before repairing.** Verification rejects undeclared contract changes.
   Doctor is the production safety net, not a substitute for release correctness.
3. **Capability isolation.** A failure blocks only the exact capability and scope whose
   guarantees cannot be established.
4. **No silent canonical mutation.** Automatic repair is limited to reversible,
   idempotent, non-canonical operational state.
5. **Engineer-in-the-Loop recovery.** Any action that changes a Tender Store, immutable
   history, provenance, approval, provider selection, or recovery disposition requires
   an attributable Tendering Engineer command.
6. **No silent replay or fallback.** Quantix never repeats a possibly accepted provider
   Turn, substitutes a provider/model, or infers missing metadata.
7. **Truth over false health.** External outages and irreparable corruption report that
   no local repair is available.

## Scope

This design covers:

- system profile and output-contract evolution;
- dynamic Construction Project metadata rendering;
- application, Tender, and capability preflight;
- typed Doctor incidents and repairs;
- automatic reconciliation of safe operational state;
- renderer surface recovery;
- production failure isolation and restart behavior;
- deterministic and release-stage verification.

It does not add:

- a general database migration framework;
- compatibility DTOs, fallback parsers, or dual storage paths;
- autonomous Tender Store repair;
- audit-chain rewriting;
- automatic provider/model changes;
- automatic update installation or rollback;
- diagnostic uploads or automatic issue creation;
- an unrestricted repair shell or plugin system.

## Architecture

### 1. Contract Registry

One Rust-owned registry describes every application-owned contract whose identity must
remain immutable across existing Tenders. Initial registry families are:

- system Agent Profile definitions;
- system Agent Profile output schemas;
- provider wire-contract request and response schemas;
- persisted application and Tender manifest schemas;
- renderer-to-Host command and response schema sets.

Each entry has the following logical shape:

```text
ContractDefinition
  key: stable namespaced identifier
  family: profile | output_schema | provider_wire | manifest | ipc
  revision: positive integer
  canonical_payload: exact canonical bytes
  sha256: digest of canonical_payload
  affected_capabilities: non-empty set
  activation: build_only | new_tender | existing_tender_review
```

The registry computes canonical payloads from the same definitions used at runtime. A
committed generated contract manifest records `(key, family, revision, sha256,
affected_capabilities, activation)` and contains no Tender content.

Verification enforces:

- a `(key, revision)` may have exactly one digest;
- changing a digest without advancing the revision fails;
- duplicate keys or revisions fail;
- removed current contracts fail unless the feature and obsolete path are removed
  together;
- a new revision must name its activation policy and affected capabilities;
- generated TypeScript declarations and the IPC contract-set digest remain aligned with
  the Rust source.

Revisions remain explicit, human-reviewable release facts. The digest makes forgetting
to advance a revision impossible to ship. Quantix does not derive a numeric revision
from a truncated hash.

### 2. System profile activation

The current `agent_profile_versions` and profile-head model remains the immutable Tender
record. No schema migration is introduced.

For a new Tender, the Engineer's existing Create Tender command authorizes seeding the
current system profiles as part of that canonical creation transaction.

For an existing Tender, preflight compares each required registry definition with its
persisted profile version:

- absent new revision plus matching prior history: `review_required`;
- present revision plus matching canonical definition: `ready`;
- present revision plus different definition: `contract_drift`;
- malformed persisted definition: Tender integrity handling, not contract activation.

`review_required` offers one exact Engineer action to activate the declared revision for
future work. Activation appends the new profile version, advances only the relevant
profile head, and records an Audit Event. Existing tasks, runs, provider threads, and
exposure sets remain bound to their original version.

`contract_drift` is an installation/release defect. Doctor must not overwrite either
definition. It blocks only dependent future commands, preserves the Tender, and offers
diagnostic export plus installation update/replacement guidance.

### 3. Dynamic Construction Project metadata

`TenderRecordKind::ProjectCharacteristic` and the existing stable record/field
collections are the extensibility boundary. A Project Characteristic keeps a typed
outer identity and an ordered list of fields:

```text
Project Characteristic
  stable_key
  title
  record revision and verification status
  ordered fields
    name
    value and normalized value
    source expression, timezone, and uncertainty where present
    basis and exact Evidence
  contradictions
```

New `stable_key` and field `name` values are domain data. They do not require a Rust DTO,
TypeScript binding, database column, or React component change. The renderer iterates
records and fields and uses a small presentation registry for supported display kinds.
An unrecognized field key renders safely as text with its provenance. A new structural
value kind changes the output contract and therefore requires a declared contract
revision.

The renderer must not branch on keys such as `project_delivery_context`. Ordering,
addition, and removal of metadata keys cannot blank the surface or discard neighboring
fields. Invalid content localizes an error to the affected record/field and leaves the
remaining Project Fingerprint readable.

Strict Serde `deny_unknown_fields` remains in force for commands and fixed envelopes.
Dynamic metadata does not mean accepting unknown authority or transport fields.

### 4. Capability Preflight

The Rust Host owns a preflight service that combines authoritative snapshots without
mutating them. It reports readiness independently for:

- application storage and setup;
- Tender Store integrity;
- Tender metadata and Project Fingerprint;
- registered files and document tools;
- AI execution and provider contract;
- system profile/output contracts;
- workflow/production commands;
- diagnostics;
- recovery.

Each result has one of these states:

```text
ready
healing
review_required
blocked
unavailable_external
```

Preflight runs:

- during verification for build-owned contracts;
- at application startup for application capabilities;
- when a Tender opens for Tender capabilities;
- immediately before each side-effecting command for its required capabilities;
- after a relevant diagnostic or repair completes.

Startup and Tender-open preflight are read-only. Command-time preflight prevents stale
health from authorizing work. A blocked capability never makes an unrelated capability
look unhealthy.

### 5. Doctor incident model

The existing Doctor report remains the public health aggregate. Findings gain enough
typed facts to support production recovery without exposing raw content:

```text
DoctorFinding
  code
  correlation_id
  scope: application | tender
  tender_id when scope is tender
  capability
  severity
  readiness_state
  title
  proven_cause or bounded probable_cause
  impact
  data_changed: boolean
  automatic_actions_taken
  safe_remediation
  repair_action when available
  retry_safety
  occurred_at and last_observed_at
```

Finding codes are stable and specific. Provider protocol rejection, candidate provenance
failure, uncertain provider acceptance, contract drift, renderer surface failure, and
workspace cleanup failure are not collapsed into one `process_failed` presentation.

Doctor report revisions continue to bind every repair command to the exact currently
observed report. A stale report cannot authorize a repair.

Operational incidents remain redacted, bounded diagnostics rather than a second Tender
Audit Event stream. Canonical repair decisions continue to use the owning Tender domain
command and Audit Event.

### 6. Automatic Reconciler

The existing startup reconciliation and Doctor safe-repair behavior become one explicit
allowlist. An automatic action is eligible only when all of these are proven:

- the target is non-canonical;
- the action is idempotent;
- no provider Turn or external side effect may have been accepted;
- no data-bearing or uncertain entry is deleted;
- the action cannot change approval, provenance, authority, selection, or Tender facts;
- success and failure can be re-inspected deterministically;
- the action has a bounded deadline and resource limit.

Initial automatic actions are:

- replace a stale provider actor before a Turn is accepted;
- remove a verified directory-only empty residual workspace;
- reconcile unreferenced staging and interrupted non-canonical operations;
- restart the diagnostics writer and reapply its retention policy;
- refresh provider capability catalogues without changing selection;
- rebuild the derived installation Tender catalogue from authoritative Tender Stores;
- retry a renderer surface after its local state is reset.

Automatic repair never changes Tender Audit Event count. Running the same action again
after success must produce no additional mutation.

### 7. Engineer Repair Coordinator

Doctor does not duplicate domain commands. It assembles an impact preview and deep-links
or dispatches the exact typed command owned by the relevant module.

Engineer review is mandatory for:

- activation of a new system profile/output-contract revision in an existing Tender;
- retry or closure of an indeterminate Agent Run;
- AI execution rebinding or selection changes;
- replacement from a verified Tender Backup;
- quarantine or disposal of data-bearing residual work;
- any operation that appends or changes canonical Tender history.

The impact preview states the exact scope, changed records, preserved history, backup or
quarantine behavior, provider retry safety, and whether the action is reversible.

Doctor never automates:

- Audit Event, approval, provenance, or existing profile-version rewrites;
- missing Project Fingerprint inference;
- replay after possible provider acceptance;
- unsupported/corrupted database schema repair;
- update installation, restart, or rollback;
- credential changes;
- Trash purge or Permanent Tender Deletion;
- diagnostics upload.

### 8. Renderer resilience

Each major renderer surface is wrapped by a surface-level error boundary. A component
failure replaces only that surface with a recovery panel. The panel offers:

- retry this surface;
- return to the Tender overview;
- open the correlated Doctor finding;
- export redacted diagnostics.

The boundary records a rate-limited renderer diagnostic through the existing Host
command. It does not persist Tender content, component props, draft text, raw exception
paths, or stack frames containing user data.

Navigation and drafts live above recoverable surface boundaries so a local render failure
does not erase them. A full application recovery state is reserved for loss of the native
Host connection.

## Runtime flows

### Startup

1. Start the Host and diagnostics store.
2. Inspect setup and application capabilities.
3. Run the automatic allowlist against proven non-canonical leftovers.
4. Reinspect affected capabilities.
5. Publish `healthy`, `healing`, `needs review`, or scoped `blocked` state.
6. Open the last safe navigation context without replaying work.

### Tender open

1. Inspect the Tender Store and derived catalogue entry.
2. Compute capability readiness and contract comparisons read-only.
3. Open every healthy surface.
4. Render affected capability blockers inline.
5. Offer exact Engineer actions for `review_required`; do not mutate on open.

### Side-effecting command

1. Resolve the command's required capabilities.
2. Re-run their preflight against current state.
3. Reject with a typed finding when any requirement is not ready.
4. Execute the existing domain command when ready.
5. Record redacted operational facts and canonical Audit Events in their existing owners.
6. Recompute only affected health facts.

### Failure and recovery

1. Preserve the current Tender, navigation, form values, attachments, and draft input.
2. Record a redacted incident and correlation ID.
3. Isolate the affected capability.
4. Attempt at most one bounded automatic action when retry safety is proven.
5. Continue the original operation only when it was not externally accepted and remains
   idempotent.
6. Otherwise present one exact Engineer action or state that no local repair exists.

### Restart after uncertain work

1. Reconcile non-canonical staging.
2. Never replay commands or provider Turns.
3. Preserve accepted-or-unknown work as indeterminate.
4. Restore the last safe navigation context.
5. Present the exact run recovery workflow before new dependent work.

## Production UX

Quantix uses the existing Application Settings Doctor, Tender workspace blockers, and
recovery surfaces. It does not add another dashboard.

A persistent Doctor indicator uses four user-facing states:

- **Healthy** — inspected capabilities are ready;
- **Healing** — bounded automatic reconciliation is running;
- **Needs review** — usable state remains, but an Engineer decision is required;
- **Blocked** — the named capability or Tender cannot safely operate.

Application findings stay in Application Settings. Tender findings appear in the owning
Tender and link to the correct focused workflow.

Every finding answers:

- what happened;
- whether the cause is proven or probable;
- what capability and Tender are affected;
- whether Doctor changed any data;
- what Doctor already attempted;
- what remains usable;
- the exact next action;
- the correlation ID used by the diagnostic timeline and support bundle.

`Repair all safe issues` executes only the automatic allowlist. It displays before/after
health and every action taken. A second run after success reports that no action was
needed.

Generic terminal text is replaced by specific, stable findings. For example:

> Tender Analyst contract revision changed. Existing runs remain unchanged. Review
> version 5 before using it for future runs.

## Security and privacy

- Doctor and diagnostics remain local, redacted, bounded, and non-authoritative.
- Findings never include Tender content, prompts, responses, credentials, tokens, hidden
  reasoning, arbitrary filesystem paths, or raw provider traffic.
- Support bundles remain local files and are never uploaded automatically.
- Automatic actions operate only through named Host functions; there is no shell,
  filesystem browser, SQL console, or generic command runner.
- Renderer recovery has no direct storage, provider, or updater authority.
- Capability failure cannot weaken Permission Grants, Safety Limits, EITL approval, or
  provider-selection requirements.

## Delivery slices

Each slice must work end to end before the next begins:

1. **Contract evolution gate** — registry, manifest, fingerprint validation, same-revision
   failure, runtime contract-drift finding, and reviewed activation for existing Tenders.
2. **Dynamic metadata boundary** — generic Project Characteristic rendering and localized
   malformed-field handling without hard-coded field keys.
3. **Health and repair coordination** — capability preflight, incident facts, automatic
   allowlist, and typed Agent Run/provider findings.
4. **Resilient production UX** — surface error boundaries, draft/navigation preservation,
   Doctor indicator, and repair summaries.
5. **Acceptance hardening** — fault injection, restart, Windows locked-directory,
   diagnostics-redaction, and end-to-end acceptance evidence.

## Verification

### Contract evolution

- The same revision with a different fingerprint fails before release.
- A declared new revision coexists with the prior immutable version.
- Existing and active runs remain bound to their original contract.
- Existing Tender work cannot use a changed contract until Engineer activation.
- New Tender creation seeds the current definitions under the existing explicit Create
  Tender command.

### Dynamic metadata

- Arbitrary valid record and field keys render without source changes.
- Field addition, removal, and reordering preserve neighboring Project Fingerprint data.
- Malformed values affect only their record/field.
- Provenance, verification, and revision remain visible.
- Unknown fixed-envelope fields remain rejected.

### Repair safety

- Every automatic action is idempotent.
- Automatic repair leaves Tender Audit Event count, approvals, records, and immutable
  profiles unchanged.
- Data-bearing or uncertain state is quarantined rather than deleted.
- A possibly accepted external Turn is never replayed.
- A second `Repair all safe issues` run performs no additional mutation.

### Failure isolation

- Provider failure leaves local Tender work usable.
- Parsing failure leaves registered Files accessible.
- One damaged Tender does not block other Tenders.
- A renderer exception affects only its surface.
- Host disconnection produces a truthful application recovery state.

### Diagnostics

- Every reported failure has a stable code and correlation ID.
- Support bundles contain no Tender content, paths, credentials, prompts, responses, or
  hidden reasoning.
- Diagnostics failure never blocks ordinary Tender work.

### Repository and release gates

Normal development must pass formatting, TypeScript/Rust checks, deterministic tests,
focused Doctor/contract/metadata/recovery/renderer tests, and `npm run verify`.

Release qualification additionally uses the repository's deterministic Product
Acceptance Run, private Windows, native-package, and public release-acceptance gates.
Production builds remain explicit release-stage actions.

No slice is complete merely because Doctor displays a finding. The affected capability
must recover safely, present an exact Engineer action, or truthfully state that no local
repair exists.

## ADR alignment

- ADR 0009 remains authoritative: the Rust Host owns persistence, recovery, process
  supervision, and exact schema validation; no general database migration framework is
  added.
- ADR 0014 remains authoritative: diagnosis is automatic, while canonical repair uses
  closed typed Host actions after Engineer review.
- ADR 0015 remains authoritative: operational diagnostics stay local, redacted,
  non-authoritative, bounded, and physically separated by application/Tender scope.
- The existing no-silent-fallback and EITL decisions remain unchanged.

The design changes failure presentation and contract governance, not those authority
boundaries.
