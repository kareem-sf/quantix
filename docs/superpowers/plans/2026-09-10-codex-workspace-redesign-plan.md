# Codex-style Workspace Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the sloppy/generic UI with a calm Codex-desktop-grade Tender Office: light airy canvas, narrow grey sidebar, centered Manager thread, black user bubbles, compact linked result cards, bottom composer, on-demand right inspector. All 6 design sections from docs/superpowers/specs/2026-09-10-codex-workspace-design.md are engineer-approved.

**Architecture:** Hybrid thread + context rail (Option C from the spec). The redesign preserves the current FastAPI/SQLite IA, same Tender-scoped facts and work. No backend interface changes. Frontend-only UI remodel with visual system updates, layout refinements, and component re-mapping. Arabic bidi support throughout. One clear next action principle preserved. Details and advanced controls in "More options".

**Tech Stack:** React + React Router + TanStack React Query + FastAPI/SQLite backend (unchanged). Vite/TS frontend. Inter font. No release packages. Synthetic data for acceptance.

**Spec:** docs/superpowers/specs/2026-09-10-codex-workspace-design.md

## Global Constraints (from spec)

- Plain construction-engineering copy. Tender Manager is the main contact.
- One clear next action; details and advanced controls in More options. Every capability from Manager, Documents, Work, Estimate, Submission, Settings remains reachable. No fake production behaviour.
- English UI; Manager replies in the engineer's language with Arabic bidi/RTL.
- Light default plus dark, semantic tokens, 16px chat text, modest headings, quiet teal for actions only, readable controls, visible keyboard focus, reduced motion honoured.
- No screenshot backgrounds, fake controls, or permanently expanded technical detail.
- Runtime home is `~/.quantix`. No AppData or repository `.quantix-dev` runtime storage. Source/dependencies/build artifacts stay in the project. OS-protected credentials and user-chosen original/export locations stay separate. Never commit customer documents, keys, private extracted content, or runtime databases.
- Keep imported, extracted, analysed, and reviewed coverage distinct.
- No release packages. Synthetic data for approvals and other acceptance mutations; approval of the real Tender and commercial sending remain the engineer's decisions.
- Verified backup while idle before any data migration.

## Top-Level Checklist (ordered by priority, lowest-risk first)

- [ ] **1. Layout & Navigation** — HashRouter shell with stable left sidebar (~272px pale grey), center thread (~880px max), on-demand right inspector (~360px), top bar minimal, bottom composer
- [ ] **2. Visual System** — Color tokens, typography, spacing, bidi support, light/dark themes
- [ ] **3. Components & Capability Mapping** — Manager thread, pending instruction, inspector variants, Documents, Estimate, Submission, Settings
- [ ] **4. Data Flow & Routing** — HashRouter stability, inspector deep-link via `?sel=<record>`, pending instruction queue, Approve & start tasks transaction
- [ ] **5. Language, Errors & Empty States** — Concise Manager copy, field errors with repair buttons, empty states
- [ ] **6. Acceptance** — UI tests, frontend typecheck, real-app visual journeys (light/dark, laptop/desktop, keyboard, Arabic bidi, zero JS errors)

---

## Task 1: Layout & Navigation — HashRouter shell restructure

**Files:**
- Modify: `src/App.tsx` (sidebar structure, top bar, thread area, inspector placement)
- Modify: `src/navigation/routes.ts` (verify route patterns support new layout)
- Modify: `src/features/Manager.tsx` (thread area restructuring)

**Interfaces:**
- Consumes: Current `tenderRoute`, `sourceRoute`, `recordRoute` from `navigation/routes.ts`
- Produces: Updated route context with `section`, `tenderId`, optional `view` and `recordId`

**Priority:** 1 (foundation — all other tasks depend on correct layout structure)
**Effort:** Medium
**Backend-dependent:** No — purely frontend routing/shell changes. Backend unchanged.

**Steps:**
- [ ] **Step 1:** Update `src/App.tsx` sidebar to ~272px width with pale grey (`#f6f7f8`) background, tender switcher at top, New Tender/Import, then Manager/Documents/Work/Estimate/Submission nav, Recents at bottom, engineer identity at very bottom. No duplicate global tabs.
- [ ] **Step 2:** Update top bar to: Tender title/context, share/export entry, theme control (light/dark). Minimal — no permanent technical detail expansion.
- [ ] **Step 3:** Center the Tender home thread: max ~880px, one main scroll per view. Engineer/Manager dialogue only. Black user bubbles with white text in light theme (white bubble with near-black text in dark theme). Compact linked result cards. Current work status. One pending-instruction card. Bottom composer.
- [ ] **Step 4:** Implement right inspector (~360px, independent scroll) that opens only on selection and closes back to the exact thread position. On-demand, not permanent rail. Preserve: Tender selection, import/new tender, source overlay, settings-return paths.
- [ ] **Step 5:** Verify `HashRouter` shell stability — all routes (`/tenders/:tenderId/*`, `/settings`) work correctly with new layout. Run typecheck.
- [ ] **Step 6:** Commit: "feat: restructure layout and navigation for Codex workspace"

**Acceptance consideration:** Sidebar width stable at ~272px. Thread centers at max ~880px. Inspector opens on selection and returns to exact position. No duplicate global tabs. All routes functional.

---
## Task 2: Visual System — Color tokens, typography, spacing, bidi support

**Files:**
- Modify: `src/styles/workspace.css` (major redesign)
- Modify: `src/styles/reset.css` (bidi, focus rings, reduced motion)
- Modify: `src/theme.tsx` (token updates if theme system exists)

**Interfaces:**
- Consumes: Current CSS token definitions
- Produces: Updated visual system per spec: canvas `#ffffff` light / `#0e1823` dark, sidebar `#f6f7f8` / `#122231`, hairline borders `#e5e8ec` light, cards white 12px radius 1px border quiet shadow, quiet teal `#117d76` (dark: `#32c2b2`) for actions/focus/links only

**Priority:** 2 (visual foundation — tasks 3-5 can reference these tokens)
**Effort:** Medium
**Backend-dependent:** No — purely styling changes.

**Steps:**
- [ ] **Step 1:** Update color tokens per spec:
  - Canvas: `#ffffff` light, `#0e1823` dark
  - Sidebar: `#f6f7f8` light, `#122231` dark
  - Hairline borders: `#e5e8ec` light (quiet dividers, not heavy)
  - Cards: white, 12px radius, 1px border, quiet shadow
  - Quiet teal: `#117d76` light, `#32c2b2` dark — reserved for actions, focus, links only. Never large teal fills.
- [ ] **Step 2:** Update typography:
  - Typeface: Inter
  - Chat text: 16px/1.6 line-height
  - Sidebar and cards: 14px
  - Metadata: 13px
  - Headings: 20/16/14 semibold with tight tracking
- [ ] **Step 3:** Update spacing scale: 4/8/12/16/24/32. Thread gutters 24px. Composer touch target 44px. 2px visible focus ring. AA normal-text contrast in both themes.
- [ ] **Step 4:** Arabic bidi support throughout: `dir="auto"` on message bubbles and cards, ensure text doesn't break layout in RTL. Test with Arabic content.
- [ ] **Step 5:** Honor reduced motion — no unnecessary animations for reducing-motion users.
- [ ] **Step 6:** Verify light/dark theme switch works correctly throughout. Test at 100/125/150% scaling without horizontal scroll.
- [ ] **Step 7:** Commit: "feat: update visual system tokens, typography, spacing, bidi support"

**Acceptance consideration:** Canvas colors correct. Teal only used for actions/focus/links, never large fills. Inter font sizes correct. Arabic bidi test passes. Reduced motion honored. 100/125/150% scaling works without horizontal scroll.

---
## Task 3: Components & Capability Mapping — Manager thread, pending instruction, inspector variants, Documents, Estimate, Submission, Settings

**Files:**
- Modify: `src/features/Manager.tsx` (thread: black user bubbles, compact linked result cards, pending instruction, pending message)
- Modify: `src/features/ManagerMessage.tsx` (message bubbles: dir="auto" for bidi, black user bubbles with white text light/white bubble near-black text dark)
- Modify: `src/features/PendingMessage.tsx` (pending instruction: one per Tender, edit/cancel/confirm, Send when ready while busy, atomic consume)
- Modify: `src/features/office/TenderOfficeWorkspace.tsx` (inspector variants, source viewer, BOQ/rate, proposal/measurement, supplier quote, requirement with fix link, plan task)
- Modify: `src/features/Documents.tsx` (searchable/filterable register with revisions, truthful imported/extracted/analysed/human-reviewed distinction)
- Modify: `src/features/Estimate.tsx` (BOQ/rates, proposals/measurements, supplier quotes/replies, mail account config in Settings with return to origin)
- Modify: `src/features/Submission.tsx` (requirements, seven supported document/programme kinds, package/export history, blocking records link to fix)
- Modify: `src/features/Settings.tsx` (AI accounts, working preferences/approved knowledge, mail configuration, backups, diagnostics. Working forms retain nonsecret drafts; passwords/keys never autosaved)

**Interfaces:**
- Consumes: Updated visual tokens from Task 2 (colors, typography, spacing)
- Produces: All Codex-specified component variants with correct copy, layout, and behavior

**Priority:** 3 (components use visual tokens from Task 2; some tasks parallelizable)
**Effort:** Large (many components)
**Backend-dependent:** No — all frontend component remapping. Backend APIs unchanged.

**Steps:**
- [ ] **Step 1:** Update Manager thread: black user bubbles with white text in light theme, white bubble with near-black text in dark theme. Compact linked result cards (grouped repeats, every original record link retained). Current work status. One pending message.
- [ ] **Step 2:** Update ManagerMessage bubbles: `dir="auto"` for Arabic bidi. Black user bubbles white text light, white bubble near-black text dark. Compact preview with show full/reveal. Result links grouped by kind (plan, finding, task, output). Citations with show individual detail.
- [ ] **Step 3:** Update PendingMessage: one per Tender, edit/cancel/confirm. Send when ready while busy. Atomic consume, no loss of later unsent edits. Visible model/permission state (equivalent of Codex "Full access" + model picker).
- [ ] **Step 4:** Update TenderOfficeWorkspace inspector variants:
  - Source viewer: total pages, bounded page jump, zoom/fit, sheet/range selection. Measurement starts at exact open page/version and returns there.
  - BOQ/rate row: display BOQ rates.
  - Proposal/measurement: display proposals and measurements.
  - Supplier quote plus operational check reply beside it.
  - Requirement with exact fix link.
  - Plan task with scope/source detail.
  - Each shows one clear next action; field errors sit at the relevant input.
- [ ] **Step 5:** Update Documents component: searchable/filterable register with revisions. Truthful imported/extracted/analysed/human-reviewed distinction. No auto-start AI work on import.
- [ ] **Step 6:** Update Estimate component: BOQ/rates, proposals/measurements, supplier quotes/replies. Mail account config stays in Settings with return to origin.
- [ ] **Step 7:** Update Submission component: requirements, all seven supported document/programme kinds, package/export history. Blocking records link to fix and return to fresh package review.
- [ ] **Step 8:** Update Settings component: AI accounts, working preferences/approved knowledge, mail configuration, backups, diagnostics. Working forms retain nonsecret drafts. Passwords/keys and engineering/commercial consent checkboxes never autosaved.
- [ ] **Step 9:** Commit: "feat: remap all Codex components — Manager, pending, inspector, Documents, Estimate, Submission, Settings"

**Acceptance consideration:** Centered thread with black bubbles. Grouped result cards with retained links. Composer with draft retention across destinations/Tenders. Inspector open/return fidelity (e.g. PDF page 12 → measurement → same page). Blocked plan access with note retention. Seven output kinds reachable. BOQ/Settings reachable.

---
## Task 4: Data Flow, Routing, and Authority — HashRouter stability, inspector deep-link, pending queue, Approve & start

**Files:**
- Modify: `src/navigation/routes.ts` (verify `?sel=<record>` query pattern for inspector deep-link/refresh)
- Modify: `src/App.tsx` (verify HashRouter stability with new layout)
- Modify: `src/features/Manager.tsx` (pending instruction queue behind scheduled work)
- Modify: `src/features/office/TenderOfficeWorkspace.tsx` (inspector selection query, return fidelity)
- Modify: `src/api.ts` (verify no backend interface changes needed)

**Interfaces:**
- Consumes: Current route context types (`RouteContext`, `WorkspaceSection`, `SourceRouteOptions`)
- Produces: Updated route patterns with `?sel=<record>` query for inspector selection; pending instruction queue logic; Approve & start tasks transaction guard

**Priority:** 4 (routing foundation — enables inspector deep-link and queue behavior)
**Effort:** Medium
**Backend-dependent:** No backend IA changes (same FastAPI/SQLite). Frontend routing only.

**Steps:**
- [ ] **Step 1:** Verify/hashRouter shell stability — all routes work correctly with new layout. Ensure `tenderRoute`, `sourceRoute`, `recordRoute` patterns are preserved. Run typecheck.
- [ ] **Step 2:** Implement inspector selection via query: `?sel=<record>` pattern. This enables deep-link and refresh preservation of source and return position. Update `parseRouteContext` and `sourceRoute` if needed to support `sel` query parameter.
- [ ] **Step 3:** Implement pending instruction queue: pending instruction queues behind all currently scheduled work. Auto dispatch only after successful completion using latest saved results and freshly validated permissions/model/spending. Failure, Stop, restart, or restore holds pending for explicit confirmation.
- [ ] **Step 4:** Implement Approve & start tasks: one transaction revalidating source, account/model, spending, work-idle, and reviewed snapshot/fingerprint. Renews only the displayed Tender/team grants. Conflict never partly grants or starts jobs.
- [ ] **Step 5:** Verify workspace compatibility revision bump (current 2 → 3) coordinated with backend. New workspace actions block on incompatible backend revision. Secure validation field paths preserved without exposing raw input/secrets.
- [ ] **Step 6:** Commit: "feat: data flow, routing, and authority updates for Codex workspace"

**Acceptance consideration:** Inspector selection `?sel=<record>` preserves position on refresh/deep-link. Pending instruction queues behind scheduled work. Approve & start is one full transaction revalidating all guards. No implicit permission; review GET never mutates grants. Workspace revision bump 2→3.

---
## Task 5: Language, Errors, and Empty States — Concise Manager copy, field errors, empty states

**Files:**
- Modify: `src/features/Manager.tsx` (Manager default concise with key limitation and one next action)
- Modify: `src/features/ManagerMessage.tsx` (error handling at field with repair button)
- Modify: `src/features/PendingMessage.tsx` (empty/ready states)
- Modify: `src/features/office/TenderOfficeWorkspace.tsx` (empty states, compatibility message)
- Modify: `src/features/Estimate.tsx` (empty state for estimate)
- Modify: `src/features/Submission.tsx` (empty state for submission)
- Modify: `src/features/Settings.tsx` (empty states for AI accounts, no plan, etc.)

**Interfaces:**
- Consumes: Updated visual tokens from Task 2, component structures from Task 3
- Produces: Actionable field errors with repair button; diagnostic references secondary; concise Manager copy with one next action; truthful empty states

**Priority:** 5 (language and states — depends on Tasks 2-3 component structure)
**Effort:** Medium
**Backend-dependent:** No — frontend copy and state changes only.

**Steps:**
- [ ] **Step 1:** Update Manager default copy: concise with key limitation and one next action. Explicitly requested depth preserved. Manager replies in engineer's language with Arabic bidi/RTL support.
- [ ] **Step 2:** Import does not automatically start AI work. (Verify this behavior in Manager/Composer.)
- [ ] **Step 3:** Errors are actionable at the field with a repair button. Diagnostic references are secondary (visible in "More options" or inline help).
- [ ] **Step 4:** Empty states:
  - No Tender leads to Import/New tender CTA.
  - No AI account leads to "Finish AI setup" CTA.
  - No plan leads to "Review documents" CTA.
  - Blocked approval links to the exact repair and returns to fresh review.
- [ ] **Step 5:** Update Estimates and Submission empty states per spec.
- [ ] **Step 6:** Commit: "feat: language, errors, and empty states updates for Codex workspace"

**Acceptance consideration:** Manager copy concise with one next action. Import does not auto-start AI. Field errors have repair button. Empty states: no Tender → Import/New; no AI account → Finish AI setup; no plan → Review documents; blocked approval → exact repair link.

---
## Task 6: Acceptance — UI tests, frontend typecheck, real-app visual journeys

**Files:**
- Modify/create: `src/features/*/*.test.tsx` (updates/additions for redesigned components)
- Modify/create: `vitest.config.ts` or `package.json` test scripts
- Modify: `tsconfig.json` (if typecheck changes needed)
- Modify: `vite.config.ts` (if test configuration changes needed)

**Interfaces:**
- Consumes: All redesigned components from Tasks 1-5
- Produces: Passing test suite with visual regressions caught; frontend typecheck passes; real-app visual journeys verified light/dark, laptop/desktop, keyboard, Arabic bidi, zero JS page errors on captured journeys

**Priority:** 6 (final verification — depends on all prior tasks completing)
**Effort:** Large (test suite, typecheck, visual journey verification)
**Backend-dependent:** No — synthetic data beneath isolated tmp root for acceptance testing only.

**Steps:**
- [ ] **Step 1:** Add/backend UI tests for redesigned components:
  - Layout tests: sidebar width, thread centering, inspector open/close
  - Visual tests: light/dark color tokens, typography, spacing
  - Component tests: Manager message bubbles, pending instruction, inspector variants
  - Empty state tests: no Tender → Import, no AI account → setup, etc.
- [ ] **Step 2:** Update frontend typecheck — run `npm run typecheck` or `tsc --noEmit`. Fix any type errors introduced by component renames or token changes.
- [ ] **Step 3:** Real-app visual journeys:
  - Light theme, dark theme
  - Laptop width, desktop width
  - 100/125/150% scaling (no horizontal scroll)
  - Keyboard operable throughout (tab order, focus rings)
  - Arabic bidi (dir="auto" on bubbles/cards, RTL layout test)
  - Reduced motion honored
  - Zero JS page errors on captured journeys
- [ ] **Step 4:** Synthetic data beneath isolated tmp root for acceptance testing:
  - Approval, queue/restart races, mail/export guards
  - Never approve the real current plan or send real commercial messages
- [ ] **Step 5:** Confirm specific acceptance criteria from spec section 8:
  - Centered thread with black bubbles ✓
  - Grouped result cards with retained links ✓
  - Composer with draft retention across destinations/Tenders ✓
  - Inspector open/return fidelity (e.g. PDF page 12 → measurement → same page) ✓
  - Blocked plan access with note retention ✓
  - Seven output kinds reachable ✓
  - BOQ/Settings reachable ✓
- [ ] **Step 6:** Commit: "feat: acceptance tests, typecheck, and visual journey verification for Codex workspace"

**Acceptance consideration:** All affected backend and UI tests pass. Frontend typecheck passes. Real-app visual journeys: light/dark, laptop/desktop, keyboard, Arabic bidi, zero JS page errors. Synthetic data under isolated tmp root. No release packages.

---