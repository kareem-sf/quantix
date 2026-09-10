# Codex-style workspace redesign — 2026-09-10

Status: engineer-approved design (all 6 sections approved in chat 2026-09-10).
Supersedes the visual/layout layer of docs/design/workspace-redesign.md (2026-09-09).
Does not change backend authority, Tender data permission, source validation, or spending authority.

## 1. Intent

Replace the sloppy/generic UI with a calm Codex-desktop-grade Tender Office:
light airy canvas, narrow grey sidebar, centered Manager thread, black user
bubbles, compact linked result cards, bottom composer, on-demand right
inspector. Reference supplied by engineer: Codex app screenshot 2026-09-10
(sidebar Projects/chats, centered thread, "Edited 46 files" card, "Do anything"
composer).

Chosen approach: **C — hybrid thread + context rail** (Codex look on current
IA plus on-demand inspector). Options A (skin only) and B (full structure
transplant) were rejected in favour of C because C delivers Codex calm without
burying BOQ/Submission depth or breaking plan-review/queue authority.

## 2. Non-negotiable constraints

- Plain construction-engineering copy. Tender Manager is the main contact.
- One clear next action; details and advanced controls in More options.
  Every capability from Manager, Documents, Work, Estimate, Submission,
  Settings remains reachable. No fake production behaviour.
- English UI; Manager replies in the engineer's language with Arabic bidi/RTL.
- Light default plus dark, semantic tokens, 16px chat text, modest headings,
  quiet teal for actions only, readable controls, visible keyboard focus,
  reduced motion honoured.
- No screenshot backgrounds, fake controls, or permanently expanded
  technical detail.
- Runtime home is `~/.quantix`. No AppData or repository `.quantix-dev`
  runtime storage. Source/dependencies/build artifacts stay in the project.
  OS-protected credentials and user-chosen original/export locations stay
  separate. Never commit customer documents, keys, private extracted content,
  or runtime databases.
- Keep imported, extracted, analysed, and reviewed coverage distinct.
- Read docs/contracts.md before changing any shared interface. Pydantic
  schemas and generated frontend types change together.
- No release packages. Synthetic data for approvals and other acceptance
  mutations; approval of the real Tender and commercial sending remain the
  engineer's decisions.
- Verified backup while idle before any data migration.

## 3. Layout and navigation

- HashRouter shell with one stable left sidebar (~272px, Codex-like pale
  grey): Tender switcher + New Tender/Import at top; Manager, Documents,
  Work, Estimate, Submission; Recents (last opened records); Settings
  separate at bottom; engineer identity at the very bottom. No duplicate
  global tabs.
- Center Tender home thread (max ~880px, one main scroll per view):
  engineer/Manager dialogue only, black user bubbles with white text in light
  theme (white bubble with near-black text in dark theme), compact linked result cards, current work status, one
  pending-instruction card, bottom composer.
- Top bar minimal: Tender title/context, share/export entry, theme control.
- Right inspector (~360px, independent scroll) opens only on selection and
  closes back to the exact thread position. It is on-demand, not a permanent
  rail. Tender selection, import/new tender, source overlay, and
  settings-return paths are preserved.
- Composer: persistent draft per Tender/destination, Send when ready while
  busy, separate Stop. Model/permission state visible (equivalent of Codex
  "Full access" + model picker), plus voice entry where already supported.

## 4. Visual system

- Canvas white `#ffffff` light / `#0e1823` dark; sidebar `#f6f7f8` /
  `#122231`; hairline borders `#e5e8ec` light; cards white 12px radius, 1px
  border, quiet shadow.
- Quiet teal `#117d76` (dark: `#32c2b2`) reserved for actions, focus, links.
  Never large teal fills. Redundant accent alerts are removed.
- Type: Inter, chat 16px/1.6; sidebar and cards 14px; metadata 13px;
  headings 20/16/14 semibold with tight tracking.
- Spacing scale 4/8/12/16/24/32; thread gutters 24px; composer touch target
  44px; 2px visible focus ring; AA normal-text contrast in both themes.
- 100/125/150% scaling without horizontal scroll; laptop and desktop widths;
  keyboard operable throughout; Arabic bidi correct in bubbles and cards.

## 5. Components and capability mapping

- Thread contains only dialogue, compact linked result cards (grouped
  repeats, every original record link retained), current work, one pending
  message. System/import logs live in Activity. Old long replies collapse
  with disclosure, never deleted.
- Pending instruction: one per Tender, edit/cancel/confirm, Send when ready
  while busy, atomic consume, no loss of later unsent edits.
- Inspector variants: source viewer (total pages, bounded page jump,
  zoom/fit, sheet/range selection; measurement starts at the exact open
  page/version and returns there); BOQ/rate row; proposal/measurement;
  supplier quote plus operational check reply beside it; requirement with
  exact fix link; plan task with scope/source detail. Each shows one clear
  next action; field errors sit at the relevant input.
- Plan review remains accessible even when blocked: scope/tasks first, one
  AI summary, meaningful changes on top, typed blockers with repair targets,
  current AI/destination/spending, optional decision note that survives
  refresh/conflict, sticky Approve & start tasks bound to the viewed
  immutable fingerprint. No implicit permission; review GET never mutates
  grants.
- Documents: searchable/filterable register with revisions, truthful
  imported/extracted/analysed/human-reviewed distinction.
- Estimate: BOQ/rates, proposals/measurements, supplier quotes/replies;
  mail account config stays in Settings with return to origin.
- Submission: requirements, all seven supported document/programme kinds,
  package/export history; blocking records link to fix and return to fresh
  package review.
- Settings: AI accounts, working preferences/approved knowledge, mail
  configuration, backups, diagnostics. Working forms retain nonsecret
  drafts; passwords/keys and engineering/commercial consent checkboxes are
  never autosaved.

## 6. Data flow, routing, and authority

- No backend IA change in this design. Same FastAPI/SQLite workspace,
  same Tender-scoped facts and work.
- Thread route is the Tender home (`#/t/:id/manager` pattern); inspector
  selection is a query selection (`?sel=<record>`), so refresh and
  deep-link preserve source and return position.
- Freeform messages use the bounded first AI conversation/routing pass.
  Greetings/status cannot read document tools or publish plans/findings.
  Explicit Review documents goes directly to engineering. Engineering work
  stays source-grounded and limited to the requested purpose.
- Queue: pending instruction queues behind all currently scheduled work;
  auto dispatch only after successful completion using latest saved results
  and freshly validated permissions/model/spending. Failure, Stop, restart,
  or restore holds pending for explicit confirmation.
- Approve & start tasks: one transaction revalidating source, account/model,
  spending, work-idle, and reviewed snapshot/fingerprint; renews only the
  displayed Tender/team grants; conflict never partly grants or starts jobs.
- Shared integration adds a workspace compatibility revision bump (current 2 → 3),
  coordinated with the backend; new workspace actions block on incompatible
  backend revision. Secure validation field
  paths preserved without exposing raw input/secrets.

## 7. Language, errors, and empty states

- Manager default is concise with key limitation and one next action;
  explicitly requested depth is preserved.
- Import does not automatically start AI work.
- Errors are actionable at the field with a repair button; diagnostic
  references are secondary.
- Empty states: no Tender leads to Import/New; no AI account leads to
  Finish AI setup; no plan leads to Review documents; blocked approval
  links to the exact repair and returns to fresh review.

## 8. Acceptance

- Affected backend and UI tests, frontend typecheck, and real-app visual
  journeys: light/dark, laptop/desktop, 100/125/150% equivalents, keyboard,
  Arabic bidi, zero JS page errors on captured journeys.
- Synthetic data beneath an isolated tmp root for approval, queue/restart
  races, and mail/export guards. Never approve the real current plan or
  send real commercial messages.
- Confirm: centered thread with black bubbles, grouped result cards with
  retained links, composer with draft retention across destinations/Tenders,
  inspector open/return fidelity (e.g. PDF page 12 → measurement → same
  page; workbook sheet/range filtering), blocked plan access with note
  retention, seven output kinds reachable, BOQ/Settings reachability.
- Out of scope: OCR/DWG interpretation engines, new BOQ engines, tablet
  delivery, cross-device sync, release installer signing.
