# Quantix Agentic Tender Intake Reliability — Design

Date: 2026-08-23
Status: Proposed for written-spec review

## Goal

Make Tender intake self-correcting and auditable. Codex extracts and repairs Tender
data; Quantix supplies exact evidence, enforces deterministic domain invariants, and
publishes only a validated result. A malformed model result, oversized request, or
provider rate limit must never become a generic `INVALIDCOMMAND`, an immediate retry
loop, or a false completed event.

## Observed failure

The Juhayna extraction runs reached the provider successfully and returned canonical
JSON accepted by the static JSON Schema. Quantix then rejected the candidates in the
domain provenance validator, erased the exact issue and candidate, recorded the
provider boundary as completed too early, and reduced the Manager failure to
`INVALIDCOMMAND`. Each extraction request carried 256 evidence inputs and consumed
about 112,000 input tokens. Later manual retries were admitted immediately and were
rate limited.

No Tender Records, extraction batches, or Manager outcomes were published by these
runs. The exact rejected historical field is unrecoverable because the rejected
candidate and validation path were discarded.

## Reference principles

The design adopts four patterns from the reviewed Hermes Agent and Agent Zero
repositories without copying their Python runtime or JSON persistence:

- normalize provider behavior and error categories at one boundary;
- parse and validate a complete response before mutating canonical state;
- make retries bounded, typed, and independently auditable;
- verify postconditions and commit state atomically before declaring success.

OpenAI Structured Outputs supports strings, enums, objects, arrays, and nested
`anyOf`, while conditional `if`/`then`/`else` schemas are unsupported. Therefore the
provider contract uses tagged variants and task-scoped string enums rather than
conditional schemas.

## Chosen architecture

### 1. Provider proposal contract

The provider-facing DTO is separate from the canonical Tender Record DTO.

- Every evidence item in one extraction task receives a deterministic short handle:
  `e0001`, `e0002`, and so on, in stable evidence order.
- The provider data view contains each handle beside its exact immutable evidence.
- The task output schema defines evidence citations as a string enum containing only
  that task's handles. Codex cannot express a foreign artifact, version, or ordinal.
- Field provenance is a tagged `anyOf` variant. An evidence basis can contain only
  evidence handles; an assumption or Tender Query can contain only its allowed
  reference and explanation; Engineer Entry and Calculation Run variants can contain
  only their permitted authority handle.
- The host resolves handles to canonical `TenderEvidenceReference` and
  `TenderRecordAuthorityReference` values before domain validation and publication.
- The old provider contract that asks the model to reproduce raw
  `{artifact_id, version, ordinal}` triples is removed.

`serde` remains the parsing boundary, `jsonschema` remains the wire-schema validator,
and existing `garde` validation is reused for simple DTO constraints. Cross-field
Tender rules stay in a small domain-specific validator because they depend on exact
task evidence, record kind, deadline parsing, and stored authorities.

### 2. Structured validation and rejected output

Domain validation returns deterministic issues instead of bare `InvalidCommand`:

```text
OutputValidationIssue {
  code: String,
  path: String,
  message: String
}
```

Codes are stable machine identifiers; paths are JSON Pointers into the provider
proposal; messages contain no source text or secrets. The rejected canonical provider
proposal, its SHA-256, and the issue list are stored in an immutable
`agent_run_rejected_outputs` row in the same transaction that marks the Agent Run
`output_invalid`. This data is Tender Internal, never written to ordinary diagnostic
logs, and is available for inspection and repair.

The final domain validator remains mandatory even when the strict provider schema
accepts the proposal. It protects rules that cannot be encoded in the provider's
supported JSON Schema subset.

### 3. One bounded Codex repair

A semantically invalid extraction triggers exactly one automatic repair Agent Run:

1. Attempt 0 finishes transport and fails deterministic Tender validation.
2. Quantix atomically persists its failed state, rejected proposal, issue list, usage,
   terminal event, and audit lineage.
3. Quantix creates a new Agent Run with `retry_of_run_id = attempt_0.run_id`, the same
   immutable evidence and authority snapshot, and a `repair-feedback-v1` data view
   containing the rejected proposal plus structured issues.
4. Codex reviews the evidence and feedback and returns a complete replacement
   proposal; it does not patch canonical Tender data directly.
5. Quantix resolves handles, validates again, and either publishes atomically or ends
   the repair run as `output_invalid`.

There is never a third semantic attempt. A database uniqueness invariant allows only
one direct repair for the failed run. Transport, authentication, interruption,
indeterminate outcomes, and rate limits do not consume the semantic repair allowance.
Each attempt owns its own provider turn reference, events, usage, and terminal state.

### 4. Atomic success and truthful events

Provider transport completion, candidate validation, and canonical publication are
distinct boundaries:

- `provider_transport_completed`: the response stream finished;
- `candidate_validated` or `candidate_rejected`: deterministic validation result;
- `result_committed`: Tender Records and batch receipt committed;
- Agent Run terminal state: emitted only after the transaction outcome is known.

Record publication, Agent Run completion, proposed result, Manager extraction-batch
receipt, and audit event stay in one `TransactionBehavior::Immediate` transaction.
The Manager links to the blocking or repairing Agent Run and never translates a typed
failure into `INVALIDCOMMAND`.

### 5. Request-size budgeting

Evidence batches are planned by deterministic serialized request bytes, not a count of
256 items.

- The first extraction pass creates an immutable batch plan in SQLite using stable
  evidence order and a named estimator version.
- The estimator includes canonical evidence payload, task inputs, permission/data-view
  manifests, schema overhead, expected duplication, and output headroom.
- A final guard serializes the actual provider request and rejects it locally if it
  exceeds the hard request budget.
- An individually oversized evidence item produces a precise intake blocker.
- Completed batch receipts remain atomic with Record publication; restart resumes the
  same persisted plan without repartitioning or reprocessing evidence.

The limit is a byte-safety budget, not a claim of exact token counting. No tokenizer or
new dependency is added unless measured acceptance data proves the conservative byte
budget insufficient.

### 6. Rate-limit recovery

`ProviderFailure` carries structured `retry_after_milliseconds`. Manager intake stores
the blocking run, retry deadline, and retry-attempt count transactionally.

- Prefer the provider's future rate-limit reset; otherwise use `Retry-After`; otherwise
  use bounded exponential delays of 60, 120, and 240 seconds.
- At most three automatic rate-limit retries are scheduled for one intake operation.
- A persisted deadline is enforced in both background admission and direct rebind APIs,
  so UI clicks cannot bypass it.
- Restart reconstructs the scheduled wait from SQLite.
- During cooldown the workspace says only that Quantix is waiting for AI capacity; it
  exposes no active retry button. After three exhausted attempts it pauses with one
  clear Engineer retry action instead of looping.

The scheduler uses existing Tokio timers and SQLite time. No generic retry framework is
added because the durable Manager workflow, not an in-memory HTTP call, owns recovery.

### 7. Engineer-facing workspace

The workspace presents Tender work, not provider mechanics:

- extraction appears as chronological Manager activity;
- automatic repair appears as “Reviewing extracted Tender data”;
- successful repair produces the normal Tender result and next-step suggestions;
- capacity cooldown appears as “Waiting for AI capacity” with the retry time;
- persistent failure offers the relevant Agent Run internally but shows a plain Tender
  data explanation to the Engineer.

The prompt stays available at the bottom. Provider, model, schema, tools, validation
codes, and raw event components remain out of the normal workspace surface.

## Alternatives rejected

### Prompt-only self-review

Adding “check your answer” to the prompt is cheap but does not prevent foreign evidence
references, preserve failure evidence, or bound retry behavior. It remains guidance,
not a contract.

### Keep raw evidence triples and enumerate them

This avoids a provider DTO but repeats long identifiers, keeps the model coupled to
internal storage identity, and makes task schemas and outputs materially larger. Short
task-scoped handles provide a smaller and safer boundary.

### Let Codex publish directly

Allowing the same model to create and approve canonical Tender data removes the
deterministic trust boundary. Codex should diagnose and repair; Quantix must remain the
authority that validates and commits.

### Unbounded agent loop

Repeatedly asking until an answer passes can hide defects, multiply cost, and create a
non-terminating workflow. One semantic repair and three rate-limit recoveries are
explicit product limits.

## Persistence changes

The next schema version replaces the old shape directly; no compatibility or migration
path is added.

- Add immutable `agent_run_rejected_outputs` for candidate, hash, and validation issues.
- Add immutable `manager_intake_extraction_plan_batches` for byte-budgeted plans.
- Add `blocking_agent_run_id`, `retry_not_before_epoch_seconds`, and
  `provider_retry_attempt_count` to `manager_intake_runs`.
- Add durable repair feedback to `tender_tasks` so a repair run can resume after a
  process restart.
- Enforce one direct linked repair per failed Agent Run with a unique database index.

Generated TypeScript bindings are regenerated by the existing test command and are
never edited manually.

## Verification requirements

Implementation is test-first. The minimum deterministic suite proves:

1. task handles round-trip to the exact canonical evidence and foreign handles cannot
   be represented by the strict schema;
2. every known schema/domain mismatch either becomes structurally impossible or returns
   a stable issue code and path;
3. one invalid result followed by a valid repair produces exactly two linked Agent Runs
   and publishes once;
4. two invalid results stop after the repair with zero publication and no third call;
5. rejected output and issues survive host restart and can prepare the repair run;
6. transport-completed, candidate-rejected, repair, commit, and terminal events appear
   in truthful order;
7. byte-budget plans are stable across restart and never omit or repeat evidence;
8. a failed publication never marks a batch complete;
9. Retry-After and fallback cooldowns survive restart and block premature calls;
10. the workspace hides retry during cooldown and shows chronological repair activity;
11. the direct ChatGPT request contains the strict task-specific schema;
12. an opted-in live Juhayna acceptance run reaches published Tender Records without
    exposing provider internals in the Engineer workspace.

Repository verification uses `npm test`, `npm run check`, `npm run format:check`, and
`npm run verify`. Production builds remain out of normal development. Live-provider
acceptance is explicit and runs only after deterministic verification is green.

## Delivery decomposition

The implementation will be planned and reviewed in three independently green stages:

1. **Agentic output boundary:** task-scoped handles, tagged provider DTO, structured
   validation, rejected-output persistence, and exactly one linked repair.
2. **Durable workload control:** byte-budgeted batch plans, request guard, rate-limit
   cooldown, bounded automatic resumption, and truthful run diagnostics.
3. **Product acceptance:** Manager workspace projection, deterministic end-to-end
   regression coverage, and the opted-in Juhayna live acceptance run.

Each stage must leave the existing deterministic verification gate green before the
next stage begins.
