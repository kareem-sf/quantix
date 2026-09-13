# Approved workspace redesign — 2026-09-09

This is the implementation contract for the user's approved redesign. It extends docs/spec.md and supersedes earlier UI organization and verification restrictions. Initial implementation was delegated to Luna/xhigh; following the user's explicit continuation override, remaining implementation, integration and review used GPT-6 Astra/xhigh. Preserve existing dirty source and all runtime/customer data.

## Product and visual system

Chat-first Tender home, English UI, Manager replies in the user's language with Arabic bidi. Light default plus dark, consistent semantic tokens, 16px chat text, modest headings, quiet teal, readable controls, visible keyboard focus, reduced motion. Use approved Manager and plan-review references in this directory. No screenshot backgrounds, fake controls or permanently expanded technical details. One stable sidebar with Tender switcher and Manager, Documents, Work, Estimate, Submission; separate Settings. Remove duplicate global tabs and permanent chat context rail. One main scroll region per view. React Router HashRouter, addressable records and preserved source/return context.

Manager contains only engineer/manager dialogue, compact linked result cards, current work and one pending message. System/import logs belong in Activity. Existing long messages collapse without deletion. Plan review remains accessible when blocked. Documents is a searchable/filterable register with revisions and source viewer. Work groups plan/tasks, decisions/findings, scope/project map and Activity. Estimate groups BOQ/rates, proposals/measurements and supplier quotes/replies. Submission groups requirements, all supported documents/programmes and package/export history. Settings groups AI accounts, working preferences/approved knowledge, mail configuration, backups and diagnostics. All capabilities remain reachable.

## Task 1: Conversation, pending instruction and backend integration

Own backend/quantix/{api.py,models.py,repository.py,jobs.py,office.py,office_types.py}, new conversation/pending modules and targeted tests. Coordinate approval service integration with Task 2; do not edit Task 2 files. Read docs/contracts.md first.

Freeform messages use a bounded first AI conversation/routing pass through the same approved route. Greetings/status in Arabic and English must not read document tools or publish plans/findings. The conversational branch structurally cannot publish engineering records. Explicit Review documents action goes directly to engineering. Engineering work remains source grounded and limited to requested purpose. Default response concise with key limitation and one next action; preserve explicitly requested depth. Remove automatic AI start after imports.

Persist one pending instruction per Tender with edit/cancel and an idempotency identity. POST message returns discriminated immediate-run vs pending outcome. Queue behind all currently scheduled work; auto dispatch only after successful completion of that work, using latest saved results and freshly validated permissions/model/spending. Failure, Stop, restart or restore holds pending for explicit confirmation; no surprise auto work. Atomic consume creates engineer message and run once. Do not lose later unsent edits. Preserve cancelled/failed terminal publication guard. Support paginated dialogue history and server-derived artifact links only where persisted relationships prove identity. Separate logs remain queryable.

Own shared integration in api.py/models.py and generated bindings coordination: Task 2 returns its separate typed router/service. Add a workspace compatibility revision, used by Task 3 to block incompatible new UI actions. Preserve secure validation field paths without exposing raw input/secrets.

## Task 2: Coherent plan review and authority

Own backend/quantix/ai_policy.py, ai_connections.py, ai_setup*.py only where authority/readiness changes require it, new plan_review.py / plan_review_models.py / plan_review_routes.py and targeted tests. Do not edit api.py/models.py/repository.py/jobs.py; request integration hooks from Task 1.

Read-only plan review returns exact fingerprint, reviewed scope/tasks/routes and one AI summary, meaningful changes, typed blockers with repair targets, can_approve. Must preserve original specialist routes, effort/output/search settings when refreshing. Current legacy plan revision4/account6 gets explicit coherent review, never implicit permission. Readiness/catalog checks must be separated from authority-changing configuration (credentials/provider/endpoint/billing/route intent). Fix null-to-checked capabilities falsely revoking unchanged authority without grandfathering legacy stale grants.

Approve & start tasks accepts reviewed fingerprint and optional rationale. In one transaction revalidate source, account/model, spending, work-idle and reviewed snapshot; renew only displayed Tender/team grants, approve plan, create work intents once. Conflict must not partly grant authority or start jobs. Coordinate scheduling-after-commit with Task 1. Preserve detailed route differences and engineer safeguards. Old same-name/model coincidence is never consent. Review GET must not mutate grants. Typed blockers include sign-in, model check, spending, sources, running work.

## Task 3: Shell, routing and visual foundations

Own src/App.tsx, src/main.tsx, src/styles.css, src/components/ui.tsx, new navigation/theme/design-system modules, package.json/lock and shell tests. Do not edit feature components or their CSS yet. Install maintained documented React Router HashRouter. Read approved visual images. Build complete real sidebar/navigation/theme/layout and stable record/return routes. Define route helpers usable by later feature owners. App should map Documents to existing Files initially, Work/Estimate/Submission to coherent route sections; request required feature exports rather than silently omitting capabilities. Preserve tender selection, import/new tender, source overlay and settings-return paths. Block new workspace actions on incompatible backend revision (coordinate exact value with Task 1). Publish interface notes for downstream features.

## Task 4: Manager and plan review frontend

After backend contracts: implement clean dialogue, canonical artifact cards, short import summary plus explicit Review documents/Finish AI setup, old long response disclosure, pagination, Arabic direction, accepted-draft clearing without losing newer edits. One pending instruction card edit/cancel/confirm, Send when ready while busy; separate Stop. Full plan review: scope/tasks first, one AI summary, visible relevant changes, optional decision note, sticky Approve & start tasks. Review accessible even blocked; specific repair links. Bind to viewed immutable fingerprint; preserve note on conflicts, refresh review explicitly. Move full team/policy/findings out of chat.

## Task 5: Documents, Work, context and drafts

Documents table/search/filters/version inspection; truthful imported/extracted/analysed/human-reviewed distinction. Source viewer total pages, bounded page jump, zoom/fit, sheet/range selection; measurement starts at exact open page/version and returns there. Work has focused plan/tasks, decisions/findings, scope/map and Activity. Nonsecret long form drafts scoped Tender/form/version survive routes. Never autosave API keys/passwords. No OCR/DWG/new BOQ engines in this scope; actionable current limits.

## Task 6: Estimate, Submission and Settings journeys

Focused BOQ/rates/proposals/measurement/quotes views. Operational Check replies beside quotes, mail account config stays in Settings with return to origin. Submission requirements -> documents/programmes -> package, all seven output capabilities preserved. Blocking records link to fix and return to fresh package review. Explicit engineering decisions remain for quantities/rates/mail/final export. Consistent grouped forms/tables/errors, long draft persistence, secrets excluded. Settings organized and return context preserved.

## Task 7: Integration and acceptance

Pydantic and generated TS updated together. Preserve original files/versions/history/plan/decisions/billing. Runtime under ~/.quantix. Verified backup before any data migration, activate changes only while idle. Affected backend and UI tests + typecheck, real-app journeys, visual comparison approved refs, light/dark laptop/desktop, 100/125/150% scale, keyboard and Arabic. No release build. Synthetic data for mutating approvals, queue/restart races and mail/export guards; never approve real current plan or send real commercial messages. Report actual verification limits.
