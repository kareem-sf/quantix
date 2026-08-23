# Agentic Tender Output Boundary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Tender Record extraction use task-scoped evidence handles, preserve precise rejected output, and allow Codex exactly one auditable repair Agent Run.

**Architecture:** A provider-only proposal DTO and task-specific Structured Outputs schema sit in front of the canonical Tender Record DTO. Quantix resolves opaque handles, returns structured semantic issues, persists rejected output atomically, and creates at most one linked repair run containing the rejected proposal and issues.

**Tech Stack:** Rust 2021, serde, serde_json, serde_json_canonicalizer, jsonschema, garde, rusqlite, sha2, existing direct ChatGPT backend, runtime fixtures.

**Spec:** docs/superpowers/specs/2026-08-23-agentic-intake-reliability-design.md

## Global Constraints

- Remove the raw provider citation path; do not add compatibility or fallback parsing.
- Use existing dependencies only.
- Permit exactly one semantic repair Agent Run.
- Never publish before handle resolution and deterministic Tender validation succeed.
- Never write rejected Tender content to ordinary diagnostic logs.
- Regenerate src/bindings with npm test; never edit generated declarations manually.
- Do not run a production build.

## File Structure

- Create src-tauri/src/tender_store/tender_record_proposals.rs for provider DTOs, schemas, handles, resolution, and validation reports.
- Modify src-tauri/src/tender_store.rs for module registration and persistence schema.
- Modify src-tauri/src/tender_store/tender_records.rs for task/data-view construction and repair preparation.
- Modify src-tauri/src/tender_store/agent_records.rs for rejected-output persistence and normalized publication.
- Modify src-tauri/src/agent_runtime.rs for public issue/repair DTOs and bounded orchestration.
- Modify src-tauri/src/agent_runtime/codex_protocol.rs for repair feedback and truthful transport diagnostics.
- Modify src-tauri/src/agent_backend/fixture_client.rs for deterministic repair scenarios.
- Test in src-tauri/tests/tender_records.rs and src-tauri/tests/agent_runtime.rs.

---

### Task 1: Task-scoped provider proposal contract

**Files:**
- Create: src-tauri/src/tender_store/tender_record_proposals.rs
- Modify: src-tauri/src/tender_store.rs
- Modify: src-tauri/src/tender_store/tender_records.rs
- Test: src-tauri/src/tender_store/tender_record_proposals.rs

**Interfaces:**
- Consumes: TenderEvidenceReference, TenderRecordAuthority, TenderRecordCandidateBatch, canonical JSON helpers.
- Produces: TenderRecordProposalContext::new, output_contract_json, provider_evidence, evidence_reference, authority_reference, resolve, and record_extraction_profile_contract.

- [ ] **Step 1: Write failing handle/schema tests**

Add module tests that construct two evidence references and assert stable handles, a strict enum, and rejection of a foreign handle:

~~~rust
#[test]
fn proposal_context_uses_only_task_scoped_evidence_handles() {
    let evidence = vec![
        evidence("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", 1, 4),
        evidence("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", 2, 7),
    ];
    let context = TenderRecordProposalContext::new(&evidence, &[]).unwrap();

    assert_eq!(context.evidence_reference("e0001"), Some(&evidence[0]));
    assert_eq!(context.evidence_reference("e0002"), Some(&evidence[1]));
    assert_eq!(context.evidence_reference("e0003"), None);

    let schema: Value =
        serde_json::from_str(&context.output_contract_json().unwrap()).unwrap();
    assert_eq!(
        schema.pointer("/$defs/evidence_handle/enum").unwrap(),
        &json!(["e0001", "e0002"]),
    );
}
~~~

Add one test per tagged basis variant. Assert an evidence basis has no basis_reference or basis_description property.

- [ ] **Step 2: Run the proposal test and verify RED**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib proposal_context_uses_only_task_scoped_evidence_handles --features runtime-fixture
~~~

Expected: FAIL because TenderRecordProposalContext does not exist.

- [ ] **Step 3: Implement deterministic handles and provider DTOs**

Create provider-only DTOs with deny_unknown_fields. Use a tagged basis enum so illegal field combinations cannot be represented:

~~~rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TenderRecordProposalBatch {
    pub records: Vec<TenderRecordProposal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum TenderRecordFieldBasisProposal {
    Evidence { evidence: Vec<String> },
    Assumption { stable_key: String, description: String },
    TenderQuery { stable_key: String, description: String },
    CalculationRun { authority: String },
    EngineerEntry { authority: String },
}

pub(crate) struct TenderRecordProposalContext {
    evidence_by_handle: BTreeMap<String, TenderEvidenceReference>,
    authority_by_handle: BTreeMap<String, TenderRecordAuthority>,
}
~~~

Assign e0001 in evidence input order and a0001 in sorted authority-id order. Reject empty evidence, duplicate canonical references, duplicate authorities, and more than 9999 handles. Build a root object schema with all properties required and additionalProperties false. Use nested anyOf for basis variants and string enums for task handles.

- [ ] **Step 4: Resolve provider handles to canonical candidates**

Implement:

~~~rust
pub(crate) fn resolve(
    &self,
    payload_json: &str,
) -> Result<ResolvedTenderRecordProposal, TenderRecordValidationReport>;

pub(crate) struct ResolvedTenderRecordProposal {
    pub provider_payload_json: String,
    pub canonical_payload_json: String,
    pub candidate: TenderRecordCandidateBatch,
}
~~~

Parse with serde, map every evidence/authority handle, emit unknown_evidence_handle or unknown_authority_handle with a JSON Pointer, canonicalize both payloads, and convert tagged basis variants into the existing canonical flat representation.

- [ ] **Step 5: Replace the static extraction contract and data-view identity**

Change record_extraction_task to call the task context output_contract_json. Change record_extraction_data_view so every evidence item includes its deterministic handle while canonical artifact/version/ordinal remain host metadata. Replace the old shared contract with record_extraction_profile_contract for the persisted profile template and the dynamic context contract for each task; only the task contract is sent to Structured Outputs.

- [ ] **Step 6: Run focused tests and verify GREEN**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib tender_record_proposals --features runtime-fixture
~~~

Expected: all proposal module tests PASS.

- [ ] **Step 7: Commit**

~~~powershell
git add src-tauri/src/tender_store.rs src-tauri/src/tender_store/tender_record_proposals.rs src-tauri/src/tender_store/tender_records.rs
git commit -m "feat: add task scoped tender proposal contract"
~~~

---

### Task 2: Structured semantic validation

**Files:**
- Modify: src-tauri/src/agent_runtime.rs
- Modify: src-tauri/src/tender_store/tender_record_proposals.rs
- Modify: src-tauri/src/tender_store/tender_records.rs
- Test: src-tauri/tests/tender_records.rs

**Interfaces:**
- Consumes: ResolvedTenderRecordProposal from Task 1 and existing Tender domain rules.
- Produces: OutputValidationIssue, TenderRecordValidationReport, and TenderStore::validate_tender_record_proposal.

- [ ] **Step 1: Write the schema/domain parity regression table**

Add schema_domain_parity_reports_stable_paths with cases for duplicate stable keys, whitespace titles, UTF-8 byte limits, duplicate fields/evidence, evidence-basis metadata, authoring format, deadline parsing, contradiction evidence, and foreign authorities. Each case must either fail jsonschema or return an exact code/path:

~~~rust
assert_eq!(
    report.issues[0],
    OutputValidationIssue {
        code: "evidence_basis_metadata_forbidden".into(),
        path: "/records/0/fields/0/basis".into(),
        message: "Evidence-backed fields cannot contain authority metadata.".into(),
    },
);
~~~

- [ ] **Step 2: Run the parity test and verify RED**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --test tender_records schema_domain_parity_reports_stable_paths --features runtime-fixture
~~~

Expected: FAIL because semantic validation returns InvalidCommand.

- [ ] **Step 3: Add typed issue/report types**

Add to agent_runtime.rs:

~~~rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OutputValidationIssue {
    pub code: String,
    pub path: String,
    pub message: String,
}
~~~

Add an internal report sorted by path then code and capped at 64 issues. Refactor candidate defects into issues. Keep database and IO integrity failures as TenderCommandError.

- [ ] **Step 4: Validate through one entry point**

Implement:

~~~rust
pub(crate) fn validate_tender_record_proposal(
    &self,
    task: &TenderTaskView,
    provider_payload_json: &str,
) -> Result<ResolvedTenderRecordProposal, TenderRecordValidationReport>;
~~~

Reconstruct exact handles from task inputs and stored authorities, resolve the provider DTO, then run domain validation. Remove the complete_agent_run path that swallows validate_tender_record_candidate errors.

- [ ] **Step 5: Run the parity test and verify GREEN**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --test tender_records schema_domain_parity_reports_stable_paths --features runtime-fixture
~~~

Expected: PASS.

- [ ] **Step 6: Commit**

~~~powershell
git add src-tauri/src/agent_runtime.rs src-tauri/src/tender_store/tender_record_proposals.rs src-tauri/src/tender_store/tender_records.rs src-tauri/tests/tender_records.rs
git commit -m "feat: report structured tender output issues"
~~~

---

### Task 3: Persist rejected output atomically

**Files:**
- Modify: src-tauri/src/tender_store.rs
- Modify: src-tauri/src/tender_store/agent_records.rs
- Modify: src-tauri/src/agent_runtime.rs
- Test: src-tauri/tests/tender_records.rs

**Interfaces:**
- Consumes: TenderRecordValidationReport and provider payload.
- Produces: RejectedAgentOutput, TenderStore::rejected_agent_output, and OutputInvalid failures containing structured issues.

- [ ] **Step 1: Write the transaction test**

Create a schema-valid/domain-invalid extraction. Assert one failed run, one immutable rejected-output row, zero proposed results, zero Tender Records, equal stored/computed SHA-256, and exact issue code/path.

- [ ] **Step 2: Run it and verify RED**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --test tender_records rejected_output_is_persisted_with_failure_atomically --features runtime-fixture
~~~

Expected: FAIL because rejected output is discarded.

- [ ] **Step 3: Add the immutable table**

Bump TENDER_SCHEMA_VERSION and add:

~~~sql
CREATE TABLE agent_run_rejected_outputs (
  run_id TEXT PRIMARY KEY,
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
  payload_sha256 TEXT NOT NULL CHECK (length(payload_sha256) = 64),
  validation_issues_json TEXT NOT NULL CHECK (json_valid(validation_issues_json)),
  created_at TEXT NOT NULL,
  FOREIGN KEY (run_id) REFERENCES agent_runs(run_id)
);
~~~

Add standard immutable update/delete triggers.

- [ ] **Step 4: Persist rejection in complete_agent_run**

Keep the provider payload until the rejected row is inserted in the same immediate transaction that updates agent_runs, writes the terminal event, and appends audit. Then clear candidate_payload_json so no proposed result/publication path runs. Add validation_issues: Vec<OutputValidationIssue> to ProviderFailure; redacted_detail remains user-safe.

- [ ] **Step 5: Run the test and verify GREEN**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --test tender_records rejected_output_is_persisted_with_failure_atomically --features runtime-fixture
~~~

Expected: PASS.

- [ ] **Step 6: Commit**

~~~powershell
git add src-tauri/src/tender_store.rs src-tauri/src/tender_store/agent_records.rs src-tauri/src/agent_runtime.rs src-tauri/tests/tender_records.rs
git commit -m "feat: preserve rejected agent output"
~~~

---

### Task 4: Exactly one linked Codex repair

**Files:**
- Modify: src-tauri/src/agent_runtime.rs
- Modify: src-tauri/src/agent_runtime/codex_protocol.rs
- Modify: src-tauri/src/tender_store.rs
- Modify: src-tauri/src/tender_store/tender_records.rs
- Modify: src-tauri/src/agent_backend/fixture_client.rs
- Test: src-tauri/tests/tender_records.rs
- Test: src-tauri/tests/agent_runtime.rs

**Interfaces:**
- Consumes: immutable RejectedAgentOutput from Task 3.
- Produces: AgentRepairFeedback, prepare_tender_record_repair_run, and a bounded extraction state machine.

- [ ] **Step 1: Write invalid-then-valid and invalid-twice tests**

The first asserts exactly two runs, linked lineage, one publication, and one Manager batch increment. The second asserts two failed runs, no third call, and zero publication.

- [ ] **Step 2: Run both and verify RED**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --test tender_records manager_intake_automatically_repairs_one_invalid_extraction --features runtime-fixture
cargo test --manifest-path src-tauri/Cargo.toml --test tender_records manager_intake_stops_after_one_failed_repair --features runtime-fixture
~~~

Expected: both FAIL.

- [ ] **Step 3: Persist repair feedback on tender_tasks**

Add repair_feedback_json and:

~~~rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentRepairFeedback {
    pub rejected_run_id: String,
    pub rejected_payload_sha256: String,
    pub validation_issues: Vec<OutputValidationIssue>,
}
~~~

Add repair_feedback: Option<AgentRepairFeedback> to TenderTaskView. Update insert_task and load_task together. Normal tasks store NULL; repair tasks store canonical JSON.

- [ ] **Step 4: Prepare one linked repair run**

Implement:

~~~rust
pub(crate) fn prepare_tender_record_repair_run(
    &mut self,
    tender_id: &TenderId,
    rejected_run_id: &str,
) -> Result<PreparedAgentRun, TenderCommandError>;
~~~

Verify the source is an extraction OutputInvalid run, load its rejected output and exact task inputs, reject an existing retry, create repair-feedback-v1.json, reuse a compatible thread, and insert retry_of_run_id. Add a unique partial index for non-null retry_of_run_id.

- [ ] **Step 5: Include repair feedback in the provider bundle**

Add repair_feedback beside tender_task. Instruct Codex to review the rejected proposal against unchanged exact evidence, correct every listed issue, and return one complete replacement object.

- [ ] **Step 6: Implement bounded orchestration**

After attempt 0 completes, inspect its typed failure. If it is extraction OutputInvalid and is not itself a retry, prepare and execute one repair run. Complete it normally and return that result. Never loop. Other categories keep their existing recovery paths.

- [ ] **Step 7: Add deterministic fixture sequences**

Add scenarios keyed by retry lineage: invalid then valid, and invalid twice. Do not add sleeps or network access.

- [ ] **Step 8: Run repair tests and verify GREEN**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --test tender_records manager_intake_automatically_repairs_one_invalid_extraction --features runtime-fixture
cargo test --manifest-path src-tauri/Cargo.toml --test tender_records manager_intake_stops_after_one_failed_repair --features runtime-fixture
~~~

Expected: PASS.

- [ ] **Step 9: Commit**

~~~powershell
git add src-tauri/src/agent_runtime.rs src-tauri/src/agent_runtime/codex_protocol.rs src-tauri/src/tender_store.rs src-tauri/src/tender_store/tender_records.rs src-tauri/src/agent_backend/fixture_client.rs src-tauri/tests/tender_records.rs src-tauri/tests/agent_runtime.rs
git commit -m "feat: add bounded Codex tender repair"
~~~

---

### Task 5: Truthful diagnostics and stage verification

**Files:**
- Modify: src-tauri/src/agent_runtime.rs
- Modify: src-tauri/src/tender_store/agent_records.rs
- Modify: src-tauri/src/agent_backend/turn_executor.rs
- Modify: src-tauri/src/agent_backend/fixture_client.rs
- Test: src-tauri/tests/agent_runtime.rs

**Interfaces:**
- Consumes: attempt-level transport, validation, and commit outcomes.
- Produces: truthful diagnostics and legal per-run event ordering.

- [ ] **Step 1: Write the event-order regression test**

Assert transport completion precedes candidate rejection on attempt 0; the repair run has independent normal events; result_committed precedes its completed terminal event; and no provider_turn_completed diagnostic exists before domain validation.

- [ ] **Step 2: Run it and verify RED**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime repaired_extraction_records_truthful_boundaries --features runtime-fixture
~~~

Expected: FAIL.

- [ ] **Step 3: Separate diagnostic boundaries**

Rename the pre-validation diagnostic to provider_transport_completed. Stop checkpointing ProviderEventKind::Terminal inside the transport callback; return the transport terminal outcome in ProviderExecution and let complete_agent_run append the one final terminal event after candidate validation and transaction outcome. Emit candidate_validated/candidate_rejected from completion and result_committed after publication. Log stable issue codes only, never Tender content.

- [ ] **Step 4: Run focused and stage checks**

~~~powershell
cargo test --manifest-path src-tauri/Cargo.toml --test agent_runtime repaired_extraction_records_truthful_boundaries --features runtime-fixture
cargo test --manifest-path src-tauri/Cargo.toml --test tender_records --features runtime-fixture
npm test
npm run check
npm run format:check
~~~

Expected: all commands exit 0.

- [ ] **Step 5: Commit**

~~~powershell
git add src-tauri/src/agent_runtime.rs src-tauri/src/tender_store/agent_records.rs src-tauri/src/agent_backend/turn_executor.rs src-tauri/src/agent_backend/fixture_client.rs src-tauri/tests/agent_runtime.rs src/bindings
git commit -m "fix: report truthful agent run boundaries"
~~~
