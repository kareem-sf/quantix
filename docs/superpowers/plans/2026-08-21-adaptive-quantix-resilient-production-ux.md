# Adaptive Quantix: Resilient Production UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep Quantix usable when one renderer surface fails, preserve the Tendering Engineer's navigation and drafts, expose the four truthful Doctor states persistently, and show exact before/after results for safe repairs.

**Architecture:** `ManagerWorkspace` remains the owner of navigation, selected Tender, drafts, and settings state. Small surface boundaries sit below that state and replace only failed children with a recovery panel. Renderer failures send only a closed diagnostic kind to the Rust Host; the Host returns an opaque diagnostic receipt. `DoctorIndicator` and Application Settings consume the health-and-repair slice's typed Doctor report instead of maintaining a second health model.

**Tech Stack:** React 19, TypeScript 5.8, React class error boundaries, Tauri 2 IPC, Rust, ts-rs generated bindings, Vitest, Testing Library.

**Spec:** `docs/superpowers/specs/2026-08-21-adaptive-quantix-doctor-design.md`

## Global Constraints

- Complete the health-and-repair plan before Tasks 5 and 6. This plan consumes `ReadinessState`, the expanded `QuantixDoctorReport`, `QuantixDoctorFinding`, and `AutomaticRepairResult`; it must not redefine them.
- Navigation, `composerDrafts`, `contextRefsByTender`, search state, and the selected Tender stay in `ManagerWorkspace`, above every surface boundary.
- A renderer diagnostic command contains only `RendererDiagnosticKind`. Never add a surface name, exception message, component props, Tender content, draft text, path, or stack.
- A renderer boundary has no persistence, provider, updater, Tender Store, or generic command authority.
- `unavailable_external` is presented as **Needs review**, not as healthy and not as a locally repairable state.
- “Repair all safe issues” calls the one Host-owned safe-repair command from the health-and-repair slice. The renderer does not loop through findings or maintain its own allowlist.
- `EngineerReviewRoute` actions remain visually and behaviorally separate from safe repair. In particular, `activate_system_profile_revision` uses the contract plan's fresh direct domain command and is never translated into `repairQuantixDoctor`.
- Generated declarations in `src/bindings` are produced by Rust/ts-rs tests and committed. Never edit them manually.
- Do not preserve the obsolete boolean `QuantixDoctorReport.healthy` presentation. The health plan has already made the minimum direct `readiness_state` adaptation; this plan must not reintroduce a boolean adapter.
- No production build in normal development. Each task runs focused tests; the final task runs `npm run verify`.

---

### Task 1: Return a content-free renderer diagnostic receipt

**Files:**

- Modify: `src-tauri/src/diagnostics.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/host.rs`
- Modify: `src-tauri/src/bin/export_bindings.rs`
- Modify: `src/quantixHost.ts`
- Generated: `src/bindings/RendererDiagnosticReceipt.ts`
- Test: inline `#[cfg(test)]` tests in `src-tauri/src/diagnostics.rs` and `src-tauri/src/lib.rs`
- Test: `src/quantixHost.test.ts`

**Interfaces:**

- Consumes: `RecordRendererDiagnosticCommand { kind: RendererDiagnosticKind }`
- Produces: `RendererDiagnosticReceipt { recorded: bool, correlation_id: Option<String> }`
- Changes: `record_renderer_diagnostic` and `recordRendererDiagnostic` return `RendererDiagnosticReceipt` instead of `bool`.
- Keeps: `RecordRendererDiagnosticCommand` closed with `#[serde(deny_unknown_fields)]` and no content-bearing fields.

- [ ] **Step 1: Write the failing Rust tests.** Add a receipt test beside the existing diagnostics tests and a command-shape test in `lib.rs`:

```rust
#[test]
fn accepted_application_fact_returns_an_opaque_event_id() {
    let root = tempfile::tempdir().expect("temporary diagnostics root");
    let store = DiagnosticsStore::new(root.path());
    store.activate();

    let event_id = store
        .record_application_with_receipt(fact("renderer_surface_unavailable"))
        .expect("accepted diagnostic receipt");

    assert!(!event_id.is_empty());
    assert!(!event_id.contains(root.path().to_string_lossy().as_ref()));
}

#[test]
fn renderer_diagnostic_command_rejects_content_fields() {
    let parsed = serde_json::from_value::<RecordRendererDiagnosticCommand>(serde_json::json!({
        "kind": "surface_unavailable",
        "message": "Tender draft or exception text"
    }));
    assert!(parsed.is_err());
}
```

- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml --lib renderer_diagnostic --features runtime-fixture` and `cargo test --manifest-path src-tauri/Cargo.toml --lib accepted_application_fact_returns_an_opaque_event_id --features runtime-fixture`.
- Expected: FAIL because `record_application_with_receipt` and `RendererDiagnosticReceipt` do not exist and the Tauri command still returns `bool`.
- [ ] **Step 3: Add the receipt DTO and a receipt-preserving diagnostics path.** Keep the existing boolean methods for unrelated callers and make them wrappers around one internal function:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RendererDiagnosticReceipt {
    pub recorded: bool,
    pub correlation_id: Option<String>,
}

impl DiagnosticsStore {
    pub(crate) fn record_application_with_receipt(
        &self,
        fact: RecordDiagnosticFact,
    ) -> Option<String> {
        self.record_with_receipt(fact, DiagnosticScope::Application, None)
    }

    pub fn record_application(&self, fact: RecordDiagnosticFact) -> bool {
        self.record_application_with_receipt(fact).is_some()
    }
}
```

In `record_with_receipt`, build the existing projection ID from the accepted event before enqueueing it, return `Some(event_id)` only when `TRACING_MESSAGE_ACCEPTED` is true, and return `None` on every existing rejection path. Do not introduce a second random-ID source.
- [ ] **Step 4: Change the command return without changing its input.** The command ends with:

```rust
let correlation_id = host
    .inner()
    .diagnostics()
    .record_application_with_receipt(fact);
RendererDiagnosticReceipt {
    recorded: correlation_id.is_some(),
    correlation_id,
}
```

When the existing rate limiter rejects the event, return `RendererDiagnosticReceipt { recorded: false, correlation_id: None }`.
- [ ] **Step 5: Export bindings and update the TypeScript wrapper.** Import `RendererDiagnosticReceipt` in `src-tauri/src/bin/export_bindings.rs` and call `RendererDiagnosticReceipt::export_all(&config)?`; never hand-edit `src/bindings/RendererDiagnosticReceipt.ts`. The wrapper signature is:

```ts
export function recordRendererDiagnostic(
  command: RecordRendererDiagnosticCommand,
): Promise<RendererDiagnosticReceipt> {
  return invoke<RendererDiagnosticReceipt>("record_renderer_diagnostic", {
    command,
  });
}
```

Add a host-wrapper test that asserts the command payload remains exactly `{ command: { kind: "surface_unavailable" } }`.
- [ ] **Step 6: Run** `npm test`. Expected: PASS and `src/bindings/RendererDiagnosticReceipt.ts` is generated.
- [ ] **Step 7: Commit** `feat: return renderer diagnostic receipts`

---

### Task 2: Build the isolated surface recovery panel

**Files:**

- Create: `src/SurfaceRecoveryPanel.tsx`
- Create: `src/SurfaceRecoveryPanel.test.tsx`
- Modify: `src/ManagerWorkspace.css`

**Interfaces:**

- Produces:

```ts
export interface SurfaceRecoveryPanelProps {
  correlationId: string | null;
  exportState: "idle" | "exporting" | "exported" | "failed";
  onRetry: () => void;
  onReturnToOverview: () => void;
  onOpenDoctor: () => void;
  onExportDiagnostics: () => void;
}
```

- Does not consume an `Error`, stack, surface name, Tender content, or arbitrary detail string.

- [ ] **Step 1: Write the failing component tests.** The tests must assert all four actions, the opaque receipt, and the no-receipt state:

```tsx
it("offers the four bounded recovery actions", () => {
  const actions = {
    onRetry: vi.fn(),
    onReturnToOverview: vi.fn(),
    onOpenDoctor: vi.fn(),
    onExportDiagnostics: vi.fn(),
  };
  render(
    <SurfaceRecoveryPanel
      correlationId="2026-08-21T10:00:00Z-9"
      exportState="idle"
      {...actions}
    />,
  );
  expect(screen.getByRole("alert")).toHaveTextContent(
    "Only this surface is unavailable",
  );
  expect(screen.getByText("2026-08-21T10:00:00Z-9")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Retry this surface" }));
  fireEvent.click(screen.getByRole("button", { name: "Return to Tender overview" }));
  fireEvent.click(screen.getByRole("button", { name: "Open Quantix Doctor" }));
  fireEvent.click(screen.getByRole("button", { name: "Export redacted diagnostics" }));
  Object.values(actions).forEach((action) => expect(action).toHaveBeenCalledOnce());
});

it("does not invent a correlation when diagnostics are unavailable", () => {
  render(<SurfaceRecoveryPanel correlationId={null} exportState="failed" {...actions} />);
  expect(screen.getByText("No diagnostic receipt was recorded.")).toBeTruthy();
});
```

- [ ] **Step 2: Run** `npm run test:renderer -- src/SurfaceRecoveryPanel.test.tsx`.
- Expected: FAIL because the component does not exist.
- [ ] **Step 3: Implement the presentational panel.** Use semantic `<section role="alert" aria-labelledby="surface-recovery-title">`; fixed copy; a `<code>` only when `correlationId` exists; and four `<button type="button">` actions. Disable export while `exportState === "exporting"` and announce exported/failed status in an `aria-live="polite"` region.
- [ ] **Step 4: Add focused CSS.** Reuse existing Quantix surface, border, error, and button tokens from `ManagerWorkspace.css`; do not add a modal or global overlay.
- [ ] **Step 5: Run** `npm run test:renderer -- src/SurfaceRecoveryPanel.test.tsx`. Expected: PASS.
- [ ] **Step 6: Commit** `feat: add renderer surface recovery panel`

---

### Task 3: Add a privacy-bounded React surface error boundary

**Files:**

- Create: `src/SurfaceErrorBoundary.tsx`
- Create: `src/SurfaceErrorBoundary.test.tsx`
- Modify: `src/SurfaceRecoveryPanel.tsx`

**Interfaces:**

- Consumes: `recordRendererDiagnostic({ kind })`, `exportDiagnosticsSupportBundle`, and callbacks owned by `ManagerWorkspace`.
- Produces:

```ts
export interface SurfaceErrorBoundaryProps {
  boundaryKey: string;
  diagnosticScope: "application" | "tender";
  tenderId: string | null;
  onReturnToOverview: () => void;
  onOpenDoctor: (correlationId: string | null) => void;
  children: React.ReactNode;
}
```

- Internal state contains only `failed`, `retryRevision`, `correlationId`, and `exportState`.

- [ ] **Step 1: Write failing tests with a throwing child.** Mock the Host functions and assert isolation, content-free recording, retry, and export:

```tsx
function ThrowingSurface({ fail }: { fail: boolean }) {
  if (fail) throw new Error("SECRET TENDER PATH C:\\customer\\bid.txt");
  return <div>Healthy Work surface</div>;
}

it("replaces only the failed child and records no exception content", async () => {
  host.recordRendererDiagnostic.mockResolvedValue({
    recorded: true,
    correlation_id: "opaque-17",
  });
  render(
    <div>
      <div>Persistent navigation</div>
      <SurfaceErrorBoundary {...props}>
        <ThrowingSurface fail />
      </SurfaceErrorBoundary>
    </div>,
  );
  expect(screen.getByText("Persistent navigation")).toBeTruthy();
  expect(await screen.findByText("opaque-17")).toBeTruthy();
  expect(host.recordRendererDiagnostic).toHaveBeenCalledWith({
    kind: "surface_unavailable",
  });
  expect(JSON.stringify(host.recordRendererDiagnostic.mock.calls)).not.toContain(
    "SECRET TENDER PATH",
  );
});

it("retries by remounting only the child", async () => {
  let fail = true;
  const child = () => <ThrowingSurface fail={fail} />;
  const view = render(<SurfaceErrorBoundary {...props}>{child()}</SurfaceErrorBoundary>);
  fail = false;
  view.rerender(<SurfaceErrorBoundary {...props}>{child()}</SurfaceErrorBoundary>);
  fireEvent.click(screen.getByRole("button", { name: "Retry this surface" }));
  expect(await screen.findByText("Healthy Work surface")).toBeTruthy();
  expect(host.recordRendererDiagnostic).toHaveBeenLastCalledWith({
    kind: "state_recovered",
  });
});
```

- [ ] **Step 2: Run** `npm run test:renderer -- src/SurfaceErrorBoundary.test.tsx`.
- Expected: FAIL because `SurfaceErrorBoundary` does not exist.
- [ ] **Step 3: Implement the class boundary.** `getDerivedStateFromError` sets `failed: true`; `componentDidCatch` deliberately ignores both parameters and calls only `recordRendererDiagnostic({ kind: "surface_unavailable" })`; `componentDidUpdate` records `state_recovered` after a successful retry. The render path is:

```tsx
if (this.state.failed) {
  return (
    <SurfaceRecoveryPanel
      correlationId={this.state.correlationId}
      exportState={this.state.exportState}
      onRetry={() =>
        this.setState((state) => ({
          failed: false,
          retryRevision: state.retryRevision + 1,
          correlationId: null,
          exportState: "idle",
        }))
      }
      onReturnToOverview={this.props.onReturnToOverview}
      onOpenDoctor={() => this.props.onOpenDoctor(this.state.correlationId)}
      onExportDiagnostics={() => void this.exportDiagnostics()}
    />
  );
}
return <React.Fragment key={`${this.props.boundaryKey}-${this.state.retryRevision}`}>{this.props.children}</React.Fragment>;
```

`exportDiagnostics()` calls `exportDiagnosticsSupportBundle` with `include_deep: false`, `policy_revision: 1`, and the exact application/Tender scope from props.
- [ ] **Step 4: Ensure rejected diagnostic promises are contained.** A record failure leaves `correlationId: null`; it must not throw from `componentDidCatch` or replace the whole application.
- [ ] **Step 5: Run** `npm run test:renderer -- src/SurfaceErrorBoundary.test.tsx`. Expected: PASS.
- [ ] **Step 6: Commit** `feat: isolate renderer surface failures`

---

### Task 4: Place boundaries below persistent workspace state

**Files:**

- Create: `src/WorkspaceSurface.tsx`
- Create: `src/WorkspaceSurface.test.tsx`
- Modify: `src/ManagerWorkspace.tsx`
- Modify: `src/ManagerWorkspace.test.tsx`
- Modify: `src/ManagerWorkspace.css`

**Interfaces:**

- Consumes: `SurfaceErrorBoundary`.
- Produces: `WorkspaceSurface { active, surfaces }`, one recoverable boundary around the currently selected main surface (`manager`, `work`, `team`, or `files`), and separate boundaries around Settings, retention, and recovery surfaces.
- Preserves: `composerDrafts`, selected Tender, `view`, navigation history, search queries, and context references in the `ManagerWorkspace` parent.

- [ ] **Step 1: Extract the pure surface selector and test it.** The component has no state and no failure behavior:

```tsx
export interface WorkspaceSurfaceProps {
  active: WorkspaceView;
  surfaces: Record<WorkspaceView, React.ReactNode>;
}

export function WorkspaceSurface({ active, surfaces }: WorkspaceSurfaceProps) {
  return surfaces[active];
}
```

Its focused test passes four labelled nodes and asserts only the active node renders.
- [ ] **Step 2: Add deterministic test-only failure injection by mocking the module in `ManagerWorkspace.test.tsx`.** Keep the injection entirely in the test file:

```tsx
const surfaceFailure = vi.hoisted(() => ({ active: null as WorkspaceView | null }));

vi.mock("./WorkspaceSurface", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./WorkspaceSurface")>();
  return {
    WorkspaceSurface(props: WorkspaceSurfaceProps) {
      if (surfaceFailure.active === props.active) {
        throw new Error("test-owned surface failure");
      }
      return actual.WorkspaceSurface(props);
    },
  };
});
```

Reset `surfaceFailure.active = null` in `beforeEach`. No crash flag or alternate component path enters production code.
- [ ] **Step 3: Write the failing workspace test.** It must type a Manager draft, set `surfaceFailure.active = "work"`, navigate to Work, verify sidebar and Tender header remain, clear the injected failure before returning to Manager, and verify the draft:

```tsx
it("isolates a Work surface crash and preserves navigation and the Manager draft", async () => {
  renderWorkspaceWithProjection(projection, { crashSurface: "work" });
  const composer = await screen.findByRole("textbox", {
    name: "Message the Tendering Manager",
  });
  fireEvent.change(composer, { target: { value: "Keep this bid note." } });
  surfaceFailure.active = "work";
  fireEvent.click(screen.getByRole("button", { name: "Work" }));
  expect(await screen.findByText("Only this surface is unavailable")).toBeTruthy();
  expect(screen.getByRole("navigation", { name: "Tenders" })).toBeTruthy();
  expect(screen.getByRole("heading", { name: projection.selected_tender!.name })).toBeTruthy();
  surfaceFailure.active = null;
  fireEvent.click(screen.getByRole("button", { name: "Return to Tender overview" }));
  expect(
    (await screen.findByRole("textbox", { name: "Message the Tendering Manager" }) as HTMLTextAreaElement).value,
  ).toBe("Keep this bid note.");
});
```

- [ ] **Step 4: Run** `npm run test:renderer -- src/WorkspaceSurface.test.tsx src/ManagerWorkspace.test.tsx -t "isolates a Work surface crash"`.
- Expected: FAIL because a child exception still escapes the current surface.
- [ ] **Step 5: Add two parent callbacks.** `returnToTenderOverview` clears only the focused action and navigates to `manager`. `openDoctor` opens Application Settings at `about`; keep the receipt in transient parent state only long enough to focus the matching Doctor finding.
- [ ] **Step 6: Replace the four inline view conditionals with one `WorkspaceSurface`.** Pass the existing `ManagerView`, `WorkView`, `TeamView`, and `FilesView` elements in a typed `Record<WorkspaceView, ReactNode>`, then wrap `WorkspaceSurface` in `SurfaceErrorBoundary` with boundary key `${selected.tender_id}:${view}`. Wrap Settings, retention, and recovery children separately. Do not put the title bar, Tender sidebar, workspace header, warning bar, operation feedback, or parent state inside a boundary.
- [ ] **Step 7: Run** the focused test and the existing draft/navigation tests: `npm run test:renderer -- src/WorkspaceSurface.test.tsx src/ManagerWorkspace.test.tsx -t "draft|navigation|surface crash"`. Expected: PASS.
- [ ] **Step 8: Commit** `feat: preserve workspace state across surface recovery`

---

### Task 5: Add the persistent four-state Doctor indicator

**Files:**

- Create: `src/DoctorIndicator.tsx`
- Create: `src/DoctorIndicator.test.tsx`
- Modify: `src/ManagerWorkspace.tsx`
- Modify: `src/ManagerWorkspace.css`
- Modify: `src/browserPreviewHost.ts`
- Modify: renderer fixtures that construct `QuantixDoctorReport`

**Interfaces:**

- Consumes: `ReadinessState` and `QuantixDoctorReport.readiness_state` from the health-and-repair plan.
- Produces:

```ts
export type DoctorIndicatorState =
  | "healthy"
  | "healing"
  | "needs_review"
  | "blocked";

export function doctorIndicatorState(
  readiness: ReadinessState,
): DoctorIndicatorState;
```

- Mapping: `ready -> healthy`, `healing -> healing`, `review_required | unavailable_external -> needs_review`, `blocked -> blocked`.

- [ ] **Step 1: Write the failing exhaustive mapping test.**

```ts
it.each([
  ["ready", "healthy", "Healthy"],
  ["healing", "healing", "Healing"],
  ["review_required", "needs_review", "Needs review"],
  ["unavailable_external", "needs_review", "Needs review"],
  ["blocked", "blocked", "Blocked"],
] as const)("maps %s truthfully", (readiness, state, label) => {
  expect(doctorIndicatorState(readiness)).toBe(state);
  render(<DoctorIndicator readiness={readiness} onOpen={vi.fn()} />);
  expect(screen.getByRole("button", { name: `Quantix Doctor: ${label}` })).toBeTruthy();
});
```

- [ ] **Step 2: Run** `npm run test:renderer -- src/DoctorIndicator.test.tsx`.
- Expected: FAIL because the component does not exist.
- [ ] **Step 3: Implement a small button indicator.** It shows the exact label, exposes `data-state`, and calls `onOpen`. It contains no finding list or repair button.
- [ ] **Step 4: Let `ManagerWorkspace` own one report snapshot.** Inspect on startup and when the selected Tender changes; replace it after relevant commands or settings close. A failed inspection must render `Needs review` with a local-only title such as “Doctor inspection unavailable”; it must not report Healthy.
- [ ] **Step 5: Place the indicator in the persistent sidebar footer immediately above Settings.** Clicking it calls `openSettings("about")`. The indicator remains available when a main surface is failed.
- [ ] **Step 6: Verify every report fixture uses `readiness_state` and no fixture or consumer has reintroduced `healthy`.** Update any new fixture introduced since the health slice, but do not add a compatibility adapter.
- [ ] **Step 7: Run** `npm run test:renderer -- src/DoctorIndicator.test.tsx src/ManagerWorkspace.test.tsx src/App.test.tsx`. Expected: PASS.
- [ ] **Step 8: Commit** `feat: expose persistent Doctor readiness`

---

### Task 6: Replace renderer-side repair loops with an exact repair summary

**Files:**

- Modify: `src/ApplicationSettings.tsx`
- Modify: `src/ApplicationSettings.test.tsx` (create if it does not yet exist)
- Modify: `src/ManagerWorkspace.test.tsx`
- Modify: `src/ManagerWorkspace.css`

**Interfaces:**

- Consumes from the health-and-repair slice:
  - `repairQuantixDoctor({ report_revision, code: "safe_repair_all", action: "run_safe_repairs", target: "application", tender_id })`
  - `QuantixDoctorReport.automatic_actions: AutomaticRepairResult[]`
  - `AutomaticRepairResult { action, scope, outcome, finding_code, before, after }`
- Preserves: the contract plan's direct `EngineerReviewRoute::ActivateSystemProfileRevision` review flow and every other domain-owned Engineer route.
- Removes: the renderer-maintained list of safe action strings and the loop that calls `repairQuantixDoctor` once per finding.
- Produces: a before/after repair summary for the single returned report.

- [ ] **Step 1: Write the failing test.** Seed a report with two automatic actions, click Repair all, and assert exactly one Host command and a readable summary:

```tsx
it("runs the Host allowlist once and shows every before and after result", async () => {
  const selectedTenderId = "11111111111111111111111111111111";
  host.repairQuantixDoctor.mockResolvedValue({
    revision: "report-2",
    readiness_state: "ready",
    findings: [],
    automatic_actions: [
      {
        action: "restart_diagnostics_writer",
        scope: "application",
        outcome: "repaired",
        finding_code: "diagnostics_writer_unavailable",
        before: "blocked",
        after: "ready",
      },
      {
        action: "remove_empty_residual_workspace",
        scope: "application",
        outcome: "not_needed",
        finding_code: "workspace_cleanup_failed",
        before: "ready",
        after: "ready",
      },
    ],
  });
  renderSettingsWithDoctor(initialReport);
  fireEvent.click(screen.getByRole("button", { name: "Repair all safe issues" }));
  fireEvent.click(screen.getByRole("button", { name: "Run safe repairs" }));
  await waitFor(() => expect(host.repairQuantixDoctor).toHaveBeenCalledOnce());
  expect(host.repairQuantixDoctor).toHaveBeenCalledWith({
    report_revision: "report-1",
    code: "safe_repair_all",
    action: "run_safe_repairs",
    target: "application",
    tender_id: selectedTenderId,
  });
  expect(screen.getByText("Diagnostics writer: blocked → ready")).toBeTruthy();
  expect(screen.getByText("No workspace cleanup was needed.")).toBeTruthy();
});
```

- [ ] **Step 2: Run** `npm run test:renderer -- src/ApplicationSettings.test.tsx -t "Host allowlist"`.
- Expected: FAIL because the renderer still loops through a hard-coded action list and does not render `automatic_actions`.
- [ ] **Step 3: Replace `repairAllSafeIssues`.** Make one `repairQuantixDoctor` call bound to the current `report_revision`, store the returned report, refresh runtime/settings once, and keep the returned automatic results visible until the next repair or Doctor inspection.
- [ ] **Step 4: Render fixed labels with an exhaustive `switch` over `AutomaticRepairAction` and `AutomaticRepairOutcome`.** Do not display raw enum values through `replace(/_/g, " ")`; TypeScript exhaustiveness must fail when a new action lacks user copy.
- [ ] **Step 5: Update the confirmation copy.** It states that only reversible operational state is eligible, canonical Tender records and provider Turns are unchanged, and Engineer decisions remain separate.
- [ ] **Step 5a: Prove Engineer routes remain outside safe repair.** Retain/add a fixture with `engineer_route: "activate_system_profile_revision"`; the route button remains available after the repair-summary refactor, and clicking “Repair all safe issues” never calls `activateTenderContract`. The contract-review confirmation remains the only path to that direct command.
- [ ] **Step 6: Add the idempotent second-run tests.** A current ready second inspection returns `automatic_actions: []` and must render “No safe repair was needed.” Also cover the race where an initially eligible owner returns `not_needed`; render its fixed no-op label and do not style it as failure.
- [ ] **Step 7: Run** `npm run test:renderer -- src/ApplicationSettings.test.tsx src/ManagerWorkspace.test.tsx`. Expected: PASS.
- [ ] **Step 8: Commit** `feat: show exact Doctor repair outcomes`

---

### Task 7: Verify renderer isolation, accessibility, privacy, and generated types

**Files:**

- Verify only; modify only files that fail a named assertion in this plan.

**Interfaces:**

- Verifies all Task 1–6 public interfaces together.

- [ ] **Step 1: Run focused Rust tests:**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib diagnostics --features runtime-fixture
cargo test --manifest-path src-tauri/Cargo.toml --lib renderer_diagnostic --features runtime-fixture
```

Expected: PASS.
- [ ] **Step 2: Run focused renderer tests:**

```powershell
npm run test:renderer -- src/SurfaceRecoveryPanel.test.tsx src/SurfaceErrorBoundary.test.tsx src/DoctorIndicator.test.tsx src/ApplicationSettings.test.tsx src/ManagerWorkspace.test.tsx
```

Expected: PASS. The crash test must prove the sidebar, Tender header, selected Tender, and Manager draft survive.
- [ ] **Step 3: Search for forbidden content-bearing diagnostic fields:**

```powershell
rg -n "RecordRendererDiagnosticCommand.*(message|stack|path|props|draft|surface)" src src-tauri
```

Expected: no matches.
- [ ] **Step 4: Run binding/type verification:** `npm run check`.
- Expected: PASS with no hand-edited declaration drift.
- [ ] **Step 5: Run the repository development gate:** `npm run verify`.
- Expected: PASS. Do not run `npm run build:desktop`.
- [ ] **Step 6: Inspect** `git diff --check` and `git status --short`; verify only this slice's intended source, test, CSS, and generated binding files are staged.
- [ ] **Step 7: Commit** `test: verify resilient production ux`
