# Adaptive Quantix: Dynamic Construction Project Metadata Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render arbitrary ordered `ProjectCharacteristic` fields without source changes for new domain keys, preserve provenance and neighboring fields when one field is malformed, and keep fixed candidate envelopes strict.

**Architecture:** The Rust-owned `TenderRecordInspection` and its ordered `fields: Vec<TenderRecordField>` remain the dynamic domain boundary; no new DTO, database column, or field-key enum is introduced. A small renderer component validates the runtime field shape, selects a presentation by structural value shape rather than by domain key, renders unknown names as bounded text with provenance, and replaces only malformed fields with a fixed local error. `TenderRecordsPanel` delegates field rendering to that component while retaining its existing Host command and pagination interfaces.

**Tech Stack:** Rust existing Tender Record contracts, React 19, TypeScript 5.8, ts-rs generated bindings, Vitest, Testing Library.

**Spec:** `docs/superpowers/specs/2026-08-21-adaptive-quantix-doctor-design.md`

## Global Constraints

- IPC, security, authority, and persisted envelope contracts remain strict. Construction Project metadata is carried inside stable record-and-field collections so new field keys do not change those envelopes.
- `TenderRecordKind::ProjectCharacteristic` and the existing stable record/field collections are the extensibility boundary.
- New `stable_key` and field `name` values are domain data. They do not require a Rust DTO, TypeScript binding, database column, or React component change.
- The renderer iterates records and fields and uses a small presentation registry for supported display kinds. An unrecognized field key renders safely as text with its provenance.
- The renderer must not branch on keys such as `project_delivery_context`. Ordering, addition, and removal of metadata keys cannot blank the surface or discard neighboring fields.
- Invalid content localizes an error to the affected record/field and leaves the remaining Project Fingerprint readable.
- Strict Serde `deny_unknown_fields` remains in force for commands and fixed envelopes. Dynamic metadata does not mean accepting unknown authority or transport fields.
- Do not add compatibility DTOs, fallback parsers, dual storage paths, migrations, or a parallel health framework.
- Preserve the existing Rust-owned `TenderRecordCandidateBatch` and `TenderRecordFieldCandidate` envelopes, their canonical JSON validation, `valid_record_key` limits, and `record_extraction_output_contract()`; only field data inside the ordered collection is dynamic.
- Generated declarations under `src/bindings` remain Rust/ts-rs owned and are never edited manually.
- Each task ends with its focused test passing and a commit. Normal development finishes with `npm run verify`; do not run a production build.

---

### Task 1: Add a bounded Project Characteristic field renderer

**Files:**

- Create: `src/ProjectCharacteristicFields.tsx`
- Create: `src/ProjectCharacteristicFields.test.tsx`

**Interfaces:**

- Consumes the generated `TenderRecordField` shape from `src/bindings/TenderRecordField.ts` and `evidenceTextAttributes(location)` from `src/evidenceTypography.ts`.
- Produces:

```ts
import type { ReactElement } from "react";

export interface ProjectCharacteristicFieldsProps {
  fields: unknown;
}

export function ProjectCharacteristicFields(
  props: ProjectCharacteristicFieldsProps,
): ReactElement;
```

- The component accepts `unknown` at its containment boundary and validates the runtime array before rendering. Its only malformed-field outputs are the fixed texts `Project metadata unavailable.` and `Field data unavailable.`; it never interpolates malformed raw values, exception messages, paths, or JSON.
- The presentation registry has exactly the structural kinds `text` and `normalized_expression`. The registry is selected from `original_expression` presence, never from `field.name` or `stable_key`.

- [ ] **Step 1: Write the failing renderer tests.** Create the exact current-binding fixture and cover arbitrary names/order, structural normalization, provenance, and a malformed middle field:

```tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { TenderRecordField } from "./bindings/TenderRecordField";
import { ProjectCharacteristicFields } from "./ProjectCharacteristicFields";

function field(name: string, value: string): TenderRecordField {
  return {
    name,
    value,
    basis_kind: "evidence",
    basis_reference: null,
    basis_description: "Exact source basis",
    basis_authority: null,
    original_expression: null,
    normalized_value: null,
    timezone: null,
    uncertainty: null,
    evidence: [],
  };
}

it("renders arbitrary Project Characteristic names in the supplied order with provenance", () => {
  const view = render(
    <ProjectCharacteristicFields
      fields={[
        field("delivery_window_new", "After handover"),
        field("site_access_condition", "Escort required"),
        field("handover_sequence", "North then south"),
      ]}
    />,
  );

  expect(screen.getAllByRole("heading", { level: 6 }).map((node) => node.textContent)).toEqual([
    "delivery_window_new",
    "site_access_condition",
    "handover_sequence",
  ]);
  expect(screen.getByText("After handover")).toBeTruthy();
  expect(screen.getAllByText(/Basis: evidence/)).toHaveLength(3);
  expect(screen.queryByText("project_delivery_context")).toBeNull();

  view.rerender(
    <ProjectCharacteristicFields
      fields={[
        field("handover_sequence", "North then south"),
        field("newly_added_characteristic", "Added after the first render"),
      ]}
    />,
  );
  expect(screen.getAllByRole("heading", { level: 6 }).map((node) => node.textContent)).toEqual([
    "handover_sequence",
    "newly_added_characteristic",
  ]);
  expect(screen.getByText("Added after the first render")).toBeTruthy();
});

it("uses the structural normalized-expression presentation without a field-key branch", () => {
  render(
    <ProjectCharacteristicFields
      fields={[
        {
          ...field("submission_window_new", "2026-09-01"),
          original_expression: "1 September 2026",
          normalized_value: "2026-09-01T00:00:00Z",
          timezone: "Africa/Cairo",
          uncertainty: "None recorded",
        },
      ]}
    />
  );

  expect(screen.getByText("Original expression")).toBeTruthy();
  expect(screen.getByText("1 September 2026")).toBeTruthy();
  expect(screen.getByText("2026-09-01T00:00:00Z")).toBeTruthy();
  expect(screen.getByText("Africa/Cairo")).toBeTruthy();
});

it("localizes malformed field content and keeps both neighboring fields readable", () => {
  const malformed = [
    field("before_malformed", "Readable before"),
    { name: "malformed_new", value: 42 },
    field("after_malformed", "Readable after"),
  ] as unknown as TenderRecordField[];

  render(<ProjectCharacteristicFields fields={malformed} />);

  expect(screen.getByText("Readable before")).toBeTruthy();
  expect(screen.getByText("Readable after")).toBeTruthy();
  expect(screen.getByRole("status")).toHaveTextContent("Field data unavailable.");
  expect(screen.queryByText("42")).toBeNull();
});

it("localizes a malformed field collection to this Project Characteristic", () => {
  render(<ProjectCharacteristicFields fields={{ unexpected: "shape" }} />);
  expect(screen.getByRole("status")).toHaveTextContent(
    "Project metadata unavailable.",
  );
});
```

- [ ] **Step 2: Run the focused tests to verify they fail.**

Run: `npm run test:renderer -- src/ProjectCharacteristicFields.test.tsx`

Expected: FAIL because `src/ProjectCharacteristicFields.tsx` does not exist and the named export cannot be imported.

- [ ] **Step 3: Implement the minimal structural renderer.** Add the public props/signature and use a closed presentation registry; the implementation must keep the field array order and use a positional key so a reordered or newly added domain field cannot replace a neighboring field:

```tsx
import type { ReactElement } from "react";

import type { TenderRecordField } from "./bindings/TenderRecordField";
import { evidenceTextAttributes } from "./evidenceTypography";

type FieldPresentationKind = "text" | "normalized_expression";
type RenderableField =
  | { kind: "valid"; field: TenderRecordField }
  | { kind: "malformed"; ordinal: number };

const fieldPresentations: Record<
  FieldPresentationKind,
  (field: TenderRecordField) => ReactElement
> = {
  text: (field) => (
    <p>{field.value ?? "No supported value in supplied Evidence"}</p>
  ),
  normalized_expression: (field) => (
    <>
      <p>{field.value ?? "No supported value in supplied Evidence"}</p>
      <dl className="deadline-normalization">
        <div><dt>Original expression</dt><dd>{field.original_expression}</dd></div>
        <div><dt>Timezone</dt><dd>{field.timezone ?? "Not recorded"}</dd></div>
        <div><dt>Normalized value</dt><dd>{field.normalized_value ?? "Not normalized"}</dd></div>
        <div><dt>Uncertainty</dt><dd>{field.uncertainty ?? "None recorded"}</dd></div>
      </dl>
    </>
  ),
};

function isSafeField(value: unknown): value is TenderRecordField {
  if (typeof value !== "object" || value === null) return false;
  const field = value as Record<string, unknown>;
  const optionalText = (item: unknown) => item === null || typeof item === "string";
  const basisKinds = new Set([
    "evidence",
    "assumption",
    "tender_query",
    "calculation_run",
    "engineer_entry",
  ]);
  return (
    typeof field.name === "string" && field.name.length > 0 &&
    optionalText(field.value) && typeof field.basis_kind === "string" &&
    basisKinds.has(field.basis_kind) &&
    optionalText(field.basis_reference) && optionalText(field.basis_description) &&
    (field.basis_authority === null || (
      typeof field.basis_authority === "object" &&
      typeof (field.basis_authority as Record<string, unknown>).authority_id === "string" &&
      typeof (field.basis_authority as Record<string, unknown>).created_by === "string" &&
      typeof (field.basis_authority as Record<string, unknown>).created_at === "string"
    )) &&
    optionalText(field.original_expression) && optionalText(field.normalized_value) &&
    optionalText(field.timezone) && optionalText(field.uncertainty) &&
    Array.isArray(field.evidence) &&
    field.evidence.every((item) => {
      if (typeof item !== "object" || item === null) return false;
      const evidence = item as Record<string, unknown>;
      const reference = evidence.reference as Record<string, unknown> | null;
      const location = evidence.location as Record<string, unknown> | null;
      return (
        typeof evidence.package_path === "string" &&
        reference !== null && typeof reference === "object" &&
        typeof reference.artifact_id === "string" &&
        typeof reference.version === "number" &&
        typeof reference.ordinal === "number" &&
        typeof location === "object" && location !== null &&
        typeof location.kind === "string" &&
        typeof location.original_text === "string" &&
        optionalText(location.translated_text) &&
        typeof location.direction === "string" &&
        typeof location.language === "string"
      );
    })
  );
}

function classifyField(value: unknown, ordinal: number): RenderableField {
  return isSafeField(value)
    ? { kind: "valid", field: value }
    : { kind: "malformed", ordinal };
}

export function ProjectCharacteristicFields({
  fields,
}: ProjectCharacteristicFieldsProps): ReactElement {
  if (!Array.isArray(fields)) {
    return <p role="status">Project metadata unavailable.</p>;
  }
  return (
    <div className="project-characteristic-fields">
      {fields.map(classifyField).map((item, index) => {
        if (item.kind === "malformed") {
          return (
            <section className="record-field" role="status" key={`malformed:${item.ordinal}`}>
              <h6>Field {item.ordinal + 1}</h6>
              <p>Field data unavailable.</p>
            </section>
          );
        }
        const presentation: FieldPresentationKind =
          item.field.original_expression === null
            ? "text"
            : "normalized_expression";
        return (
          <section className="record-field" key={`${item.field.name}:${index}`}>
            <h6>{item.field.name}</h6>
            {fieldPresentations[presentation](item.field)}
            <p className="record-basis">
              Basis: {item.field.basis_kind.replace(/_/g, " ")}
              {item.field.basis_description ? ` — ${item.field.basis_description}` : ""}
            </p>
            {item.field.basis_authority ? (
              <p className="record-basis-attribution">
                Exact authority {item.field.basis_authority.authority_id} · recorded by {item.field.basis_authority.created_by} at {item.field.basis_authority.created_at}
              </p>
            ) : null}
            {item.field.evidence.map((evidence) => (
              <details className="record-evidence" key={`${evidence.reference.artifact_id}:${evidence.reference.version}:${evidence.reference.ordinal}`}>
                <summary>{evidence.package_path} · v{evidence.reference.version} · {evidence.location.kind} #{evidence.reference.ordinal}</summary>
                <blockquote {...evidenceTextAttributes(evidence.location)}>{evidence.location.original_text}</blockquote>
                {evidence.location.translated_text ? (
                  <div className="derived-translation">
                    <strong>Derived translation — non-authoritative</strong>
                    <blockquote dir="auto">{evidence.location.translated_text}</blockquote>
                  </div>
                ) : null}
              </details>
            ))}
          </section>
        );
      })}
    </div>
  );
}
```

The implementation intentionally does not inspect or enumerate domain field names. It renders an unrecognized name through the `text` presentation and exposes the existing exact basis/evidence metadata. The guard is a renderer containment boundary; it does not relax Rust deserialization or candidate validation.

- [ ] **Step 4: Run the focused tests to verify they pass.**

Run: `npm run test:renderer -- src/ProjectCharacteristicFields.test.tsx`

Expected: PASS, including the unknown-key order, normalized-expression, and localized-malformed-field assertions.

- [ ] **Step 5: Commit the renderer component.**

```powershell
git add src/ProjectCharacteristicFields.tsx src/ProjectCharacteristicFields.test.tsx
git commit -m "feat: render dynamic Project Characteristic fields"
```

---

### Task 2: Integrate dynamic fields into the Tender Records surface

**Files:**

- Modify: `src/TenderRecordsPanel.tsx`
- Create: `src/TenderRecordsPanel.test.tsx`

**Interfaces:**

- Consumes: `inspectTenderRecords(tenderId: string, cursor: string | null, limit: number): Promise<TenderRecordPage>`, the existing `TenderRecordInspection` DTO, and `ProjectCharacteristicFields({ fields })` from Task 1.
- Produces: the existing `TenderRecordsPanelProps` and `TenderRecordPage` Host/pagination behavior with field rendering delegated to the bounded component.
- Removes only the duplicated inline field presentation and its direct `evidenceTextAttributes` dependency. It does not change `InspectTenderRecordsCommand`, candidate envelopes, record pagination, review commands, extraction commands, or Engineer decision commands.

- [ ] **Step 1: Write the failing integration test against the current panel.** Mock every Host function imported by the panel, return one Project Characteristic with an unknown key and a malformed middle field, and prove the panel keeps the surrounding content:

```tsx
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const host = vi.hoisted(() => ({
  inspectTenderRecords: vi.fn(),
  inspectTenderRecordAuthorities: vi.fn(),
  inspectEvidence: vi.fn(),
  decideTenderRecord: vi.fn(),
  createTenderEngineerEntry: vi.fn(),
  runTenderRecordExtraction: vi.fn(),
  runTenderRecordReview: vi.fn(),
}));

vi.mock("./quantixHost", () => host);

import { TenderRecordsPanel } from "./TenderRecordsPanel";

afterEach(cleanup);

beforeEach(() => {
  host.inspectTenderRecordAuthorities.mockResolvedValue([]);
  host.inspectTenderRecords.mockResolvedValue({
    records: [
      {
        record_id: "record-1",
        stable_key: "project-characteristic",
        version: 1,
        kind: "project_characteristic",
        title: "Project Fingerprint",
        verification_status: "verified",
        trust_class: "verified",
        fields: [
          {
            name: "new_delivery_constraint",
            value: "Keep the north access route open",
            basis_kind: "evidence",
            basis_reference: null,
            basis_description: "Exact source basis",
            basis_authority: null,
            original_expression: null,
            normalized_value: null,
            timezone: null,
            uncertainty: null,
            evidence: [],
          },
          { name: "damaged_field", value: 7 },
          {
            name: "neighboring_characteristic",
            value: "Still readable",
            basis_kind: "evidence",
            basis_reference: null,
            basis_description: "Second exact source basis",
            basis_authority: null,
            original_expression: null,
            normalized_value: null,
            timezone: null,
            uncertainty: null,
            evidence: [],
          },
        ],
        generation_instruction: null,
        contradictions: [],
        source_relationships: [],
        reviews: [],
        author_run_id: "run-1",
        author_profile_id: "profile-1",
        created_at: "2026-08-21T00:00:00Z",
      },
    ],
    next_cursor: null,
  });
});

it("keeps Project Fingerprint neighbors visible and localizes one malformed field", async () => {
  render(
    <TenderRecordsPanel
      tenderId={"tender-1"}
      runtimeReady={false}
      reportCommandFailure={vi.fn()}
    />,
  );

  expect(await screen.findByText("Keep the north access route open")).toBeTruthy();
  expect(screen.getByText("Still readable")).toBeTruthy();
  expect(screen.getByRole("status")).toHaveTextContent("Field data unavailable.");
  expect(screen.queryByText("7")).toBeNull();
  expect(screen.getByText("project characteristic · version 1")).toBeTruthy();
  expect(screen.getByText("Exact target record-1 · verified")).toBeTruthy();
  expect(screen.getByText("Stable key project-characteristic")).toBeTruthy();
  expect(
    screen.getByText("Authored by run run-1 · profile profile-1 · 2026-08-21T00:00:00Z"),
  ).toBeTruthy();
});
```

- [ ] **Step 2: Run the integration test to verify it fails.**

Run: `npm run test:renderer -- src/TenderRecordsPanel.test.tsx`

Expected: FAIL because the current panel prints the malformed numeric value through its inline `{field.value ?? ...}` renderer and does not produce `Field data unavailable.`.

- [ ] **Step 3: Delegate field rendering to the new component.** Import `ProjectCharacteristicFields` and replace the entire existing `record.fields.map(...)` section (including basis, normalization, and evidence JSX) with:

```tsx
<ProjectCharacteristicFields fields={record.fields} />
```

Keep the surrounding record header, trust badge, identity, contradictions, source relationships, reviews, actions, and pagination unchanged. Keep the existing `TenderEvidenceReference` import because evidence selection state still uses it, but remove the now-unused `evidenceTextAttributes` import.

Immediately below the existing exact-target identity, render the outer dynamic identity and provenance that already exist in `TenderRecordInspection`:

```tsx
<p className="record-stable-key">Stable key {record.stable_key}</p>
<p className="record-author-provenance">
  Authored by run {record.author_run_id} · profile {record.author_profile_id} · {record.created_at}
</p>
```

- [ ] **Step 4: Run the focused integration and component tests.**

Run: `npm run test:renderer -- src/ProjectCharacteristicFields.test.tsx src/TenderRecordsPanel.test.tsx`

Expected: PASS. The Host is called with the unchanged `{ tenderId, cursor, limit }` behavior, arbitrary field names remain ordered, and the malformed field cannot hide either neighboring value.

- [ ] **Step 5: Add a source-level boundary assertion.** Extend `src/ProjectCharacteristicFields.test.tsx` with this exact check so a future renderer edit cannot reintroduce a domain-key branch:

```tsx
import { readFileSync } from "node:fs";

it("does not encode a Project Characteristic domain key", () => {
  const source = readFileSync(new URL("./ProjectCharacteristicFields.tsx", import.meta.url), "utf8");
  expect(source).not.toContain("project_delivery_context");
});
```

- [ ] **Step 6: Run the boundary assertion and TypeScript check.**

Run:

```powershell
npm run test:renderer -- src/ProjectCharacteristicFields.test.tsx src/TenderRecordsPanel.test.tsx
npx tsc --noEmit
```

Expected: PASS with no new binding edits and no TypeScript errors.

- [ ] **Step 7: Verify the strict Rust envelope and dynamic boundary.** Confirm the implementation did not alter the existing Rust contract by running these commands:

Run: `rg -n -U "#\[serde\(deny_unknown_fields\)\][\\s\\S]{0,220}(struct TenderRecordCandidateBatch|struct TenderRecordCandidate|struct TenderRecordFieldCandidate)" src-tauri/src/tender_store/tender_records.rs`

Expected: each candidate envelope is still marked `deny_unknown_fields`; no `serde(flatten)`, `Value`, dynamic map, or compatibility DTO is introduced.

Run: `rg -n "ProjectCharacteristic|pub fields: Vec<TenderRecordField>|record\.fields\.map|project_delivery_context" src-tauri/src/tender_store/tender_records.rs src/ProjectCharacteristicFields.tsx src/TenderRecordsPanel.tsx`

Expected: `ProjectCharacteristic` and ordered `Vec<TenderRecordField>` remain the Rust boundary; the renderer has no `project_delivery_context` branch; field iteration is delegated to `ProjectCharacteristicFields`.

- [ ] **Step 8: Run the repository development gate.**

Run: `npm run verify`

Expected: PASS for identity validation, formatting, TypeScript/Rust checks, renderer tests, and deterministic Rust tests. Do not run `npm run build:desktop`.

- [ ] **Step 9: Commit the integration.**

```powershell
git add src/TenderRecordsPanel.tsx src/TenderRecordsPanel.test.tsx src/ProjectCharacteristicFields.test.tsx
git commit -m "feat: isolate malformed Project Fingerprint fields"
```

---

### Task 3: Lock the dynamic persistence boundary and strict envelope with Rust regressions

**Files:**

- Modify: `src-tauri/tests/support/runtime_fixture.rs`
- Modify: `src-tauri/tests/tender_records.rs`

**Interfaces:**

- Consumes: the existing `record-extraction` runtime fixture, `RunTenderRecordExtractionCommand`, cold-open `QuantixHost`, and `inspect_tender_integrity`.
- Produces two fixture-only scenarios:
  - `record-extraction-dynamic-project-fields` returns one valid `ProjectCharacteristic` with an arbitrary stable key and an ordered set of arbitrary field names;
  - `record-extraction-extra-envelope-field` adds an unknown top-level authority field to a fixed `TenderRecordCandidate` envelope.
- Does not change `TenderRecordInspection`, `TenderRecordField`, the database schema, or any production parser.

- [ ] **Step 1: Write the failing cold-open regression.** Add this test to `src-tauri/tests/tender_records.rs`:

```rust
#[tokio::test]
async fn arbitrary_project_characteristic_fields_preserve_order_across_cold_open() {
    let harness = RuntimeHarness::new("record-extraction-dynamic-project-fields");
    let evidence = harness
        .parsed_pdf_evidence("dynamic-project-fields", b"TENDER_RECORD_GOLDEN")
        .await;
    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("extract arbitrary Project Characteristic fields");
    let record = records_for_run(&harness.host, &harness.tender_id, &extraction.run.run_id)
        .into_iter()
        .find(|record| record.stable_key == "dynamic_project_fingerprint")
        .expect("dynamic Project Characteristic");
    assert_eq!(record.kind, TenderRecordKind::ProjectCharacteristic);
    assert_eq!(
        record.fields.iter().map(|field| field.name.as_str()).collect::<Vec<_>>(),
        vec!["delivery_window_new", "site_access_condition", "handover_sequence"],
    );

    let cold_host = QuantixHost::with_setup_platform(
        &harness.application_home,
        Arc::new(ReadySetupPlatform),
    );
    assert_eq!(ensure_quantix_setup(&cold_host).state, SetupState::Ready);
    assert_eq!(
        cold_host
            .inspect_tender_integrity(&harness.tender_id)
            .expect("cold-open dynamic Project Characteristic")
            .state,
        TenderIntegrityState::Ready,
    );
    let reopened = inspect_all_records(&cold_host, &harness.tender_id)
        .into_iter()
        .find(|candidate| candidate.record_id == record.record_id)
        .expect("reopened dynamic Project Characteristic");
    assert_eq!(reopened.stable_key, "dynamic_project_fingerprint");
    assert_eq!(reopened.fields, record.fields);
}
```

- [ ] **Step 2: Write the failing fixed-envelope regression.** It proves dynamic field names do not relax an authority-bearing outer envelope:

```rust
#[tokio::test]
async fn dynamic_metadata_does_not_accept_unknown_fixed_envelope_fields() {
    let harness = RuntimeHarness::new("record-extraction-extra-envelope-field");
    let evidence = harness
        .parsed_pdf_evidence("strict-envelope", b"TENDER_RECORD_GOLDEN")
        .await;
    let extraction = harness
        .host
        .run_tender_record_extraction(RunTenderRecordExtractionCommand {
            tender_id: harness.tender_id.clone(),
            evidence: evidence.references,
            authorities: Vec::new(),
        })
        .await
        .expect("persist rejected fixed-envelope candidate");
    assert_eq!(extraction.run.state, AgentRunState::Failed);
    assert_eq!(
        extraction.run.failure.map(|failure| failure.category),
        Some(ProviderFailureCategory::OutputInvalid),
    );
    assert_eq!(extraction.published_record_count, 0);
}
```

- [ ] **Step 3: Run**:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test tender_records --features runtime-fixture arbitrary_project_characteristic_fields_preserve_order_across_cold_open
cargo test --manifest-path src-tauri/Cargo.toml --test tender_records --features runtime-fixture dynamic_metadata_does_not_accept_unknown_fixed_envelope_fields
```
- Expected: FAIL because both named runtime-fixture scenarios are absent.
- [ ] **Step 4: Add the two closed fixture branches.** Add both scenario names to the existing record-scenario allowlist. Build the dynamic candidate from the current valid Project Characteristic so evidence remains exact:

```rust
} else if scenario == "record-extraction-dynamic-project-fields" {
    let mut candidate = record_extraction_candidate(provider_data_view)?;
    let mut record = candidate["records"]
        .as_array()
        .and_then(|records| records.iter().find(|record| {
            record.get("kind").and_then(serde_json::Value::as_str)
                == Some("project_characteristic")
        }))
        .cloned()
        .ok_or("Project Characteristic fixture")?;
    let template = record["fields"]
        .as_array()
        .and_then(|fields| fields.first())
        .cloned()
        .ok_or("Project Characteristic field fixture")?;
    let fields = [
        ("delivery_window_new", "After handover"),
        ("site_access_condition", "Escort required"),
        ("handover_sequence", "North then south"),
    ]
    .into_iter()
    .map(|(name, value)| {
        let mut field = template.clone();
        field["name"] = serde_json::json!(name);
        field["value"] = serde_json::json!(value);
        field
    })
    .collect::<Vec<_>>();
    record["stable_key"] = serde_json::json!("dynamic_project_fingerprint");
    record["title"] = serde_json::json!("Dynamic Project Fingerprint");
    record["fields"] = serde_json::Value::Array(fields);
    candidate["records"] = serde_json::json!([record]);
    candidate
} else if scenario == "record-extraction-extra-envelope-field" {
    let mut candidate = record_extraction_candidate(provider_data_view)?;
    candidate["records"][0]["unexpected_authority"] = serde_json::json!("must reject");
    candidate
```

- [ ] **Step 5: Run** the two focused Rust tests again. Expected: PASS without a production source or schema change.
- [ ] **Step 6: Run** `cargo test --manifest-path src-tauri/Cargo.toml --test tender_records --features runtime-fixture` and `npm run verify`. Expected: PASS.
- [ ] **Step 7: Inspect** `git diff --check` and confirm generated declarations are unchanged.
- [ ] **Step 8: Commit**:

```powershell
git add src-tauri/tests/support/runtime_fixture.rs src-tauri/tests/tender_records.rs
git commit -m "test: lock dynamic Project metadata boundary"
```
