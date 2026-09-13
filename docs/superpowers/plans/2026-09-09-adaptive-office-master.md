# Quantix Complete Adaptive Tender Office Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development or superpowers:executing-plans to execute the tasks in this file. Steps use checkboxes. This file is the single execution plan; do not split required details into new subsystem plans.

**Goal:** Deliver the complete agreed engineer-controlled Tender Office: a customizable Manager that dynamically forms staff, composes methods for unfamiliar requests, coordinates real work, retrieves evidence, calculates, researches, drafts, checks and recovers across the included Tender lifecycle.

**Architecture:** Extend the current Python/FastAPI/Pydantic/SQLite domain service, React workspace and Tauri desktop. Keep one authoritative Tender store and publication path; add focused versioned services, governed tool/skill/plugin boundaries, durable work records and typed work products. Provider execution retains the current direct SDK and permitted original-client routes; staff identities do not select credentials or create authority.

**Tech Stack:** Existing Python 3.12, Pydantic AI/provider SDKs, FastAPI, SQLite, React/TypeScript/Vite, Tauri, PDFium, openpyxl, python-docx, FastEmbed and NumPy. Exact installed locks remain the baseline. Additional libraries are capability-specific candidates with qualification tasks and acceptance gates in this file, not assumed working dependencies.

**Spec:** This consolidates docs/spec.md, docs/design/adaptive-office-scope.md, docs/design/live-dynamic-office.md, docs/design/workspace-redesign.md, docs/design/office-completeness-review.md, both 9 September research/concept reports, the 10 September coverage audit and the engineer's conversation decisions. The superseded fixed-staff foundation must not be executed. The earlier dynamic-staff plan is historical context; its remaining work and acceptance are included here.

**Plan revision:** Consolidated 10 September 2026. Source audit and documented prior evidence, not a claim that queued features have been implemented. This revision supersedes the earlier high-level content of this same master-plan file. Existing requirement/contracts documents remain factual implementation references; executors do not need another plan to know the work or acceptance.

## 1. Global constraints and settled decisions

- The primary agent owns architecture, integration, review, and verification. Inspect all changes.
- Prefer maintained libraries, official SDKs, and documented APIs. Validate before implementing. Keep components small and responsibilities clear.
- Build complete working increments; no compatibility layers, fake production behaviour, or speculative infrastructure.
- All product copy uses plain construction-engineering language. The main contact is the Tender Manager.
- Quantix's core UX principle is simple plain-language guidance with one clear next action. Keep details and advanced controls in More options while preserving every capability and actionable error.
- Preserve supplied Tender files. Do not commit customer documents, API keys, private extracted content, or runtime databases.
- Use source references for factual findings. Keep imported, extracted, analysed, and reviewed coverage distinct.
- Read docs/contracts.md before changing a shared interface. Update API schemas and generated frontend types together.
- User approval already covers architecture decisions, local implementation, installation of project dependencies, local verification, and reversible fixes. Continue work without routine permission questions.
- Normal application data belongs under ~/.quantix: database, imported copies, outputs, AI software/accounts, caches, logs, scratch work, connection records and supported WebView state. Source/dependencies/build artifacts stay in the project; OS-protected credentials and user-selected originals/exports remain separate. Explicit isolated backend roots are verification-only.
- Preserve direct API keys for OpenAI, Anthropic, Google, xAI and OpenAI-compatible BYOK/custom; preserve only documented/permitted ChatGPT/Codex and Grok subscription routes through their original official clients. Never copy OAuth tokens into API credentials, silently select another account/model/billing route or treat unknown cost as zero.
- Only the Tender Manager exists by default. All other names, roles, titles, personas, personalities, responsibilities, task briefs and requested capabilities are generated live for actual work. No roster, role enum, title-based routing, employee template or fake staff activity.
- The same fully customizable Manager identity serves all projects with separate Tender context/history. Staff persist only within their Tender unless deliberately approved knowledge is reused; no automatic staff copying to new Tenders.
- Saved workflows are optional methods, never a closed menu or mandatory wizard. The Manager can answer directly, reuse/adapt methods or compose a new plan. Missing inputs/capabilities produce precise gaps and useful supported work.
- The selected UI is Manager conversation beside the current document/result, expandable live office, distinct local illustrated AI portraits, important exchanges by default and complete retained details on demand. Motion reflects committed events; support keyboard, reduced motion, English UI and multilingual/Arabic content.
- Desktop/text first. Connected tablet access and push-to-talk are required later delivery tasks in this file. Close-to-tray continues authorized work while awake; explicit Quit stops safely. No automatic OS startup, wake lock, always-listening microphone or cloud hosting is implied.
- Ollama and research section 11 connector expansion are excluded: new Microsoft 365, Google Workspace, Autodesk/Procore/Speckle/CostX, live scheduling-system, supplier-system, ERP/accounting, dedicated tender-portal/feed, EC3 and company-automation integrations. Existing mail/import/export and public web research remain. Generic MCP/plugins cannot smuggle excluded presets into scope.
- BIM/IFC, model-based takeoff and BIM connectors are deferred. Supported 2D CAD/local document and schedule processing remain included.
- No release installers/packages, production deployment, real Tender approval or commercial sending as verification. Use synthetic records/recipients/provider boundaries for mutations. Live provider or native evidence must be labelled separately.
- Use affected backend/UI tests, generated declarations, frontend typecheck and actual-app visual journeys, including light/dark and scaling. Older README/no-tests passages are historical and superseded by current AGENTS.md.
- Do not overwrite pre-existing dirty source. Preserve the captured working baseline and hash-check integration. No blanket cleanup/reset of the engineer's workspace.

## 2. Honest starting point and completion boundaries

The existing first office increment supplies customizable/version-pinned Manager profiles, generated/versioned staff and work orders, isolated assignment context/source receipts, reviewed delegation, aggregate budgets, real messages/drafts, Stop/explicit Resume, event polling, portraits/desks and balanced UI. Previous verification reported 651 backend tests with one platform skip, 209 UI tests, typecheck, 26 native tests and five launcher tests; these are historical observations, not passing evidence for future changes.

Known first-increment limits are included as tasks: depth/concurrency currently 1/1; staff cannot generally read full handed-off staff-result payloads; older-root transfer needs deliberate revalidation; result notes are not continuing notebooks; retirement/reactivation is incomplete; root Resume is not step continuation; structured blind reviews and live steering are unfinished. Native tray/actual OS DPI acceptance remains separate from browser layout checks.

**Desktop gate:** T071 requires all mandatory desktop tasks and connected engineering stories. A missing required feature cannot be called complete because the UI has a polite unavailable message. Supported-format/platform exceptions must be explicit, but positive capability acceptance is required for each advertised support entry.

**Full agreed programme gate:** T075 remains the later close after T072 connected tablet and T073 push-to-talk. **Current execution (2026-09-10 override):** T072 tablet access and T073 microphone/push-to-talk are future delivery, not current remaining required work. T074 is the explicitly optional UX candidate register. Excluded integrations/BIM and optional framework/database choices are not unfulfilled requirements.

Statuses: **Implemented foundation** = preserve verified core, complete stated remaining acceptance; **Partial** = working primitives plus missing behavior; **Queued** = no completion claim; **Later accepted delivery** = required after desktop; **Optional candidates** = not a hidden expansion. Every task's acceptance checkbox starts unchecked for this consolidated gate; historical tests alone do not close new requirements.

### Current execution status correction — 10 September 2026

The captured task index and earlier execution entries that labelled T001–T013
"Implemented" are historical observations and are superseded as completion
claims by the runtime audits. The current status for T001–T013 is **Partial**:
focused services and regressions exist, but the task acceptance gates remain
open. T003's immutable lifecycle/replay repair, T004's assignment/source/actor
fences, and T012's persisted arithmetic review checks have focused evidence;
they do not close their full lifecycle, notebook handoff, independent-review,
UI, native or common-contract criteria. T013's direct/MCP/nested result fence
and Manager read-scope repairs have focused evidence. Its generic driver now
checks invalid numeric output rather than rejecting legitimate saved decision
fields; that example is not current programme proof.

The runtime still admits only depth 1 and concurrency 1. Work-graph, complete
handoff-store consumption and checkpoint continuation are not wired through the
full execution path. T024's reader reprocessing is partial and does not claim
that a derivative feeds complete RAG. T061 has no validated OS/VM sandbox; a
fixed demo subprocess or same-user venv cannot satisfy its isolation gate.
T014/T015 are the current exact reviewed skill/Methods increment: two bundled
1.1.0 methods are available, not all 39 planned skill rows or a complete
plugin catalogue/runtime. The 95 agentic results are real fixture/driver
evidence after repairs, not proof that all 75 tasks pass. The resumed increment's
full backend gate passed **819 tests with one existing platform skip**; its full
UI gate passed **233 tests**, with later focused UI recovery checks recorded in
the acceptance report. Typecheck, generated bindings and five launcher tests
passed. These gates validate this source increment, not all unchecked task
criteria. Historical exclusions
(BIM/IFC, Ollama and the section 11 connector expansion) and future tablet/voice
delivery remain unchanged.

**Resumed working increment:** [reviewed methods and foundation repairs](../../reports/reviewed-methods-resume-2026-09-10.md). Settings now supports real inspect/activate/disable and Tender overrides; work reviews select exact versions, and the actual Manager/staff direct and MCP bridges progressively load them with retained use evidence and publication revalidation. Immutable staff history, scoped notes, source-read commit boundaries, arithmetic review, PDF reprocess membership/cancellation and method backups were repaired. Bounded Astra reviews pass. Remaining tasks and acceptance checkboxes below remain authoritative; no new subsystem plan replaces them.

## 3. Execution ownership, order and working increments

Implementation uses Luna/xhigh workers and Astra/xhigh architecture/integration/review, following the engineer's model preference. The current environment supports three simultaneous children beside the primary. The earlier 50-item queue is historical; the 75 tasks and their checklists here replace it as the complete execution backlog. Do not claim 50 simultaneous agents. Dispatch only independent, bounded work with a concrete file/interface owner.

Shared files (api.py, jobs.py, office.py, models.py, repository.py, generated bindings, App.tsx and root dependencies) have one integration owner per increment. Workers modify their focused modules/tests and send integration changes to that owner. Two workers never edit the same shared file concurrently. Feature slices must include actual service/tool/UI wiring and acceptance; a directory, registry or prompt alone is not a working increment.

Dependencies below govern order, not task numbers or fixed engineering workflows. Run at most the supported concurrency from each ready set. An implementation task may be divided into reviewable checklist slices, but all details/status remain in this file. Preserve one final integration/review owner. Commits, if made under the chosen repository workflow, include reviewed source only; never commit the dirty baseline wholesale or private acceptance artifacts.

Each task consumes the named dependencies' records/services and the shared contracts below. Its proposed interface names are new design contracts unless explicitly marked retained. Update actual Pydantic schemas, generated TypeScript and docs/contracts.md together when implementing; never advertise a proposed endpoint before it is wired.

## 4. Shared data, authority and execution contracts

### 4.1 Common identity and version rules

IDs are opaque server-generated UUID hex strings; timestamps UTC ISO strings, with original local date/time plus IANA timezone retained for contractual deadlines. Decimal quantities/money cross JSON as strings; null means unknown, never zero. Currentness, authorship, review and lifecycle are separate fields. Reject unrecognized input fields and nonfinite values.

Every draft/derived record has id, tender_id, version, author identity/profile version, originating root/assignment, created_at, input/basis fingerprint, exact source/decision/method references and applicability. Models may choose professional content and references; they cannot mint server identities, grant versions, sender roles or approval receipts.

A source reference contains artifact ID/version/content hash, extraction/evidence version/hash, source ID, locator (page/region or sheet/cell range), occurrence/scope identity and actual inspected extent where relevant. Preserve original values and formulas. A newer extraction/source does not rewrite a prior result or establish contractual precedence.

Mutations use expected revision and/or exact displayed basis fingerprint plus an idempotency key. Same key/same normalized body returns its original receipt; same key/different body conflicts. List/read pages have bounded sizes and stable scoped cursors. Missing rows or truncated text are explicitly partial, never implied complete.

### 4.2 Trusted service context

All proposed service signatures use server-only ctx with the following contract:

```python
from dataclasses import dataclass
from typing import Literal

@dataclass(frozen=True)
class OfficeExecutionIdentity:
    tender_id: str | None
    actor_kind: Literal["engineer", "manager", "staff", "worker"]
    actor_id: str
    root_run_id: str | None
    budget_scope_id: str | None
    assignment_id: str | None
    profile_version: int | None
    route_binding_id: str | None
    instruction_revision_id: str | None
    grant_fingerprint: str | None
    ownership_epoch: int | None
    trusted_invocation_id: str | None
```

This is not a public Pydantic/HTTP argument model. Construct it from the authenticated engineer session or existing active controller/binding records. Engineer-only methods require the real engineer action/decision context; personality or provider text cannot impersonate it. A read-only inspection context cannot create work or authority. Reuse current authority services rather than add a second permission store.

Only authenticated global Settings/library inspection or configuration may have tender_id/grant_fingerprint absent. Every Tender data read, prompt, execution, handoff and publication requires a non-null server-resolved Tender identity and the applicable current grant. An absent value is never a wildcard. Global company/plugin configuration itself grants no access to Tender contents.

Request DTOs named in task signatures are narrow task-local Pydantic models. Their business fields and invariants are defined in the task's records, request contract and steps; common mutation metadata is expected_revision, expected_basis_fingerprint and idempotency_key where applicable. Reject fields irrelevant to the selected operation. Outputs use the declared record shape plus common provenance/version metadata. A task's implementer must publish its exact schema before a dependent task consumes it.

### 4.3 Authority, budgets and transactions

Keep existing connections.authority_guard before repo.atomic. No await, provider/network call or asynchronous worker wait inside authority/publication SQL transactions, including schema initialization. Validate current scope/route/basis, reserve/claim, commit, perform external work, then revalidate ownership/authority and commit results/events. All committed state changes and their operation receipts/events are atomic.

One explicit engineer work scope owns aggregate requests, money, search, depth, staff/assignment/concurrency and resource limits. Descendants, reviews, retries and continued steps inherit/intersect it; they cannot regain allowance by creating another child. Explicit new engineer work can create a separately reviewed root under current authority. Resume retains lineage and outstanding uncertain usage; it is not free work.

T009 makes the accounting owner explicit as `BudgetScope {id, origin_kind:engineer_request|watch_activation, origin_ref, grant_fingerprint, limits, reserved, reported, uncertain, revision}`. This is an accounting extension of the existing authority, not another grant store. `root_run_id` identifies an execution attempt; `budget_scope_id` remains stable across that work's descendants and checkpoint continuations. Existing usage rows migrate with their original root as the initial budget scope; preserve their run attribution. A new explicit request or reviewed top-up can establish/change an allowance, while retry/resume alone cannot reset it. T013 may initially resolve the existing root accounting identity; T009 supplies durable grouping across later attempts and watches.

Count distinct generated staff identities separately from assignments. Reusing one colleague for three tasks consumes one distinct-staff allocation and three assignment allocations; retirement/deletion must not erase consumption or refund already spent work. Existing approval receipts cannot gain new skill/tool/plugin/delegation power through a software update. Preserve old receipts readably and require a current review for a material authority expansion.

Binding pins actual account/model/version/capability/readiness/billing/reasoning/search/output settings and permitted alternatives. Recheck at admission, dispatch/lease and publication. Account/PDFium constraints remain real resource leases. Stop durably revokes new publication/reservations before returning, while late attributable accounting and cleanup remain tracked.

Drafts, notes, scenarios, handoffs and staff checks are not accepted quantities/rates/assumptions or released bids. Only the existing authoritative publication/decision path changes accepted domain records. Commercial sending and final release retain exact displayed scope and separate explicit engineer action. A delivery acknowledgement is not inspection; inspection is not checking; checking is not acceptance.

### 4.4 State machines and recovery

- Staff identity lifecycle: available, retired, archived. Assignment activity (queued/running/waiting/completed/failed/cancelled/interrupted) is derived separately; retirement cannot orphan active work.
- Work ownership: unclaimed → leased(epoch) → completed/released/expired. Only current owner/epoch may publish; late usage from old owners remains attributable.
- Step execution: prepared → reserved → running → completed, or failed/interrupted/uncertain. Only completed durable outputs with matching fingerprints can be reused.
- Handoff: delivered → acknowledged → inspected extent. Review/engineer decision references provide checked/accepted facts separately.
- Review: proposed → active → waiting_evidence/resolved/needs_engineer/exhausted/cancelled. Preserve initial reviewer conclusion and later contributions.
- Skill/plugin/method versions: draft/inspected/evaluated/active/disabled/withdrawn states as applicable; immutable identity/hash and historical references survive changes.
- External effect: prepared → attempted → confirmed or uncertain. A timeout cannot safely be recast as unattempted; reconcile before retry.
- Watcher: proposed → active/paused/exhausted/expired/stopped, with last successful check and missed intervals.
- Restore/startup: reconstruct saved state, mark abandoned work interrupted, revalidate before continuation; never auto-renew authority or blindly repeat external effects.

### 4.5 API and presentation rules

Preserve authenticated /api loopback routes and HashRouter workspace. New domain reads live under /api/tenders/{tid}/office, /staff, /work-products or their focused existing feature router; application capability/company-library settings remain separate from Tender authority. Proposed routes are not migration shortcuts around current endpoints. Do not expose the desktop bearer remotely or in a URL.

Use schema-derived field errors and typed blockers with record-specific repair targets. Invalid input returns 422, stale revision/fingerprint or conflicting replay 409, unauthorized scope 403/opaque 404 as appropriate, missing capability 409/503 with a specific next action. Do not leak record existence, secrets or private input in errors. Read GET creates no staff, grants, model calls or side effects.

Office polling remains bounded/authenticated until a tested cancellable transport change is justified. Cursor gaps/instance changes reset history baselines and animation. Pagination covers full retained staff/profile/order/assignment/result/receipt/message/note/review histories; a snapshot length is not a history offset. Hidden-window animation may pause while work continues. Disconnection preserves last-known data with an honest state.

### 4.6 Migration and runtime ownership

Each schema addition has a numbered transactional migration, indexes and foreign-key/idempotency constraints, populated-upgrade and fresh-home fixtures, and failure rollback. Take/verify a workspace backup before normal-home migration and activate only at an appropriate idle boundary. Preserve uncertainty, revoked authority and more recent delivery history when restoring older data.

All new records/objects/library caches/worker profiles/temp/audio/device sessions belong in the normal home and its ownership inventories; secrets stay in supported protected stores. Include every new resource in backup, restore, reset, retention and worker shutdown tests. Do not delete originals/exports or follow archive/reparse paths outside owned roots.

Uninstalling a capability may remove its executable/runtime, but each retained result keeps its source/input snapshot references, code or formula, method/version, package/library hashes and check record. Reproduction needing removed software offers installation of the exact supported version after review; it must not silently run a newer method or claim the dependency is still installed. Historical outputs remain inspectable regardless of runtime availability.


### 4.7 Initial technical bounds and conflict behavior

These are editable capability defaults for new surfaces, not hardcoded employee counts or engineering workflows. Preserve stricter existing bounds. A task may request a larger bound only within a reviewed capability/resource envelope, and its actual effective limits are saved in the receipt.

| Boundary | Initial rule |
| --- | --- |
| Generic history/list endpoints | Default 50, maximum 200 items; existing smaller staff/event/source limits remain. Return a continuation cursor/partial state rather than truncate silently. |
| General work products | Page data rather than place entire large tables in model context. Record exact total rows and selected extent; downloads preserve the full admitted artifact. |
| Selected web capture | 30-second total timeout, at most five redirects, at most 10 MiB response body before extraction; reject decompression growth beyond the same admitted bound. Check every resolved destination and redirect. |
| New plugin/MCP tool catalogue entries | Bounded schema/description (64 KiB per definition initially), explicit namespace/version and finite result limits; larger returned work uses an artifact reference and scoped reader. |
| New generic JSON tool result | Initial 1 MiB serialized payload; return a complete bounded page or artifact reference with remaining extent, never silently omit rows required by a calculation. |
| Restricted composition | Initial 30-second wall time and 64 MiB interpreter memory where enforceable, plus approved nested-call/root budgets; refuse the capability if its chosen interpreter cannot enforce its declared bounds. |
| Full Python analysis | Initial requested ceiling 120 seconds, 4 GiB guest memory and 64 MiB imported outputs, with declared CPU/process limits enforced by the qualified provider; a provider unable to enforce the reviewed profile is unsupported for that profile. |
| Watcher catch-up | One revalidated catch-up per watch after an unavailable interval, with missed interval count; no unbounded replay of every missed trigger. Frequency/root spend remain explicit approval fields. |
| Device pairing | One-time code valid five minutes, at most five failed attempts per pairing; device sessions are independently revocable. Code is not a bearer credential and cannot authorize data access without desktop confirmation. |
| Push-to-talk | Initial 60-second clip limit within checked provider/media limits; explicit stop/cancel, no background microphone and no transcript auto-send. |
| Staff execution | Manager chooses zero or more useful staff. Effective depth/concurrency is bounded by the reviewed envelope, available resources and account leases; no fixed employee roster or target count. |

Every public mutation handles validation failure, stale version, lost scope, unavailable capability, cancellation, retry and partial outcome explicitly. A 409 preserves the user's input and returns the current repair/review target; reloading must not silently submit old approval. Idempotent completed receipts remain readable without re-executing work; altered replay conflicts. Important warnings/errors remain outside collapsed advanced details.

## 5. Test contracts, commands and evidence

Every task includes a concrete regression example using the test-only run_case fixture from T001. Its inputs are scenario seeds, not public API credentials or production constants. T001 defines CaseRegistry/CaseDriver; the owning task adds its driver in backend/tests/agentic/cases.py or a feature-owned imported test driver module. The driver creates synthetic data through real services/routes and returns observed state. Domain/storage/permission/calculation implementations are not mocked; only provider/SMTP/clock/OS boundaries may be controlled where the particular test is not a native/live acceptance case. A missing driver fails visibly.

```python
# Test-only contract, implemented by T001; never product routing.
from collections.abc import Callable
from typing import Protocol, TypedDict
import pytest

class CaseEnvironment(Protocol):
    client: object       # authenticated real TestClient for the isolated app
    repo: object         # real Repository using the disposable home
    provider: object     # controlled provider boundary with recorded calls
    clock: object        # deterministic clock where time is a fixture

class CaseResult(TypedDict, total=False):
    scenario: str

CaseDriver = Callable[[CaseEnvironment, dict], dict]

class CaseRegistry:
    def __init__(self):
        self.drivers: dict[str, CaseDriver] = {}

    def register(self, case_id: str, driver: CaseDriver) -> None:
        if case_id in self.drivers:
            raise ValueError("Duplicate acceptance case")
        self.drivers[case_id] = driver

    def run(self, env: CaseEnvironment, case_id: str, inputs: dict) -> dict:
        if case_id not in self.drivers:
            raise LookupError("Acceptance driver is not implemented")
        return self.drivers[case_id](env, inputs)

@pytest.fixture
def run_case(case_environment, case_registry):
    return lambda case_id, inputs: case_registry.run(
        case_environment, case_id, inputs
    )
```

case_environment is a test fixture built by T001 from the existing real temporary-root API/repository test setup; it closes diagnostic writers, workers and files on teardown. case_registry is populated only by implemented drivers. Drivers must not read asserted expected-output values to manufacture success, mark UI/native observations without exercising that surface, or auto-approve a real Tender.

For every task: write the failing regression/driver assertions first, run and observe the meaningful failure; implement the smallest complete feature slice, then pass the focused checks and independent review. Existing behavior may start green; add a new gap/failure fixture before modifying it. Do not make low-value tests that simply mirror implementation. A task may not be marked complete until all listed acceptance conditions, not just the sample, pass.

Windows source-root commands (use backend/.venv/bin/python equivalents on macOS/Linux):

```powershell
$env:PYTHONPATH = (Join-Path (Get-Location) 'backend')
& .\backend\.venv\Scripts\python.exe -m pytest backend/tests/agentic/test_t001.py -q --tb=short
npm run check:ui
node scripts/test-ui.mjs src/features/office/StaffDesk.test.tsx
node --test scripts/running-workspace.test.mjs
cargo test --manifest-path src-tauri/Cargo.toml
```

Replace test_t001.py with the task's exact listed test file. Every changed UI feature gets its adjacent .test.tsx/.test.ts file and actual browser/native journey when relevant. Backend-only tasks need no artificial frontend test. Run npm run bindings only after confirming reset recovery is not pending. The current exporter creates the API against an explicit temporary home under ~/.quantix/runtime/tmp and writes the schema to ~/.quantix/runtime/openapi.json; preserve that separation and verify the imported service source. T001 must make pending-reset refusal occur before creating ordinary schema-export directories/services. Never open the normal Tender database to export schema or write runtime data into the repository. For final integration use the full affected suites; no build:desktop/package:service/release packaging.

Evidence entry per task in this file: source revision/hashes; date; test command/result; fixture/version; synthetic versus live; actual browser/native/OS environment; saved artifact references; review findings/fixes; limitations; final status. Do not replace evidence with an agent's unverified completion message.

## 6. Detailed task backlog

The numbered tasks below are engineering delivery units, not Quantix's product workflows. Optional input requests during a task must be precise and nonblocking work should continue; never ask another broad product questionnaire for settled choices.

### Task index

| ID | Deliverable | Initial status | Depends on |
| --- | --- | --- | --- |
| [T001](#t001) | Protect the working baseline and establish executable acceptance fixtures | Partial 2026-09-10 — fixtures/reset guard and migration evidence; full common acceptance remains open | None |
| [T002](#t002) | Close Manager/profile and dynamic-generation foundation acceptance | Partial 2026-09-10 — profile/generation and portrait evidence; full UI/native/live acceptance remains open | T001 |
| [T003](#t003) | Complete staff lifecycle and colleague working preferences | Implemented 2026-09-10 — lifecycle/preference repair plus Active/History roster filter and reuse action; native-window/live acceptance not claimed | T002, T013 |
| [T004](#t004) | Add persistent, scoped staff notebooks and context reconstruction | Implemented 2026-09-10 — scoped notebook repair plus desk Work-notes views, kind/current page filters and reference navigation; native-window/live acceptance not claimed | T003 |
| [T005](#t005) | Enable full exact handoff consumption and revalidated prior-work reuse | Implemented 2026-09-10 — staff read_handoff tool, desk list/viewer, transfer/review/replay fencing; native-window/live acceptance not claimed | T004 |
| [T006](#t006) | Complete office message delivery, acknowledgements and ownership exchanges | Implemented 2026-09-10 — durable delivery/required-response reads with desk display; scheduler epochs stay with T008; native-window/live acceptance not claimed | T005 |
| [T007](#t007) | Persist adaptive work graphs and instruction-bound dependencies | Implemented 2026-09-10 — persistence/revision plus status derivation and latest-graph read; runtime wiring stays with T008/T009 | T006, T013 |
| [T008](#t008) | Allow bounded dynamic descendants with fenced ownership | Implemented 2026-09-10 — real child assignments, envelope depth caps, ownership epochs, serial execution; concurrency stays with T009 | T007 |
| [T009](#t009) | Run independent staff concurrently with aggregate resource accounting | Implemented 2026-09-10 — concurrent drain with reservations, per-account locks, prerequisite gating; live original-client load remains unverified | T008, T013 |
| [T010](#t010) | Separate status replies from execution and implement safe steering | Implemented 2026-09-10 — steering admission with turn-boundary application and cancel-through-Stop; native-window/live acceptance not claimed | T007, T009 |
| [T011](#t011) | Implement step checkpoints, selective continuation and effect reconciliation | Implemented 2026-09-10 — lineage-scoped reuse with records, effect state machine, resume surfacing; native-window/live acceptance not claimed | T010 |
| [T012](#t012) | Build bounded review sessions and reproducible independent checks | Implemented 2026-09-10 — discussion/resolution lifecycle with review-room API and desk UI; native-window/live acceptance not claimed | T005, T006, T007, T009 |
| [T013](#t013) | Unify versioned capability and tool contracts | Implemented 2026-09-10 — execution identity, dispatch fences, catalog; re-verified with adjacent suites (combined T001/T002/T013 + tool tests pass); backend-only, no live provider | T001 |
| [T014](#t014) | Create the versioned skill package library and validation | Partial 2026-09-10 — focused package evidence only; backup/scope review repairs and full checklist/native/live evidence remain | T013 |
| [T015](#t015) | Load and execute engineering skills progressively across providers | Partial 2026-09-10 — reviewed Methods UI increment only; two bundled 1.1.0 methods, no full runtime/plugin catalogue, and full checklist/native/live evidence remain | T014 |
| [T016](#t016) | Compose methods, activate company packs and learn checked corrections | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T007, T015 |
| [T017](#t017) | Add general versioned work products and safe task-specific surfaces | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T005, T013 |
| [T018](#t018) | Create isolated estimate and procurement scenario workspaces | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T017, T032 |
| [T019](#t019) | Build bounded local watchers and meaningful notifications | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T009, T010, T011 |
| [T020](#t020) | Model the Tender profile, measurement basis and engineering calendar | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T001, T013 |
| [T021](#t021) | Build the controlled company capability and document library | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T020, T014 |
| [T022](#t022) | Deliver public opportunity research, qualification and bid/no-bid briefs | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T020, T021, T040 |
| [T023](#t023) | Complete document classification, drawing registers and package reconciliation | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T001, T020 |
| [T024](#t024) | Add extraction lineage, OCR and supported reader reprocessing | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T023 |
| [T025](#t025) | Strengthen spreadsheet integrity and structured BOQ interpretation | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T023 |
| [T026](#t026) | Create structural evidence chunks and project vocabulary | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T024, T025, T020 |
| [T027](#t027) | Implement measured hybrid retrieval, bounded reranking and bilingual search | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T026 |
| [T028](#t028) | Enforce knowledge collections and evidence relationship traversal | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T021, T027 |
| [T029](#t029) | Validate claim support, contradictions and contractual precedence | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T027, T028, T012 |
| [T030](#t030) | Propagate revision, decision and assumption impact | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T028, T029, T032 |
| [T031](#t031) | Complete scope, responsibilities, compliance and interface review | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T023, T028, T029, T030, T015 |
| [T032](#t032) | Create the reproducible unit-aware calculation core | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T013, T020 |
| [T033](#t033) | Complete calibrated measurement and supplied-quantity reconciliation | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T025, T032 |
| [T034](#t034) | Add assemblies, measurement rules, deductions and assisted repetition | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T033, T024, T015 |
| [T035](#t035) | Validate supported 2D CAD and legacy document conversion | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T024, T032, T033 |
| [T036](#t036) | Extend rate build-ups, productivity resources and preliminaries | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T032, T020, T041 |
| [T037](#t037) | Complete commercial bases, price provenance and estimate completeness | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T036, T031, T041 |
| [T038](#t038) | Explain cost movement and evaluate value-engineering alternatives | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T018, T030, T037 |
| [T039](#t039) | Model cash flow, purchasing timing and negotiation scenarios | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T018, T037, T038, T047 |
| [T040](#t040) | Add reusable research briefs and a governed search/content gateway | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T013, T020 |
| [T041](#t041) | Persist complete market observations and claim-level commercial evidence | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T040, T029, T032 |
| [T042](#t042) | Build comparable market intelligence and refresh priorities | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T041, T019, T018 |
| [T043](#t043) | Research and verify suppliers against explicit evidence | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T040, T041, T021 |
| [T044](#t044) | Complete RFQ scope composition while preserving exact sending authority | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T031, T043, T017 |
| [T045](#t045) | Associate replies and extract quotation lines without losing originals | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T044, T024, T025 |
| [T046](#t046) | Deliver technical comparison and commercial quotation levelling | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T045, T041, T032, T029 |
| [T047](#t047) | Connect procurement lead times, validity and follow-up to programme needs | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T046, T020, T019, T048 |
| [T048](#t048) | Extend programmes with productivity, calendars and resource checks | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T020, T031, T032 |
| [T049](#t049) | Add validated local schedule-file interoperability | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T048, T058 |
| [T050](#t050) | Produce complete technical, method, quality and HSE drafts | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T015, T021, T031, T017 |
| [T051](#t051) | Expand structure-preserving client-form population | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T025, T050 |
| [T052](#t052) | Deliver bilingual evidence, terminology and document quality | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T026, T050, T051 |
| [T053](#t053) | Add source-backed sustainability options using supplied factors | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T032, T041, T038 |
| [T054](#t054) | Complete submission requirements and cross-document consistency | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T030, T050, T051, T048 |
| [T055](#t055) | Deliver submission rehearsal, exact release manifests and transmission records | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T054, T012, T044 |
| [T056](#t056) | Support post-tender clarifications and complete award handover | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T055, T039, T030 |
| [T057](#t057) | Implement win/loss review and matched estimated-versus-actual learning | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T056, T038, T016, T021 |
| [T058](#t058) | Implement curated plugin packages and capability lifecycle | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T013, T014 |
| [T059](#t059) | Add scoped MCP clients and separately authorized outward tools | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T058, T013, T062 |
| [T060](#t060) | Pilot bounded code composition over approved tools | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T013, T015, T058, T032 |
| [T061](#t061) | Implement a validated full-Python isolation provider | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T060, T058, T062, T067 |
| [T062](#t062) | Unify authority, outbound-data records, diagnostics and resource accounting | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T013 |
| [T063](#t063) | Complete the balanced Manager workspace and interaction continuity | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T010, T017, T020 |
| [T064](#t064) | Complete the live office, staff desks, relationships and motion | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T003, T004, T005, T006, T007, T012, T063 |
| [T065](#t065) | Finish engineering tables, coverage measures and the decision inbox | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T025, T030, T037, T046, T054, T063 |
| [T066](#t066) | Build one coherent capability, method and connection Settings experience | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T015, T016, T058, T059, T060, T061, T062, T063 |
| [T067](#t067) | Verify native desktop lifecycle, storage, backup, restore and reset | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T002, T011, T058 |
| [T068](#t068) | Establish engineering benchmarks and evidence-backed staff quality | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T001, T012, T015, T027, T029, T032, T041, T048, T057 |
| [T069](#t069) | Prove the connected Tender lifecycle with exact acceptance stories | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T022, T031, T034, T035, T037, T039, T042, T046, T047, T050, T051, T052, T053, T055, T056, T057, T064, T065 |
| [T070](#t070) | Validate security, concurrency, recovery and realistic performance | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T009, T011, T024, T027, T058, T059, T060, T061, T062, T067 |
| [T071](#t071) | Close the desktop/text delivery gate with traceable evidence | Partial 2026-09-10 — T001 regression passed; gate correctly refuses a complete claim | T049, T066, T068, T069, T070 |
| [T072](#t072) | Deliver opt-in connected tablet access with one authoritative office | Later accepted delivery — future; not current remaining required work | T071, T010, T062, T067 |
| [T073](#t073) | Deliver push-to-talk with editable transcripts and explicit data routing | Later accepted delivery — future; not current remaining required work | T071, T013, T062, T063 |
| [T074](#t074) | Keep remaining optional UX ideas explicitly accounted for | Partial 2026-09-10 — T001 regression passed; full checklist/native/live evidence remains | T063, T064, T065 |
| [T075](#t075) | Close the complete agreed programme after later device and voice delivery | Later accepted delivery — waits for future tablet and microphone; desktop/current acceptance also remains open | T071, T072, T073 |

<a id="t001"></a>

### T001: Protect the working baseline and establish executable acceptance fixtures

**Historical status:** Previously recorded as Implemented 2026-09-10. **Current status:** Partial — focused fixtures/reset guard and migration evidence; full common acceptance remains open. **Dependencies:** None. **Coverage:** AO03, AO13; C02/C14/C36/C39; research phase 0.

**Files and responsibility:** Modify `backend/tests/conftest.py`; create `backend/tests/agentic/conftest.py`, `backend/tests/agentic/cases.py`, `backend/tests/agentic/fixtures.py`; inspect `docs/progress.md`, `backend/quantix/db.py`; modify `scripts/export_openapi.py` for the pending-reset guard.

**Test owner:** Create `backend/tests/agentic/test_t001.py`; register driver `T001` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** Test-only CaseRegistry.register(case_id: str, driver: CaseDriver) -> None; run_case(case_id: str, inputs: dict) -> dict. CaseDriver receives an isolated real FastAPI application, repository, synthetic inputs, fake clock and controlled provider boundary; it must exercise the actual feature and return observed records.

**Request/record details:** Test-only scenario seeds; isolate home, clock, synthetic files, controlled provider/mail boundaries and fault timing. No production request schema or task-name routing is added. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [x] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [x] 1. Capture dirty tracked/untracked source hashes and current contracts; create an isolated working-source copy for execution and compare hashes before integration. Never copy customer/runtime data into source.
- [x] 2. Create reusable synthetic PDF/DOCX/XLSX/XLSM, quotation, drawing, source-revision and provider fixtures; label all example employees as generated test data. Register one real driver per task when that task is implemented.
- [x] 3. Create pre-migration home backup/restore verification using disposable homes; use serial reversible schema migrations, compatibility revisions and a rollback decision that restores source plus matching data together.
- [x] Add a pending-reset guard to scripts/export_openapi.py before prepare_process_environment or create_app; a reset-recovery fixture must prove schema export refuses without recreating the ordinary home. Retain the existing isolated TemporaryDirectory API export and normal runtime schema destination.
- [x] 4. Record current test failures separately; adapt obsolete test boundaries without restoring removed production APIs. Keep plan task statuses and evidence in this file.
- [x] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [x] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t001(run_case):
    result = run_case("T001", {"scenario": "isolated_baseline", "tenders": 2, "automatic_ai": False})
    assert result['specialists_per_tender'] == [0, 0]
    assert result['original_hashes_preserved'] is True
    assert result['provider_requests'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t001.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [x] T001.A1: Missing case IDs raise an error; a driver returning its expected fixture without exercising the service is rejected in review.
- [x] T001.A2: A synthetic import preserves source bytes and starts no AI. A second fresh Tender has zero specialists.
- [x] T001.A3: An altered original working file blocks integration instead of being overwritten; failed schema migration leaves the prior database usable.
- [x] T001.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** 2026-09-10. Synthetic execution on Windows (PowerShell). Command: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t001.py -q --tb=short` → 12 passed. Adjacent: `test_backup.py` and `test_factory_reset.py` with the T001 re-run → 60 passed, 1 existing platform skip. Ruff check on the T001 paths passed. No live provider, browser or native tray journey; T001 is backend-only. Schema export still writes `~/.quantix/runtime/openapi.json` from an isolated TemporaryDirectory and refuses a pending-reset journal before `prepare_process_environment`. Failed forward migrations restore the prior SQLite file; `PRAGMA user_version` remains 1. Working-source integration compares captured SHA-256 hashes and refuses to overwrite an independently edited file. SHA-256 of the T001 sources: `backend/quantix/db.py` 6150df569bce9c87411601b89e12d69cc5492781b591f6448eaa60c217dc44db; `scripts/export_openapi.py` 5abcbc0b173359b7dbe8f57a4b91065ced2d23b9637d497384c6084ae83d3be3; `backend/tests/agentic/test_t001.py` b3376b822f880b82e29294b53c8170acb7ad9d58b389488d8985ca237c013859; harness `cases.py` a9b6d280108e570c848fc8860f6a9d216b7dbfab62200ecad1328974d007ca5a, `fixtures.py` 167030303b06d12ee0a88a6e1f88bf03f46e56d27280381404653ca40cde8722, `baseline.py` d763a7e75b72c8a9ae26b58fad867f97bad49399fce0df711a42c40607a955fe. Review fix: schema export restores process temp directories so later tests are not left pointing at a deleted isolated home; DOCX fixture labels are asserted through python-docx, not compressed ZIP bytes. Limitations: no actual-app visual journey and no live provider observation. Status: implemented.

<a id="t002"></a>

### T002: Close Manager/profile and dynamic-generation foundation acceptance

**Historical status:** Previously recorded as Implemented 2026-09-10. **Current status:** Partial — focused profile/generation and portrait evidence; full UI/native/live acceptance remains open. **Dependencies:** T001. **Coverage:** AO03/AO19/AO20; C02–C05/C26; concept §§1–3.

**Files and responsibility:** Modify `backend/quantix/manager_profile.py`, `backend/quantix/manager_runtime.py`, `backend/quantix/staff_models.py`, `backend/quantix/staff_generation.py`, `backend/quantix/staff_store.py`; `src/features/ManagerPersonality.tsx`, `src/features/StaffPortrait.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t002.py`; register driver `T002` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** Retain ManagerProfileService editing with expected_version and StaffStore.create_generated/revise_generated. Complete profile contains name, role, title, specialisms, persona, personality, initiative, uncertainty handling, habits, responsibilities, objectives, success criteria, deliverables, context needs and requested skills/tools; server supplies identity/version/lineage only.

**Request/record details:** ManagerProfileEdit: expected_version and full editable professional/personality fields. StaffProfileDraft/StaffWorkOrder: generated professional fields, source IDs and requested capabilities; no client IDs, grants or sender identity. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [x] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [x] 1. Run current Manager/version/creation receipt tests; extend with unfamiliar names, long titles, Arabic and arbitrary capability combinations without any role enum or name-to-route lookup.
- [x] 2. Verify global Manager identity and full editable personality with per-Tender history; pin a profile version at run admission across conversation-to-engineering transitions.
- [x] 3. Preserve local portrait identity through rename, archived history and restore; show generated creation reason and no fictitious human qualifications.
- [x] 4. Exercise setup/no-account/unsupported-tool states in the actual UI; inspection and personality editing must never start staff or provider work.
- [x] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [x] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t002(run_case):
    result = run_case("T002", {"scenario": "manager_profile_pin", "edit_during_run": True, "role": "Facade procurement timing analyst"})
    assert result['run_profile_version'] == 1
    assert result['current_profile_version'] == 2
    assert result['cross_tender_leaks'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t002.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [x] T002.A1: Two Tenders share one Manager profile identity but no source/history leak; an in-flight run retains version 1 after version 2 is saved.
- [x] T002.A2: A new role string executes through its approved capabilities, not a hardcoded title; unsupported skills remain concrete blockers.
- [x] T002.A3: 409 conflict preserves the user's draft; no portrait network call; failed portrait rendering does not block work.
- [x] T002.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** 2026-09-10. Existing Manager/staff foundation was retained; no production Manager/staff modules needed behaviour changes. New T002 driver exercises the real API, conversation-to-engineering transition, live `create_staff` tool, unsupported skill/tool blockers, stale 409 profile save and local `notionists-v1` portrait seed. Command: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t002.py -q --tb=short` → 3 passed. Adjacent backend: manager profile/runtime, office integration and staff generation with T001/T002 → 41 passed. UI: `node scripts/test-ui.mjs src/features/ManagerPersonality.test.tsx src/features/StaffPortrait.test.tsx` → 18 passed (409 draft kept, local SVG portraits, failed-render fallback, Arabic names). SHA-256: `backend/tests/agentic/test_t002.py` 5272e9d776c372534d5b67db967cc318dc0c0848ef0ecdf87acd3a712c0e38b4; `cases.py` 23d88173d6445975ad9a4e062624d5019567a3691863682fd792b97c12c4ff6e; `fixtures.py` a1e3e259b1cc9419607836cfb1fae669fe74432ed49b4c066a0fde6d57f41552. No live provider and no native-window visual journey. Status: implemented.

<a id="t003"></a>

### T003: Complete staff lifecycle and colleague working preferences

**Historical status:** Previously recorded as Implemented 2026-09-10. **Current status:** Implemented 2026-09-10 — immutable lifecycle/replay repair plus Active/History roster filter and one clear reuse action with focused backend/UI evidence; native-window and live-provider acceptance remain open. **Dependencies:** T002, T013. **Coverage:** AO03/AO16; C04; concept §3/§8.

**Files and responsibility:** Create `backend/quantix/staff_lifecycle.py` and `backend/quantix/staff_lifecycle_models.py`; modify `backend/quantix/staff_store.py`, `backend/quantix/staff_generation.py`, `backend/quantix/staff_routes.py`; `src/features/office/StaffDesk.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t003.py`; register driver `T003` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** StaffLifecycleService.transition(ctx, StaffLifecycleRequest{staff_id,expected_version,target:available|retired|archived,reason,idempotency_key}) -> StaffProfileRecord; revise_preferences(ctx, StaffPreferenceRequest{staff_id,expected_version,preferences,applies_from}) -> StaffProfileRecord. Task activity remains a separate derived state.

**Request/record details:** Transition: staff_id, expected_version, target lifecycle, reason, idempotency_key. Preference revision: staff_id, expected_version, text/structured preferences and subsequent-work or explicit safe-boundary applicability. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [x] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [x] 1. Migrate the existing planned lifecycle to a documented available identity state without changing assignment status or creating work.
- [x] 2. Implement Manager-authorized retire/archive/reactivate and engineer preference requests through Manager coordination; deny retirement of active assignments unless a reviewed stop/transfer has completed.
- [x] 3. Keep profile and portrait versions, historical speakers, evidence and work orders intact. Apply preference changes to subsequent or explicitly revised work, never mutate an in-flight brief.
- [x] 4. Add active/history filters and one clear reuse/reactivate action; archive is not deletion.
- [x] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [x] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t003(run_case):
    result = run_case("T003", {"scenario": "retire_active_then_reactivate"})
    assert result['busy_retirement_blocked'] is True
    assert result['reactivation_provider_requests'] == 0
    assert result['identity_preserved'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t003.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [x] T003.A1: Retiring a busy colleague returns an actionable conflict and leaves ownership intact.
- [x] T003.A2: Reactivate makes no provider request and grants no route/budget; the same portrait/history remains.
- [x] T003.A3: Be shorter; keep source references changes future communication preferences without weakening evidence or permission requirements.
- [x] T003.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** 2026-09-10. Backend lifecycle/preference behavior retained from the reviewed increment; this continuation closed the remaining UI acceptance. `src/features/office/LiveOffice.tsx` adds an Active colleagues / History roster filter bound to the existing `GET /tenders/{tid}/staff?lifecycle=available|history` page, auto-loads the first server page on filter switch, and exposes one `Reactivate for later work` action per retired/archived card (409 conflict text preserved, selection kept). Archive is labelled as not deletion. No Pydantic/HTTP schema changed, so generated bindings are untouched. Commands: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t003.py backend/tests/test_staff_api.py backend/tests/test_staff_lifecycle_versions.py backend/tests/test_staff_retired_admission.py -q` → 14 passed; `node scripts/test-ui.mjs src/features/office/LiveOffice.test.tsx` → 6 passed (2 new: history filter + reactivate, conflict keeps card); `node scripts/test-ui.mjs src/features/office/StaffDesk.test.tsx` → 11 passed; `npm run check:ui` (tsc --noEmit) → exit 0; Prettier --write applied to the three touched frontend files. No live provider, native-window tray journey, or real-app light/dark visual walkthrough in this session; no release package, commit, real Tender approval or commercial send. Status: implemented.

<a id="t004"></a>

### T004: Add persistent, scoped staff notebooks and context reconstruction

**Historical status:** Previously recorded as Implemented 2026-09-10. **Current status:** Implemented 2026-09-10 — scoped notebook/context repair plus paged desk Work-notes views, server-side kind/current filters and full reference/correction display; native-window and live-provider acceptance remain open. **Dependencies:** T003. **Coverage:** AO04/AO18; C07/C36; concept §4.

**Files and responsibility:** Create `backend/quantix/staff_notebooks.py`, `backend/quantix/staff_notebook_models.py`; modify `backend/quantix/staff_context.py`, `backend/quantix/office_read.py`, `backend/quantix/staff_routes.py`; `src/features/office/StaffDesk.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t004.py`; register driver `T004` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** NotebookEntry{id,tender_id,staff_id,assignment_id,kind:finding|assumption|open_question|failed_approach|handover|preference,text,refs,applicability,supersedes_id,created_at}; append(ctx, NotebookEntryDraft) -> NotebookEntry; retrieve(ctx, NotebookQuery{staff_id,query,limit,cursor}) -> NotebookPage.

**Request/record details:** Append: kind, text, source/work-product refs, applicability, supersedes_id and originating assignment from ctx. Query: staff_id, query, kind/date filters, limit, cursor; no arbitrary other-Tender selector. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [x] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [x] 1. Persist append-only authored work notes and correction links; record exact originating brief/source/method and applicability. Do not store credentials or promise raw hidden model reasoning.
- [x] 2. Build context from the current brief plus ranked permitted notebook entries; revalidate older sources/decisions before reuse and label stale entries.
- [x] 3. Enforce same Tender/staff ownership and explicit handoff grants for shared material; two simultaneous assignments get separate mutable contexts.
- [x] 4. Add paged notes/open-issues/history in the desk with full reference navigation and user-visible correction scope.
- [x] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [x] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t004(run_case):
    result = run_case("T004", {"scenario": "notebook_reconstruction", "notes": 1000, "restart": True})
    assert result['relevant_note_restored'] is True
    assert result['unrelated_notes_in_prompt'] == 0
    assert result['received_implies_source_read'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t004.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [x] T004.A1: A restarted assignment retrieves a relevant earlier finding but not another Tender's notes or an unrelated staff member's private context.
- [x] T004.A2: Superseding a note keeps its history; stale notes cannot become current evidence.
- [x] T004.A3: 1000 notes paginate without omission, duplicate IDs or accidental whole-notebook context loading.
- [x] T004.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** 2026-09-10. Backend scoped-notebook/context repair retained; this continuation closed the remaining desk-UI and view-filter acceptance. `NotebookQuery` gains `current: bool | None` with a `current=?` retrieve filter; `GET /tenders/{tid}/staff/{sid}/notebook` gains `kind` (Literal-validated, invalid → 422) and `current` query params; bindings regenerated (`npm run bindings` exit 0; the large diff also synchronizes long-stale endpoints, notebook paths were previously absent). New `src/features/office/StaffNotebook.tsx` in the staff desk adds All notes / Open questions / History views, text search, bounded paging with retry, per-note kind/stale/corrected labels, source citations plus saved-result/output reference buttons, and correction scope (assignment, applicability, supersedes, source scope, profile version) under More options. Commands: `pytest backend/tests/agentic/test_t004.py backend/tests/test_staff_notebook_scope.py backend/tests/test_staff_api.py backend/tests/test_staff_context.py` → 46 passed; `node scripts/test-ui.mjs StaffNotebook/StaffDesk/LiveOffice` → 21 passed (4 new); `npm run check:ui` exit 0; Ruff check clean on touched files (remaining format drift is pre-existing lines); Prettier applied to new files. No live provider, native-window, real-app visual walkthrough, release package, commit, real Tender approval or commercial send. Status: implemented.

<a id="t005"></a>

### T005: Enable full exact handoff consumption and revalidated prior-work reuse

**Historical status:** Previously recorded as Implemented 2026-09-10. **Current status:** Implemented 2026-09-10 — staff read_handoff tool with fenced exact paging and inspection events, received/sent desk list with exact row viewer, and full transfer/review/replay fencing; native-window and live-provider acceptance remain open. **Dependencies:** T004. **Coverage:** AO06/AO13; C09; concept §5; concrete audit gap.

**Files and responsibility:** Create `backend/quantix/office_handoffs.py`, `backend/quantix/office_handoff_models.py`; modify `backend/quantix/office_messages.py`, `backend/quantix/office_tools.py`, `backend/quantix/office_manager_tools.py`, `backend/quantix/staff_context.py`, `backend/quantix/staff_runtime.py`.

**Test owner:** Create `backend/tests/agentic/test_t005.py`; register driver `T005` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** Handoff{id,sender_snapshot,recipient_assignment_id,purpose,required_response,refs,basis_fingerprint}; read_handoff(ctx,HandoffSelection{handoff_id,record_id,offset,limit}) -> HandoffPayload{items,next_cursor,exact_versions,applicability}; transfer_saved_result(ctx,TransferRequest{result_id,recipient_assignment_id,expected_basis_fingerprint,idempotency_key}) -> Handoff.

**Request/record details:** Read: handoff_id, selected record/section/row range, offset/limit. Transfer: result_id, recipient_assignment_id, purpose, expected_basis_fingerprint, idempotency_key; sender/root/grant are server-owned. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [x] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [x] 1. Expose a staff reader only for delivered, Manager-authorized handoffs; return complete selected staff-result tables/calculations/outputs through bounded paging, not just IDs or a rewritten summary.
- [x] 2. Validate every embedded source, business record and destination against the receiver's current grant before constructing the payload; forbid context leakage through nested content.
- [x] 3. Permit deliberate same-Tender older-root transfers after current source/decision/method/grant validation. Retain original root/author/version; stale material is historical and cannot authorize publication.
- [x] 4. Record received and actually inspected extents separately; receipt does not union the author's source-read set or renew spending.
- [x] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [x] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t005(run_case):
    result = run_case("T005", {"scenario": "full_handoff", "rows": 750, "selected_row": 701, "prior_root": True})
    assert result['selected_row'] == 701
    assert result['values_exact'] is True
    assert result['copied_source_receipts'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t005.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [x] T005.A1: The recipient reads row 701 of a 750-row handed-off table with the exact Decimal/unit/formula basis; no silent truncation.
- [x] T005.A2: Guessing another recipient's handoff ID fails; altering replay content conflicts.
- [x] T005.A3: A valid prior-root result can be reused by explicit transfer; a revised source makes applicability needs_review, not current.
- [x] T005.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** 2026-09-10. Transfer/review/replay primitives retained; this continuation closed full handoff-store consumption. New `read_handoff` staff tool (`office_tools.source_tools`, auto-registered in the capability catalogue) revalidates the live assignment identity, requires the granted capability, fences reads to recipient/sender with opaque not-found on guesses, pages 1–50 exact rows, and stages a durable `handoff_inspected` run event (handoff ID, offset, limit, row hash, applicability) that commits only on successful validation — never unioning source-read receipts, spending, or source-inspected marks. New `GET /tenders/{tid}/handoffs?staff_id=&direction=received|sent` list with counterpart names under an opaque anchored cursor, plus `HandoffView/HandoffPage` schemas; bindings regenerated; handoff consumption contract added to `docs/contracts.md`. New `StaffHandoffs` desk section adds Received/Sent views, applicability badges, and an exact row viewer with bounded paging. Commands: `pytest test_office_handoffs.py test_t005 test_tool_policy_reads test_tool_invocations test_staff_api` → 34 passed; office UI (Handoffs/Desk/Notebook/LiveOffice) → 24 passed (3 new); `npm run check:ui` exit 0; Ruff check/format clean on touched files; Prettier applied. No live provider, native-window, real-app visual walkthrough, release package, commit, real Tender approval or commercial send. Status: implemented.

<a id="t006"></a>

### T006: Complete office message delivery, acknowledgements and ownership exchanges

**Historical status:** Previously recorded as Implemented 2026-09-10. **Current status:** Implemented 2026-09-10 — durable delivery/required-response reads with desk display, required-response persistence, grouping evidence, and full delivery/ack fencing; scheduler-owned ownership epochs stay with T008, native-window/live acceptance remain open. **Dependencies:** T005. **Coverage:** AO05/AO06; C09/C10; concept §§5/7.

**Files and responsibility:** Create `backend/quantix/office_coordination.py`, `backend/quantix/office_delivery_models.py`; modify `backend/quantix/office_messages.py`, `backend/quantix/office_message_models.py`, `backend/quantix/office_read.py`; `src/features/office/OfficeMessages.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t006.py`; register driver `T006` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** DeliveryReceipt{message_id,recipient_assignment_id,state:delivered|acknowledged|inspected,extent,at}; CoordinationRequest{kind:calculation_handoff|review_challenge|change_notice|work_transfer|escalation,refs,reply_to,required_response}; acknowledge(ctx, ReceiptRequest)->DeliveryReceipt. Checked and accepted are links to separate review/engineer records.

**Request/record details:** Post: recipient assignments, kind, text, exact refs, reply_to, correlation, required_response, operation key. Acknowledge: message_id and received extent. Inspection/check/accept claims cannot be freely supplied. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [x] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [x] 1. Extend typed exchanges while preserving the current retained transcript and immutable sender snapshots; infer authorship from server context.
- [x] 2. Add correlation, acknowledgement and required-response records; inspection is produced only by the actual reader; check/accept status is derived from authoritative review/decision records.
- [x] 3. Persist ownership transfer requests and abandoned/expired-assignment signals; the scheduler task supplies fenced ownership epochs.
- [x] 4. Group repeated questions by evidence/context identity and distribute one scoped answer to all legitimate recipients; do not merge different contractual questions by wording alone.
- [x] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [x] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t006(run_case):
    result = run_case("T006", {"scenario": "delivery_semantics", "duplicate_deliveries": 3})
    assert result['delivery_count'] == 1
    assert result['acknowledgement_count'] == 1
    assert result['engineer_decisions_created'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t006.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [x] T006.A1: Retrying delivery or acknowledgement creates one receipt; received never means checked.
- [x] T006.A2: Two similar questions with different source versions remain distinct; identical questions get one Manager question and attributable replies.
- [x] T006.A3: A forged checked/accepted state in model arguments is rejected.
- [x] T006.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** 2026-09-10. Delivery/ack/grouping/ownership-request primitives retained; this continuation closed the durable read side. `office_delivery_receipts` gains `required_response` (additive migration); `DeliveryReceipt` carries it through deliver/acknowledge/replay; message reads attach `delivery_state` + `required_response` per message; new `GET office/messages/{id}/delivery` returns the latest durable receipt (unknown/undelivered → 404). Desk conversation shows "Needs a reply: …" and server-confirmed Received that survives refresh. Question-grouping distinct/grouped behavior is driver-covered (T006.A2). Commands: `pytest test_office_coordination.py test_t006 test_office_messages test_office_integration` → 28 passed (2 new); OfficeMessages UI → 4 passed (2 new); `npm run check:ui` exit 0; bindings regenerated; Ruff/Prettier clean on touched files. Fenced scheduler ownership epochs remain T008's; no live provider, native-window, real-app visual walkthrough, release package, commit, real Tender approval or commercial send. Status: implemented.

<a id="t007"></a>

### T007: Persist adaptive work graphs and instruction-bound dependencies

**Historical status:** Previously recorded as Implemented 2026-09-10. **Current status:** Implemented 2026-09-10 — graph persistence/revision plus derived node statuses with remedies and a latest-graph read endpoint; runtime execution wiring stays with T008/T009, native-window/live acceptance remain open. **Dependencies:** T006, T013. **Coverage:** AO01/AO02/AO05/AO07; C01/C06/C11.

**Files and responsibility:** Create `backend/quantix/assignment_graph.py`, `backend/quantix/assignment_graph_models.py`, `backend/quantix/instruction_models.py`; modify `backend/quantix/staff_assignments.py`, `backend/quantix/office_manager_tools.py`, `backend/quantix/plan_review_models.py`.

**Test owner:** Create `backend/tests/agentic/test_t007.py`; register driver `T007` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** WorkGraphVersion{id,root_id,revision,instruction_revision_id,nodes,edges,basis_fingerprint}; WorkNode{id,owner_staff_id,expected_output,completion_criteria,prerequisite_ids,state}; propose_graph(ctx, GraphDraft)->WorkGraphVersion; revise_graph(ctx,GraphRevisionRequest{expected_revision,changes,reason})->WorkGraphVersion.

**Request/record details:** GraphDraft: outcome, node professional briefs/owner choices/output contracts/criteria/prerequisites. Revision: expected_revision, add/change/remove draft nodes/edges, reason and instruction link; server resolves immutable IDs. Common provenance/version fields and authority rules in section 4 are mandatory.

T007 owns the initial `InstructionRevision` schema and version-1 row: id, root_id, original_engineer_message_id, revision, content, basis_fingerprint, created_at. Create it from the actual admitted engineer message when the first graph is saved. T010 extends this same schema/service boundary for steering; it does not create a competing instruction store. Existing non-graph runs keep their current message lineage until explicitly admitted to this graph path.

**Implementation checklist:**

- [x] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [x] 1. Persist task-specific nodes/dependencies and graph revisions; workflows supply suggested methods rather than a closed graph/name catalogue.
- [x] 2. Validate references, ownership, scope, acyclicity and terminal-output applicability before saving; retain completed nodes and original briefs.
- [x] 3. Distinguish blocked prerequisite from failed owner, unavailable tool and missing engineer input; present the concrete next remedy.
- [x] 4. Integrate graph changes inside the existing reviewed envelope without routine reapproval; expanding authority returns a prepared material-change review.
- [x] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [x] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t007(run_case):
    result = run_case("T007", {"scenario": "graph_cycle_and_revision", "cycle": ["A", "B", "A"]})
    assert result['cycle_rejected'] is True
    assert result['partial_mutations'] == 0
    assert result['unaffected_outputs_preserved'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t007.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [x] T007.A1: An unseen buying-versus-staged-delivery request creates its own graph without registering a workflow.
- [x] T007.A2: A cycle A→B→A is rejected atomically; no partial nodes/events remain.
- [x] T007.A3: Changing one constraint invalidates only affected unfinished/reusable branches and preserves completed history.
- [x] T007.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** 2026-09-10. Graph propose/revise primitives retained; this continuation closed status derivation and the read side. New pure `node_statuses()` derives ready/waiting/working/completed/needs_owner/needs_attempt with concrete remedies (waiting names exact prerequisite keys); new `GET office/graphs/latest?root_run_id=` returns `WorkGraphStatus` (unknown root → 404, cross-Tender → 404); bindings regenerated. Backend-only slice: the task lists no UI files and plan §5 permits backend-only tasks without artificial frontend tests. Commands: `pytest test_assignment_graph.py test_t007` → 5 passed (4 new: revision retention, status matrix, cycle atomicity, route scoping); `npm run check:ui` exit 0; Ruff check/format clean on touched files. Runtime execution wiring (scheduler admission, concurrent descendants) stays with T008/T009; no live provider, native-window, real-app visual walkthrough, release package, commit, real Tender approval or commercial send. Status: implemented.

<a id="t008"></a>

### T008: Allow bounded dynamic descendants with fenced ownership

**Historical status:** Previously recorded as Implemented 2026-09-10. **Current status:** Implemented 2026-09-10 — real child assignments with envelope depth caps, staff child-request tool, epoch-fenced ownership, and serial controller execution; concurrency stays with T009, native-window/live acceptance remain open. **Dependencies:** T007. **Coverage:** AO05/AO13; C13; research §12.

**Files and responsibility:** Create `backend/quantix/office_scheduler.py`, `backend/quantix/office_ownership.py`; modify `backend/quantix/staff_routing.py`, `backend/quantix/staff_routing_models.py`, `backend/quantix/staff_assignments.py`, `backend/quantix/staff_runtime.py`, `backend/quantix/jobs.py`.

**Test owner:** Create `backend/tests/agentic/test_t008.py`; register driver `T008` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** AssignmentExecution{parent_assignment_id,root_id,depth,ownership_epoch,lease_expires_at}; spawn_child(ctx,ChildAssignmentRequest{work_order_id,prerequisites,requested_route,capabilities,idempotency_key})->StaffAssignment; claim(ctx,ClaimRequest{assignment_id,expected_revision})->OwnershipLease; transfer(ctx,TransferOwnershipRequest{expected_epoch,new_owner,checkpoint_id})->OwnershipLease.

**Request/record details:** Child: work_order_id, prerequisites, requested route/capability subset and operation key. Claim: assignment_id, expected_revision. Transfer: assignment_id, expected_ownership_epoch, new_owner, completed checkpoint and reason. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [x] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [x] 1. Permit a specialist to request/create a child only through Manager-delegated capabilities; inherit the same root scope and budget, intersect sources/tools/destinations and enforce approved depth/task limits.
- [x] 2. Claim work atomically with expiry and epoch fencing; persist waiting and abandoned ownership honestly.
- [x] 3. Release provider/Tender orchestration leases before a parent waits for a child; avoid child queueing behind a parent-held lane.
- [x] 4. Retain late usage under its exact old identity but reject late publication after Stop, expiry or reassignment.
- [x] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [x] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t008(run_case):
    result = run_case("T008", {"scenario": "bounded_descendants", "max_depth": 2, "attempt_depth": 3, "claimants": 2})
    assert result['depth_three_blocked'] is True
    assert result['owners'] == 1
    assert result['budget_owners'] == 1
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t008.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [x] T008.A1: Depth cap 2 admits a grandchild at depth 2 and blocks depth 3 without a new root allowance.
- [x] T008.A2: Two claims for one revision yield one owner; an expired owner cannot save accepted proposals.
- [x] T008.A3: A same-account parent/child scenario finishes without nested official-client lease deadlock.
- [x] T008.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** 2026-09-10. Replaced the side-table stub with real wiring: `office_assignments` gains `parent_assignment_id/depth/prerequisites_json` (additive migration); `queue_child` + `bind_child` enforce same colleague/root/budget, capability subsets of pinned parent tools, and envelope depth caps; `request_child_assignment` staff tool (explicitly allowlisted local draft mutation, truthful write metadata); envelope shape gate raised to reviewed 1–8 depth (concurrency stays 1 for T009); ownership gains transfer/release/expiry with transfer log; scheduler reimplemented on real assignments with honest single-owner budget count. The root controller drains queued children serially after each provider lease closes. Commands: new `test_office_scheduler.py` (4 passed: envelope-cap depth incl. request-cap min, rogue capability, claim/transfer/release/expiry, serial parent→child with lease observation and single usage root); `test_t008` rewritten to real semantics → passes; affected routing/assignment/runtime/controller suites → 71 passed total; full UI suite → 246 passed (2 assignment fixtures updated for new required fields); `npm run check:ui` exit 0; bindings regenerated; Ruff/Prettier clean on touched files. No live provider, native-window, real-app visual walkthrough, release package, commit, real Tender approval or commercial send. Status: implemented.

<a id="t009"></a>

### T009: Run independent staff concurrently with aggregate resource accounting

**Historical status:** Previously recorded as Implemented 2026-09-10. **Current status:** Implemented 2026-09-10 — concurrent ready-branch drain with aggregate reservations, per-account locks, prerequisite gating and failure isolation; effective concurrency follows the reviewed envelope; native-window/live acceptance remain open. **Dependencies:** T008, T013. **Coverage:** AO05/AO13; C13/C38.

**Files and responsibility:** Modify `backend/quantix/office_scheduler.py`, `backend/quantix/staff_budget.py`, `backend/quantix/ai_policy.py`, `backend/quantix/ai_connections.py`, `backend/quantix/jobs.py`; create `backend/quantix/resource_leases.py`.

**Test owner:** Create `backend/tests/agentic/test_t009.py`; register driver `T009` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** ResourceLease{kind:provider_account|pdfium|cpu|memory|search,owner_id,limit,expires_at}; admit_ready(ctx, SchedulingRequest{graph_version}) -> list[AssignmentExecution]; reserve_usage(ctx, UsageReservationRequest)->UsageReservation using the existing root ledger.

Persist the shared `BudgetScope` above and link all usage/reservation/attempt records to it without discarding original run IDs. Atomically compare aggregate reserved + reported + uncertain consumption against its current limit before each request. Concurrent resumes/retries and watcher triggers use the same accounting scope unless a distinct engineer-reviewed allowance actually exists. Current route/assignment ownership validation remains tied to the active execution attempt, not a terminal historical root.

**Request/record details:** Scheduling: current graph ID/version, approved priority and ready nodes; resource request: capability/owner/amount/time bound. Root allowance and actual model prices are resolved from the saved grant, not supplied. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [x] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [x] 1. Schedule ready independent branches up to the reviewed cap; serialize only constrained original-client accounts/PDFium or requested resources.
- [x] 2. Atomically reserve model/search/request allowance across Manager, child, review and retry work before dispatch; use actual bound model prices and retain unknown/uncertain usage.
- [x] 3. Prioritize dependency-unblocking work, stated deadlines and engineer priority; show reasons, not invented percent complete.
- [x] 4. Keep independent valid branches running after another branch fails, while dependants wait. Add configurable resource ceilings and backpressure.
- [x] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [x] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t009(run_case):
    result = run_case("T009", {"scenario": "parallel_budget_race", "remaining_requests": 2, "simultaneous_requests": 3})
    assert result['admitted'] == 2
    assert result['overspend'] is False
    assert result['late_usage_recorded'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t009.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [x] T009.A1: With two available requests and three simultaneous reservations, exactly two are admitted.
- [x] T009.A2: A one-account original-client route stays serial while independent permitted accounts can overlap.
- [x] T009.A3: Stopping the root promptly prevents every new reservation; cleanup remains tracked and late usage is retained.
- [x] T009.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** 2026-09-10. New `office_concurrency.drain_queued_assignments` wired into the root controller: ready branches (all prerequisites completed) overlap up to the reviewed `max_concurrency` as coroutines with one branch-owned repository connection each; original-client accounts serialize behind a per-account asyncio lock while direct API work overlaps; every dispatch holds an atomic aggregate reservation (BEGIN IMMEDIATE compare-and-insert; stopped scopes deny with late usage kept); failed prerequisites fail dependants fast with the reason; dependency cycles fail fast instead of looping; sibling failures never cancel the wave and the first failure re-raises with completed siblings retained. A thread-worker design was prototyped and rejected after it provably leaked uncancellable provider turns on Stop (the threaded run hung `test_office_recovery.py`); the coroutine design passes that suite 6/6. Commands: new `test_office_concurrency.py` → 6 passed (barrier-proven overlap at cap 2, serial order at cap 1, shared-lock serialization, reserve/stop/late, sibling isolation, stopped-root admission); affected office/routing/scheduler/tool suites → 49 passed; `npm run check:ui` exit 0; Ruff clean. Priority ordering is depth-then-creation with prerequisite gating; per-branch reasons surface through assignment states rather than percents. Live original-client execution (signed-in account overlap/serial under load) remains unverified; no release package, commit, real Tender approval or commercial send. Status: implemented.

<a id="t010"></a>

### T010: Separate status replies from execution and implement safe steering

**Historical status:** Previously recorded as Implemented 2026-09-10. **Current status:** Implemented 2026-09-10 — steering admission with turn-boundary application, cancel-through-Stop, and saved status reads; held-text pending UI unchanged; native-window/live acceptance remain open. **Dependencies:** T007, T009. **Coverage:** AO07/AO13; C11; concept §9.

**Files and responsibility:** Create `backend/quantix/office_instructions.py`; extend T007's `backend/quantix/instruction_models.py`; modify `backend/quantix/conversation.py`, `backend/quantix/pending.py`, `backend/quantix/jobs.py`, `backend/quantix/office.py`, `backend/quantix/manager_runtime.py`; `src/features/Composer.tsx`, `src/features/PendingMessage.tsx`, `src/features/Manager.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t010.py`; register driver `T010` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** GET /api/tenders/{tid}/office/status -> WorkStatusSnapshot; POST /office/instructions -> InstructionAdmission; InstructionRevisionRequest{expected_work_revision,kind:question|constraint|replace|urgent|cancel,content,selection,idempotency_key}; InstructionRevision{original_message_id,revision,basis,applies_from,affected_nodes}.

**Request/record details:** Instruction: content, kind, expected_work_revision, selected source/work context and idempotency_key. A kind requiring engineering interpretation is bounded under current authority; cancel/status are explicit structural actions. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [x] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [x] 1. Answer operational status from saved records with zero engineering/provider work; an actual technical question uses separately admitted bounded work without duplicating the active plan.
- [x] 2. Classify steering against current work and expose ambiguity as one precise question; version accepted changes and apply them between safe steps.
- [x] 3. Keep completed outputs bound to their original brief and mark superseded applicability. Narrowing within scope needs no repeated authority approval; wider data/billing/commercial scope does.
- [x] 4. Migrate existing single pending records explicitly; retain held-after-failure/restart behavior, idempotent submission and newer unsent drafts.
- [x] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [x] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t010(run_case):
    result = run_case("T010", {"scenario": "status_and_constraint_during_work"})
    assert result['status_provider_requests'] == 0
    assert result['mixed_revision_publications'] == 0
    assert result['draft_preserved'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t010.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [x] T010.A1: Status during a slow child returns current owners/blockers without creating staff or a new full analysis.
- [x] T010.A2: A constraint arriving during calculation affects the next safe step, never half of one published result.
- [x] T010.A3: Edited/cancelled pending receipts cannot resolve to replacement text; restart starts no held instruction automatically.
- [x] T010.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** 2026-09-10. Status primitive retained and extended with assignment-derived owners/blockers (still zero provider work). New steering admission (`POST office/instructions`, idempotent, active-root fenced) applied at the next turn boundary inside `office.execute` with a never-rewrite-published-results guard and mark-applied-after-validation; `cancel` admissions interrupt assignments and raise through the normal jobs Stop path; admitted steering survives failed turns for retry. Held-text pending flow and composer UI unchanged (covered by existing pending tests). Commands: new `test_office_instructions.py` → 4 passed (admit/replay/fencing, status owners, _run-level application without repetition, cancel without provider reach); `test_t010`/office/conversation suites → 46 passed; `npm run check:ui` exit 0; bindings regenerated; contracts updated; Ruff/Prettier clean on touched files. No live provider, native-window, real-app visual walkthrough, release package, commit, real Tender approval or commercial send. Status: implemented.

<a id="t011"></a>

### T011: Implement step checkpoints, selective continuation and effect reconciliation

**Historical status:** Previously recorded as Implemented 2026-09-10. **Current status:** Implemented 2026-09-10 — lineage-scoped checkpoint reuse with records, effect state machine, and resume surfacing; native-window/live acceptance remain open. **Dependencies:** T010. **Coverage:** AO13; C11/C33/C36; research §12.

**Files and responsibility:** Create `backend/quantix/office_checkpoints.py`, `backend/quantix/checkpoint_models.py`, `backend/quantix/effect_receipts.py`; modify `backend/quantix/office_resume.py`, `backend/quantix/office_scheduler.py`, `backend/quantix/jobs.py`, `backend/quantix/correspondence.py`.

**Test owner:** Create `backend/tests/agentic/test_t011.py`; register driver `T011` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** StepCheckpoint{step_id,assignment_id,instruction_revision,input_fingerprint,source_bases,method_version,outputs,remaining_dependencies,effect_receipts}; checkpoint(ctx,CheckpointDraft)->StepCheckpoint; resume_checkpoint(ctx,ResumeCheckpointRequest{checkpoint_id,expected_basis_fingerprint,idempotency_key})->ResumeReceipt; EffectReceipt{operation_key,state:prepared|attempted|confirmed|uncertain,destination_basis,result_ref}.

**Request/record details:** Checkpoint: step identity, input/source/decision/method fingerprint, durable output/effect refs and remaining prerequisites. Resume: selected checkpoint/assignment, expected_basis_fingerprint, reason and idempotency_key. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [x] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [x] 1. Checkpoint actual durable output and input/method identity after each expensive completed step; a prose progress summary cannot serve as a completed checkpoint.
- [x] 2. Revalidate scope, current sources, decisions, method and ownership before reuse; rerun only invalidated steps and their dependants.
- [x] 3. Preserve original instruction and distinct attempt lineage without duplicate dialogue; retained reservations count until reconciled.
- [x] 4. For external effects distinguish unattempted, confirmed and uncertain; reconcile against existing mail/delivery records or obtain the specific missing decision, never resend merely because a run resumed.
- [x] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [x] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t011(run_case):
    result = run_case("T011", {"scenario": "checkpoint_crash", "crash_after": "extraction", "smtp_outcome": "uncertain"})
    assert result['extraction_executions'] == 1
    assert result['publication_count'] == 1
    assert result['automatic_resends'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t011.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [x] T011.A1: Crash after extraction but before calculation reuses valid extraction and produces one calculation/publication.
- [x] T011.A2: Source revision change invalidates the affected checkpoint branch; unrelated complete work remains.
- [x] T011.A3: Crash after SMTP acceptance but before local confirmation produces uncertain delivery and no automatic duplicate send.
- [x] T011.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** 2026-09-10. Checkpoint/effect records retained and wired: `resume_checkpoint` now requires the same root or a shared resume lineage (unrelated roots rejected), records attributable reuse rows, and keeps exact fingerprint revalidation; effects move through a guarded `prepared→attempted→confirmed|uncertain` machine with latest-state queries (first knowledge may be `uncertain`; `confirmed` terminal); resumed attempts list reusable checkpoints in their resume receipt and first Manager turn, labelled pre-interruption work to verify. Mail-send uncertainty guards were already present in correspondence (no blind resend of sent/uncertain drafts). Commands: new `test_office_checkpoints.py` → 2 passed (lineage/reuse/fingerprint, effect machine); `test_t011`/office/instructions/job-roots/recovery suites → 18 passed; Ruff clean on touched files (jobs.py/office.py format drift is pre-existing). No API schema change; no live provider, native-window, real-app visual walkthrough, release package, commit, real Tender approval or commercial send. Status: implemented.

<a id="t012"></a>

### T012: Build bounded review sessions and reproducible independent checks

**Historical status:** Previously recorded as Implemented 2026-09-10. **Current status:** Implemented 2026-09-10 — discussion/resolution session lifecycle with review-room API and desk UI, on top of the arithmetic/blind-review repair; native-window/live acceptance remain open. **Dependencies:** T005, T006, T007, T009. **Coverage:** AO14; C10; concept §7/§12.

**Files and responsibility:** Create `backend/quantix/office_reviews.py`, `backend/quantix/review_models.py`; modify `backend/quantix/office_manager_tools.py`, `backend/quantix/office_read.py`; create `src/features/office/ReviewSession.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t012.py`; register driver `T012` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** ReviewSession{question,basis,participants,artifact_refs,budget,stop_conditions,state}; ReviewContribution{phase:initial|discussion|resolution,claim,evidence,method,uncertainty,impact,next_action}; convene(ctx,ReviewRequest)->ReviewSession; contribute(ctx,ContributionDraft)->ReviewContribution.

**Request/record details:** Review: question, exact basis/artifacts, participant choices, approved effort ceiling and stop conditions. Contribution: phase, claim, evidence refs, method, uncertainty, impact, next action; actor/phase authority comes from ctx. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [x] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [x] 1. Have the Manager state the unresolved issue and expected value of another check before allocating approved effort; use deterministic arithmetic checks first where sufficient.
- [x] 2. Give an independent reviewer original inputs and record its immutable initial conclusion before releasing the author's recommendation when practical.
- [x] 3. Persist structured challenges, reproduction results and disagreements; stop on resolution, missing evidence, budget exhaustion or required engineer decision.
- [x] 4. Expose Review room inside the office with one question and clear decision links; never turn consensus into engineer acceptance.
- [x] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [x] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t012(run_case):
    result = run_case("T012", {"scenario": "blind_review", "author_total": "110.00", "correct_total": "100.00"})
    assert result['reviewer_initial_total'] == '100.00'
    assert result['initial_author_recommendation_visible'] is False
    assert result['accepted_record_mutations'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t012.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [x] T012.A1: The reviewer identifies a duplicated allowance from workings rather than seeing the author's conclusion first.
- [x] T012.A2: Agreement on area with disagreement on the deduction rule remains two separate recorded findings.
- [x] T012.A3: Unapproved alternative model/budget is blocked; a resolved review does not authorize a quantity/rate change.
- [x] T012.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** 2026-09-10. Arithmetic/blind-review repair retained; this continuation closed the session lifecycle and review room. Reviews gain idempotent discussion contributions (agreement and disagreement stay separate findings; contributing to a resolved session conflicts) and an explicit resolve action that records the resolution without mutating quantities, rates, findings or decisions (verified: decisions ledger empty after resolve). Review-room API (`POST/GET /reviews`, contributions, resolve) in later_routes with Tender scoping, plus a Reviews section in the Work view (status filter, findings/limitations/basis detail, contribute/resolve forms, decision link). Commands: new `test_office_review_sessions.py` → 3 passed; `test_t012`/truthfulness/later-routes/tool suites pass; ReviewRoom UI → 3 passed; Work UI passes; `npm run check:ui` exit 0; bindings regenerated; Ruff/Prettier clean on touched files. Full backend suite → 848 passed, 1 pre-existing platform skip, 0 failed (this run also repaired a real regression my T009 work caused: per-branch repo clones broke `repo is` identity gates — fixed by keying held DB connections per thread+task instead, plus a `_local` accessor migration in office_read and its acceptance test). No live provider, native-window, real-app visual walkthrough, release package, commit, real Tender approval or commercial send. Status: implemented.

<a id="t013"></a>

### T013: Unify versioned capability and tool contracts

**Historical status:** Previously recorded as Implemented 2026-09-10. **Current status:** Implemented 2026-09-10 — re-verified against the current tree **Dependencies:** T001. **Coverage:** AO12; C23/C24/C37; research §9/§13.

**Files and responsibility:** Create `backend/quantix/tool_policy.py`, `backend/quantix/capability_models.py`, `backend/quantix/execution_context.py`; modify `backend/quantix/ai_tools.py`, `backend/quantix/office_tools.py`, `backend/quantix/ai_runtime_mcp.py`, `backend/quantix/ai_api_engine.py`, `backend/quantix/staff_capabilities.py`.

**Test owner:** Create `backend/tests/agentic/test_t013.py`; register driver `T013` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** ToolCapability{id,namespace,version,input_schema,output_schema,source_requirements,data_categories,destinations,effect,limits,retry_policy,idempotency_policy,provider}; invoke(ctx:OfficeExecutionIdentity, capability_id:str, payload:dict)->ExecutionReceipt; ExecutionReceipt{status,evidence,artifacts,changed_records,usage,limitations,request_id}.

**Request/record details:** Invoke: registered capability_id/version plus its validated professional payload. Input/output JSON schemas and limits are immutable version metadata; caller cannot override destination, root, permissions or status receipt. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [x] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [x] 1. Define a server-only immutable execution identity from existing root/staff/binding authority; do not introduce user-supplied Tender/root or a second approval store.
- [x] 2. Validate typed input and output, runtime limits, record preconditions and intersection of Tender/task/staff/plugin permissions at every invocation.
- [x] 3. Register existing source/business/calculation tools with stable IDs and truthful effect semantics; enforce the same wrapper for direct SDK, original-client MCP and future code-mode calls.
- [x] 4. Return completed/partial/needs_input/blocked/failed/uncertain_external_outcome with precise actionable limitations; tool discovery shows only relevant permitted capabilities.
- [x] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [x] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t013(run_case):
    result = run_case("T013", {"scenario": "unified_tool_fence", "entrypoints": ["direct", "mcp", "nested"]})
    assert result['denials'] == 3
    assert result['scope_widened'] is False
    assert result['accepted_invalid_outputs'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t013.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [x] T013.A1: The same prohibited source read fails through direct, MCP and nested-call entrypoints.
- [x] T013.A2: A plugin annotation or skill allowed-tools list cannot grant a capability.
- [x] T013.A3: Malformed tool output is rejected before domain publication; timeout/abort preserves uncertainty and request identity.
- [x] T013.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** 2026-09-10. Added server-only `OfficeExecutionIdentity`, `ExecutionReceipt`, and `tool_policy.dispatch` used by the direct SDK bridge, original-client MCP bridge and nested calls. Payload `tender_id` cannot widen scope. Plugin/skill allowed-tools lists cannot grant staff tools. Approval-shaped tool JSON is rejected before publication. Timeouts return `uncertain_external_outcome` with the original request identity. Command: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t013.py -q --tb=short` plus tool-invocation tests. Combined T001/T002/T013 and adjacent staff/tool tests: 56 passed. T013 is backend-only; no live provider. Status: implemented.

<a id="t014"></a>

### T014: Create the versioned skill package library and validation

**Current status:** Partial 2026-09-10 — focused package evidence only; backup/scope review repairs and remaining checklist/native/live evidence are open. **Dependencies:** T013. **Coverage:** AO02/AO12; C22; research §5.

**Files and responsibility:** Create `backend/quantix/skill_models.py`, `backend/quantix/skills.py`, `backend/quantix/skill_packages.py`; add source-owned skills/quantix/; extend `backend/quantix/storage.py` and backup/reset inventories.

**Test owner:** Create `backend/tests/agentic/test_t014.py`; register driver `T014` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** SkillVersion{id,version,publisher,instructions_hash,description,applicability,trades,jurisdictions,input_schema,output_schema,required_capabilities,references,templates,scripts,evaluations,license,runtime_requirements}; inspect_skill(ctx,PackageInput)->SkillPreview; activate_skill(ctx,SkillActivationRequest{fingerprint})->SkillVersion.

**Request/record details:** Package inspection: selected local package/content hash. Activation: exact preview fingerprint, requested library scope; package manifest fields listed in task. No arbitrary remote execution/install script. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Read SKILL.md-compatible instructions plus a separate validated Quantix manifest; use managed library roots and immutable package hashes.
- [ ] 2. Reject traversal, symlinks/reparse escapes, conflicting identity/version, unsupported runtime and executable install hooks; editable company text remains distinct from pinned executable packages.
- [ ] 3. Record references/editions/lawful access, exact dependency identities and required capabilities as requests, not grants.
- [ ] 4. Implement inspect/activate/deactivate/version-list with preserved historical execution references and backup/reset ownership.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t014(run_case):
    result = run_case("T014", {"scenario": "skill_package_validation", "path": "../outside.txt", "reuse_version_with_new_hash": True})
    assert result['path_escape_rejected'] is True
    assert result['version_collision_rejected'] is True
    assert result['permissions_created'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t014.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T014.A1: Changing bytes under an existing version conflicts; updating a skill cannot rewrite an old calculation's method record.
- [ ] T014.A2: A malicious allowed-tools field or referenced outside path grants nothing and exposes no file.
- [ ] T014.A3: A missing reference/runtime is an actionable unavailable capability, not an empty successful load.
- [ ] T014.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t015"></a>

### T015: Load and execute engineering skills progressively across providers

**Current status:** Partial 2026-09-10 — reviewed Methods UI increment only; two bundled 1.1.0 methods are available, while full skill execution/plugin scope and checklist/native/live evidence remain open. **Dependencies:** T014. **Coverage:** AO02/AO12/AO15; research skill families §5.

**Files and responsibility:** Create `backend/quantix/skill_execution.py`; modify `backend/quantix/office.py`, `backend/quantix/staff_runtime.py`, `backend/quantix/staff_context.py`, `backend/quantix/ai_runtime_mcp.py`; source-owned skills/quantix/*; `src/features/office/StaffDesk.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t015.py`; register driver `T015` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** discover_skills(ctx,SkillQuery{outcome,trade,jurisdiction}) -> list[SkillSummary]; load_skill(ctx,SkillLoadRequest{id,version}) -> SkillInstructions; read_skill_resource(ctx,SkillResourceRequest{version,path}) -> ResourceReceipt; execute_skill_script(ctx,SkillScriptRequest{version,script,inputs}) -> ExecutionReceipt.

**Request/record details:** Discovery: outcome, trade, geography/jurisdiction. Load: skill ID/version. Resource: skill version and relative path. Script: skill/script identity and typed inputs; all reads/calls use the current context fence. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Load only descriptions initially, then selected instructions/resources; expose identical version/content through direct and supported subscription execution.
- [ ] 2. Ship evaluated methods for package triage, drawing/missing-document/spreadsheet checks, reconciliation/measurement, rates/preliminaries, supplier/RFQ/levelling, risk, programme, documents, evidence/arithmetic/submission checking and approved reuse.
- [ ] 3. Bind each invocation to exact skill and capability versions; scripts run reviewed tools through the same gate; later full Python remains a separate capability.
- [ ] 4. Show skills actually used and unsupported requirements in staff desks/results without forcing engineers to choose a skill before making a request.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t015(run_case):
    result = run_case("T015", {"scenario": "progressive_skill_loading", "selected": "quantity-reconciliation", "providers": ["direct", "original_client"]})
    assert result['instruction_hashes_equal'] is True
    assert result['unrelated_resources_loaded'] == 0
    assert result['missing_edition_reported'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t015.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T015.A1: A quantity request does not preload unrelated procurement/company material.
- [ ] T015.A2: The same skill ID/version yields the same instruction hash for direct and original-client routes.
- [ ] T015.A3: A missing jurisdiction edition produces a gap; an unselected skill cannot run its script.
- [ ] T015.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t016"></a>

### T016: Compose methods, activate company packs and learn checked corrections

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T007, T015. **Coverage:** AO01/AO02/AO18; C01/C06/C22; concept checked-method learning.

**Files and responsibility:** Create `backend/quantix/office_methods.py`, `backend/quantix/method_pack_models.py`; modify `backend/quantix/knowledge.py`, `backend/quantix/office_knowledge.py`, `backend/quantix/office_manager_tools.py`; create `src/features/MethodLibrary.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t016.py`; register driver `T016` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** MethodVersion{id,steps,skill_versions,applicability,checks,provenance,status:draft|evaluated|approved|withdrawn}; CompanyMethodPack{id,version,methods,templates,vocabulary,review_rules,compatibility}; propose_method(ctx,MethodDraft)->MethodVersion; activate_pack(ctx,PackActivation{fingerprint,scope})->PackReceipt.

**Request/record details:** Method: outcome, applicable steps/skill versions, inputs/outputs/checks, fixture refs, provenance. Pack activation: exact version/fingerprint and target scope. Correction: selected result/Tender/company-proposal scope. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Let the Manager reuse applicable steps, combine methods or create a one-off plan when no method matches; method names are not a request allowlist.
- [ ] 2. Save successful corrected methods as drafts with concrete fixtures and failure cases; promotion needs explicit scope, evaluation and company reuse approval.
- [ ] 3. Detect conflicting pack terminology/calculation rules before activation; display exact precedence/applicability and avoid silent substitution.
- [ ] 4. Provide this result/this Tender/company proposal correction scopes; never move private Tender notebooks into company knowledge automatically.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t016(run_case):
    result = run_case("T016", {"scenario": "method_composition_and_promotion", "scope": "tender", "conflicting_packs": True})
    assert result['workflow_registration_required'] is False
    assert result['pack_conflict_visible'] is True
    assert result['automatic_company_promotion'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t016.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T016.A1: An unfamiliar procurement/cash-flow request completes using a new combination without a saved workflow.
- [ ] T016.A2: Two packs prescribing incompatible rounding/rules cannot silently co-activate for the same scope.
- [ ] T016.A3: A Tender-specific correction stays local until an explicitly approved, evaluated company version exists.
- [ ] T016.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t017"></a>

### T017: Add general versioned work products and safe task-specific surfaces

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T005, T013. **Coverage:** AO01/AO15/AO16; research §2; task-specific surfaces.

**Files and responsibility:** Create `backend/quantix/work_products.py`, `backend/quantix/work_product_models.py`; modify `backend/quantix/outputs.py`, `backend/quantix/office_types.py`, `backend/quantix/office_read.py`; create `src/features/WorkProduct.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t017.py`; register driver `T017` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** WorkProductVersion{id,kind:table|chart|calculation_sheet|note|comparison|timeline|document,schema,rows_or_content,source_refs,method_refs,author,basis,status,sha256}; save_draft(ctx,WorkProductDraft)->WorkProductVersion; GET /api/tenders/{tid}/work-products/{id}/versions/{version}.

**Request/record details:** Draft: kind, validated view/data schema, typed rows/content/series, exact source/method refs and current basis. Read/export: exact product ID/version and bounded selection; no arbitrary JS/SQL/template path. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Provide composable validated table/series/timeline/form schemas rather than introducing one product screen per unfamiliar task.
- [ ] 2. Store exact values, units, references, author and version; separate view specification from underlying data and accepted domain records.
- [ ] 3. Render only allowlisted components/actions and sanitized text/links; disallow arbitrary model JavaScript, SQL, iframe origins or application commands.
- [ ] 4. Support bounded paging, downloads and exact handoff reads. Reuse established typed acceptance paths if a draft later proposes a quantity, rate or release.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t017(run_case):
    result = run_case("T017", {"scenario": "generic_work_product", "rows": 10000, "unsafe_markup": "<script>bad()</script>"})
    assert result['export_rows'] == 10000
    assert result['executed_scripts'] == 0
    assert result['old_version_accessible'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t017.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T017.A1: A 10000-row custom comparison opens/searches/pages without truncating export or hiding missing rows.
- [ ] T017.A2: Injected script/unsafe URL remains inert; view actions cannot mint approvals.
- [ ] T017.A3: Editing a draft creates a new version while historical links remain stable.
- [ ] T017.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t018"></a>

### T018: Create isolated estimate and procurement scenario workspaces

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T017, T032. **Coverage:** AO11/AO14/AO15; research 38/41/42/69; concept scenarios.

**Files and responsibility:** Create `backend/quantix/scenarios.py`, `backend/quantix/scenario_models.py`; modify `backend/quantix/estimates.py`, `backend/quantix/outputs.py`; create `src/features/ScenarioComparison.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t018.py`; register driver `T018` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** ScenarioVersion{id,parent_basis,assumptions,overrides,calculation_refs,results,differences,currentness}; calculate_scenario(ctx,ScenarioRequest{basis_fingerprint,overrides,assumptions}) -> ScenarioVersion; compare_scenarios(ctx,ScenarioComparisonRequest{version_ids}) -> WorkProductVersion.

**Request/record details:** Scenario: exact base fingerprint, selected overrides with typed units/currencies, explicit assumptions and requested outputs. Comparison: exact scenario version IDs; adoption uses existing per-domain decisions. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Snapshot selected accepted/current draft bases and store explicit overrides without modifying originals or accepted quantities/rates.
- [ ] 2. Recalculate only supported deterministic dependencies; expose missing price/payment/lead-time assumptions rather than inventing values.
- [ ] 3. Show base/proposal/delta/reason/source and downstream impact; adoption goes through each existing meaningful engineer decision.
- [ ] 4. Mark scenarios stale when their source/decision basis changes; preserve historical comparisons and allow deliberate rebase.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t018(run_case):
    result = run_case("T018", {"scenario": "isolated_quantity_scenario", "quantity": "10", "override": "12", "rate": "50"})
    assert result['base_total'] == '500.00'
    assert result['scenario_total'] == '600.00'
    assert result['accepted_total'] == '500.00'
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t018.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T018.A1: Changing scenario quantity 10→12 at rate 50 gives delta 100 while the accepted base stays 500.
- [ ] T018.A2: Missing currency conversion or tax basis prevents a combined definitive total.
- [ ] T018.A3: A stale scenario cannot be applied through an old review fingerprint.
- [ ] T018.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t019"></a>

### T019: Build bounded local watchers and meaningful notifications

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T009, T010, T011. **Coverage:** AO17/AO21; C32/C33; research §12.

**Files and responsibility:** Create `backend/quantix/office_watchers.py`, `backend/quantix/watcher_models.py`; modify `backend/quantix/jobs.py`, `backend/quantix/correspondence.py`, `backend/quantix/office_events.py`; create `src/features/Watchers.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t019.py`; register driver `T019` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** WatchSpec{id,scope,trigger,schedule,timezone,last_successful_check,next_due,stop_condition,budget,notification_policy,state}; create_watch(ctx,WatchDraft)->WatchReview; activate_watch(ctx,WatchActivation{fingerprint})->WatchSpec; run_due_watches(now)->WatchRunReceipt.

Watch activation creates a persistent approved accounting scope. Each due check is an attributable child attempt charged to that same scope; per-check run creation cannot replenish the lifetime or explicitly reviewed period allowance. Exhaustion pauses work until a new explicit allowance review. Record the next reset only when the engineer approved a recurring period budget; never infer one from a schedule. The scheduler supplies trusted identity from the saved activation, not from trigger text.

**Request/record details:** Watch: trigger type, selected sources/records/query, schedule/timezone, destinations, budget, stop/expiry, notification policy. Activation: preview fingerprint. Pause/stop/revise: expected revision and exact watch identity. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Support addendum/source change, supplier reply, deadline, quote expiry, market observation, failed reader and completed prerequisite triggers using current local records or permitted public research.
- [ ] 2. Give every watch explicit scope, frequency, budget, expiry/stop, owner and meaningful-change deduplication; activation authorizes only its displayed actions.
- [ ] 3. Persist last success and missed intervals; closed window may continue, sleep/stopped service may not claim continuous checking. Do not burst all missed checks on resume.
- [ ] 4. Allow pause/resume/stop, quiet unchanged state and exact blocker navigation; no watch may implicitly send correspondence or release a bid.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t019(run_case):
    result = run_case("T019", {"scenario": "watch_dedup_and_sleep", "same_observations": 3, "missed_checks": 2})
    assert result['notifications'] == 1
    assert result['missed_checks'] == 2
    assert result['commercial_sends'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t019.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T019.A1: Three identical observations create one notification and no duplicate draft/send.
- [ ] T019.A2: Sleeping through two checks shows missed checks and one revalidated catch-up, not fabricated success.
- [ ] T019.A3: Paused/stopped/exhausted watches make zero new model/search calls.
- [ ] T019.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t020"></a>

### T020: Model the Tender profile, measurement basis and engineering calendar

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T001, T013. **Coverage:** R05/R06; AO10/AO11; C30.

**Files and responsibility:** Create `backend/quantix/tender_profile.py`, `backend/quantix/tender_profile_models.py`, `backend/quantix/tender_calendar.py`; modify `backend/quantix/models.py`, `backend/quantix/repository.py`; `src/features/TenderDialogs.tsx` and new `src/features/TenderBrief.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t020.py`; register driver `T020` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** TenderProfile{contract_type,country,geography,currencies,working_languages,timezone,measurement_method,edition,source_refs,permitted_destinations}; CalendarEvent{id,kind,local_time,timezone,utc_time,mandatory,source_refs,owner,reminder_rule}; update_profile(ctx,ProfilePatch{expected_revision,...})->TenderProfile; save_event(ctx,CalendarEventDraft)->CalendarEvent.

**Request/record details:** Profile patch: expected revision, confirmed/unknown project fields and source refs. Calendar event: kind, original local time, timezone, mandatory/conditional state, owner, reminders and evidence. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Add explicit project profile fields without treating company defaults as confirmed Tender facts; retain unknown values and governing source/edition.
- [ ] 2. Store closing, clarification, site-visit, bond and internal-review events with IANA timezone plus original local wording; resolve ambiguous/nonexistent DST times explicitly.
- [ ] 3. Bind calculations/research/method selection to the applicable geography, currencies, units and measurement edition; a profile change invalidates affected draft bases.
- [ ] 4. Expose a concise brief and calendar inside existing Manager/Work views; connect approved reminders to T019.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t020(run_case):
    result = run_case("T020", {"scenario": "tender_profile_calendar", "local_deadline": "2026-09-30T12:00:00+03:00"})
    assert result['deadline_utc'] == '2026-09-30T09:00:00Z'
    assert result['unknown_measurement_rule_preserved'] is True
    assert result['accepted_money_changed'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t020.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T020.A1: A fixture deadline 12:00 at explicit UTC+03:00 normalizes to 09:00Z without losing original timezone text.
- [ ] T020.A2: Missing measurement rule remains unknown and prevents an assumed deduction rule.
- [ ] T020.A3: Changing profile currency does not convert accepted money or rewrite historical records.
- [ ] T020.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t021"></a>

### T021: Build the controlled company capability and document library

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T020, T014. **Coverage:** R04/R59/R72; AO18.

**Files and responsibility:** Create `backend/quantix/company_library.py`, `backend/quantix/company_library_models.py`; modify `backend/quantix/knowledge.py`, `backend/quantix/knowledge_routes.py`, `backend/quantix/storage.py`; create `src/features/CompanyLibrary.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t021.py`; register driver `T021` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** CompanyAsset{id,kind:project_sheet|cv|certificate|equipment|financial|template|method,artifact_version,scope,jurisdiction,issuer,valid_from,valid_until,verification,permitted_reuse}; propose_asset(ctx,CompanyAssetDraft)->CompanyAsset; approve_reuse(ctx,ReuseApproval{fingerprint,scope})->KnowledgeRecord.

**Request/record details:** Company asset: selected file version, kind, issuer/scope/jurisdiction/validity, verification and reuse policy. Approval/revocation: exact displayed asset fingerprint, permitted scope and rationale. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Preserve selected company source files and provenance, issue/expiry dates and permitted reuse separately from current-Tender evidence.
- [ ] 2. Extend existing approved notes rather than replacing them; allow searchable project sheets, CVs, certificates, equipment and financial attachments with explicit sensitive-data scope.
- [ ] 3. Offer only applicable current assets to qualification/document skills; expired or unverified records are visible gaps.
- [ ] 4. Include owned library objects in backups/reset and prevent a Tender-local correction or staff notebook from automatic company publication.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t021(run_case):
    result = run_case("T021", {"scenario": "company_asset_expiry_and_scope", "certificate_expired": True})
    assert result['eligibility_satisfied'] is False
    assert result['unauthorized_prompt_bytes'] == 0
    assert result['history_retained'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t021.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T021.A1: An expired certificate cannot satisfy a current eligibility requirement.
- [ ] T021.A2: Revoking company reuse prevents new task access while preserving existing historical references.
- [ ] T021.A3: A financial attachment cannot enter a provider prompt without its permitted destination/Tender scope.
- [ ] T021.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t022"></a>

### T022: Deliver public opportunity research, qualification and bid/no-bid briefs

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T020, T021, T040. **Coverage:** R01/R02/R03/R05; research §4/§8.

**Files and responsibility:** Create `backend/quantix/qualification.py`, `backend/quantix/qualification_models.py`; modify `backend/quantix/office_research.py`, `backend/quantix/office_manager_tools.py`; create `src/features/Qualification.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t022.py`; register driver `T022` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** Opportunity{notice_id,authority,url,geography,trades,value_range,currency,published_at,deadline,source_refs}; QualificationCheck{criterion,status:pass|fail|unknown,evidence,missing_input,exposure,effort,reviewer}; assess_opportunity(ctx,QualificationRequest)->QualificationBrief{checks,fit,effort,missing_capabilities,commercial_exposure,recommendation_basis}.

**Request/record details:** Opportunity query: geography/trades/scale/deadline/source preferences. Qualification: selected opportunity version, criteria and permitted company asset versions; missing attendance/experience remains explicit. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Research public notices only through approved search/page access; filter geography, trade, scale and deadline with source/date limits. Do not add a tender-portal feed connector.
- [ ] 2. Compare supplied experience, certificate and attendance criteria with approved company evidence; record pass/fail/unknown and exact gap.
- [ ] 3. Produce a concise bid/no-bid recommendation with estimating effort, capacity/fit, missing capability and commercial exposure; reserve the management decision to the engineer.
- [ ] 4. Connect deadline and optional approved monitoring to the calendar/watcher, preserving source history and notice duplicates.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t022(run_case):
    result = run_case("T022", {"scenario": "qualification_missing_attendance", "experience_matches": True, "attendance_record": None})
    assert result['experience_status'] == 'pass'
    assert result['attendance_status'] == 'unknown'
    assert result['commitments_created'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t022.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T022.A1: Missing compulsory-attendance evidence returns unknown, not pass.
- [ ] T022.A2: A stale/closed opportunity is flagged with its observed date; an unsupported automated portal path is not attempted.
- [ ] T022.A3: The brief cites each material eligibility finding and does not create a commercial commitment.
- [ ] T022.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t023"></a>

### T023: Complete document classification, drawing registers and package reconciliation

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T001, T020. **Coverage:** R07/R08/R09/R10/R11; C14.

**Files and responsibility:** Create `backend/quantix/document_control.py`, `backend/quantix/document_control_models.py`; modify `backend/quantix/intake.py`, `backend/quantix/documents.py`, `backend/quantix/repository.py`; `src/features/Files.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t023.py`; register driver `T023` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** DocumentClassification{artifact_id,version,kind,discipline,area,confidence,evidence,origin}; DrawingRegisterRow{drawing_number,title,revision,issue_purpose,sheet_size,title_block_locator}; reconcile_package(ctx,PackageReconciliationRequest{transmittal_refs,occurrences})->PackageReconciliation{missing,duplicates,unlisted,exceptions}.

**Request/record details:** Classification: selected occurrence/version, proposed kind/title-block fields, evidence/confidence; correction expected revision. Reconciliation: received occurrences and exact transmittal/list/reference sources. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Keep every received occurrence/path/hash/date/transmittal and byte-identical duplicate relation; reuse extraction content without collapsing building/package meaning.
- [ ] 2. Extract proposed document type and title-block fields with locators/confidence; allow engineer corrections without rewriting source bytes.
- [ ] 3. Compare transmittals, drawing lists and cross-references with actual received occurrences; retain ambiguous references as questions.
- [ ] 4. Show searchable/editable register fields and missing/partial records, with imported/extracted/analysed/reviewed counts computed independently.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t023(run_case):
    result = run_case("T023", {"scenario": "package_reconciliation", "received": ["Spec.pdf", "A-101.pdf", "Copies/A-101.pdf"], "listed": ["Spec.pdf", "A-101.pdf", "A-102.pdf"]})
    assert result['occurrences'] == 3
    assert result['unique_objects'] == 2
    assert result['missing'] == ['A-102.pdf']
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t023.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T023.A1: Three received occurrences containing two unique objects and one listed-but-missing drawing report all three facts separately.
- [ ] T023.A2: Two buildings sharing identical specifications retain two scope occurrences.
- [ ] T023.A3: A generated drawing number without located title-block evidence remains proposed/unknown.
- [ ] T023.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t024"></a>

### T024: Add extraction lineage, OCR and supported reader reprocessing

**Current status:** Partial 2026-09-10 — bounded reader reprocessing preserves originals; no claim that derivatives feed complete RAG, and remaining checklist/native/live evidence is open. **Dependencies:** T023. **Coverage:** R12/R13; AO08; research §7/§10.

**Files and responsibility:** Create `backend/quantix/extraction_adapters.py`, `backend/quantix/extraction_models.py`, `backend/quantix/extraction_worker.py`; modify `backend/quantix/documents.py`, `backend/quantix/document_word.py`, `backend/quantix/intake.py`, `backend/quantix/semantic.py`, `backend/quantix/storage.py`; `src/features/Files.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t024.py`; register driver `T024` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** ExtractionVersion{id,artifact_basis,reader_id,reader_version,model_hashes,settings,segments,coverage,exceptions,derived_object_hash}; inspect_reader(ctx,ReaderCandidate)->ReaderCheck; reprocess(ctx,ReprocessRequest{artifact_id,version,reader_fingerprint,selection}) -> RunReceipt.

**Request/record details:** Reader candidate: ID/version/models/settings/platform. Reprocess: artifact version/hash, exact reader fingerprint, selected pages/sheets, resource and approved destination/cost scope. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Benchmark Docling/layout/table extraction and OCR candidates on synthetic scanned/digital/mixed Arabic-English PDFs; retain precise existing spreadsheet and Word readers where they are better.
- [ ] 2. Pin chosen package/models/native runtime and licences; run heavy readers in bounded owned workers with CPU/memory/page/time limits and honest cancellation.
- [ ] 3. Save derivatives and extraction versions separately from immutable originals; do not replace evidence used by past results. Reindex only selected successful derivations.
- [ ] 4. Provide reprocess preview showing reader, cost/data destination, supported pages and impact; failed/partial OCR keeps page-level exceptions and original inspection available.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t024(run_case):
    result = run_case("T024", {"scenario": "ocr_partial_reprocess", "pages": 5, "successful_pages": 3})
    assert result['original_hash_unchanged'] is True
    assert result['extracted_pages'] == 3
    assert result['exception_pages'] == 2
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t024.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T024.A1: Reprocessing a scan creates a new extraction version while original hash and historical evidence remain unchanged.
- [ ] T024.A2: A timeout after 3 of 5 pages reports 3 extracted and 2 exceptions, never full success.
- [ ] T024.A3: Malicious/oversized input cannot escape the selected input/output boundary; no secret or source text enters diagnostics.
- [ ] T024.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t025"></a>

### T025: Strengthen spreadsheet integrity and structured BOQ interpretation

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T023. **Coverage:** R13/R23/R60.

**Files and responsibility:** Create `backend/quantix/spreadsheet_integrity.py`, `backend/quantix/spreadsheet_models.py`; modify `backend/quantix/documents.py`, `backend/quantix/estimates.py`, `backend/quantix/client_boq.py`; `src/features/Files.tsx`, `src/features/Estimate.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t025.py`; register driver `T025` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** SpreadsheetIssue{artifact_basis,sheet,range,kind,formula,cached_value,hidden_state,header_candidates,source_refs,severity}; inspect_workbook(ctx,WorkbookInspectionRequest)->WorkbookInspection; propose_boq_mapping(ctx,BoqMappingDraft)->MappingReview.

**Request/record details:** Inspection: exact workbook version/sheets/ranges. Mapping: explicit source rows/header candidates/quantity-unit-description columns and reviewed selections; original supplied values remain untouched. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Detect formula errors, missing caches, hidden sheets/rows/columns, merged headers, subtotal duplication and inconsistent quantity/unit columns; retain exact strings and formulas.
- [ ] 2. Present ambiguous BOQ mappings with actual representative rows; no automatic confirmation based solely on a misleading header.
- [ ] 3. Separate reading from recalculation; only use a validated optional calculation engine in an explicit derivative, preserving original VBA/OOXML and warnings.
- [ ] 4. Add row/cell issue navigation and intentional editable mapping; retain supplied quantities as baseline.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t025(run_case):
    result = run_case("T025", {"scenario": "spreadsheet_integrity", "formula": "#REF!", "cached_value": None, "reversed_headers": True})
    assert result['formula_error_reported'] is True
    assert result['missing_cache_value'] is None
    assert result['mapping_auto_approved'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t025.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T025.A1: A reversed header workbook maps actual quantity/unit only after displayed review; #REF! remains an issue.
- [ ] T025.A2: A missing cached formula is unknown, never zero; hidden totals are not silently included twice.
- [ ] T025.A3: Client XLSM macros and unmapped XML parts remain byte-preserved.
- [ ] T025.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t026"></a>

### T026: Create structural evidence chunks and project vocabulary

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T024, T025, T020. **Coverage:** R14/R61; AO08; research §7.

**Files and responsibility:** Create `backend/quantix/evidence_chunks.py`, `backend/quantix/evidence_chunk_models.py`, `backend/quantix/project_vocabulary.py`; modify `backend/quantix/semantic.py`, `backend/quantix/documents.py`; `src/features/DocumentSearch.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t026.py`; register driver `T026` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** EvidenceChunk{id,artifact_basis,extraction_version,parent_id,heading_path,kind,text,table_context,locator,coordinates,language,original_values}; build_chunks(ctx,ChunkBuildRequest)->ChunkManifest; VocabularyEntry{term,language,equivalent,context,source,status}.

**Request/record details:** Chunk build: selected extraction versions and chunk algorithm/version/settings. Vocabulary: original term/languages/equivalent/context/source/review state. Expansion: exact parent/adjacent/table extent. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Chunk clauses with headings and parent context; table rows retain column labels, units, cell coordinates, original numeric representations/formulas and merged-header context.
- [ ] 2. Normalize identifiers/units/terminology in separate search fields; preserve punctuation, original text and ambiguous bilingual abbreviations.
- [ ] 3. Persist extraction/chunk/model fingerprints and scope occurrences, including repeated identical sources across buildings.
- [ ] 4. Support page/region/table/adjacent-context expansion by exact locator and bounded selection.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t026(run_case):
    result = run_case("T026", {"scenario": "structural_table_chunk", "identifier": "A-101", "near_identifier": "A-101A"})
    assert result['headers_and_units_retained'] is True
    assert result['identifier_collision'] is False
    assert result['source_text_unchanged'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t026.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T026.A1: A retrieved rate row includes its unit, currency and column headers without changing original numeric text.
- [ ] T026.A2: A-101 and A-101A remain distinct identifiers.
- [ ] T026.A3: An ambiguous abbreviation does not silently rewrite the source or become an approved glossary entry.
- [ ] T026.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t027"></a>

### T027: Implement measured hybrid retrieval, bounded reranking and bilingual search

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T026. **Coverage:** AO08; C15; research retrieval pipeline.

**Files and responsibility:** Create `backend/quantix/retrieval.py`, `backend/quantix/retrieval_models.py`; modify `backend/quantix/semantic.py`, `backend/quantix/office_tools.py`; `src/features/DocumentSearch.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t027.py`; register driver `T027` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** RetrievalQuery{text,collection_ids,source_basis,filters,limit,mode}; RetrievalHit{source_ref,passage,locator,method,ranks,fusion_score,rerank_score,limitations}; retrieve(ctx,RetrievalQuery)->RetrievalPage.

**Request/record details:** Query: text, allowed collection/source selection, discipline/area/revision filters, mode/limit; no model-supplied arbitrary SQL. Reranker identity and candidate limits come from installed capability policy. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Filter Tender/collection/revision/discipline/area before candidate generation; retrieve exact identifiers/keywords, semantic candidates and permitted structured records.
- [ ] 2. Use reciprocal-rank fusion with recorded parameters rather than mixing raw scores; benchmark bounded reranking on the same labelled corpus.
- [ ] 3. Keep exact search usable when embeddings/reranker are unavailable and disclose actual retrieval mode; no silent semantic-success claim.
- [ ] 4. Evaluate Arabic↔English terminology/numerals, contradictions and historical comparison inputs; return source-linked gaps when retrieval is insufficient.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t027(run_case):
    result = run_case("T027", {"scenario": "hybrid_retrieval", "semantic_available": False, "query": "A-101"})
    assert result['exact_identifier_found'] is True
    assert result['semantic_used'] is False
    assert result['cross_tender_hits'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t027.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T027.A1: All protected cross-Tender candidates are excluded before model context; identifier set retrieves the exact revision.
- [ ] T027.A2: On the locked benchmark, recall@10 meets the declared gate and does not regress baseline groups unnoticed.
- [ ] T027.A3: A unavailable semantic model returns explicit limitations and valid exact results if requested, not fabricated embeddings.
- [ ] T027.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t028"></a>

### T028: Enforce knowledge collections and evidence relationship traversal

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T021, T027. **Coverage:** R14/R15; AO04/AO08/AO18.

**Files and responsibility:** Create `backend/quantix/evidence_relationships.py`, `backend/quantix/knowledge_collection_models.py`; modify `backend/quantix/office_knowledge.py`, `backend/quantix/office_business.py`, `backend/quantix/office_project.py`, `backend/quantix/repository.py`.

**Test owner:** Create `backend/tests/agentic/test_t028.py`; register driver `T028` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** KnowledgeCollection{kind:tender_evidence|tender_working|company_approved|external_research,scope,policy}; EvidenceRelation{from_ref,to_ref,kind,basis,status,author}; query_relations(ctx,RelationQuery{start_ref,kinds,depth,limit})->RelationPage.

**Request/record details:** Relation: exact from/to refs, relation kind, basis, confidence and author from ctx. Query: start_ref, allowlisted relation types/depth/limit. Collection scope resolved by current permissions. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Maintain the four distinct knowledge collections with inherited access/destination policy; do not expose all company/Tender content because one collection is allowed.
- [ ] 2. Connect BOQ rows, requirements, details, schedules, drawings, quotations, calculations and outputs through typed evidence-backed relationships.
- [ ] 3. Provide bounded relationship and scoped aggregate queries; query templates operate on allowed domain records, never arbitrary model SQL.
- [ ] 4. Retain relationship confidence and engineer resolution; stale links remain historical and cannot establish current applicability.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t028(run_case):
    result = run_case("T028", {"scenario": "collection_relationship_scope", "collection": "tender_evidence"})
    assert result['unapproved_company_records'] == 0
    assert result['relationship_versions_exact'] is True
    assert result['arbitrary_sql_executed'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t028.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T028.A1: A permitted BOQ→specification→drawing→quote traversal returns exact versions and respects each linked record's scope.
- [ ] T028.A2: An aggregate unpriced-items query uses only the selected package and exact units/currencies.
- [ ] T028.A3: Unapproved cross-Tender/company records never enter the result or prompt.
- [ ] T028.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t029"></a>

### T029: Validate claim support, contradictions and contractual precedence

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T027, T028, T012. **Coverage:** AO08/AO14; research §7/§8/§14.

**Files and responsibility:** Create `backend/quantix/claim_support.py`, `backend/quantix/claim_models.py`, `backend/quantix/precedence_rules.py`; modify `backend/quantix/office.py`, `backend/quantix/office_types.py`, `backend/quantix/office_research.py`; `src/features/WorkDecisions.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t029.py`; register driver `T029` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** ClaimAssessment{claim,source_refs,inspected_extents,support:supported|contradicted|insufficient,method,limitations}; assess_claim(ctx,ClaimRequest)->ClaimAssessment; PrecedenceDecision{conflicting_refs,applicability,rule_source,engineer_resolution,version}.

**Request/record details:** Claim: statement and exact inspected supporting/contradicting refs. Precedence: conflicting source versions, governing rule/evidence and explicit engineer resolution if supplied; no timestamp-only override. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Check that cited passages/rows/regions support each material claim, not merely that the IDs/URLs were returned or read.
- [ ] 2. Separate deterministic identity/numeric checks, model support assessment and engineer applicability; expose uncertain support and abstain instead of fabricating facts.
- [ ] 3. Record issue purpose/contract hierarchy and explicit conflict resolution; latest timestamp alone never supersedes a governing source.
- [ ] 4. Gate observation/quantity/rate proposals on complete evidence or explicit labelled assumptions, preserving required engineer decisions.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t029(run_case):
    result = run_case("T029", {"scenario": "claim_support_and_precedence", "url_exists": True, "price_in_passage": False})
    assert result['observed_price_supported'] is False
    assert result['latest_timestamp_wins_automatically'] is False
    assert result['unresolved_conflict_visible'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t029.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T029.A1: A genuine returned URL without the claimed price passage fails observed-price support.
- [ ] T029.A2: A newer informal email does not automatically override a contract drawing.
- [ ] T029.A3: Contradictory fire-rating clauses produce a linked unresolved conflict and no unsupported accepted rating.
- [ ] T029.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t030"></a>

### T030: Propagate revision, decision and assumption impact

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T028, T029, T032. **Coverage:** R21/R22/R39; AO09/AO13.

**Files and responsibility:** Create `backend/quantix/evidence_dependencies.py`, `backend/quantix/dependency_models.py`, `backend/quantix/assumption_impacts.py`; modify `backend/quantix/repository.py`, `backend/quantix/estimates.py`, `backend/quantix/tender_requirements.py`, `backend/quantix/outputs.py`, `backend/quantix/office_events.py`.

**Test owner:** Create `backend/tests/agentic/test_t030.py`; register driver `T030` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** DependencyEdge{input_ref,derived_ref,relationship,method_version}; AssumptionRecord{statement,origin,refs,affected_records,cost_impact,programme_impact,expiry_condition,state,submission_locations}; assess_impact(ctx,ChangeBasis)->ImpactPreview; revalidate_dependants(ctx,ImpactReviewRequest)->ImpactResult.

**Request/record details:** Impact: changed source/decision/method reference and proposed replacement. Assumption: statement, origin, refs, affected IDs, expiry predicate and impact basis. Revalidation: exact impact fingerprint and scope. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Track source/extraction/decision/method→requirement→quantity→rate/quote→estimate/programme→output edges at draft creation/publication.
- [ ] 2. On change, traverse affected edges, mark review/currentness accurately and preserve immutable accepted history; batch invalidation atomically with events.
- [ ] 3. Give assumptions explicit validity/expiry conditions and links to calculations, scope, cost/programme and submission use.
- [ ] 4. Preview what a decision will change before acceptance; recompute drafts only within reviewed authority, and never silently reprice accepted records.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t030(run_case):
    result = run_case("T030", {"scenario": "revision_dependency_propagation", "changed_package": "A", "unchanged_package": "B"})
    assert result['package_A_needs_review'] is True
    assert result['package_B_current'] is True
    assert result['accepted_history_erased'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t030.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T030.A1: Changing one drawing marks its dependent quantities/rates/outputs while leaving an unrelated package current.
- [ ] T030.A2: An expired assumption invalidates linked scenario/calculation applicability and creates one actionable question.
- [ ] T030.A3: A withdrawn accepted decision remains inspectable; a stale approval fingerprint cannot apply the new basis.
- [ ] T030.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t031"></a>

### T031: Complete scope, responsibilities, compliance and interface review

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T023, T028, T029, T030, T015. **Coverage:** R15–R22; AO15.

**Files and responsibility:** Create `backend/quantix/scope_review.py`, `backend/quantix/scope_review_models.py`; modify `backend/quantix/project_map.py`, `backend/quantix/tender_requirements.py`, `backend/quantix/office_project.py`; `src/features/ProjectMap.tsx`, `src/features/SubmissionRequirements.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t031.py`; register driver `T031` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** ScopeReview{scope_nodes,responsibilities,compliance_rows,interfaces,clarifications}; ResponsibilityBoundary{party,scope,interface,evidence,owner,unresolved_question}; ComplianceRow{requirement,criterion,proposed_compliance,evidence,gap,reviewer}; review_scope(ctx,ScopeReviewRequest)->ScopeReview.

**Request/record details:** Scope review: selected packages/buildings/trades, requirement/BOQ/drawing/spec versions and responsibility criteria. Proposed rows include evidence, unknown owner, review state and requested clarification. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Extend existing building/level/zone/system/trade/package hierarchy and evidence links without parallel scope storage.
- [ ] 2. Compare BOQ descriptions/quantities/ratings/finishes with specs, schedules and drawings; keep mismatches as traceable proposals.
- [ ] 3. Review contractor/subcontractor/supplier/employer/nominated-party boundaries and builder's work, penetrations, supports, power, controls, testing and reinstatement interfaces.
- [ ] 4. Draft concise source-linked clarifications and assumptions/exclusions with required decisions, affected records and expiry/impact.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t031(run_case):
    result = run_case("T031", {"scenario": "scope_interface_conflict", "boq_rating": "60 min", "spec_rating": "120 min", "builders_work_owner": None})
    assert result['rating_conflict'] is True
    assert result['interface_owner'] is None
    assert result['clarifications_have_sources'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t031.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T031.A1: A fire-rating mismatch and missing builder's-work responsibility are detected as separate issues with exact sources.
- [ ] T031.A2: An unresolved interface is not assigned to a party by guess or treated as priced.
- [ ] T031.A3: A compliance matrix can be exported with proposed/check/engineer-review states distinct.
- [ ] T031.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t032"></a>

### T032: Create the reproducible unit-aware calculation core

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T013, T020. **Coverage:** R24/R28/R32/R33; AO11.

**Files and responsibility:** Create `backend/quantix/calculations.py`, `backend/quantix/calculation_models.py`, `backend/quantix/units.py`; modify `backend/quantix/estimates.py`, `backend/quantix/measurements.py`, `backend/quantix/office_quantities.py`; create `src/features/CalculationInspector.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t032.py`; register driver `T032` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** CalculationRecord{id,method_id,method_version,code_or_formula_hash,typed_inputs,input_refs,units,assumptions,precision,rounding,outputs,checks,basis_fingerprint,status}; calculate(ctx,CalculationRequest)->CalculationRecord; check_calculation(ctx,CalculationCheckRequest)->CalculationCheck.

**Request/record details:** Calculation: method ID/version, named typed inputs with units/source refs, explicit assumptions, precision/rounding/tolerance and output schema. Check: exact calculation version and independent method/check selection. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Keep Decimal money arithmetic; add dimension-checked quantities and explicit geometry/numerical tolerances. Validate Pint or an equivalent maintained unit engine before adoption.
- [ ] 2. Record every input, conversion, selected method/edition, formula/version, source snapshot, assumption, rounding stage and independent check.
- [ ] 3. Expose deterministic tools for sum/product/unit conversion/formula templates; user-defined or generated methods require the appropriate reviewed/script/sandbox capability.
- [ ] 4. Provide Explain this number with source inputs, workings, status and reproducible recalculation; a calculation draft changes no accepted BOQ basis.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t032(run_case):
    result = run_case("T032", {"scenario": "unit_calculation", "quantity": "2.5", "factor": "0.24", "invalid_add_units": ["m2", "m3"]})
    assert result['product'] == '0.60'
    assert result['dimension_error'] is True
    assert result['reproducible'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t032.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T032.A1: 2.5 tonnes × 0.24 tCO2e/tonne yields exactly 0.60 tCO2e under its declared rounding.
- [ ] T032.A2: Adding m2 to m3 fails dimensional validation; unknown units never become unitless.
- [ ] T032.A3: Recomputing unchanged inputs/method returns identical outputs/hash; changed rounding changes the method basis.
- [ ] T032.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t033"></a>

### T033: Complete calibrated measurement and supplied-quantity reconciliation

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T025, T032. **Coverage:** R23/R24/R25/R31.

**Files and responsibility:** Modify `backend/quantix/measurements.py`, `backend/quantix/measurement_models.py`, `backend/quantix/office_measurement.py`, `backend/quantix/office_quantities.py`, `backend/quantix/estimates.py`; `src/features/MeasurementCanvas.tsx`, `src/features/Measurements.tsx`, `src/features/QuantityForm.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t033.py`; register driver `T033` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** Extend MeasurementInput with sheet/viewport calibration identity, stated-dimension check and method tolerance; reconcile_quantity(ctx,QuantityReconciliationRequest{supplied_item_id,measurement_refs,rule_version})->QuantityProposal.

**Request/record details:** Measurement: artifact/page/viewport/hash, geometry/coordinate system, calibration/dimension basis, tolerance and rule version. Reconcile: supplied item ID/basis and measurement refs, never an implicit approved replacement. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Preserve supplied BOQ as commercial default; calculate length/area/count/volume proposals from explicit geometry and supporting dimensions.
- [ ] 2. Bind calibration to page/viewport/version/hash, coordinate system, known dimension and tolerance; flag conflicts with stated dimensions.
- [ ] 3. Keep quantities, rule/deductions, drawing scale, selected BOQ row and evidence visible together; allow manual inspection/confirmation.
- [ ] 4. Revalidate exact source/item/calibration basis before adopting a measurement; preserve original supplied value and accepted history.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t033(run_case):
    result = run_case("T033", {"scenario": "boq_measurement_basis", "supplied": "12.5", "measured": "15"})
    assert result['effective_before_approval'] == '12.5'
    assert result['proposed_quantity'] == '15'
    assert result['wrong_page_calibration_blocked'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t033.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T033.A1: Supplied 12.5 m3 and measured 15 m3 remain separate; estimate uses 12.5 until explicit approval.
- [ ] T033.A2: A changed drawing invalidates the measurement/current proposal without erasing historical workings.
- [ ] T033.A3: A calibration from another page/viewport cannot silently apply.
- [ ] T033.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t034"></a>

### T034: Add assemblies, measurement rules, deductions and assisted repetition

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T033, T024, T015. **Coverage:** R26/R27/R28/R31; BIM excluded.

**Files and responsibility:** Create `backend/quantix/assembly_takeoff.py`, `backend/quantix/measurement_rules.py`, `backend/quantix/repetition_detection.py`; modify `backend/quantix/measurements.py`, `backend/quantix/office_measurement.py`; `src/features/MeasurementCanvas.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t034.py`; register driver `T034` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** AssemblyCalculation{components,dimensions,counts,rule_version,deductions,allowances,source_refs}; RepetitionProposal{region,template,instances,confidence,coverage,confirmation}; calculate_assembly(ctx,AssemblyRequest)->CalculationRecord; propose_repetitions(ctx,RepetitionRequest)->RepetitionProposal.

**Request/record details:** Assembly: component IDs/dimensions/counts/source refs, rule/edition, deduction/allowance basis. Repetition: source region/template and permitted detection settings; confirmations identify actual inspected instances. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Implement room/wall/slab/foundation/service-system assemblies from located dimensions and explicit counts; compose existing calculation tools.
- [ ] 2. Version governing measurement rules, thresholds, opening deductions, overlaps, laps and wastage; require applicability/edition rather than universal assumptions.
- [ ] 3. Evaluate supported visual template/symbol detection; show proposed instances and false/ambiguous matches for confirmation, never claim complete takeoff from partial visual coverage.
- [ ] 4. Cross-check assemblies against BOQ and prevent overlapping selections from double counting.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t034(run_case):
    result = run_case("T034", {"scenario": "assembly_and_repetition", "gross_area": "30", "opening": "2", "true_instances": 8, "ambiguous_instances": 2})
    assert result['net_area'] == '28.00'
    assert result['ambiguous_instances'] == 2
    assert result['auto_approved_count'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t034.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T034.A1: Wall gross 30 m2 less one 2 m2 opening yields 28 m2 only under the selected deductible-opening rule.
- [ ] T034.A2: A repeated-symbol fixture with 8 true and 2 ambiguous candidates does not auto-approve 10 items.
- [ ] T034.A3: Changing the deduction edition invalidates the affected calculation and retains the original.
- [ ] T034.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t035"></a>

### T035: Validate supported 2D CAD and legacy document conversion

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T024, T032, T033. **Coverage:** R30; spec supported DOC/DWG; research §10.

**Files and responsibility:** Create `backend/quantix/cad_reader.py`, `backend/quantix/cad_models.py`, `backend/quantix/legacy_converter.py`; modify `backend/quantix/extraction_adapters.py`, `backend/quantix/documents.py`, `backend/quantix/ai_components.py`; `src/features/Files.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t035.py`; register driver `T035` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** ConversionCapability{formats,versions,licence,runtime_hash,coverage,platforms}; inspect_cad(ctx,CadReadRequest{artifact_basis,layers,entities}) -> CadEvidence; convert_local(ctx,ConversionRequest{source_basis,converter_version}) -> ExtractionVersion.

**Request/record details:** CAD read: exact source/derivative version, layers/entities, units and converter capability. Conversion: selected format/version/converter hash/settings; no arbitrary command line or uncontrolled xref paths. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Pilot ezdxf for DXF entity/layer/units/extents and a permitted ODA/licensed route for DWG; pilot a supported local DOC conversion route. Record exact supported versions and redistribution constraints.
- [ ] 2. Treat xrefs, fonts, proxy/custom entities, missing units and unsupported geometry as coverage exceptions; preserve originals and derivative provenance.
- [ ] 3. Bind selected CAD geometry to calculation/takeoff tools with explicit unit/scale and source entities; inspect visual conversion fidelity.
- [ ] 4. Keep converter binaries/dependencies/owned temp under established managed boundaries, process limits and cancellation; no Autodesk/BIM account connector.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t035(run_case):
    result = run_case("T035", {"scenario": "cad_units_and_xref", "width": "4", "height": "5", "unit": "m", "missing_xref": True})
    assert result['area'] == '20.00'
    assert result['complete_coverage'] is False
    assert result['original_preserved'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t035.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T035.A1: A DXF rectangle with explicit 4 m × 5 m dimensions yields 20 m2 and exact entity references.
- [ ] T035.A2: A missing xref or unit blocks a claim of complete drawing coverage.
- [ ] T035.A3: DWG/DOC converter unavailable produces a supported fallback/exception, not falsely successful extraction.
- [ ] T035.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t036"></a>

### T036: Extend rate build-ups, productivity resources and preliminaries

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T032, T020, T041. **Coverage:** R33/R34/R35/R36.

**Files and responsibility:** Create `backend/quantix/rate_engine.py`, `backend/quantix/preliminaries.py`, `backend/quantix/rate_engine_models.py`; modify `backend/quantix/estimates.py`, `backend/quantix/estimate_models.py`, `backend/quantix/office_business.py`; `src/features/RateForm.tsx`, `src/features/EstimateEditor.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t036.py`; register driver `T036` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** ResourceComponent{category:material|labour|plant|subcontract,resource_id,quantity,unit,rate,currency,productivity_basis,source_ref}; RateBuildUp{components,commercial_layers,method_version}; PreliminaryItem{fixed_or_time_related,duration_basis,resource_cost,calendar,assumptions}; calculate_rate(ctx,RateRequest)->CalculationRecord.

**Request/record details:** Rate: typed resource components/productivity, explicit commercial layers with base/order, precision and provenance. Preliminary: fixed/time-related type, duration/calendar/resource rates and assumptions. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Separate material/labour/plant/subcontract and productivity inputs, with exact units and source provenance; retain existing direct-rate path.
- [ ] 2. Model fixed and duration-dependent staff/facilities/mobilization/temp services/project allowances explicitly.
- [ ] 3. Calculate commercial layers only on their declared base and order; expose uncertainty and source refresh requirements.
- [ ] 4. Show full workings and independent check before creating a rate proposal; preserve engineer rate/quantity authority.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t036(run_case):
    result = run_case("T036", {"scenario": "layered_rate", "direct": "125", "waste_percent": "5", "delivery": "10", "overhead_percent": "10", "markup_percent": "8", "vat_percent": "14"})
    assert result['rate_ex_vat'] == '167.81'
    assert result['rate_inc_vat'] == '191.30'
    assert result['layer_bases_visible'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t036.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T036.A1: Material 80 + labour 40 + plant 5 = direct 125; 5% waste on direct, delivery 10, 10% overhead then 8% markup yields 167.81 ex-VAT using HALF_UP at final rate.
- [ ] T036.A2: 14% VAT on that rounded rate yields 191.30; the example tax is a stated fixture, not jurisdictional advice.
- [ ] T036.A3: A duration change affects time-related preliminaries only; fixed mobilization stays unchanged.
- [ ] T036.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t037"></a>

### T037: Complete commercial bases, price provenance and estimate completeness

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T036, T031, T041. **Coverage:** R35/R36/R37/R40.

**Files and responsibility:** Create `backend/quantix/estimate_completeness.py`, `backend/quantix/commercial_basis_models.py`; modify `backend/quantix/estimate_models.py`, `backend/quantix/estimates.py`, `backend/quantix/office_business.py`; `src/features/Estimate.tsx`, `src/features/RateProposals.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t037.py`; register driver `T037` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** CommercialBasis{currency,supply_basis,delivery,discount,waste,overhead,markup_or_margin,contingency,tax,validity,exclusions}; PriceProvenance{kind:observed|quoted|historical|allowance|analyst_estimate,observation_ref,quote_ref,dates,conditions}; assess_estimate(ctx,EstimateCheckRequest)->EstimateCheck{omissions,duplicates,unit_conflicts,unpriced,exposure_ranking}.

**Request/record details:** Estimate check: selected current item/rate/quantity/requirement bases and commercial policy. Price classification: source evidence, dates/validity, geography/conditions and explicit observed/quoted/historical/allowance/estimate basis. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Separate markup from gross-margin semantics, tax/FX/date basis and mutually exclusive inclusions; migrate observed/estimated history without inventing quote status.
- [ ] 2. Prioritize missing prices by stated quantity/exposure/mandatory scope, reporting unknown exposure separately.
- [ ] 3. Check duplicated pricing, incompatible units, omitted mandatory work, stale rates/quantities and incomplete commercial terms.
- [ ] 4. Present current/proposed value, reason, sources and affected outputs with exact acceptance fingerprint; unknown tax/currency yields incomplete totals.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t037(run_case):
    result = run_case("T037", {"scenario": "estimate_completeness", "mandatory_unpriced": 1, "currencies": ["EGP", "USD"], "fx": None})
    assert result['complete'] is False
    assert result['combined_total'] is None
    assert result['missing_price_priority_visible'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t037.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T037.A1: A mandatory unpriced item blocks estimate completeness even when all visible priced lines add correctly.
- [ ] T037.A2: A supplier web estimate cannot be labelled a quotation without a quote artifact.
- [ ] T037.A3: Two currencies without an approved dated FX conversion are not summed into one total.
- [ ] T037.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t038"></a>

### T038: Explain cost movement and evaluate value-engineering alternatives

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T018, T030, T037. **Coverage:** R38/R39/R41/R69/R72.

**Files and responsibility:** Create `backend/quantix/cost_movement.py`, `backend/quantix/value_engineering.py`; modify `backend/quantix/scenarios.py`; `src/features/ScenarioComparison.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t038.py`; register driver `T038` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** CostMovement{quantity_effect,specification_effect,supplier_effect,fx_effect,duration_effect,residual,method}; ValueOption{base,alternative,savings,compliance,warranty,programme,maintenance,approval_requirements,evidence}; compare_cost_bases(ctx,CostComparisonRequest)->CostMovement; assess_alternative(ctx,ValueOptionRequest)->ValueOption.

**Request/record details:** Comparison: exact old/new scope/quantity/rate/FX/duration bases and decomposition order. Alternative: proposed product/method, source-backed compliance/warranty/programme/maintenance/approval effects. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Define and record a decomposition order so interaction effects are attributed once; preserve original and normalized bases.
- [ ] 2. Compare alternatives using complete scope/commercial equivalence, supported performance/warranty/maintenance evidence and required approval.
- [ ] 3. Do not rank by lowest headline price where missing terms or noncompliance prevent comparison.
- [ ] 4. Return inspectable scenario artifacts and unresolved decisions; adopting an alternative never silently changes accepted design/commercial scope.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t038(run_case):
    result = run_case("T038", {"scenario": "cost_decomposition", "base_quantity": "100", "base_rate": "100", "new_quantity": "98", "new_rate": "105"})
    assert result['quantity_effect'] == '-200.00'
    assert result['rate_effect'] == '490.00'
    assert result['total_change'] == '290.00'
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t038.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T038.A1: Estimate 100×100=10000 and actual 98×105=10290 decompose quantity -200 plus rate +490 = +290 under the declared order.
- [ ] T038.A2: A cheaper noncompliant alternative is flagged, not recommended as equivalent.
- [ ] T038.A3: All displayed effects sum to total movement or expose an explicit residual.
- [ ] T038.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t039"></a>

### T039: Model cash flow, purchasing timing and negotiation scenarios

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T018, T037, T038, T047. **Coverage:** R38/R42/R69; novel purchase-timing acceptance.

**Files and responsibility:** Create `backend/quantix/cash_flow.py`, `backend/quantix/cash_flow_models.py`, `backend/quantix/negotiation.py`; modify `backend/quantix/scenarios.py`; `src/features/ScenarioComparison.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t039.py`; register driver `T039` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** CashFlowBasis{payment_milestones,advance,retention,invoice_dates,procurement_dates,tax_basis,currencies,fx_refs,opening_cash}; CashFlowResult{periods,inflows,outflows,balance,peak_deficit,missing_terms}; NegotiationScenario{released_basis,discount,scope_changes,alternatives,effects}; calculate_cash_flow(ctx,CashFlowRequest)->CashFlowResult.

**Request/record details:** Cash flow: dated inflows/outflows/milestones, retention/advance/tax/currency/opening cash and procurement assumptions. Negotiation: exact submitted basis, discount semantics, exclusions/scope alternatives and rationale. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Use explicitly supplied payment/retention/procurement/order/delivery assumptions and calendar; unknown terms create incomplete periods, not convenient dates.
- [ ] 2. Compare buying together versus staged procurement with cash limits, storage constraints and programme dependencies.
- [ ] 3. Model percentage discount, exclusions, scope changes and alternatives against an immutable estimate/submitted baseline; record commercial effects separately.
- [ ] 4. Keep all scenarios draft; material adoption/revised offer requires the exact reviewed decision, with no mutation of the released bid.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t039(run_case):
    result = run_case("T039", {"scenario": "cash_and_negotiation", "opening": "1000", "outflow": "700", "inflow": "200", "released": "10000", "discount_percent": "5"})
    assert result['balances'] == ['300.00','500.00']
    assert result['discounted_scenario'] == '9500.00'
    assert result['released_total'] == '10000.00'
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t039.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T039.A1: Opening cash 1000, outflow 700 then inflow 200 gives balances 300 and 500.
- [ ] T039.A2: Missing payment date is visible and prevents a claimed complete cash forecast.
- [ ] T039.A3: A 5% discount on 10000 yields scenario 9500 while released total stays 10000.
- [ ] T039.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t040"></a>

### T040: Add reusable research briefs and a governed search/content gateway

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T013, T020. **Coverage:** R01/R43/R44; AO10; research §8.

**Files and responsibility:** Create `backend/quantix/research_gateway.py`, `backend/quantix/research_models.py`, `backend/quantix/web_capture.py`; modify `backend/quantix/office_research.py`, `backend/quantix/ai_api_engine.py`, `backend/quantix/ai_policy.py`; `src/features/Work.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t040.py`; register driver `T040` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** ResearchBrief{question,tender_scope,geography,specifications,date_range,preferred_sources,output,search_budget,stop_criteria}; search(ctx,SearchRequest)->SearchReceipt; capture_page(ctx,PageCaptureRequest{url,selection}) -> CapturedSource{url,redirect_chain,passage,locator,hash,dates,rights_limits}.

**Request/record details:** Research: question, geography/spec/date/source criteria, output/stopping conditions and scope. Capture: provider-returned or explicitly permitted URL plus selected extent; capability owns redirect/time/size/egress policy. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Retain approved provider-native search as baseline; compare one independent adapter candidate on the same Tender queries if coverage/control is insufficient, recording cost/language/freshness/rights results.
- [ ] 2. Provide opportunity, product/engineering and supplier/rate research methods with subquestions, original-source reads, stopping criteria and useful partial results.
- [ ] 3. Validate HTTP(S) destinations, redirects, private-address/DNS changes, credentials and allowed content size; no paywall/CAPTCHA bypass or excluded portal automation.
- [ ] 4. Use simple selected-page capture first; enable a controlled browser/extraction fallback only for permitted pages and under the same scope/budget. Search discovery snippets are not sufficient price evidence.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t040(run_case):
    result = run_case("T040", {"scenario": "research_budget_and_redirect", "max_searches": 2, "attempts": 3, "redirect": "http://127.0.0.1/private"})
    assert result['searches'] == 2
    assert result['private_redirect_blocked'] is True
    assert result['partial_findings_retained'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t040.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T040.A1: A redirect into a prohibited/private address is denied before content access.
- [ ] T040.A2: Search budget exhaustion stops further calls while preserving sourced partial findings.
- [ ] T040.A3: A provider-independent route change requires an approved account/destination/cost basis, never silent billing fallback.
- [ ] T040.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t041"></a>

### T041: Persist complete market observations and claim-level commercial evidence

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T040, T029, T032. **Coverage:** R35/R36/R43; AO10.

**Files and responsibility:** Create `backend/quantix/market_observations.py`, `backend/quantix/market_models.py`; modify `backend/quantix/office_research.py`, `backend/quantix/estimate_models.py`, `backend/quantix/office_business.py`; create `src/features/MarketEvidence.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t041.py`; register driver `T041` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** MarketObservation{product,manufacturer,model,grade,dimensions,rating,amount,currency,unit,pack_size,minimum_order,quantity_break,supply_basis,delivery_point,freight,unloading,waste,exclusions,tax,payment,discount,validity,lead_time,published_on,observed_on,retrieved_at,geography,source_ref,passage_hash,status,missing_terms}; record_observation(ctx,ObservationDraft)->MarketObservation.

**Request/record details:** Observation: original product/price/pack/order/supply/tax/payment/validity/location/time fields, captured passage/source hash and assessment. Normalization: explicit conversion/calculation refs; unknowns remain null. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Capture original product/commercial text and typed normalized fields with source passage/locator/hash; retain unknown fields explicitly.
- [ ] 2. Distinguish observed page price, supplier quotation, historical accepted rate, allowance and estimate; ensure source support and date applicability.
- [ ] 3. Record conversions as reproducible calculations without overwriting original price/unit/pack data; tax, freight and supply-only/installed comparisons require explicit bases.
- [ ] 4. Provide source-linked observation review and revalidation when reused in another date/package/Tender scope.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t041(run_case):
    result = run_case("T041", {"scenario": "market_observation", "price": "100", "pack_kg": "20", "price_supported": True, "delivery_basis": None})
    assert result['normalized_per_kg'] == '5.00'
    assert result['missing_terms'] == ['delivery_basis']
    assert result['complete_commercial_basis'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t041.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T041.A1: A price 100 per 20 kg bag converts to 5 per kg only with confirmed pack size.
- [ ] T041.A2: A returned URL lacking the claimed price cannot produce an observed status.
- [ ] T041.A3: A future observation date, missing delivery basis or incompatible grade remains rejected/incomplete as appropriate.
- [ ] T041.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t042"></a>

### T042: Build comparable market intelligence and refresh priorities

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T041, T019, T018. **Coverage:** R43/R51/R52; research §8 market intelligence.

**Files and responsibility:** Create `backend/quantix/market_intelligence.py`, `backend/quantix/market_series_models.py`; modify `backend/quantix/scenarios.py`, `backend/quantix/office_watchers.py`; `src/features/MarketEvidence.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t042.py`; register driver `T042` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** MarketSeriesKey{product_spec,unit,geography,supply_basis,tax_basis,currency}; RefreshPriority{observation_id,volatility,expiry,exposure,procurement_date,reason}; market_brief(ctx,MarketBriefRequest)->WorkProductVersion; propose_equivalence(ctx,EquivalenceRequest)->EquivalenceReview.

**Request/record details:** Series: exact comparability dimensions/date range; refresh: selected records/exposure/order-date inputs. Equivalence: required performance/approval/warranty criteria and sourced candidate evidence. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Maintain price series only for comparable product/grade/supply/tax/location bases; preserve dates and quantity breaks.
- [ ] 2. Prioritize refresh by volatility, expiry, project exposure and order date; watches notify only material changes.
- [ ] 3. Compare product equivalence through required performance, certification, warranty and approval evidence; no inferred substitution.
- [ ] 4. Provide supplier/package coverage maps, procurement-timing comparisons and concise market-change briefs with uncertainties and next actions.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t042(run_case):
    result = run_case("T042", {"scenario": "market_series", "bases": ["supply_only", "installed"], "quote_expires_before_order": True})
    assert result['series_count'] == 2
    assert result['refresh_gaps'] == 1
    assert result['unsupported_equivalence'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t042.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T042.A1: Supply-only and installed prices or different grades never silently join one series.
- [ ] T042.A2: A quote expiring before expected order date creates one refresh gap.
- [ ] T042.A3: An alternative missing required warranty evidence is not declared equivalent.
- [ ] T042.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t043"></a>

### T043: Research and verify suppliers against explicit evidence

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T040, T041, T021. **Coverage:** R44/R45.

**Files and responsibility:** Create `backend/quantix/supplier_registry.py`, `backend/quantix/supplier_models.py`; modify `backend/quantix/office_business.py`, `backend/quantix/correspondence.py`; `src/features/Quotes.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t043.py`; register driver `T043` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** SupplierRecord{legal_name,type,capabilities,brands,authorized_brand_evidence,service_area,certificates,contacts,verified_at,unknowns,source_refs}; discover_suppliers(ctx,SupplierQuery)->SupplierCandidates; verify_supplier(ctx,SupplierVerificationRequest)->SupplierVerification.

**Request/record details:** Supplier query: capability/product/geography/certification criteria. Verification: exact supplier/source versions and individual claims; discovered contacts do not become approved recipients. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Discover manufacturers, distributors and subcontractors by requested capability/geography from permitted public sources.
- [ ] 2. Verify legal/contact/product/brand/certification/service-area claims individually and retain issue/expiry dates and uncertainty.
- [ ] 3. Keep discovered contact strings untrusted until supported and reviewed for the existing sending path; never infer authorization from a convincing website or name.
- [ ] 4. Link candidates to packages, required certifications and unanswered questions; reuse requires currentness checks.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t043(run_case):
    result = run_case("T043", {"scenario": "supplier_verification", "authorization_evidence": None, "certificate_expired": True})
    assert result['authorized_distributor_status'] == 'unknown'
    assert result['certificate_current'] is False
    assert result['sent_messages'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t043.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T043.A1: A distributor without manufacturer authorization evidence remains unverified for that claim.
- [ ] T043.A2: An expired certificate is flagged independently of a valid contact address.
- [ ] T043.A3: Discovery creates no RFQ send, account connection or commercial approval.
- [ ] T043.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t044"></a>

### T044: Complete RFQ scope composition while preserving exact sending authority

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T031, T043, T017. **Coverage:** R46/R47; preserved correspondence.

**Files and responsibility:** Create `backend/quantix/rfq_scope.py`, `backend/quantix/rfq_models.py`; modify `backend/quantix/correspondence.py`, `backend/quantix/correspondence_models.py`, `backend/quantix/office_business.py`; `src/features/Quotes.tsx`, `src/features/MailReplyCheck.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t044.py`; register driver `T044` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** RfqRevision{items,quantities,units,spec_refs,drawing_refs,questions,return_schedule,due_at,recipients,attachment_hashes,fingerprint}; prepare_rfq(ctx,RfqScopeRequest)->RfqRevision; reuse existing preview/approve/send service for exact displayed recipients/content/attachments.

**Request/record details:** RFQ: selected BOQ quantities/units/source versions, questions, return schedule/due time, proposed recipients and attachments. Sending consumes existing exact reviewed mail fingerprint and separate authorization. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Compose package-specific RFQs with BOQ items, quantities/units, relevant source versions, clarifications and requested technical/commercial return fields.
- [ ] 2. Generate internal draft and preview; group common questions without leaking supplier-specific confidential content.
- [ ] 3. Preserve native SMTP settings, exact fingerprint approval, recipient validation, delivery/partial-refusal records and uncertainty; changing any attachment/content invalidates sending approval.
- [ ] 4. Expose one Prepare request action with a clear review-to-send path; no new supplier/account connector.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t044(run_case):
    result = run_case("T044", {"scenario": "rfq_exact_scope", "attachment_changed_after_review": True})
    assert result['stale_send_blocked'] is True
    assert result['unapproved_sends'] == 0
    assert result['rfq_lines_have_source_refs'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t044.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T044.A1: Changing one attachment byte after review blocks sending until a fresh exact review.
- [ ] T044.A2: Duplicate approved operation keys create one send attempt/receipt.
- [ ] T044.A3: A Manager-generated RFQ remains unsent without explicit scope authorization.
- [ ] T044.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t045"></a>

### T045: Associate replies and extract quotation lines without losing originals

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T044, T024, T025. **Coverage:** R48/R49/R50.

**Files and responsibility:** Create `backend/quantix/quotation_intake.py`, `backend/quantix/quotation_models.py`; modify `backend/quantix/correspondence.py`, `backend/quantix/correspondence_models.py`, `backend/quantix/documents.py`; `src/features/Quotes.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t045.py`; register driver `T045` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** QuotationVersion{reply_id,rfq_revision,supplier,raw_hash,attachments,lines,terms,dates,confidence,unmapped}; QuotationLine{supplier_line_id,boq_mapping,description,quantity,unit,pack,unit_price,currency,tax,inclusions,exclusions,lead_time,source_refs}; extract_quote(ctx,QuoteExtractionRequest)->QuotationVersion.

**Request/record details:** Quote extraction: exact preserved reply/attachment refs and candidate RFQ revision. Association: exact message/header identities or explicit reviewed manual match; no subject-only automatic certainty. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Preserve raw messages/attachments and message IDs, In-Reply-To/References, IMAP UID/UIDVALIDITY where available; reject subject-only association as definitive.
- [ ] 2. Deduplicate exact reply identity while retaining corrected quotation versions and received dates.
- [ ] 3. Extract line items and commercial terms with cell/page/passages and confidence; show unmapped/ambiguous lines for review.
- [ ] 4. Treat reply bodies/attachments as untrusted evidence; do not run embedded macros/instructions or grant new recipients/data access.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t045(run_case):
    result = run_case("T045", {"scenario": "quote_reply_identity", "duplicate_message_id": True, "subject_only_other_reply": True, "payment_terms": None})
    assert result['exact_reply_count'] == 1
    assert result['subject_only_auto_association'] is False
    assert result['payment_terms'] is None
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t045.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T045.A1: An exact repeated Message-ID imports once; a revised attachment is a new version with history.
- [ ] T045.A2: A reply sharing only a subject is left unassociated/needs_review.
- [ ] T045.A3: Missing payment terms remain null and are not silently filled from another supplier.
- [ ] T045.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t046"></a>

### T046: Deliver technical comparison and commercial quotation levelling

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T045, T041, T032, T029. **Coverage:** R49/R50.

**Files and responsibility:** Create `backend/quantix/quote_levelling.py`, `backend/quantix/quote_comparison_models.py`; modify `backend/quantix/output_workbooks.py`, `backend/quantix/office_business.py`; `src/features/Quotes.tsx`, `src/features/ScenarioComparison.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t046.py`; register driver `T046` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** QuoteComparison{basis,technical_dimensions,normalizations,coverage,missing_terms,comparable_totals,noncomparable_reasons,clarifications}; level_quotes(ctx,QuoteComparisonRequest{quotation_versions,required_scope,normalization_basis})->QuoteComparison.

**Request/record details:** Levelling: exact quotation versions, common required scope/technical criteria and supported unit/pack/FX/delivery/tax conversions. Missing terms are explicit; prices alone cannot imply technical equivalence. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Compare the same specification/performance/certification/warranty dimensions and expose deviations separately from price.
- [ ] 2. Normalize only supported quantity/unit/pack/currency/delivery/discount/tax bases; retain original and converted amounts, methods and dated inputs.
- [ ] 3. Show exclusions, testing/installation scope, validity, payment and uncertainty beside totals; do not force ranking of incomparable offers.
- [ ] 4. Generate clarification drafts and a checked comparison schedule with per-cell sources and exact quotation versions.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t046(run_case):
    result = run_case("T046", {"scenario": "quote_levelling", "a": {"quantity": "10", "rate": "100", "delivery": "50"}, "b": {"quantity": "10", "rate": "95", "delivery": "100", "payment": None}})
    assert result['totals_ex_vat'] == ['1050.00','1050.00']
    assert result['missing_payment_visible'] is True
    assert result['terms_equal'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t046.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T046.A1: A:10×100+50 delivery and B:10×95+100 delivery both equal 1050 ex-VAT; display their unequal testing/payment/lead-time terms.
- [ ] T046.A2: Missing payment terms remain visible; unsupported unit conversion makes that line noncomparable.
- [ ] T046.A3: A source/quote revision invalidates the displayed comparison/decision fingerprint.
- [ ] T046.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t047"></a>

### T047: Connect procurement lead times, validity and follow-up to programme needs

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T046, T020, T019, T048. **Coverage:** R51/R52/R55.

**Files and responsibility:** Create `backend/quantix/procurement_planning.py`, `backend/quantix/procurement_models.py`; modify `backend/quantix/office_watchers.py`, `backend/quantix/output_programme.py`; `src/features/Quotes.tsx`, `src/features/ProgrammeForm.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t047.py`; register driver `T047` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** ProcurementChain{submittal,approval,manufacture,delivery,installation,required_on_site,calendar,owners,source_refs}; assess_procurement(ctx,ProcurementRequest)->ProcurementAssessment{latest_order,arrival,risk,validity_gaps,questions}.

**Request/record details:** Procurement: exact quote/lead-time basis, order/readiness/required-on-site dates, approval/manufacture/delivery/install activities and calendars. Follow-up uses exact RFQ/reply/version deduplication. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Link quoted lead times and staged procurement activities to explicit order/readiness/on-site dates and calendar basis.
- [ ] 2. Check offer validity against expected order date, payment conditions against ordering assumptions and required approvals before manufacture.
- [ ] 3. Identify long-lead constraints and material missing data; prepare clarification/follow-up drafts under existing mail authority.
- [ ] 4. Use deduplicated expiry/reply watches; reprioritize actual dependencies through Manager steering without silently changing promised dates.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t047(run_case):
    result = run_case("T047", {"scenario": "procurement_lead_times", "order_date": "2026-09-14", "lead_days": [7, 21], "basis": "calendar_days", "required_on_site": "2026-09-25"})
    assert result['arrivals'] == ['2026-09-21','2026-10-05']
    assert result['late'] == [False,True]
    assert result['duplicate_followups'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t047.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T047.A1: From an explicit 14 September order, 7 calendar days arrives 21 September and 21 days arrives 5 October; against required 25 September only the latter is late.
- [ ] T047.A2: An unspecified calendar-day/working-day basis is a question rather than guessed.
- [ ] T047.A3: A repeated missing reply generates one pending follow-up, not duplicate sends.
- [ ] T047.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t048"></a>

### T048: Extend programmes with productivity, calendars and resource checks

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T020, T031, T032. **Coverage:** R53/R54/R55/R56.

**Files and responsibility:** Create `backend/quantix/programme_engine.py`, `backend/quantix/programme_models.py`; modify `backend/quantix/output_programme.py`, `backend/quantix/submission_models.py`, `backend/quantix/office_types.py`; `src/features/ProgrammeForm.tsx`, `src/features/ProgrammeSuggestions.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t048.py`; register driver `T048` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** ProgrammeActivityBasis{scope_refs,quantity,unit,crew,productivity,calendar,dependencies,resources,method_version}; ProgrammeCheck{cycles,missing_logic,calendar_conflicts,resource_conflicts,unknown_inputs}; derive_programme(ctx,ProgrammeRequest)->ProgrammeVersion.

**Request/record details:** Programme: explicit scope/quantities/crew/productivity/resources/calendars/relations/lags/methods; unknown input fields are preserved. Optimization request states objective/constraints and permitted scenario scope. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Derive durations from supplied quantity/crew/productivity and explicit rounding; never invent reliable productivity or resource availability.
- [ ] 2. Extend dependency types/lags and per-activity calendars only with documented semantics; preserve current finish-to-start whole-day baseline and imported history.
- [ ] 3. Calculate dates, float/critical-path where defined, and missing predecessor/cycle/calendar/resource conflicts; integrate procurement chains as ordinary scoped activities.
- [ ] 4. Make resource levelling/optimization a separately evaluated deterministic capability; show assumptions and proposed changes before adopting.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t048(run_case):
    result = run_case("T048", {"scenario": "programme_calendar", "quantity": "100", "productivity": "20", "start": "2026-09-14", "holiday": "2026-09-17", "durations": [2, 3]})
    assert result['productivity_duration'] == 5
    assert result['chain_finish'] == '2026-09-21'
    assert result['cycle_rejected'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t048.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T048.A1: 100 m2 at 20 m2/day yields 5 working days.
- [ ] T048.A2: Starting 14 September on Mon–Fri with 17 September holiday, a 2-day task then 3-day task finishes on 21 September.
- [ ] T048.A3: A resource assigned beyond explicit available capacity is flagged; a cycle blocks scheduling.
- [ ] T048.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t049"></a>

### T049: Add validated local schedule-file interoperability

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T048, T058. **Coverage:** R53/R56; research planning pack/library.

**Files and responsibility:** Create `backend/quantix/schedule_interop.py`, `backend/quantix/schedule_format_models.py`; modify `backend/quantix/extraction_adapters.py`, `backend/quantix/outputs.py`; `src/features/ProgrammeForm.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t049.py`; register driver `T049` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** ScheduleFormatCapability{format,version,read,write,unsupported_fields,runtime}; import_schedule(ctx,ScheduleImportRequest)->ProgrammeImportReview; export_schedule(ctx,ScheduleExportRequest)->OutputRecord.

**Request/record details:** Import/export: selected original/programme versions, exact format/runtime capability and mapping/loss review fingerprint. Format name alone cannot claim converter support. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Evaluate MPXJ/other maintained local format tools against actual requested formats and runtime/licence/redistribution requirements; record exact read/write capability separately.
- [ ] 2. Preserve imported original files and map activity IDs, calendars, relations/lags, resources and constraints with explicit unsupported-field exceptions.
- [ ] 3. Require review of lossy conversions; roundtrip supported fields against a fixture and retain the mapping/derivative hash.
- [ ] 4. No live scheduling-system account connector or assumed format support from a file extension.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t049(run_case):
    result = run_case("T049", {"scenario": "schedule_roundtrip", "unsupported_constraint": True})
    assert result['supported_fields_preserved'] is True
    assert result['lossy_export_requires_review'] is True
    assert result['original_unchanged'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t049.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T049.A1: Supported activity IDs/calendars/dependencies survive roundtrip exactly.
- [ ] T049.A2: An unsupported relation/constraint is visible before export and never silently dropped.
- [ ] T049.A3: A failed converter leaves the original and current programme unchanged.
- [ ] T049.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t050"></a>

### T050: Produce complete technical, method, quality and HSE drafts

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T015, T021, T031, T017. **Coverage:** R57/R58/R59.

**Files and responsibility:** Create `backend/quantix/technical_documents.py`, `backend/quantix/technical_document_models.py`; modify `backend/quantix/output_word.py`, `backend/quantix/outputs.py`, `backend/quantix/office_types.py`; `src/features/OutputForm.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t050.py`; register driver `T050` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** TechnicalDocumentDraft{kind:method_statement|technical_proposal|quality_plan|hse_submission,method_version,project_constraints,organization,resources,compliance_refs,company_assets,missing_inputs,review_requirements}; compose_document(ctx,TechnicalDocumentRequest)->WorkProductVersion.

**Request/record details:** Document: selected kind/method/company assets/project constraints/resources/compliance/source refs and internal/client selection. No invented credentials, regulatory method or signature authority. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Use selected approved company content/methods and actual project constraints; preserve input/source versions and document ownership.
- [ ] 2. Include project understanding, selected methods, organization/resources, quality/HSE controls and compliance response only where evidence supports them.
- [ ] 3. Expose missing site/project inputs and required qualified review; do not invent certificates, personnel credentials or jurisdiction-specific safety rules.
- [ ] 4. Render and inspect DOCX/PDF derivatives for layout/content; retain internal versus client-facing content selections.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t050(run_case):
    result = run_case("T050", {"scenario": "technical_document_inputs", "site_constraint": None, "certificate_expired": True})
    assert result['missing_inputs_visible'] is True
    assert result['fabricated_credentials'] == 0
    assert result['render_verified'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t050.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T050.A1: A missing approved method or site-specific input appears as a gap, not fabricated prose stated as fact.
- [ ] T050.A2: Expired company credentials cannot be included as current without explicit resolution.
- [ ] T050.A3: Rendered tables/headings/source references and internal-only notes follow the actual selected output basis.
- [ ] T050.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t051"></a>

### T051: Expand structure-preserving client-form population

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T025, T050. **Coverage:** R60.

**Files and responsibility:** Create `backend/quantix/form_population.py`, `backend/quantix/form_mapping_models.py`; modify `backend/quantix/client_boq.py`, `backend/quantix/document_word.py`, `backend/quantix/output_word.py`, `backend/quantix/outputs.py`; `src/features/ClientBoqForm.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t051.py`; register driver `T051` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** FormMapping{template_basis,field_locator,original_value,proposed_value,source_refs,rule,validation}; FormPopulationReview{mappings,unsupported_controls,structural_checks,fingerprint}; populate_form(ctx,FormPopulationRequest{fingerprint})->OutputRecord.

**Request/record details:** Form: exact template version/hash, field/cell/control mappings, source values/formulas/calculation refs and reviewed fingerprint. Reject unmapped/unsupported protected control mutation. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Retain current workbook/VBA preservation and extend only validated DOCX fields/bookmarks/content controls or additional form types with explicit format support.
- [ ] 2. Keep field-level provenance, original and populated values, mapped ranges/controls and exact template version/hash.
- [ ] 3. Detect protected/encrypted/signed/unsupported controls and formula-recalculation limitations before writing; never silently flatten a required client template.
- [ ] 4. Render/compare the output, check unmapped parts and require current reviewed mappings before export.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t051(run_case):
    result = run_case("T051", {"scenario": "client_form_preservation", "template_changed_after_review": True})
    assert result['stale_mapping_blocked'] is True
    assert result['unmapped_content_preserved'] is True
    assert result['unattributed_values'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t051.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T051.A1: Unmapped workbook XML/VBA and DOCX content remain preserved according to the supported format contract.
- [ ] T051.A2: Changing a template after review blocks stale population.
- [ ] T051.A3: Every populated commercial value has a source/calculation reference; unsupported protected fields stay explicit blockers.
- [ ] T051.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t052"></a>

### T052: Deliver bilingual evidence, terminology and document quality

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T026, T050, T051. **Coverage:** R61; C28/C30; research engineering UX.

**Files and responsibility:** Create `backend/quantix/bilingual_documents.py`, `backend/quantix/glossary_models.py`; modify `backend/quantix/output_word.py`, `backend/quantix/work_products.py`; `src/features/WorkProduct.tsx`, `src/features/Sources.tsx`; related styles.

**Test owner:** Create `backend/tests/agentic/test_t052.py`; register driver `T052` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** TranslationSegment{original_text,original_ref,target_text,languages,glossary_version,review_state,uncertainties}; translate_document(ctx,TranslationRequest)->BilingualDraft; check_bilingual(ctx,BilingualCheckRequest)->BilingualCheck.

**Request/record details:** Translation: source segments/refs, source-target languages, glossary version, target form/layout and review needs. Original identifiers/numerals/contract references remain separate preserved fields. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Keep original clauses/numbers and translations side by side with exact source IDs; translate wording without replacing authoritative contract text.
- [ ] 2. Use scoped approved glossary/project vocabulary and expose ambiguous technical abbreviations, numeral/decimal/date and unit mappings.
- [ ] 3. Implement RTL/bidi isolation, readable mixed-language tables, fonts and export layout; preserve original reference numbering.
- [ ] 4. Validate language support by actual extraction/retrieval/render fixtures, not conversational fluency alone.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t052(run_case):
    result = run_case("T052", {"scenario": "bilingual_identifiers", "clause": "4.2.1", "quantity": "12.5 m3", "drawing": "A-101"})
    assert result['identifiers_preserved'] is True
    assert result['original_text_retained'] is True
    assert result['layout_verified'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t052.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T052.A1: Arabic translation retains clause 4.2.1, 12.5 m3 and drawing A-101 exactly in their intended direction.
- [ ] T052.A2: Ambiguous abbreviation is flagged for context-specific resolution.
- [ ] T052.A3: Light/dark UI and exported document have readable mixed-direction text without reversed identifiers or clipped cells.
- [ ] T052.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t053"></a>

### T053: Add source-backed sustainability options using supplied factors

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T032, T041, T038. **Coverage:** R62; excluded EC3 connector.

**Files and responsibility:** Create `backend/quantix/sustainability.py`, `backend/quantix/sustainability_models.py`; modify `backend/quantix/work_products.py`; `src/features/ScenarioComparison.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t053.py`; register driver `T053` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** CarbonFactor{epd_ref,product,declared_unit,value,unit,lifecycle_modules,geography,date,validity,boundary,exclusions}; calculate_carbon(ctx,CarbonCalculationRequest{quantities,factor_refs,conversions}) -> CalculationRecord; compare_carbon_options(ctx,CarbonComparisonRequest)->WorkProductVersion.

**Request/record details:** Carbon: typed quantities, supplied factor/EPD versions, declared units/modules/boundaries/date/geography and explicit conversions/exclusions. No guessed factor or EC3 account. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Read supplied/authorized EPD or factor evidence with product, declared unit, lifecycle modules, date/geography and exclusions.
- [ ] 2. Convert quantities through explicit unit bases and keep carbon boundary separate from price/compliance conclusions.
- [ ] 3. Reject comparisons with incompatible modules/boundaries unless a justified normalization is separately recorded.
- [ ] 4. No EC3 account/feed connector; missing factors remain unknown rather than default values.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t053(run_case):
    result = run_case("T053", {"scenario": "carbon_boundary", "quantity": "2.5", "factor": "0.24", "boundaries": ["A1-A3", "A1-C4"]})
    assert result['emissions'] == '0.60'
    assert result['directly_comparable'] is False
    assert result['factor_reference_retained'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t053.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T053.A1: 2.5 tonnes × 0.24 tCO2e/tonne produces 0.60 tCO2e with exact factor/version.
- [ ] T053.A2: A1–A3 cannot silently compare as equivalent to A1–C4.
- [ ] T053.A3: Expired/unmatched EPD and unknown conversion remain visible limitations.
- [ ] T053.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t054"></a>

### T054: Complete submission requirements and cross-document consistency

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T030, T050, T051, T048. **Coverage:** R63/R64/R65.

**Files and responsibility:** Create `backend/quantix/submission_checks.py`, `backend/quantix/consistency_models.py`; modify `backend/quantix/tender_requirements.py`, `backend/quantix/submissions.py`, `backend/quantix/submission_models.py`; `src/features/SubmissionRequirements.tsx`, `src/features/Submissions.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t054.py`; register driver `T054` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** SubmissionRequirement{mandatory_or_conditional,type:form|attachment|signature|format|deadline,condition,source_refs,output_ref,exception}; ConsistencyIssue{field_path,expected,observed,source_or_output_refs,severity,remedy}; assess_submission(ctx,SubmissionAssessmentRequest)->SubmissionAssessment.

**Request/record details:** Submission assessment: exact requirements/conditions/outputs/versions/hashes, reviewed exceptions and checklist criteria. No client-set can_release flag. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Map every required form/attachment/signature/format/deadline to an exact current output or explicit reviewed exception; resolve conditions with their source basis.
- [ ] 2. Check totals, project names, dates, qualifications, methods and internal/client selections across selected documents.
- [ ] 3. Keep technical readiness, commercial readiness and required release authority separate; link blockers to concrete repair views with preserved return/drafts.
- [ ] 4. Revalidate output/source/requirement fingerprints at review and final release; a generated signature is never proof of authorization.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t054(run_case):
    result = run_case("T054", {"scenario": "submission_completeness", "signed_form_missing": True, "output_changed": True})
    assert result['can_release'] is False
    assert result['typed_blockers_present'] is True
    assert result['stale_review_rejected'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t054.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T054.A1: A missing mandatory signed form blocks readiness with the exact requirement link.
- [ ] T054.A2: Changing one output after preview invalidates its fingerprint and requires fresh review.
- [ ] T054.A3: An output-level project-name or total mismatch is detected even when individual files render correctly.
- [ ] T054.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t055"></a>

### T055: Deliver submission rehearsal, exact release manifests and transmission records

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T054, T012, T044. **Coverage:** R63–R67; concept submission rehearsal.

**Files and responsibility:** Create `backend/quantix/submission_rehearsal.py`, `backend/quantix/transmission_records.py`; modify `backend/quantix/submissions.py`, `backend/quantix/outputs.py`; `src/features/Submissions.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t055.py`; register driver `T055` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** SubmissionRehearsal{criteria,basis,findings,unknowns,reviewer,checks}; ReleaseManifest{outputs,versions,paths,hashes,sizes,inclusion_reasons,requirements,package_hash}; TransmissionRecord{manifest_hash,method:manual|existing_mail,operator,attempted_at,confirmation,outcome}; rehearse(ctx,RehearsalRequest)->SubmissionRehearsal.

**Request/record details:** Rehearsal: supplied criteria, selected package basis, participants/effort. Manifest uses exact reviewed outputs. Transmission record: actual method/operator/time/confirmation and uncertain state; no new portal connector. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Run a clearly labelled reviewer against supplied Tender criteria, with actual documents and independent checking where valuable.
- [ ] 2. Prepare the exact local package and show internal/client content, included/excluded files and remaining material decisions.
- [ ] 3. Preserve explicit release approval and separate transmission; provide manual steps/confirmation recording or the existing authorized mail path only.
- [ ] 4. Track confirmed/uncertain outcomes without duplicate export/send effects; new portal upload/feed integrations remain excluded.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t055(run_case):
    result = run_case("T055", {"scenario": "submission_rehearsal_release", "missing_attachment": True, "transmission_confirmation": None})
    assert result['reviewer_auto_approval'] is False
    assert result['missing_attachment_found'] is True
    assert result['transmission_outcome'] == 'uncertain'
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t055.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T055.A1: Rehearsal catches a missing required attachment and cannot approve the package itself.
- [ ] T055.A2: Manifest hashes identify exactly the reviewed files; replacing a file blocks reuse.
- [ ] T055.A3: Manual transmission record with missing confirmation is uncertain, not successful delivery.
- [ ] T055.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t056"></a>

### T056: Support post-tender clarifications and complete award handover

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T055, T039, T030. **Coverage:** R68/R69/R70.

**Files and responsibility:** Create `backend/quantix/post_tender.py`, `backend/quantix/handover.py`, `backend/quantix/post_tender_models.py`; modify `backend/quantix/work_products.py`, `backend/quantix/submissions.py`; create `src/features/PostTender.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t056.py`; register driver `T056` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** SubmittedBaseline{manifest_hash,output_refs,commercial_basis}; PostTenderClarification{baseline_ref,request_ref,response,changed_fields,delta_refs,decision}; AwardHandover{accepted_scope,quantities,rates,suppliers,risks,assumptions,commitments,owners,due_dates,source_refs}; prepare_handover(ctx,HandoverRequest)->WorkProductVersion.

**Request/record details:** Post-tender: immutable submitted manifest, received clarification/source, response/delta refs and decisions. Handover: selected accepted records, commitments/owners/dates and explicit unresolved items. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Anchor every clarification to the exact submitted manifest and preserve revisions as separate deltas, not edits to the released bid.
- [ ] 2. Use negotiation scenarios for requested discounts/exclusions/alternatives and retain explicit commercial approval.
- [ ] 3. Assemble accepted scope, quantities/rates, quote/supplier references, risks, assumptions and commitments with owners/dates and unresolved items.
- [ ] 4. Provide local handover exports within Submission; no ERP/project-management connector is introduced.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t056(run_case):
    result = run_case("T056", {"scenario": "post_tender_handover", "new_tender_revision": True, "unapproved_rate": True})
    assert result['submitted_baseline_preserved'] is True
    assert result['delta_separate'] is True
    assert result['unapproved_rate_labelled_accepted'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t056.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T056.A1: A clarification after a new Tender revision still cites the exact submitted baseline and shows its delta.
- [ ] T056.A2: Unaccepted rate/assumption cannot be relabelled accepted in handover.
- [ ] T056.A3: The released manifest and output hashes remain unchanged.
- [ ] T056.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t057"></a>

### T057: Implement win/loss review and matched estimated-versus-actual learning

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T056, T038, T016, T021. **Coverage:** R71/R72; AO18.

**Files and responsibility:** Create `backend/quantix/learning.py`, `backend/quantix/learning_models.py`; modify `backend/quantix/knowledge.py`, `backend/quantix/office_knowledge.py`; `src/features/Knowledge.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t057.py`; register driver `T057` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** OutcomeReview{outcome,reasons,evidence,speculation,reviewer}; MatchedCostComparison{estimate_refs,actual_refs,scope_unit_currency_tax_inclusion_basis,variance,method}; LessonCandidate{finding,applicability,evidence,evaluation,status}; propose_lesson(ctx,LessonRequest)->LessonCandidate.

**Request/record details:** Learning: selected outcome reasons/evidence/speculation, matched estimate/actual bases, variance method, applicability and lesson fixtures. Promotion uses explicit approved company scope. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Record known win/loss reasons separately from speculation and preserve supporting evidence/sample context.
- [ ] 2. Match actual costs and estimates by scope, quantities, units, currency, tax, inclusions and time basis before attributing variance.
- [ ] 3. Create lesson/method proposals with applicability and evaluation cases; company reuse requires explicit approval and dated revalidation.
- [ ] 4. Never silently train/promote from customer data, private notebooks or a single persuasive explanation.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t057(run_case):
    result = run_case("T057", {"scenario": "learning_basis", "estimated_quantity": "100", "estimated_rate": "100", "actual_quantity": "98", "actual_rate": "105", "reason_speculative": True})
    assert result['variance'] == '290.00'
    assert result['speculation_marked'] is True
    assert result['automatic_company_approval'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t057.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T057.A1: 100×100 versus 98×105 yields +290 explained by -200 quantity and +490 rate effects.
- [ ] T057.A2: Unmatched scope/currency cannot produce a definitive productivity lesson.
- [ ] T057.A3: Speculative win/loss reasons are not promoted as verified company knowledge.
- [ ] T057.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t058"></a>

### T058: Implement curated plugin packages and capability lifecycle

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T013, T014. **Coverage:** AO12; C23; research §6.

**Files and responsibility:** Create `backend/quantix/plugin_manifest.py`, `backend/quantix/plugin_store.py`, `backend/quantix/plugin_runtime.py`, `backend/quantix/plugin_models.py`; modify `backend/quantix/storage.py`, `backend/quantix/backup.py`, `backend/quantix/factory_reset.py`; create `src/features/CapabilityLibrary.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t058.py`; register driver `T058` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** PluginVersion{id,publisher,version,hashes,licences,compatibility,dependencies,capabilities,permissions,migrations}; inspect_plugin(ctx,PluginPackage)->PluginPreview; install_plugin(ctx,PluginInstallRequest{fingerprint})->PluginVersion; set_plugin_state(ctx,PluginStateRequest)->PluginStateReceipt.

**Request/record details:** Plugin inspect: selected immutable package/hash. Install/update: exact preview fingerprint and dependency/permission choices. State changes: installed version, expected revision, target state and owned output-preservation policy. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Ship a curated catalogue for document intelligence, estimating, market research, supplier preparation, submission, company knowledge, CAD, planning, regional methods and supplied-factor sustainability; no excluded connectors.
- [ ] 2. Validate immutable hashes, publisher identity, dependency/native/model licences and platform compatibility; prevent traversal/symlink/reparse extraction and untrusted install hooks.
- [ ] 3. Show changed permissions on update; separate installed, configured, permitted-for-Tender and successfully-checked states.
- [ ] 4. Implement disabled/rollback/uninstall with dependency ownership and migration compatibility; retain output records and required historical metadata after removal.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t058(run_case):
    result = run_case("T058", {"scenario": "plugin_lifecycle", "tampered_package": True, "uninstall_after_output": True})
    assert result['tampered_install_rejected'] is True
    assert result['automatic_tender_grants'] == 0
    assert result['output_preserved_after_uninstall'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t058.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T058.A1: Tampered bytes/path escape or conflicting versions fail before executable activation.
- [ ] T058.A2: Installing a plugin grants no Tender file/network/billing authority.
- [ ] T058.A3: Disable/uninstall preserves produced documents, source references and historical method identity; incompatible rollback cannot corrupt data.
- [ ] T058.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t059"></a>

### T059: Add scoped MCP clients and separately authorized outward tools

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T058, T013, T062. **Coverage:** AO12; research §9.

**Files and responsibility:** Create `backend/quantix/mcp_capabilities.py`, `backend/quantix/mcp_connections.py`, `backend/quantix/mcp_connection_models.py`; modify `backend/quantix/ai_runtime_mcp.py`, `backend/quantix/ai_tools.py`; `src/features/CapabilityLibrary.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t059.py`; register driver `T059` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** McpConnection{server_identity,transport,endpoint_or_executable,version,tool_namespace,auth_scope,allowed_destinations,health}; discover_mcp(ctx,ConnectionCheckRequest)->CapabilityPreview; call_mcp(ctx,McpCallRequest)->ExecutionReceipt; outward_tool_catalog(authorized_session)->list[ToolCapability].

**Request/record details:** MCP: selected server identity/transport/version/auth audience/resources/capability namespace; invocation contains registered tool/payload only. Outward client access is a distinct scoped authorized session. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Support selected pinned local servers and separately validated remote servers, negotiating actual protocol/capabilities; keep existing private bridge private.
- [ ] 2. Validate schemas/namespaces, bounded resources/prompts/tools, timeouts/cancellation, token audience/resource scope and reconnect/idempotency behavior.
- [ ] 3. Require exact account/Tender/task permissions at discovery, prompt/resource access and execution; remote OAuth tokens never become model API credentials.
- [ ] 4. Expose only a narrow separately authorized Quantix tool set to another approved client; no database/filesystem-wide default access, business connector presets or A2A requirement.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t059(run_case):
    result = run_case("T059", {"scenario": "mcp_scope_and_identity", "wrong_audience": True, "namespace_collision": True})
    assert result['wrong_audience_rejected'] is True
    assert result['collision_rejected'] is True
    assert result['private_bridge_exposed'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t059.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T059.A1: A server rename/colliding tool namespace cannot replace a trusted operation.
- [ ] T059.A2: A token for another audience and a malicious redirect are rejected.
- [ ] T059.A3: Revoked/expired permissions block calls while retained outputs remain inspectable; an old client cannot gain new catalogue access.
- [ ] T059.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t060"></a>

### T060: Pilot bounded code composition over approved tools

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T013, T015, T058, T032. **Coverage:** AO11/AO12; research §10.

**Files and responsibility:** Create `backend/quantix/code_composition.py`, `backend/quantix/code_composition_models.py`; modify `backend/quantix/staff_runtime.py`, `backend/quantix/staff_budget.py`; `src/features/CalculationInspector.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t060.py`; register driver `T060` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** CompositionRequest{code,language_subset,tool_versions,input_refs,limits}; execute_composition(ctx,CompositionRequest)->CompositionReceipt{code_hash,nested_calls,outputs,usage,limitations}; permit only registered task-scoped callable tools.

**Request/record details:** Composition: restricted code, versioned allowed tool set, selected input refs and resource/output limits. No arbitrary imports, host paths, credentials, package installation or direct network capability. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Validate a maintained restricted interpreter such as the documented Code Mode/Monty subset against the pinned runtime; do not mistake it for a full third-party Python environment.
- [ ] 2. Expose only approved versioned tools; every nested invocation passes the same input/output/scope/budget/cancellation checks.
- [ ] 3. Bound instruction count/time/memory/output size and preserve exact code/input/tool identities; no imports, ambient files/network/credentials or dependency installation.
- [ ] 4. Show inspectable workings/results and a precise capability gap if the program needs full libraries outside this subset.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t060(run_case):
    result = run_case("T060", {"scenario": "bounded_composition", "rows": 100, "attempt_import": "os"})
    assert result['processed_rows'] == 100
    assert result['ambient_access_blocked'] is True
    assert result['unmetered_calls'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t060.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T060.A1: Batching 100 rows through permitted tools produces exact results and one attributable nested-call ledger.
- [ ] T060.A2: Attempted file/network/import access is blocked, not delegated to an unrestricted subprocess.
- [ ] T060.A3: Exhausted root allowance stops the next nested call; partial outputs remain clearly partial.
- [ ] T060.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t061"></a>

### T061: Implement a validated full-Python isolation provider

**Current status:** Partial 2026-09-10 — no validated OS/VM sandbox; a fixed demo subprocess or same-user venv does not satisfy the isolation gate. Remaining checklist/native/live evidence is open. **Dependencies:** T060, T058, T062, T067. **Coverage:** AO11/AO12; C24/C37; research §10.

**Files and responsibility:** Create `backend/quantix/python_analysis.py`, `backend/quantix/python_analysis_models.py`, `backend/quantix/sandbox_protocol.py`, `backend/quantix/sandbox_windows.py`, `backend/quantix/sandbox_macos.py`, `backend/quantix/sandbox_linux.py`; create `src-tauri/src/sandbox.rs` only if the qualified adapter needs native launch/lifecycle control; modify `src/features/CalculationInspector.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t061.py`; register driver `T061` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** SandboxCapability{platform,mechanism,runtime_hash,library_lock,limits,validated_checks}; PythonAnalysisRequest{code_hash,input_snapshot,approved_libraries,output_contract,cpu,memory,wall_time,network_policy}; run_analysis(ctx,PythonAnalysisRequest)->AnalysisReceipt{outputs,code,input_hashes,usage,checks,termination}.

**Request/record details:** Python: exact code/input/library image hashes, supported sandbox mechanism, resource caps, output contract and separately approved gateway needs. Host-mounted inputs are read-only selected snapshots. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Qualify an OS/VM-backed sandbox on each advertised platform; evaluate Windows Sandbox/VM on supported Windows and maintained VM-backed providers on macOS/Linux. Record support/preflight, storage path control and negative isolation tests before enabling full Python.
- [ ] 2. Stage only selected immutable input snapshots and a new bounded output directory; default deny networking, clipboard/device sharing, ambient credentials, unrelated host files and package installation.
- [ ] 3. Pin Python/library/native/model dependencies; make any separately approved network access go through the governed gateway, not arbitrary egress.
- [ ] 4. Kill/reap owned execution on Stop/timeout/reset; validate output type/size/paths/content and hash before importing it as a draft. Unsupported hosts keep native tools/code composition available with a precise error.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t061(run_case):
    result = run_case("T061", {"scenario": "sandbox_negative_controls", "attempts": ["host_secret", "network", "path_escape", "background_process"]})
    assert result['blocked_attempts'] == 4
    assert result['host_files_changed'] == 0
    assert result['reproducible_output'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t061.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T061.A1: A program cannot read a host secret marker, escape via symlink/reparse path, contact a private endpoint, persist a process or write outside its output boundary.
- [ ] T061.A2: CPU/memory/output/time limits terminate a hostile test and leave the application responsive.
- [ ] T061.A3: A pandas/NumPy-style approved analysis reproduces its output from code/input/library hashes; a venv or same-user process alone cannot pass this gate.
- [ ] T061.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t062"></a>

### T062: Unify authority, outbound-data records, diagnostics and resource accounting

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T013. **Coverage:** AO12/AO13; C12/C13/C37; research §13.

**Files and responsibility:** Create `backend/quantix/data_transmission_records.py`, `backend/quantix/execution_trace_models.py`; modify `backend/quantix/ai_policy.py`, `backend/quantix/staff_budget.py`, `backend/quantix/diagnostics.py`, `backend/quantix/diagnostic_routes.py`, `backend/quantix/storage.py`; `src/features/AIUsage.tsx`, `src/features/Diagnostics.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t062.py`; register driver `T062` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** DataTransmissionRecord{actor,root,account,destination,purpose,source_refs,data_categories,bytes_or_extent,time,result,retention_policy}; ExecutionTrace{request_id,parent_id,operation_version,status,duration,usage,redacted_error}; record_transmission(ctx,TransmissionDraft)->DataTransmissionRecord.

**Request/record details:** Transmission/trace: trusted ctx plus actual destination/account/purpose/data extent and measured result/usage; private content and secrets excluded from diagnostic payload. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Keep source/account/destination/model/billing approvals version-bound and enforced below all model/tool/skill/plugin/code boundaries.
- [ ] 2. Record actual external data extents and reported usage/cost for the engineer without putting private content, keys, auth codes or provider bodies in diagnostics.
- [ ] 3. Correlate root/staff/tool/reader/retry/check costs, including unknown/reserved amounts; optional OpenTelemetry export remains disabled without explicit data permission.
- [ ] 4. Apply limits to archives, OCR, model files, plugin packages, redirects and outputs; preserve safe actionable errors and request IDs, cancellation and credential revocation.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t062(run_case):
    result = run_case("T062", {"scenario": "transmission_and_diagnostics", "secret_marker": "SYNTHETIC_SECRET_MARKER", "revoke_before_dispatch": True})
    assert result['secret_in_logs'] is False
    assert result['transmission_record_present'] is True
    assert result['revoked_dispatch_blocked'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t062.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T062.A1: A synthetic secret/Tender text marker never appears in diagnostic files or UI error payloads.
- [ ] T062.A2: The engineer can see which selected documents/categories were sent to which approved destination.
- [ ] T062.A3: Revocation during a queued/leased call blocks dispatch/publication as applicable; measured late usage remains attributable.
- [ ] T062.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t063"></a>

### T063: Complete the balanced Manager workspace and interaction continuity

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T010, T017, T020. **Coverage:** AO16; C25/C29/C31; selected UI decisions.

**Files and responsibility:** Modify `src/App.tsx`, `src/features/Manager.tsx`, `src/features/Sources.tsx`, `src/features/Composer.tsx`, `src/features/useFormDraft.ts`; `src/features/office/TenderOfficeWorkspace.tsx`; `src/styles/workspace.css`.

**Test owner:** Create `backend/tests/agentic/test_t063.py`; register driver `T063` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** Extend existing HashRouter record routes with validated work-product/review/scenario identities and selection context; SourceSelection{artifact_id,version,hash,page,region,sheet,cell_range}; preserve server-derived result links and nonsecret versioned drafts.

**Request/record details:** UI route/selection: allowed internal destination plus record/version/hash/page/region/sheet/range/row identity. Draft storage: Tender/form/version and nonsecret values; no persisted approval toggles. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Keep Manager plus actual current evidence/result, collapsing empty work space; office expansion preserves mounted draft/source state and returns focus.
- [ ] 2. Connect exact selected BOQ rows, page/region, quotation paragraph or result to a Manager request through validated selection context; do not duplicate a specialist instruction channel.
- [ ] 3. Show actual outcome, implication, evidence and one next action; keep active blockers visible and advanced controls under More options.
- [ ] 4. Preserve Tender/record/profile-version/row selection, page/zoom/sheet/range, scroll and unsent drafts across routes, resets of transport and supported reopening; never autosave credentials/approval toggles.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t063(run_case):
    result = run_case("T063", {"scenario": "workspace_return_continuity", "page": 2, "draft": "Compare the concrete clauses"})
    assert result['returned_page'] == 2
    assert result['draft'] == 'Compare the concrete clauses'
    assert result['cross_tender_response_applied'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t063.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T063.A1: Opening a source, staff desk, review, Settings and returning preserves the latest unsent text and exact PDF page/sheet selection.
- [ ] T063.A2: A forged/outside return URL is rejected; old Tender responses cannot paint the new Tender.
- [ ] T063.A3: No empty filler rail, clipped composer or horizontally overflowing essential controls at specified desktop sizes.
- [ ] T063.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t064"></a>

### T064: Complete the live office, staff desks, relationships and motion

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T003, T004, T005, T006, T007, T012, T063. **Coverage:** AO20; C25–C29; live office brief.

**Files and responsibility:** Modify `src/features/office/LiveOffice.tsx`, `src/features/office/StaffDesk.tsx`, `src/features/office/OfficeMessages.tsx`, `src/features/office/useOffice.ts`, `src/features/office/StaffDraftContent.tsx`, `src/features/office/office.css`; `src/features/StaffPortrait.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t064.py`; register driver `T064` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** Extend paged office read models for notebooks, ownership, dependencies, reviews, handoffs and lifecycle; preserve instance_id/sequence reset semantics and aborted stale requests. UI state is derived solely from committed records.

**Request/record details:** Read state: existing authenticated Tender snapshot/event/history cursors plus selected staff/assignment/profile version/review; presentation preferences cannot change retained records or start work. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Show active generated colleagues and historical/retired staff with name/title/AI label/task/state/one action; full profile, creation reason, tools/skills, notebooks, receipts and version history remain inspectable.
- [ ] 2. Connect actual relationships and handoffs through an optional accessible SVG layer plus complete textual list; no decorative permanent staff or invented chatter.
- [ ] 3. Keep highlights default and all retained exchanges paged/filterable; preserve selection during new snapshots and independent history exhaustion/cursors.
- [ ] 4. Animate only newly committed relevant events, respect reduced motion/visibility/focus, and disclose disconnected/last-known/gap/reset states; support many/long/bilingual profiles.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t064(run_case):
    result = run_case("T064", {"scenario": "office_events_and_history", "retained_staff": 200, "new_instance": True, "reduced_motion": True})
    assert result['old_arrival_animations'] == 0
    assert result['staff_history_complete'] is True
    assert result['focus_stolen'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t064.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T064.A1: Reconnecting or restoring a new service instance does not replay old arrival animation or lose historical selection.
- [ ] T064.A2: Zero staff creates no placeholder employees; 200 retained staff remain pageable and readable.
- [ ] T064.A3: A new event does not steal focus/scroll from evidence; keyboard users can inspect every action and exact handoff.
- [ ] T064.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t065"></a>

### T065: Finish engineering tables, coverage measures and the decision inbox

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T025, T030, T037, T046, T054, T063. **Coverage:** AO16; research UX §3.

**Files and responsibility:** Modify `src/features/Estimate.tsx`, `src/features/EstimateEditor.tsx`, `src/features/WorkDecisions.tsx`, `src/features/Files.tsx`, `src/features/Quotes.tsx`, `src/features/SubmissionRequirements.tsx`; create `src/components/EngineeringTable.tsx`, `src/features/useTableSelection.ts`, `backend/quantix/coverage_models.py`, `backend/quantix/coverage.py`, `backend/quantix/decision_impacts.py`.

**Test owner:** Create `backend/tests/agentic/test_t065.py`; register driver `T065` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** CoverageSummary{imported:{numerator,denominator,exceptions},extracted,analysed,reviewed}; DecisionImpact{decision_ref,scope_effect,price_effect,programme_effect,submission_effect,affected_refs,unknowns}; GET scoped coverage/impact through actual service-derived records.

**Request/record details:** Coverage: declared scope denominator and actual independent state receipts. Bulk edit: selected record versions, typed cell changes and explicit error/atomicity mode; accepted-value changes use normal decisions. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Support keyboard cell/row navigation, rectangular copy/paste where semantically valid, bulk classification, saved scoped filters, frozen identifiers and visible units; choose a maintained grid only after measured need/licence review.
- [ ] 2. Validate each bulk edit atomically by displayed basis or return precise per-row failures without losing valid drafts; prevent formula injection in text export.
- [ ] 3. Group material decisions by scope/price/programme/submission, showing current/proposed value, reason, sources and affected outputs.
- [ ] 4. Show independent coverage counts with honest denominators and unreadable/partial/obsolete exceptions; source read, AI analysis and engineer review cannot increment each other.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t065(run_case):
    result = run_case("T065", {"scenario": "coverage_and_grid", "counts": [10, 8, 3, 1], "denominator": 10})
    assert result['displayed_counts'] == [10,8,3,1]
    assert result['reviewed_from_read_receipts'] == 0
    assert result['invalid_unit_write_blocked'] is True
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t065.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T065.A1: Ten imported, eight extracted, three analysed and one reviewed display four separate fractions and named exceptions.
- [ ] T065.A2: Pasting a mixed valid/invalid unit range cannot silently corrupt accepted quantities.
- [ ] T065.A3: Saved filters stay Tender-scoped; opening a decision's source and returning preserves selected rows.
- [ ] T065.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t066"></a>

### T066: Build one coherent capability, method and connection Settings experience

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T015, T016, T058, T059, T060, T061, T062, T063. **Coverage:** AO12/AO16; C23/C31.

**Files and responsibility:** Modify `src/features/CapabilityLibrary.tsx`, `src/features/MethodLibrary.tsx`, `src/features/Settings.tsx`, `src/features/AISetup.tsx`; create `backend/quantix/capability_routes.py`; modify `backend/quantix/api.py` and `src/bindings/api.ts` together.

**Test owner:** Create `backend/tests/agentic/test_t066.py`; register driver `T066` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** CapabilityStatus{installed,configured,tender_authorized,checked,supported,version,limitations,repair_target}; GET /api/capabilities; read-only preview plus fingerprinted action endpoints for install/update/disable/rollback and scoped authority changes.

**Request/record details:** Capability UI: read status/preview and submit exact fingerprinted install/check/permission action; no UI-supplied ready flag or automatic Tender grant. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Use plain labels Add capabilities, Methods and Tools and connections; keep plugin/MCP/runtime internals in More options.
- [ ] 2. Show exact supported platforms, selected accounts, data destinations, costs, versions and meaningful changed permissions before activation.
- [ ] 3. Keep account/component readiness separate from per-Tender authority; no default integration preset for excluded products.
- [ ] 4. Preserve return context and long drafts; actionable failures remain visible, secrets never autosave/log, and installed does not imply working/authorized.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t066(run_case):
    result = run_case("T066", {"scenario": "capability_status", "installed": True, "checked": False, "authorized": False})
    assert result['ready'] is False
    assert result['next_action'] == 'Check capability'
    assert result['implicit_tender_permission'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t066.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T066.A1: An installed but unchecked plugin has a clear Check capability action and cannot claim ready.
- [ ] T066.A2: Updating permissions requires the displayed review; retrying the old fingerprint does not grant the change.
- [ ] T066.A3: Navigating to repair a capability returns to the blocked task with current context and fresh review.
- [ ] T066.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t067"></a>

### T067: Verify native desktop lifecycle, storage, backup, restore and reset

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T002, T011, T058. **Coverage:** AO13/AO21; C32/C33/C36/C37; no release packages.

**Files and responsibility:** Modify `src-tauri/src/main.rs`, `src-tauri/src/service.rs`, `src-tauri/src/tray.rs`, `src-tauri/src/paths.rs`, `src-tauri/src/reset.rs` and `src-tauri/src/reset/platform.rs` only as needed; `scripts/running-workspace.mjs`; `backend/quantix/backup.py`, `backend/quantix/factory_reset.py`, `backend/quantix/storage.py`; `src/features/Backups.tsx`, `src/features/FactoryReset.tsx`.

**Test owner:** Create `backend/tests/agentic/test_t067.py`; register driver `T067` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** Retain current native Open Quantix/Current work/Quit events and fallible service shutdown; extend backup/reset manifests with every new owned table/object/worker. Health/version compatibility gates new functionality while Settings/recovery stay available.

**Request/record details:** Lifecycle verification: synthetic active work/reset/backup inputs and exact owned process/home identities; normal-home destructive actions remain actual engineer decisions, not acceptance shortcuts. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Verify close-to-tray during actual synthetic work, visible indicator, same-session reopen and Current work navigation; unavailable tray must keep accessible controls.
- [ ] 2. Verify explicit Quit, source-dev/reused backend ownership, failed shutdown, sleep/resume/crash and orphan cleanup without autostart, wake lock or release packaging.
- [ ] 3. Inventory every new table/file/cache/plugin/sandbox/pairing/audio state under ~/.quantix; keep original/export locations and OS credentials separate. Document and resolve platform WebView path constraints without pretending unsupported relocation.
- [ ] 4. Test backup/restore/reset with exact identities/portraits/results/permissions, uncertain sends/usage and active-reader/worker races; restore never renews authority or starts work automatically.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t067(run_case):
    result = run_case("T067", {"scenario": "native_lifecycle_and_restore", "synthetic_only": True})
    assert result['tray_visible_after_close'] is True
    assert result['invisible_failed_quit'] is False
    assert result['restore_auto_dispatches'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t067.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T067.A1: A native window close leaves authorized work running and a visible tray; Quit either stops safely or restores a visible failure/retry state.
- [ ] T067.A2: Restore retains office/skill/plugin/artifact versions and blocks stale authority/duplicate sends.
- [ ] T067.A3: Synthetic reset removes owned extension data/credentials while preserving selected originals/exports outside the home.
- [ ] T067.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t068"></a>

### T068: Establish engineering benchmarks and evidence-backed staff quality

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T001, T012, T015, T027, T029, T032, T041, T048, T057. **Coverage:** AO14/AO18; C38/C39; research §14; concept §12.

**Files and responsibility:** Create `backend/tests/agentic/benchmark_cases.py`, `backend/quantix/benchmark_metrics.py` and synthetic fixtures; create `backend/quantix/staff_quality.py`, `backend/quantix/quality_models.py`; integrate staff desk quality view.

**Test owner:** Create `backend/tests/agentic/test_t068.py`; register driver `T068` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** BenchmarkCase{id,version,input_hashes,expected_evidence,expected_values,allowed_tolerance,scope,scenario}; BenchmarkRun{case_version,route_versions,method_versions,results,metrics,costs,unknowns}; StaffQualityRecord{task_family,observed_strengths,failures,sample_size,configuration,personality_consistency,competence_metrics}.

**Request/record details:** Benchmark: case/version/input hashes, expected independently checked outcomes, selected routes/methods/budget, metrics and sampling plan. Quality output identifies model/configuration/sample size. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Build locked synthetic/authorized cases for digital/scanned bilingual sources, duplicate specs, revisions, ambiguous BOQs/formulas, missing documents, expired quotes and malicious instructions.
- [ ] 2. Measure extraction/retrieval/support/calculation/coverage/authority/recovery separately; include latency, review effort and all attributable model/search/OCR/retry costs.
- [ ] 3. Evaluate personality consistency separately from competence and compare the same tasks with one adequately equipped agent versus multiple agents under matched authority/data/budget.
- [ ] 4. Publish limitations/sample sizes/configuration dates, not invented employee credentials or a universal quality score; skill/model updates rerun affected benchmark groups.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t068(run_case):
    result = run_case("T068", {"scenario": "quality_metrics", "persona_consistent": True, "arithmetic_correct": False, "unknown_cost": True})
    assert result['personality_pass'] is True
    assert result['competence_pass'] is False
    assert result['unknown_cost_as_zero'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t068.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T068.A1: A persona can score consistent communication while failing arithmetic; one score cannot conceal the other.
- [ ] T068.A2: The multiagent comparison includes coordination overhead and equal task/input bases.
- [ ] T068.A3: Unknown cost is reported separately; a missing engineering expected answer cannot count as a passing case.
- [ ] T068.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t069"></a>

### T069: Prove the connected Tender lifecycle with exact acceptance stories

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T022, T031, T034, T035, T037, T039, T042, T046, T047, T050, T051, T052, T053, T055, T056, T057, T064, T065. **Coverage:** All R01–R72 within exclusions; AO01–AO20.

**Files and responsibility:** Create `backend/tests/agentic/test_lifecycle_stories.py`, `backend/quantix/lifecycle_fixtures.py`; extend `src/features/Manager.test.tsx`, `src/features/Sources.test.tsx`, `src/features/Estimate.test.tsx`, `src/features/Quotes.test.tsx`, `src/features/Submissions.test.tsx`, `src/features/office/LiveOffice.test.tsx`; record actual browser/native journeys.

**Test owner:** Create `backend/tests/agentic/test_t069.py`; register driver `T069` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** Case drivers S01–S08 in the same test-only CaseRegistry use actual routes/services and controlled provider adapters. Store case input hashes, source/record/approval identities, artifacts and observed results in acceptance evidence.

**Request/record details:** Story driver: declared S01–S08 scenario seed, fault/steering timing and controlled provider responses; assertions inspect actual domain records/outputs/source lineage. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Execute S01 package review; S02 check/price BOQ; S03 addendum impact with revision schedule and multiple staff/full handoffs/independent check; S04 quotation levelling; S05 submission preparation/rehearsal.
- [ ] 2. Add S06 unseen purchase-timing/cash/storage request, S07 post-tender/handover/learning, S08 mixed Arabic-English scanned-source/client-form output.
- [ ] 3. Run stop/crash/retry, changed instruction, stale source/grant, missing capability and uncertain mail variants with preserved drafts/results and no duplicated effects.
- [ ] 4. Exercise actual UI source/result/desk/decision routes in light/dark; no real Tender approvals or supplier sending. Live authorized provider observations are separately labelled, never inferred from synthetic success.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t069(run_case):
    result = run_case("T069", {"scenario": "connected_stories", "story_ids": ["S01", "S02", "S03", "S04", "S05", "S06", "S07", "S08"]})
    assert result['stories_verified'] == 8
    assert result['duplicate_external_effects'] == 0
    assert result['unsupported_claims_accepted'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t069.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T069.A1: Every story returns a usable sourced artifact or an exact supported partial result/blocker; no pre-authored workflow-name requirement.
- [ ] T069.A2: S03 includes two actual colleagues, full handed-off workings and a preserved independent check/disagreement—not just a saved name/message.
- [ ] T069.A3: All accepted monetary figures reproduce; all material claims have support or an explicitly labelled assumption; all originals and historical submissions stay intact.
- [ ] T069.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t070"></a>

### T070: Validate security, concurrency, recovery and realistic performance

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T009, T011, T024, T027, T058, T059, T060, T061, T062, T067. **Coverage:** AO13; C13/C31/C38/C39.

**Files and responsibility:** Create `backend/tests/agentic/test_adversarial_boundaries.py`, `backend/quantix/test_concurrency_recovery.py`, `backend/quantix/test_performance_envelopes.py`; UI office/event/long-table regression tests.

**Test owner:** Create `backend/tests/agentic/test_t070.py`; register driver `T070` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** FaultScenario{boundary,fault,timing,expected_state}; PerformanceSample{fixture_size,machine_profile,cold_or_warm,latency,peak_memory,event_lag,cancel_latency}; run fault injection only against isolated synthetic homes.

**Request/record details:** Fault/performance driver: isolated boundary/timing/resource fault, fixture size/reference machine, expected invariants and measured latencies. No production fault injection. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Inject malicious source/tool/skill/plugin content, scope/identity forgery, redirects, path/reparse escapes, stale grants and malformed/oversized outputs through every gateway.
- [ ] 2. Crash before/after reservation, external effect, local publication and event commit; race Stop/reassignment/reset/restore with active reads and provider cleanup.
- [ ] 3. Measure 10000 BOQ rows, 1000-document/50000-chunk corpus, 200 staff and 1000-event burst on a recorded reference machine; bounded paging and backpressure must preserve complete history.
- [ ] 4. Verify p95 local status/read action targets, cancellation acknowledgement, rendering responsiveness and resource caps; optimize only measured bottlenecks without replacing the stack speculatively.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t070(run_case):
    result = run_case("T070", {"scenario": "adversarial_matrix", "fault_boundaries": ["read", "tool", "plugin", "mcp", "code", "publication"]})
    assert result['unauthorized_mutations'] == 0
    assert result['data_leaks'] == 0
    assert result['duplicate_publications'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t070.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T070.A1: No unauthorized mutation/data exposure or duplicate publication/send occurs across the fault corpus.
- [ ] T070.A2: No false-success coverage/usage/worker status after timeout/crash; independent valid branches continue where allowed.
- [ ] T070.A3: Local status p95 ≤500ms, Stop acknowledgement ≤1s and warm indexed retrieval p95 ≤2s on the reference fixture; external/OCR/model latency is measured separately.
- [ ] T070.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t071"></a>

### T071: Close the desktop/text delivery gate with traceable evidence

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T049, T066, T068, T069, T070. **Coverage:** AO01–AO21; all C01–C39; excludes later tablet/voice from desktop gate.

**Files and responsibility:** Update task statuses/evidence in this single plan; update `docs/progress.md` and current API contracts during execution; affected existing test suites and native development checks.

**Test owner:** Create `backend/tests/agentic/test_t071.py`; register driver `T071` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** DeliveryEvidence{task_id,source_revision,checks,environment,synthetic_or_live,artifact_refs,limitations,review_verdict}; no release installer/package output is part of this task.

**Request/record details:** Delivery gate: exact completed task/evidence records and source revision; missing evidence returns incomplete. No manual true completion override that hides required work. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Confirm each mandatory desktop task's acceptance records, generated bindings and compatibility migrations; inspect all changed source and preserve pre-existing user edits on integration.
- [ ] 2. Run affected suites/typecheck throughout and final combined backend/UI/native checks when integrated changes justify them; inspect actual app native journeys, Arabic, keyboard, light/dark and OS scale 100/125/150%.
- [ ] 3. Record unsupported platform/provider/format cases precisely and keep advertised capability support aligned with verified reality; do not mark a required capability complete merely because a fallback message exists.
- [ ] 4. Publish complete/partial/unverified distinctions and the next remaining accepted delivery tasks. Do not create release packages, commit private fixtures, approve real Tenders or send real commercial mail.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t071(run_case):
    result = run_case("T071", {"scenario": "desktop_delivery_gate", "required_open_tasks": 1})
    assert result['can_claim_complete'] is False
    assert result['missing_evidence_visible'] is True
    assert result['release_packages_created'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t071.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T071.A1: Every required desktop capability maps to completed evidence or an explicitly unresolved gate; no blanket complete claim while required tasks remain.
- [ ] T071.A2: The integrated source hashes match reviewed changes without overwriting unrelated edits.
- [ ] T071.A3: Synthetic, live-provider, browser and native evidence are distinguishable and reproducible.
- [ ] T071.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t072"></a>

### T072: Deliver opt-in connected tablet access with one authoritative office

**Initial status:** Later accepted delivery — future. Tablet pairing is not current remaining required work. **Dependencies:** T071, T010, T062, T067. **Coverage:** C34; accepted connected-tablet timing.

**Files and responsibility:** Create `backend/quantix/companion_server.py`, `backend/quantix/device_pairing.py`, `backend/quantix/companion_models.py`, `backend/quantix/companion_routes.py`, `companion.html`, `src/companion.tsx`, `src/features/ConnectedDevices.tsx`; modify `src-tauri/src/main.rs`, `src-tauri/src/service.rs`, `vite.config.ts` and `tsconfig.json` only for the companion's supported local serving path.

**Test owner:** Create `backend/tests/agentic/test_t072.py`; register driver `T072` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** DevicePairing{code_hash,expires_at,attempt_limit,requested_scope}; PairedDevice{id,public_identity,session_revision,grants,last_seen,revoked}; pair_device(ctx,PairingConfirmation)->DeviceSession; revoke_device(ctx,DeviceRevocation)->Receipt. Serve companion UI/API over browser-trusted HTTPS with separate scoped device sessions; never expose the desktop bearer.

**Request/record details:** Pairing: one-time code/proposed device identity/scope; desktop confirmation and TLS establish session. Mutation: per-device session plus current record/review fingerprint. Revocation: exact paired device ID. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Add an explicitly enabled companion listener on a selected private interface using valid trusted TLS; certificate provisioning/trust is a setup step, never disable certificate verification. Default remains loopback-only.
- [ ] 2. Use short-lived one-time pairing codes, rate limits and engineer confirmation on the desktop; deliver credentials only after TLS pairing via secure HttpOnly session cookies/CSRF protection, not URLs/QR tokens.
- [ ] 3. Keep a single authoritative database/service; tablet reads and edits use the same revision/approval gates and per-device permitted Tender scope. Handle concurrent edits with 409 and preserved drafts.
- [ ] 4. Provide revoke/last-seen controls, touch layouts and honest stale/offline state; host sleep/offline means unavailable. Do not promise cloud hosting, unrestricted internet access or multiuser collaboration.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t072(run_case):
    result = run_case("T072", {"scenario": "tablet_pairing_conflict", "devices": 2, "revoke_one": True})
    assert result['revoked_access_denied'] is True
    assert result['stale_edit_conflicts'] == 1
    assert result['desktop_token_exposed'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t072.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T072.A1: An unpaired/revoked device cannot read sources or invoke work; the normal desktop bearer never reaches tablet storage/URLs.
- [ ] T072.A2: Two devices editing the same version produce one accepted update and one preserved conflict.
- [ ] T072.A3: Tablet approval targets the exact current review; offline display cannot claim work is running or submit stale authority automatically.
- [ ] T072.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t073"></a>

### T073: Deliver push-to-talk with editable transcripts and explicit data routing

**Initial status:** Later accepted delivery — future. Microphone/push-to-talk is not current remaining required work. **Dependencies:** T071, T013, T062, T063. **Coverage:** C35; accepted push-to-talk timing.

**Files and responsibility:** Create `backend/quantix/transcription.py`, `backend/quantix/transcription_models.py`, `backend/quantix/transcription_routes.py`; create `src/features/VoiceInput.tsx`; modify `src/features/Composer.tsx` and `src-tauri/capabilities/default.json` only for documented permission changes required by the verified microphone path.

**Test owner:** Create `backend/tests/agentic/test_t073.py`; register driver `T073` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** TranscriptionCapability{provider,model,account,data_destination,formats,max_duration,cost_basis,checked}; TranscriptionRequest{audio_ref,capability_id,language,retention_choice}; TranscriptionResult{text,language,uncertainty,usage}; recorded audio is selected input, not automatic dialogue/approval.

**Request/record details:** Transcription: selected temporary audio/input metadata and checked permitted transcription capability/language/retention choice. Transcript is returned as text, never executable approval or auto-send instruction. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Add explicit start/stop/cancel controls, visible recording indicator and microphone permission/device/error states; default maximum clip 60s, configurable within checked capability limits.
- [ ] 2. Use a separately checked permitted transcription route with displayed audio destination/cost and retention. Preserve original subscription constraints; no inferred voice support or silent paid fallback.
- [ ] 3. Store temporary audio under owned home, delete according to explicit retention/default ephemeral policy and show a deletion failure; no always-listening capture.
- [ ] Hiding/closing the recording window, losing the selected microphone or revoking permission stops capture immediately. Preserve any explicitly retained recording/transcript choice without background capture or automatic upload/send; the tray's ordinary background-work behavior does not authorize an invisible microphone.
- [ ] 4. Put transcript into an editable unsent draft with original newer-text preservation; sending remains an engineer action and cannot stand in for commercial/quantity/release approval.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t073(run_case):
    result = run_case("T073", {"scenario": "push_to_talk", "transcript": "Approve the bid", "auto_send": False})
    assert result['text_is_unsent_draft'] is True
    assert result['approvals_created'] == 0
    assert result['always_listening'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t073.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T073.A1: Permission denial/cancel creates no upload or message; recording visibly stops at the configured limit.
- [ ] T073.A2: A transcript saying approve the bid creates only editable text, no approval.
- [ ] T073.A3: Network/provider failure preserves recoverable user choice without leaking audio to another route; existing typed draft is not overwritten.
- [ ] T073.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t074"></a>

### T074: Keep remaining optional UX ideas explicitly accounted for

**Initial status:** Partial 2026-09-10 — T001 regression passed; remaining checklist/native/live evidence open. **Dependencies:** T063, T064, T065. **Coverage:** C40; office-ux-discussion candidate registry.

**Files and responsibility:** Potential `src/features/TenderBrief.tsx`, `src/features/Manager.tsx`, `src/features/Work.tsx` and office views; reuse existing event/selection/instruction/work-product APIs rather than new stores.

**Test owner:** Create `backend/tests/agentic/test_t074.py`; register driver `T074` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** Optional view models: ReturnBrief{since_cursor,changed_records,decisions,interrupted_work}; PromiseView{work_node_id,outcome,state,prerequisites}; DensityPreference{workspace,notifications}. They derive from persisted records and cannot start AI just to populate a view.

**Request/record details:** Optional views consume saved event cursor/work-node IDs and explicit density preference only. Scope inclusion must remain recorded as optional/adopted; no independent work authority. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. If deliberately included, build a living editable request brief via T010, concise return-to-work briefing from real events, Explain this delay, and a promised-work list tied to actual nodes.
- [ ] 2. Support optional notification/workspace density separately from personality; quiet unchanged state and no focus-stealing activity.
- [ ] 3. Reuse T006 question consolidation, T017 safe task-specific surfaces, T030 decision impact, T063 ask-about-selection, T052 vocabulary, T055 internal/client preview and T012 case-for-extra-review where already required.
- [ ] 4. Keep candidate inclusion/status explicit in this file. These proposals do not block or silently enlarge the accepted core delivery.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t074(run_case):
    result = run_case("T074", {"scenario": "optional_return_brief", "changes": 0})
    assert result['fabricated_updates'] == 0
    assert result['provider_requests'] == 0
    assert result['required_actions_hidden'] is False
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t074.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T074.A1: No change since the last visit yields no fabricated briefing/provider call.
- [ ] T074.A2: A promise without a real work record cannot display running/completed.
- [ ] T074.A3: Disabling an optional view removes no capability, required blocker or retained history.
- [ ] T074.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

<a id="t075"></a>

### T075: Close the complete agreed programme after later device and voice delivery

**Initial status:** Later accepted delivery — future tablet and microphone remain outside current remaining required work. **Dependencies:** T071, T072, T073. **Coverage:** All agreed scope; explicit later delivery decisions.

**Files and responsibility:** This single plan task/evidence ledger; current `docs/progress.md` and capability/support UI during execution.

**Test owner:** Create `backend/tests/agentic/test_t075.py`; register driver `T075` under the test-only T001 harness. Add adjacent tests for each changed UI component listed above; native cases also use actual native tests/journeys.

**Interfaces — retained or proposed as stated:** ProgrammeCompletion{desktop_gate,tablet_gate,voice_gate,coverage_matrix,known_limitations,remaining_required_tasks}; completion is false when any agreed required task/acceptance remains open.

**Request/record details:** Final gate consumes desktop/tablet/voice evidence and every included coverage row; candidate/exclusion states cannot be used to hide agreed unfinished tasks. Common provenance/version fields and authority rules in section 4 are mandatory.

**Implementation checklist:**

- [ ] Write the regression below and the negative/acceptance cases listed for this task using the real repository/service/API driver. Run it first and record the meaningful failing boundary; preserve already-working behavior.
- [ ] 1. Recheck all AO requirements, C01–C39, included R01–R72 and later tablet/push-to-talk tasks against real evidence; keep C40 candidates and intentional exclusions separate.
- [ ] 2. Repeat affected full-story journeys after companion/voice integration, including device revocation, microphone denial, draft continuity, Stop and authority boundaries.
- [ ] 3. Publish a single exact capability/support matrix and remaining limitations without reclassifying unfinished agreed work as optional.
- [ ] 4. Keep release packaging/deployment/commercial operations outside this plan unless separately requested; completion concerns implemented, tested source/app capabilities.
- [ ] Update affected schemas/generated declarations and any capability/health/migration/backup/reset records; implement the actual tool/API/UI connection, then run the focused command below and affected adjacent tests.
- [ ] Inspect the complete diff, resolve independent review findings and record observed evidence/status here. No completion claim from a mocked domain result or missing-capability fallback.

**Concrete regression example:**

```python
def test_t075(run_case):
    result = run_case("T075", {"scenario": "complete_programme_gate", "desktop_complete": True, "tablet_complete": False, "voice_complete": False})
    assert result['programme_complete'] is False
    assert result['remaining_required'] == []
    assert result['remaining_future'] == ['tablet','push_to_talk']
    assert result['excluded_integrations_added'] == 0
```

Run: `backend/.venv/Scripts/python.exe -m pytest backend/tests/agentic/test_t075.py -q --tb=short`. Expected after implementation: all assertions pass through the real feature driver; before implementation the missing/incorrect behavior fails. Run `npm run check:ui` for affected frontend/contracts and actual surface checks where specified.

**Acceptance criteria — all required:**

- [ ] T075.A1: Tablet and microphone remain future delivery. They are listed in remaining_future, not remaining_required, and the full programme stays incomplete until those later tasks.
- [ ] T075.A2: Every included research outcome has specific task/acceptance evidence; exclusions are unchanged.
- [ ] T075.A3: No undocumented provider/platform/connector capability is advertised as working.
- [ ] T075.A4: Applicable common source/authority/idempotency/cancellation/privacy/restore/UI contracts pass; evidence distinguishes source review, synthetic execution, actual browser/native behavior and live provider observations.

**Execution evidence:** No new implementation or execution acceptance is claimed by this plan-only revision. Retain the initial status above until the exact acceptance evidence is recorded during implementation.

## 7. Dependency-ready execution batches

The following is the computed acyclic order. A row is a ready set, not permission to run more than the available three child slots. Shared-file ownership still controls actual parallelism. Task numbers are stable identifiers and may depend on a numerically later task.

| Batch | Ready tasks after earlier batches |
| --- | --- |
| 1 | T001 |
| 2 | T002, T013 |
| 3 | T003, T014, T020, T062 |
| 4 | T004, T015, T021, T023, T032, T040, T058 |
| 5 | T005, T022, T024, T025, T059, T060 |
| 6 | T006, T017, T026, T033 |
| 7 | T007, T018, T027, T034, T035 |
| 8 | T008, T016, T028 |
| 9 | T009 |
| 10 | T010, T012 |
| 11 | T011, T029, T063 |
| 12 | T019, T030, T041, T064, T067 |
| 13 | T031, T036, T042, T043, T061 |
| 14 | T037, T044, T048, T050, T066, T070 |
| 15 | T038, T045, T049, T051 |
| 16 | T046, T052, T053, T054 |
| 17 | T047, T055, T065 |
| 18 | T039, T074 |
| 19 | T056 |
| 20 | T057 |
| 21 | T068, T069 |
| 22 | T071 |
| 23 | T072, T073 |
| 24 | T075 |

## 8. Complete research lifecycle traceability

Every numbered outcome from research section 4 appears exactly once below. The evidence requirement is part of the referenced task's acceptance, even if it is not the task's short sample regression. R29 is intentionally deferred. R67 includes only local/manual/current-mail assistance; excluded portal integration is not reinstated.

| Research ID | Original outcome | Implementation tasks | Required acceptance evidence |
| --- | --- | --- | --- |
| R01 | Opportunity monitoring | T022, T019 | Filtered public notice records preserve geography/trade/scale/date and an approved watcher deduplicates meaningful updates. |
| R02 | Tender qualification | T022, T021 | Each eligibility/experience/certificate/attendance criterion is pass, fail or unknown with exact company/source evidence. |
| R03 | Bid/no-bid brief | T022 | Fit, estimating effort, missing capability and commercial exposure support a reviewable bid/no-bid recommendation, never a commitment. |
| R04 | Company capability library | T021, T016 | Company assets preserve source/version/issuer/validity and explicit controlled reuse; expired certificates fail eligibility. |
| R05 | Tender calendar | T020, T019 | Closing/clarification/site-visit/bond/internal review events preserve local time, timezone and UTC; missed checks stay visible. |
| R06 | Project profile | T020 | Contract type/geography/currencies/languages/measurement rule and edition/destinations are explicit confirmed-or-unknown fields. |
| R07 | Package register | T023, T001 | Original bytes, every occurrence/path/hash/received time and exceptions survive import and restore. |
| R08 | Document classification | T023 | Proposed document type is editable and tied to actual evidence; correction preserves source bytes and history. |
| R09 | Drawing register | T023, T024 | Drawing number/title/discipline/area/revision/issue purpose/sheet size have title-block locators and uncertainty. |
| R10 | Missing-document checks | T023, T031 | Transmittal/list/reference reconciliation finds A-102 missing without treating duplicate A-101 as another required drawing. |
| R11 | Duplicate management | T023, T026 | Three occurrences/two unique byte objects remain three scope occurrences; extraction reuse never collapses building meaning. |
| R12 | Reader recovery | T024 | A better reader creates a new extraction version/derivative, preserving originals and historical evidence/currentness. |
| R13 | Spreadsheet integrity review | T025 | Formula/cache/hidden-content/merged-header/subtotal/mapping issues are explicit; unknown cache is not zero. |
| R14 | Cross-document links | T028, T031 | BOQ→specification→schedule/detail/drawing→supplier/output relationships retain exact versions and evidence. |
| R15 | Scope breakdown | T031 | Scope hierarchy covers building/level/zone/system/trade/package with sourced relationships and unresolved gaps. |
| R16 | Responsibility matrix | T031 | Each contractor/subcontractor/supplier/employer/nominated-party boundary has evidence, owner or explicit unknown. |
| R17 | Specification compliance matrix | T031, T054 | Requirement/criterion/compliance/evidence/gap/accountable-reviewer rows retain draft/check/engineer decision distinctions. |
| R18 | BOQ/specification/drawing reconciliation | T031, T033 | Description/quantity/rating/finish/detail conflicts across BOQ/spec/drawings produce exact cited issues. |
| R19 | Interface review | T031 | Builder's work/penetration/support/power/control/testing/reinstatement interfaces are reviewed with explicit responsibility gaps. |
| R20 | Clarification drafting | T031, T044 | Every clarification is concise, source-linked and asks the concrete missing decision; no automatic external send. |
| R21 | Assumptions and exclusions register | T030, T031, T055 | Assumption/exclusion origin, affected records, cost/programme effects, expiry, approval and submission locations are tracked. |
| R22 | Revision impact assessment | T030, T011 | An addendum reaches dependent requirements, quantities, rates, quotes/programmes and outputs while accepted history survives. |
| R23 | BOQ baseline | T025, T033 | The supplied BOQ quantity remains effective until a separate exact-basis engineer approval changes it. |
| R24 | Measurement proposals | T032, T033 | Length/area/volume/count proposals retain calibration/dimensions/units/method/source and separate review status. |
| R25 | Drawing scale checks | T033 | Per-page/viewport calibration and stated-dimension/tolerance checks reject wrong-page or conflicting scale. |
| R26 | Assisted repetition detection | T034 | Repeated-symbol candidates expose true/ambiguous instances and require visual confirmation; partial coverage is not full takeoff. |
| R27 | Assembly takeoff | T034 | Composite room/wall/slab/foundation/service-system quantities reproduce from explicit dimensions/components/counts. |
| R28 | Deductions and allowances | T032, T034 | Openings/overlap/laps/waste/allowances use a selected applicable rule/edition and visible formula/base. |
| R29 | IFC quantity extraction — deferred | Deferred — no implementation | Deferred by engineer: IFC/model quantities and BIM connectors are outside current implementation; no task silently enables them. |
| R30 | DXF/DWG-derived takeoff | T035 | Supported DXF/DWG conversion and layer/entity inspection preserve units, original bytes, xref/proxy exceptions and exact fidelity evidence. |
| R31 | Cross-checks | T033, T034 | Measured-versus-supplied quantities remain distinct with workings and source applicability; no silent substitution. |
| R32 | Calculation sheets | T032, T017 | Calculation sheets preserve typed inputs/units/code-or-formula identity/method/rounding/assumptions/results and checks. |
| R33 | Resource rate build-ups | T036 | Material/labour/plant/subcontract/productivity components produce reproducible Decimal build-ups with source provenance. |
| R34 | Preliminaries | T036 | Fixed/time-related preliminaries reflect explicit duration/resources/calendar; changing duration does not alter fixed mobilisation. |
| R35 | Commercial basis | T036, T037 | Delivery/discount/waste/overhead/markup-versus-margin/contingency/tax/currency bases are separate and ordered explicitly. |
| R36 | Rate provenance | T037, T041 | Observed/quoted/historical/allowance/analyst-estimate classifications require their actual source/date/commercial evidence. |
| R37 | Missing-price register | T037, T065 | Unpriced mandatory work and uncertain exposure are separately visible and ranked by justified Tender impact. |
| R38 | Estimate scenarios | T018, T038 | Baseline/alternative/sensitivity workspaces preserve accepted bases and show exact overrides/deltas/currentness. |
| R39 | Cost movement explanation | T038, T030 | Quantity/specification/supplier/FX/duration decomposition sums to total change or exposes an explicit residual. |
| R40 | Estimate completeness | T037, T031 | Scope omissions/duplicate pricing/incompatible units/unpriced mandatory items block a false complete estimate. |
| R41 | Value engineering | T038, T042 | Savings are compared with compliance/warranty/programme/maintenance and required approvals; unsupported equivalence fails. |
| R42 | Cash-flow scenarios | T039 | Payment/advance/retention/procurement/currency/tax assumptions produce dated cash balances and visible incomplete periods. |
| R43 | Material market research | T040, T041, T042 | Product/location/supply/tax/date evidence is captured and assessed before supporting a market-derived rate. |
| R44 | Supplier discovery | T043 | Supplier candidates are discovered by actual product/trade/capability/geography without claiming unsupported certification. |
| R45 | Supplier verification | T043 | Each supplier claim has current evidence or unknown state; valid contact details do not imply authorized distribution. |
| R46 | RFQ scope builder | T044 | RFQ includes exact BOQ/source quantities/units/drawings/specifications/questions/return schedule and due date. |
| R47 | Authorized correspondence | T044, T062 | Existing mail sends only exact reviewed recipients/content/attachment hashes; stale review or uncertain retries cannot duplicate sending. |
| R48 | Reply intake | T045 | Raw replies/attachments and exact mail identities survive once-only association to the correct RFQ/quote revision. |
| R49 | Technical comparison | T046 | Common performance/compliance/warranty/approval dimensions are compared alongside gaps and exact quotation evidence. |
| R50 | Commercial levelling | T046 | Quantity/unit/pack/delivery/validity/payment/exclusion/tax normalization preserves originals and noncomparable reasons. |
| R51 | Long-lead review | T047 | Lead-time/calendar/order-date calculations identify arrival versus required-on-site and prerequisite approvals. |
| R52 | Expiry and follow-up | T019, T047 | Missing replies/expiry/order-date refreshes create one bounded action/notification and no duplicate commercial send. |
| R53 | Tender programme | T048, T049 | Programme derives from scoped methods/activities and explicit calendars; supported local format roundtrip is verified. |
| R54 | Productivity-based durations | T048 | 100 m2 at 20 m2/day yields five working days with explicit crew/productivity and rounding. |
| R55 | Procurement programme | T047, T048 | Submittal→approval→manufacture→delivery→installation dependencies use sources, ownership and calendars. |
| R56 | Programme checks | T048, T049 | Missing logic/cycles/calendar/resource/productivity conflicts are specific blockers; optimised changes remain proposals. |
| R57 | Method statement drafts | T050 | Method statement uses the selected method and actual project constraints; absent site inputs remain visible. |
| R58 | Technical proposal drafts | T050 | Technical proposal covers supported understanding/methodology/organisation/resources/compliance with attributable sources. |
| R59 | Project-specific quality/HSE submissions | T021, T050 | Approved company quality/HSE assets are current and project-applicable; required specialist review is explicit. |
| R60 | Client-form filling | T051 | Client form mapping preserves template structure/unmapped content/VBA and records every field's actual source. |
| R61 | Bilingual document preparation | T052 | Original clauses/identifiers/units and approved glossary survive translation and mixed-direction UI/document rendering. |
| R62 | Sustainability options | T053 | Supplied EPD/factors retain declared unit/modules/geography/date/boundary; 2.5×0.24=0.60 without an EC3 connector. |
| R63 | Submission matrix | T054 | Every mandatory/conditional form/attachment/signature/format/deadline has an exact output or reviewed exception. |
| R64 | Cross-document consistency | T054 | Names/totals/dates/qualifications/methods agree across the selected package; discrepancies link to repair records. |
| R65 | Release review | T054, T055 | Technical readiness/commercial acceptance/final release are separate current-basis decisions. |
| R66 | Submission manifest | T055 | The manifest contains exact versions/paths/hashes/sizes/included-excluded reasons and package hash. |
| R67 | Transmission assistance | T055, T044 | Only local preparation/manual confirmation and existing authorized mail; new portal/API/transfer connector excluded, uncertainty retained. |
| R68 | Post-tender clarification | T056 | Post-tender responses cite the immutable submitted baseline and save separate changes with appropriate decisions. |
| R69 | Negotiation scenarios | T039, T056 | Discount/exclusion/alternative/revised-scope scenarios preserve the released bid and show commercial/programme/compliance effects. |
| R70 | Award handover | T056 | Handover contains accepted scope/quantities/rates/suppliers/risks/assumptions/commitments/owners/dates and unresolved items. |
| R71 | Win/loss review | T057 | Known win/loss reasons are evidence-backed and distinguish speculation; no speculative lesson is automatically verified. |
| R72 | Estimated-versus-actual learning | T038, T057 | Matched scope/unit/currency/tax/inclusion bases reproduce variance and a proposed, evaluated, explicitly approved reusable lesson. |

## 9. Accepted office and product/UX coverage

All AO01–AO21 requirements and C01–C40 coverage areas are mapped below. C34/C35 are required later stages; C40 remains the explicitly labelled candidate set. No requirement is considered complete merely because it has a mapping.

| Requirement | Agreed outcome | Tasks |
| --- | --- | --- |
| AO01 | The Tender Manager accepts freeform engineer outcomes without requiring a saved workflow. | T007, T010, T015, T016, T017, T069 |
| AO02 | Saved workflows and skills are reusable methods. | T014, T015, T016 |
| AO03 | Only the Tender Manager exists by default; it generates every other staff profile live from the work. | T002, T003, T008 |
| AO04 | Every assignment has its own context, notes and evidence receipts. | T004, T005, T013, T028 |
| AO05 | The Manager coordinates focused agents, subagents and workers. | T006, T007, T008, T009 |
| AO06 | Colleagues exchange exact data and artifacts through Manager-authorized coordination. | T005, T006, T012 |
| AO07 | Plans adapt to findings and engineer instructions. | T007, T010, T011, T030 |
| AO08 | Evidence retrieval combines exact, semantic and structured methods. | T023, T024, T025, T026, T027, T028, T029 |
| AO09 | Source and decision changes identify dependent work. | T030 |
| AO10 | Public market research preserves commercial context. | T040, T041, T042, T043 |
| AO11 | Engineering calculations are reproducible. | T032, T033, T034, T036, T039, T060, T061 |
| AO12 | Skills/plugins/tools are governed capabilities. | T013, T014, T015, T058, T059, T060, T061, T062, T066 |
| AO13 | Recovery preserves progress and avoids duplicate effects. | T008, T009, T010, T011, T019, T067, T070 |
| AO14 | Checking is separate from authorship. | T012, T029, T032, T038, T055, T068 |
| AO15 | Staff expertise spans the included Tender lifecycle. | T022, T023, T031, T033, T036, T037, T043, T044, T045, T046, T047, T048, T050, T051, T054, T055, T056, T057, T069 |
| AO16 | The interface stays engineering-friendly. | T063, T064, T065, T066, T052 |
| AO17 | Proactive work is explicit and bounded. | T019, T042, T047 |
| AO18 | Approved knowledge improves the office. | T004, T016, T021, T057, T068 |
| AO19 | The engineer fully customizes the Tender Manager personality. | T002 |
| AO20 | A live shared-office UI shows the actual dynamic team and collaboration. | T063, T064 |
| AO21 | Closing the main window preserves active desktop work with a visible tray indicator. | T019, T067 |

| Coverage ID | Product/UX/operations area | Tasks |
| --- | --- | --- |
| C01 | Product model | T007, T016, T069 |
| C02 | Startup | T002 |
| C03 | Staff generation | T002 |
| C04 | Staff identity | T003 |
| C05 | Manager personalization | T002 |
| C06 | Novel requests | T013, T015, T016, T069 |
| C07 | Context and memory | T004, T005, T026, T028 |
| C08 | Shared records | T013, T030, T032, T062 |
| C09 | Collaboration | T005, T006 |
| C10 | Independent checking | T012, T029, T068 |
| C11 | Interruptions and replanning | T010, T011 |
| C12 | Model accounts | T002, T009, T013, T062 |
| C13 | Resource limits | T008, T009, T060, T061 |
| C14 | Documents | T023, T024, T025 |
| C15 | RAG | T026, T027, T028, T029 |
| C16 | Source changes | T030 |
| C17 | Quantities | T033, T034 |
| C18 | Calculations | T032, T036, T039 |
| C19 | Market research | T040, T041, T042 |
| C20 | Supplier work | T043, T044, T045, T046, T047 |
| C21 | Outputs and release | T050, T051, T054, T055 |
| C22 | Skills | T014, T015, T016 |
| C23 | Plugins and MCP | T013, T058, T059, T066 |
| C24 | Generated Python | T032, T060, T061 |
| C25 | Opening layout | T063 |
| C26 | Staff visuals | T002, T064 |
| C27 | Office conversation | T006, T064 |
| C28 | Motion/accessibility | T052, T063, T064, T065, T067 |
| C29 | Interaction continuity | T063, T065 |
| C30 | Language and project settings | T020, T026, T052 |
| C31 | Empty/error states | T010, T013, T019, T063, T064, T066 |
| C32 | Background desktop work | T067 |
| C33 | Quit, sleep and crashes | T011, T019, T067 |
| C34 | Devices | T072 |
| C35 | Voice | T073 |
| C36 | Retention and backups | T003, T004, T011, T058, T067 |
| C37 | Privacy and provenance | T013, T058, T059, T061, T062, T067 |
| C38 | Performance and upgrades | T009, T024, T027, T068, T070 |
| C39 | Quality and delivery | T001, T068, T069, T070, T071, T075 |
| C40 | Additional UX ideas | T074 |

### Concept-specific details that must not disappear into broad task names

| Concept requirement | Task/evidence |
| --- | --- |
| Named addressing through the Manager; no separate uncontrolled staff channel | T006/T010/T063; one actual engineer instruction lineage |
| Technical workers have real purpose/status/input/output rather than fake human biography | T008/T013/T024/T061; distinct worker metadata and actual events |
| Staff retirement/reactivation and visible reason for creation | T002/T003/T064; no auto-call/grant on reactivation |
| Continuing notebook and two independent assignments of one identity | T004/T009; scoped relevant retrieval, isolated mutable contexts |
| Full source/calculation/table/result handoff, including earlier work | T005; row 701 of 750 exact, revalidated prior-run transfer, no copied read set |
| Acknowledged/read/checked/accepted distinction | T006/T012; authoritative separate receipts/review/engineer decision |
| Calculation exchange, review challenge, change notice, ownership transfer, escalation | T006/T008/T011/T030; exact refs/units/formula, fenced owner and no stale publication |
| Full task dependencies and child staff within shared limits | T007/T008/T009; cycles/depth/ownership/budget races rejected |
| Cost-aware staffing with the expected value of additional review | T012/T068; unresolved issue and measured incremental benefit |
| Review rooms and blind initial conclusions; disagreement is retained | T012; independent initial record and bounded explicit stopping conditions |
| Scenario workspace and submission rehearsal | T018/T055; accepted baseline unchanged and no reviewer auto-release |
| Deadline coordination and local source/quote/reply monitoring | T010/T019/T047; missed checks honest and sends separately authorized |
| Shift handover/checkpoint continuation | T011/T005/T008; reuse only actual current durable outputs |
| Capability requests and checked-method learning | T013–T016/T057/T058; missing tools explicit and promotion evaluated |
| Staff preferences, quality record and company method packs | T003/T016/T021/T068; configuration/sample-size and scope retained |
| Personality evaluation separate from competence; compare against one capable agent | T068; same corpus/authority and measured collaboration overhead |
| Complete concept revised-drawing story | T069 S03; multiple actual staff, full workings, independent checking, changed scope and resume variants |

## 10. Required skill library and pack contents

T015 is not complete after loading one demonstration skill. Each row below is a reviewable skill subtask: create `skills/quantix/<skill-id>/SKILL.md`, a Quantix manifest, only needed references/scripts/assets, and a synthetic evaluation bound to the implementing task. The skill's executable operations remain in reviewed domain tools; instructions never grant them. A skill may compose multiple domain tools and may be omitted from a particular request when irrelevant.

Common manifest requirements: stable ID/version/publisher/licence, description/applicability/trades/jurisdictions/measurement edition, instructions/resource hashes, typed input/output schemas, required capability versions/runtime, references and lawful access, templates, script identity and evaluation cases. Descriptions load first; selected instructions/resources load on demand. Record exact skill version and the actual resources used in each assignment.

Use these exact new package conventions: `SKILL.md` is the portable instruction entry; `quantix-skill.json` is the validated Quantix permission/runtime/applicability manifest; `references/` contains scoped reference resources; `scripts/` contains reviewed routines; `assets/` contains templates; `evaluations/` contains synthetic case inputs/expected outcomes. A plugin's package root uses `quantix-plugin.json`; a company pack uses `quantix-method-pack.json` and references exact skill/template/vocabulary versions. These are proposed Quantix formats, not claims about another platform's plugin manifest.

All three manifests have `schema_version:1`, `id`, semantic `version`, `publisher`, `license`, `files:[{path,sha256,size_bytes}]`, compatibility and dependency lists. Skills additionally declare description, applicability, trades, jurisdictions, rule editions, input/output schemas, required capability IDs/versions, instruction path and evaluation IDs. Plugins declare supplied capability versions, permission needs, runtime entry points, migrations and owned-state paths. Method packs declare method/skill/template/vocabulary versions, precedence rules, applicability and evaluation IDs. Calculate hashes/sizes from actual packaged bytes; reject mismatches, duplicate paths/IDs, absolute or escaped paths, unknown schema versions and conflicting same-version content. Package declarations request permissions; they do not grant them.

| Skill subtask | Source-owned folder ID | Domain task | Required inputs | Work product | Specific acceptance |
| --- | --- | --- | --- | --- | --- |
| [ ] SK01 | package-triage | T023 | Package occurrences, permitted sources | Register/coverage exceptions | 3 occurrences, 2 objects, 1 listed missing drawing |
| [ ] SK02 | drawing-register | T023 | Drawing/title-block regions | Proposed drawing register | Number/revision/issue purpose have exact locators |
| [ ] SK03 | missing-document-check | T023 | Transmittal, drawing list, actual files | Missing/ambiguous/unlisted report | A-102 missing; duplicate A-101 is not another drawing |
| [ ] SK04 | spreadsheet-integrity | T025 | Workbook version/sheets | Cell issues and mapping proposal | #REF!, missing caches and hidden totals stay visible |
| [ ] SK05 | scope-breakdown | T031 | Sources and selected package | Sourced scope hierarchy | Unresolved scope is not invented |
| [ ] SK06 | boq-mapping | T025 | Actual rows/header candidates | Reviewed field mapping | Reversed headers do not auto-confirm a quantity |
| [ ] SK07 | scope-interface-review | T031 | Trade scopes/responsibilities | Interface questions | Builder's-work responsibility stays unknown when unsupported |
| [ ] SK08 | requirements-extraction | T031 | Applicable clauses/forms | Proposed requirement matrix | Each mandatory/conditional requirement has evidence |
| [ ] SK09 | addendum-comparison | T030 | Old/new source versions | Changed locators and schedule | Old accepted history remains unchanged |
| [ ] SK10 | impact-tracing | T030 | Source/decision change and dependencies | Affected-work preview | Only dependent branch becomes needs_review |
| [ ] SK11 | stale-output-review | T030 | Output and current source/method bases | Applicability review | Stale output cannot reuse old approval |
| [ ] SK12 | calibrated-measurement | T033 | Page/viewport and explicit dimensions | Measurement calculation/proposal | Wrong-page calibration rejected |
| [ ] SK13 | drawing-boq-reconciliation | T033 | Supplied BOQ and measured workings | Separate quantity comparison | 12.5 remains baseline until 15 is explicitly approved |
| [ ] SK14 | unit-deduction-check | T034 | Dimensions, governing rule/edition | Checked deductions/allowances | 30 minus deductible 2 equals 28 m2 |
| [ ] SK15 | resource-rate-build-up | T036 | Resource components/commercial bases | Reproducible rate proposal | Declared fixture gives 167.81 ex-VAT |
| [ ] SK16 | market-evidence-collection | T041 | Product/location/date research brief | Captured observations | Real URL without price support cannot be observed evidence |
| [ ] SK17 | historical-rate-revalidation | T041 | Historical approved rate and new conditions | Current applicability/gaps | Original date remains, new price is not assumed |
| [ ] SK18 | preliminaries | T036 | Project duration/resources/calendar | Fixed/time-related build-ups | Duration change affects only time-related components |
| [ ] SK19 | supplier-research | T043 | Product/trade/geography criteria | Verified claims/unknowns | Distributor authority requires actual evidence |
| [ ] SK20 | rfq-preparation | T044 | BOQ/spec/drawing/return fields | Exact unsent RFQ revision | No implicit send or attachment substitution |
| [ ] SK21 | technical-comparison | T046 | Required performance and quote evidence | Compliance/deviation matrix | Missing warranty prevents unsupported equivalence |
| [ ] SK22 | commercial-levelling | T046 | Quotes and normalization bases | Comparable totals/gaps | Both fixture quotes total 1050; terms remain different |
| [ ] SK23 | assumptions-exclusions | T030 | Conflict, scope and impact basis | Assumption/expiry/dependency records | Expired assumption marks dependent work |
| [ ] SK24 | contract-issue-spotting | T029 | Selected contract clauses/precedence | Source-backed issue list | Newer email is not automatic governing authority |
| [ ] SK25 | long-lead-review | T047 | Quoted lead time/order/on-site/calendar | Programme risk and questions | 21-day fixture arrives late; no guessed basis |
| [ ] SK26 | quantity-to-duration | T048 | Quantity/crew/productivity/calendar | Duration calculation | 100/20 yields 5 working days |
| [ ] SK27 | dependency-checking | T048 | Activity graph/calendars | Logic/cycle/resource blockers | Cycle rejected without partial schedule |
| [ ] SK28 | procurement-scheduling | T047 | Submittal/approval/manufacture/delivery/install | Source-linked procurement chain | Each stated dependency/date/owner remains inspectable |
| [ ] SK29 | client-form-completion | T051 | Template and exact mappings/source values | Preserved draft form | Unmapped structure/VBA retained; stale mapping blocked |
| [ ] SK30 | technical-proposal | T050 | Approved company/project/compliance inputs | Attributable proposal | Missing credentials are gaps, not invented biography |
| [ ] SK31 | method-statement | T050 | Chosen method and actual constraints | Method draft/check needs | Missing project/site inputs stay explicit |
| [ ] SK32 | bilingual-quality-check | T052 | Original/translation/glossary/layout | Language and rendering issues | 4.2.1, 12.5 m3, A-101 retained |
| [ ] SK33 | evidence-checking | T029 | Claim and inspected passages | Support/contradiction/insufficient report | Genuine source ID alone does not pass |
| [ ] SK34 | arithmetic-checking | T032 | Exact formula/input/unit/rounding | Independent calculation check | m2+m3 fails; method/output hashes reproduce |
| [ ] SK35 | scope-completeness | T031 | Required scope and priced/proposed coverage | Missing/duplicate/interface report | Mandatory unpriced work is not complete |
| [ ] SK36 | submission-readiness | T054 | Requirements and selected outputs | Readiness with repair targets | Missing signed form blocks readiness |
| [ ] SK37 | approved-company-reuse | T021 | Approved current company assets and task scope | Scoped reuse receipt | Expired/revoked/unpermitted assets excluded |
| [ ] SK38 | lessons-learned | T057 | Evidence and outcome/correction | Evaluated lesson proposal | Speculation never auto-promoted |
| [ ] SK39 | estimated-actual-comparison | T057 | Matched estimate/actual bases | Variance and lesson candidate | -200 quantity +490 rate = +290 |

Curated capability packs under T058 bundle the above skills with their actual checked tools; they are not named employee rosters:

| Pack | Required scope | Task owners |
| --- | --- | --- |
| Document Intelligence | Classification, OCR/layout/table/reprocessing and source provenance | T023–T027 |
| Estimating | Units, assemblies, rates, preliminaries, scenarios and checks | T032–T039 |
| Market Research | Approved search/capture, observations, comparison and refresh | T040–T042 |
| Supplier Procurement | Discovery/verification, local RFQ, existing mail, replies and levelling | T043–T047 |
| Submission | Templates, requirements, consistency, rehearsal and exact manifest | T050–T056 |
| Company Knowledge | Approved assets, terminology, templates, methods and lessons | T016/T021/T052/T057 |
| CAD | Supported local DXF/DWG conversion and 2D inspection | T035 |
| Planning | Calendars/productivity/resources/local schedule interchange | T047–T049 |
| Regional Methods | Sourced measurement editions/terminology/currency conventions; no portal connector | T020/T026/T032/T052 |
| Sustainability | Supplied EPD/factors, explicit boundaries and calculations | T053 |

Installation preview, compatibility, dependency ownership, hashes/licences, migration, actual health check, permission change, disable/rollback/uninstall and output preservation are acceptance for every pack. There is no public marketplace requirement before curated packs work.

## 11. Library, provider and platform qualification register

Each candidate has a concrete delivery owner and adoption gate. Validate exact installed/selected API, licence and transitive/native/model dependencies against representative positive and failure fixtures; record the selected version/hash and adopt/defer/reject reason in that task's evidence. A conditional choice is resolved by the implementation owner within approved architecture, not another broad product interview.

| Candidate / baseline | Owner | Decision and required evidence |
| --- | --- | --- |
| Existing Python/Pydantic AI/direct SDKs | T001/T002/T013 | Keep the pinned baseline; verify direct and original-client typed tools, structured output, cancellation/usage and exact model/account routing. Do not migrate frameworks without a measured requirement. |
| Agent Skills/Pydantic skill loader | T014/T015 | Adopt format-compatible versioned loading; confirm actual installed SDK APIs. Scoped resource access and script execution remain Quantix's responsibility. |
| Deferred tool/approval SDK capabilities | T010/T011/T013 | Integrate only with existing authoritative decisions/effect receipts; no second approval subsystem or lost restart identity. |
| Docling | T024/T026 | Pilot structural/OCR/table extraction on failing document cases; preserve original locators and existing exact spreadsheet reading. |
| PaddleOCR/OCRmyPDF or another supported OCR adapter | T024 | Compare Arabic/English scans, installation/native/model licences, time/memory and exact page coverage. Select the minimum useful supported adapter; preserve original PDF and derivative lineage. |
| PDFium/openpyxl/python-docx | T024/T025/T051 | Keep current responsibilities and serialized PDFium constraints; reading formulas is not recalculation. Verify OOXML/VBA and Word control preservation. |
| Pint or equivalent unit validation | T032 | Reject dimension mismatch, preserve Decimal monetary path and explicit commercial pack semantics; validate selected unit definitions. |
| Polars/pandas candidate | T025/T045/T060/T061 | Choose a primary dataframe method only for measured table/analysis need. Exact source strings/Decimal conversions must survive; approved libraries only in qualified execution. |
| DuckDB | T028/T057 | Optional scoped analytical queries over snapshots; no replacement of accepted SQLite transactions or arbitrary model SQL. |
| NumPy/SciPy | T032/T038/T061 | NumPy already exists. Additional numerical methods require explicit tolerances, reproducible inputs and independently checked sensitivity cases. |
| Shapely | T034 | Adopt only for needed planar intersection/deduction operations, with scale/coordinate validation and geometry fixtures; not BIM. |
| ezdxf | T035 | Positive DXF entity/layer/units/extent fixture, missing xrefs/proxy exceptions and cancellation. DXF support does not claim DWG support. |
| ODA or another licensed local CAD route | T035 | Verify target DWG versions, fidelity, redistribution/commercial rights and local managed state before bundling. |
| Supported legacy DOC converter | T035/T051 | Validate Word structure/images/tables/fields and explicit unsupported features. No silent destructive conversion of originals. |
| OR-Tools | T048 | Optional deterministic optimisation after credible resources/durations; objective/constraints and infeasibility must be inspectable. |
| MPXJ or validated schedule tool | T049 | Separate per-format read/write support, runtime/licence, unsupported fields and roundtrip tests; no live account connector. |
| FastEmbed; optional Qdrant | T026/T027 | Keep local baseline; add external vector service only if measured corpus requirements justify it. Corpus/index fingerprints and correct source filters remain mandatory. |
| Graph database | T028/T030 | Relational dependency edges first. A graph service is optional, not required merely because the app has agents. |
| Tavily/Exa/Brave candidate | T040 | Benchmark one independent search option only where native coverage/control is insufficient; approve destination/billing and preserve returned/captured sources. |
| Firecrawl/controlled browser fallback | T040 | Use only for permitted page extraction that simpler capture cannot satisfy; enforce destination/redirect/size rules and access restrictions. |
| Restricted Code Mode/Monty-style runtime | T060 | Validate language subset/interrupt/restart/budget behavior; no assumed third-party imports or full Python guarantee. |
| OS/VM sandbox provider | T061 | Positive full-library analysis and negative host/network/path/process/resource tests on each advertised supported platform. Venv/same-user subprocess is insufficient. |
| Pydantic Evals/Ragas | T068 | Optional harness/metrics support; independent engineering/deterministic expected answers remain primary. LLM judges cannot certify commercial correctness. |
| OpenTelemetry | T062 | Prefer local/redacted correlation. External trace export requires deliberate destination/data permission; no private prompt/provider-body logging. |
| DBOS/Temporal/LangGraph | T007/T011 | Alternatives, not cumulative dependencies. Implement current ledger checkpoints first; migration needs measured operational/recovery benefit and a reviewed state migration. |
| A2A | T059 | Not required for internal staff. Only evaluate for an actually authorized independently operated agent integration; no automatic external-agent scope. |
| DiceBear/local illustrated portraits | T002/T064 | Preserve currently selected local version/style/seed and licence checks; no remote avatar account or portrait cost requirement. |
| Companion TLS/auth stack | T072 | Browser-trusted TLS, scoped device sessions, revocation/CSRF/rate limits and offline/conflict tests; no public loopback bearer exposure. |
| Transcription capability | T073 | Check actual provider/model/access-method/formats/cost/retention/microphone support before showing ready; no assumed subscription entitlement. |

Official references refreshed during this planning turn: the [Agent Skills specification](https://agentskills.io/specification) describes the folder/metadata/progressive-loading format; Quantix adds its own authority manifest. [Docling's OCR example](https://docling-project.github.io/docling/_generated/examples/full_page_ocr/) demonstrates configurable extraction/OCR options, not guaranteed Tender accuracy. [Microsoft Windows Sandbox configuration](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-configure-using-wsb-file) exposes network/mapped-folder/device controls; defaults must not be treated as the required denied-access profile. These references support candidate qualification, not a claim that the libraries/sandbox were integrated in this documentation task.

For MCP, negotiate the actual installed server/SDK capabilities and check the applicable [official authorization documentation](https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization) during T059. The latest-alias fetch was unavailable during this planning read; no unverified newest protocol revision is asserted. Retain the earlier dated research's sources as historical context, then record actual chosen protocol/API versions in execution evidence.

## 12. Whole-story fixtures and acceptance gates

Synthetic fixture values below are declared test inputs, not market quotations, tax advice, measurement rules or product constants. Generated employee examples exist only in fixtures; production begins with the Manager alone.

| Story | Given / engineer request | Required actual sequence | Expected evidence |
| --- | --- | --- | --- |
| S01 | Review this received package. Transmittal lists Spec, A-101, A-102; received Spec, A-101 and duplicate A-101. | Import → classify → compare transmittal → Manager chooses staff/methods → source review → concise decisions. | 3 occurrences, 2 unique objects, A-102 missing; no automatic AI during import; complete profile and coverage states. |
| S02 | Check and price this BOQ. Supplied 12.5 m3; measured 15 m3; fixture rate layers from T036. | Inspect actual workbook/drawing → reconcile → calculate → independently check → review quantity/rate proposals → draft estimate. | Supplied basis stays effective until explicit decision; rate 167.81 ex-VAT/191.30 including stated fixture VAT; reproducible sources/rounding. |
| S03 | Review this revised drawing package and identify quantity/cost impacts. | Manager creates at least two actual colleagues; first saves revision schedule; second reads full exact schedule/workings; independent reviewer where warranted; source change/scope steering/Stop/reassignment variants. | Actual source receipts per actor, preserved blind initial check/disagreement, affected dependencies, exact handoffs and one publication per outcome; no copied read sets. |
| S04 | Compare supplier A and B quotations. A:10×100+50 delivery; B:10×95+100 delivery. Different testing/payment/lead-time terms. | Preserve/extract replies → verify mapping → technical comparison → commercial normalization → clarification drafts. | Equal 1050 ex-VAT totals, visible unequal terms and unknown payment, source-linked cells and no unapproved sending. |
| S05 | Prepare the submission against supplied requirements. | Map forms/attachments/signatures → generate approved-template drafts → check consistency/rendering → rehearsal → exact release review → local manifest. | Missing signed form blocks; changed output invalidates preview; reviewer cannot release; actual hashes and separate transmission record. |
| S06 | Compare buying together versus stages under our cash/storage limits; show what must arrive first. No saved workflow. | Discover applicable skills/tools → compose new graph → quote/calculation/cash/programme steps → scenario → questions and draft work product. | No workflow registration, source-backed complete supported calculation, explicit missing payment/lead-time/storage inputs and no invented dates. |
| S07 | Respond to a post-tender change and prepare award handover/lessons. | Select immutable submitted baseline → separate clarification/negotiation delta → approved handover → matched actual comparison → lesson proposal. | Released bid unchanged; 100×100 versus 98×105 variance +290 = -200 quantity +490 rate; speculation/lesson approval scoped. |
| S08 | Review an Arabic/English scanned package and fill the client form. | OCR/extraction versions → structural/bilingual retrieval → source check → glossary-aware draft → precise field mapping → visual QA. | Original clauses 4.2.1 / 12.5 m3 / A-101 retained, supported rendering, exact source locators and explicit unreadable/unsupported pages. |

### Schedule fixture with independently calculable expected dates

Start Monday 14 September 2026, Monday–Friday working week, holiday Thursday 17 September. Zero-lag finish-to-start chain: submittal 2 days (14–15 September), approval 3 days (16,18,21 September), manufacture 7 days (22–30 September), delivery 2 days (1–2 October), installation 5 days (5–9 October). Expected finish is 9 October. Retain the stated calendar and source assumptions. A missing predecessor/cycle/resource constraint must be an explicit blocker.

### Failure variants required for the applicable stories

- Source changes before read, after read, after draft, after review and before publication.
- Instruction narrows/replaces scope while a worker is running; completed output retains the original brief.
- Root Stop/reassign/lease expiry during provider cleanup; late usage recorded, late publication denied.
- Crash after durable step, before/after external effect and before/after event commit.
- Missing capability/account/readiness/price/permission; no silent alternate paid route.
- Duplicate submit/tool/message/delivery and conflicting same-key retry.
- Unknown unit/pack/tax/currency/lead-time/contract rule; no zero/default fact substitution.
- Mixed-language identifiers, scanned/unreadable regions, hidden spreadsheet content, stale quote/company certificate.
- Prompt injection inside source, colleague artifact, skill, plugin metadata and MCP/tool result.
- Restore older workspace with newer send/usage history; no renewed authority or duplicate effects.

### Quantitative benchmark targets

These are planning acceptance targets, not measured performance claims. T001/T068 record the reference hardware, OS, corpus, package/model versions and seeded cases. Use warm/cold and local/external timing separately. If a target is missed, record the cause and implement the required repair; do not silently relax the gate.

- Exact identifiers/revision selection: 100% on the locked deterministic identifier set.
- Retrieval recall@10: at least 0.90 overall and 0.85 in each declared Arabic/English/document-type group on at least 100 labelled questions; report precision and current baseline differences too.
- Structured benchmark values/units/source locators used in accepted calculations: exact, within declared method tolerances; no missing field guessed as zero. OCR accuracy is measured per type; low-confidence/incorrect extraction cannot pass through acceptance as a verified quantity.
- Every material accepted claim: supported by inspected applicable evidence or explicitly labelled, separately approved assumption where required; zero unsupported material claims silently accepted.
- Every accepted monetary result: reproducible from exact input/method/rounding; no merged currencies or unspecified tax/commercial basis.
- Zero unauthorized mutation/data exposure/duplicate external action in the adversarial/recovery corpus.
- Local status p95 ≤500ms, Stop acknowledgement ≤1s, warm indexed retrieval p95 ≤2s on the declared 1000-document/50000-chunk reference corpus; external/OCR/provider timings reported separately.
- Office event update visible within two configured poll intervals after a commit; no duplicated history, stale-instance animation or focus theft during a 1000-event burst.
- 10000-row BOQ and 200-staff history: complete bounded paging, usable selection/export and no unbounded per-render/network work.
- Staff evaluation: report personality consistency, source/calculation competence, useful findings, latency, overhead and full attributable cost separately. Compare at least 20 paired unfamiliar tasks with one adequately equipped agent and the dynamic team, matched by source/authority/budget; do not claim universal multiagent superiority.
- S01–S08 each have success plus applicable failure variants; actual native and later tablet/voice checks remain separate required evidence.

## 13. UI/UX and native acceptance checklist

- [ ] UI01 Fresh Tender shows only the Manager, no seeded names/departments, fake conversation or decorative progress.
- [ ] UI02 Manager conversation and useful current artifact share the workspace; absent artifact leaves usable conversation space.
- [ ] UI03 Open/close office preserves draft, Tender, page/zoom/sheet/range/rows and returns keyboard focus.
- [ ] UI04 Generated portraits/names/roles/AI labels are stable, readable and source-driven; fallback does not block work.
- [ ] UI05 Full profile/personality/creation reason/methods/capabilities/history are available in the desk, outside default card clutter.
- [ ] UI06 Active/idle/retired/history views never erase work or imply that a stored profile is consuming model calls.
- [ ] UI07 Highlights default; all exchanges, versions, receipts, notes/reviews/results are completely pageable and filterable.
- [ ] UI08 Handoff/result/source/calculation links open the exact full selected version; historical/stale applicability stays visible.
- [ ] UI09 Actual relationships/tasks have a complete accessible text equivalent; no required information depends only on animation/color.
- [ ] UI10 New events, reconnects and restored histories do not steal focus/scroll or replay false arrivals.
- [ ] UI11 Reduced motion and hidden-window behavior work while backend execution remains independent.
- [ ] UI12 Manager customization/edit conflicts preserve drafts; colleague preference changes preserve historical snapshots.
- [ ] UI13 Busy/status/steering/pending/Stop/retry/Resume states show one clear next action and actual affected work.
- [ ] UI14 No-data, partial-source, missing-capability, disconnected, waiting, failed, interrupted, uncertain and stale states remain distinct.
- [ ] UI15 All numerical work exposes unit/formula/input/source/date/rounding/status; unknown values are not displayed as zero.
- [ ] UI16 Decision inbox groups engineering impact and opens exact repair/source records with preserved return state.
- [ ] UI17 Coverage fractions show their denominators and exceptions independently for imported/extracted/analysed/reviewed.
- [ ] UI18 Keyboard/copy-paste/bulk edits/filters/frozen identifiers work with actual validation and accepted-value safeguards.
- [ ] UI19 Light/dark, long Arabic/English names/text, mixed-direction numerals and identifiers are readable; essential contrast/focus/text labels persist.
- [ ] UI20 Real desktop at 100%,125%,150% OS scale: no clipped composer/controls, essential horizontal overflow or hidden primary actions. Browser-equivalent checks are not substituted for OS evidence.
- [ ] UI21 Native close-to-tray/reopen/Current work/Quit/failed shutdown/no tray/sleep/crash/reset paths retain actual session and visible control.
- [ ] UI22 Internal versus client output selections and exact release/sending authority are explicit; no autosaved secret/approval control.
- [ ] UI23 Later tablet: trusted pairing/revocation, touch controls, HTTPS, stale/offline state and concurrent-edit preservation.
- [ ] UI24 Later voice: visible recording/start/stop/cancel, microphone denial, correct data route/retention and editable unsent transcript.
- [ ] UI25 Capability/settings repair returns to the originating blocked task; installed/configured/authorized/checked statuses are separate.
- [ ] UI26 Native actual-window/provider/live evidence is clearly distinguished from synthetic data and browser previews.

## 14. Plan self-review and final completion record

The primary performs this check directly after writing/editing the plan and again at programme closure:

- [x] Exactly 75 task IDs have files, request/interface details, dependencies, implementation checklist, concrete regression and acceptance conditions.
- [x] R01–R72 are all present once, with R29 deferred and the excluded part of R67 explicitly removed.
- [x] AO01–AO21 and C01–C40 have valid task mappings; C34/C35 remain required later and C40 optional.
- [x] All 39 initial skill subtask rows and 10 curated capability packs have planning owners (this is structural coverage, not implementation or runtime evidence).
- [x] Dependencies are acyclic and no task references a missing task. Shared-file and budget ownership are explicit.
- [x] Proposed interfaces are not described as already implemented; retained contracts and migration expectations are accurate.
- [x] Optional technology choices, excluded integrations and accepted later delivery are not confused with completed features.
- [x] No vague substitute for an acceptance result: named concrete inputs, outputs, invariants and failure behavior exist.
- [x] No required detail is outsourced to another plan; references provide source rationale/current implementation facts only.
- [x] No plan-only action is reported as new application implementation, passed future tests or live provider acceptance.

Planning verification on 10 September 2026: 75 detailed task sections; 24 acyclic ready batches; 72 research rows; 21 accepted office requirements; 40 product/UX coverage rows; 39 skill subtasks; 26 UI/native/device checklist cases. All 77 embedded Python blocks parse, source links resolve and the placeholder scan is clear. This checks the plan's structure and coverage; it is not execution of the future application tests or proof that no implementation-time issue can arise. Task and application acceptance checkboxes remain unchecked.

**Current completion record:** This consolidation defines the detailed programme; it does not close any future implementation gate. Historical first-increment evidence remains recorded in section 2. T071 and T075 stay open until their respective task/acceptance evidence is complete. During execution update the affected task evidence and this record in the same file.

### Full agentic office integration — 13 September 2026

The engineer approved the [full desktop/text agentic implementation](2026-09-12-full-agentic-office.md). Its integration extends the existing task services and closes individual gaps; it does not replace this programme or automatically check off its broader acceptance criteria. Detailed observed results and open gates are in [the acceptance record](../../reports/2026-09-13-full-agentic-office-acceptance.md).

| Affected tasks | Implemented integration and observed evidence | Remaining gate |
| --- | --- | --- |
| T002–T005 | Reusable versioned definitions, exact profile references in staff/bindings, manual library controls; synthetic browser create/edit/history/source return | Full regression fixes and live cross-engine reuse |
| T008–T013 | Reviewed 4/12/2/2 defaults, current grant validation, typed shared tools, actual metered streaming and durable read-only chat observation | Final suite and live concurrent/delegated work |
| T014–T017 | Exact method/package/runtime scopes, real Monty callbacks, work-product identities, explicit versions and bounded safe inspectors | Final pagination/cancellation/revision regression checks |
| T026–T030 | Structural/bilingual retrieval, actual multilingual vectors, canonical public passage citations, source-linked memory and version impact | Staff note visibility, mounted UI refresh and final acceptance |
| T032 | Maintained Pint unit factors with exact rational conversions, explicit Decimal precision, bounded unit expressions and trustworthy invalid-operation checks | Final whole-suite rerun; broader engineering methods retain their own gates |
| T040–T041 | Real public fetching/citation, distinct observed prices and estimates, exact reviewed research worker route | Qualified isolated rendered-page execution and live market acceptance |
| T058–T062 | Versioned plugin lifecycle, reviewed MCP effects/billing/catalog, logical uncertain-operation reconciliation, private runtime setup and shared reservations | WSL2/full-Python/stdio/browser qualification; macOS/Linux platform evidence |
| T063–T067 | Agent/runtime/plugin Settings, native session/artifact inspectors, draft Reviews and Research workspace; actual browser light/dark/narrow/keyboard journeys | Final UI regression, native window/scaling/tray/Quit/reset evidence |
| T068–T071 | 24 explicit synthetic cases, 42 immutable originals, literal tolerances/evidence, actual calculation probes and exact independent-review scoring | 72 live attempts per approved engine/model configuration; final regression and delivery gates |

The first full backend gate completed with 1,013 passes, ten failures and one platform skip. The first full UI gate completed with 314 passes and one then-unfinished revision-refresh regression. These discovery results were retained while fixes were rechecked; the linked acceptance record contains both failed and final observations. No release package, real Tender approval or commercial test send was performed.

**Final local execution update:** the full backend gate passed **1,096 tests with one existing platform skip**, and the final full UI gate passed **330 tests in 75 files**. Frontend typecheck passed and shared bindings are refreshed. Four filesystem-watcher tests passed for the development stability correction. The current implementation includes the missing local-code inspector, exact calculation reads, working-memory paging/promotion, server-produced staff checkpoints and enforced verified-benchmark adoption. Earlier failures remain documented in the acceptance report. These results close this increment's local regression gate; they do not check off live engine, real container, native window/scaling or broader programme acceptance criteria.

**Source references:** [specification](../../spec.md), [current contracts](../../contracts.md), [accepted scope](../../design/adaptive-office-scope.md), [live office](../../design/live-dynamic-office.md), [workspace redesign](../../design/workspace-redesign.md), [coverage review](../../design/office-completeness-review.md), [research](../../reports/2026-09-09-agentic-tender-office-research.md), [office concept](../../reports/2026-09-09-adaptive-ai-office-concept.md), [coverage audit](../../reports/2026-09-10-agentic-office-coverage-audit.md), [existing increment evidence](../../reports/dynamic-office-2026-09-10.md). The old staff foundation and earlier dynamic-staff plan are superseded execution instructions for this programme; required work is fully stated above.
