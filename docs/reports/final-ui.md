# Final interface integration

This increment connects the existing quotation, rate-proposal and reusable-note interfaces to the current Tender Office. The implementation retains the accepted navy rail, white working surface, teal controls, source drawers and explicit engineer decisions.

## Changes

- Settings now exposes Reusable notes when the service advertises `knowledge`. The note form is separate from connection settings and requires a title, reviewed text, approval rationale and explicit confirmation. Notes retain original-Tender source references; opening a reference reads that Tender's full revision register. Withdrawal preserves approved text, provenance and decision history. Price and tax notes remain subject to fresh commercial verification.
- Backups reads typed `GET /backups/latest` as well as the existing pending-restoration endpoint. A completed restoration remains visible after a renderer reload, with its completion time and the service's actual outcome. A damaged previous workspace is identified as recovery evidence, with its path in the local data folder; it is never described as a complete backup. Failed history reads stay visible and offer an explicit retry.
- Current Work quotation and Estimate rate-proposal wiring was inspected. The existing Words, Meaning and Both search modes, full revision register, inspected-archive binding, and Tender-specific draft storage were retained.
- Reusable-note form fields use the existing 16px control size. Long note titles wrap, and restoration results use the established open-section styling.

## Tests

The Settings integration regression first failed because the Reusable notes entry point was absent. The two restoration-history regressions first failed because completion details and failed-history retry controls were absent. After implementation, all three targeted files passed **12 tests**. The integrated UI suite then passed **61 tests across 16 files** with `npm run test:ui`.

Existing note tests verify explicit approval, failed-save preservation, original-Tender source scope, source inspection without form submission, source clearing when the supporting Tender changes, category filtering, price revalidation, withdrawal refusal and immutable approved content. Existing supplier, rate, search and composer tests retain their decision and recovery coverage.

Before the user's code-only override, the added output/source/submission/measurement-boundary files passed eight targeted tests and TypeScript. No verification was started after that override. Project map work is code-only and remains untested at the user's request.

## Browser verification

The flow under test is: Manager loads, an unsent instruction survives reload, Files searches and opens original sources, Estimate shows the real candidate rows and review controls, Work opens a supplier draft form, and Settings exposes note approval, mail settings and guarded restoration controls.

The previous CUA tab setup/reacquisition failed with `Debugger unattached`. The primary agent explicitly authorized the Playwright fallback. The guarded script uses installed Playwright with the Chrome channel at `http://127.0.0.1:1420/`, an isolated browser profile, 1505 × 1045 desktop and 390 × 844 narrow viewports. All API methods except GET and OPTIONS are blocked before navigation. No API tokens or saved credentials are read by the QA script.

The first attempt stopped at its capability precondition because the running service predated the integrated knowledge API. A later guarded run reached the reusable-note form after checking the live 133-row BOQ candidate register, Words/Meaning/Both, historical register fetch, source opening, unsent-draft reload and quotation attachments. It stopped at an overly strict Category locator; that script locator was corrected, but the run was not repeated after the user requested no further browser checks. These partial screenshots are evidence of that earlier state, not final verification of the later code.

Private screenshots, the local reference Tender identifier and the read-only verification script/results remain in ignored `.quantix-dev/qa/`. They must not be committed or published.

## Ownership and limits

Owned edits in this finishing increment: `src/features/Settings.tsx`, `Settings.test.tsx`, `Backups.tsx`, `Backups.test.tsx`, `src/styles/knowledge.css`, `src/styles/features.css`, and this report. Preexisting `Knowledge.tsx`, `Knowledge.test.tsx`, `Work.tsx`, `Estimate.tsx`, quotation, mail, rate, search and draft modules were inspected. No generated bindings, backend, measurements modules, dependency manifests, root configuration, native launcher or release packages were changed by this worker.

Live supplier sending, mailbox reading, provider calls, pricing/source/quantity decisions, restoration, note creation and withdrawal are excluded from real-workspace browser QA. Synthetic frontend behavior tests cover the UI decision paths; the primary agent owns backend integration, native verification and the full milestone gate.

## Further implementation and code-only handoff

The user subsequently requested completion without tests, browser checks, typechecking, lint or live probes. No new test files or verification commands were added for the Project map work.

- `Outputs.tsx`, `OutputForm.tsx`, `ProgrammeForm.tsx`, `Submissions.tsx` and `styles/deliverables.css` now expose all six documented draft kinds and reviewed local exports. Technical documents require a selected completed specialist task. Programme forms start with no activities, durations, dates or working days; every input is supplied explicitly. Export approval binds the server preview fingerprint, selected output IDs, engineer scope, every stated gap and final-review consent. Refusals require a refreshed preview and renewed consent.
- `Sources.tsx` now exposes drawing measurement, hash-verified original download and exact historical-artifact loading. Artifact selections can carry an exact PDF page for reviewed-page navigation.
- `EstimateEditor.tsx` permits explicit source confirmation where a current approved measurement supplies a quantity that remains unresolved in the original BOQ. The original supplied quantity and warning remain unchanged.
- `ProjectMap.tsx`, `styles/project-map.css` and the Project map tab in `App.tsx` use the root-generated map schemas. The interface provides source coverage, hierarchy and filters, original folder groups, proposed items with supporting-source selection, engineer approval/withdrawal, currentness and saved source identities, related finding/BOQ inspectors and explicit artifact/page/passage review forms. Whole-document review requires its own confirmation.

These later screens have not received final rendered verification. The primary agent owns final integration and the handoff under the user's code-only instruction.
