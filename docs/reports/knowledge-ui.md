# Reusable knowledge UI

`Knowledge()` is a global Settings surface for explicitly approved reusable notes. It does not infer notes from imported files or promote one tender's facts into another tender.

## Owned files

- `src/features/Knowledge.tsx` — exported component and focused form/detail/source helpers.
- `src/features/Knowledge.test.tsx` — four behavior tests with synthetic API responses.
- `src/styles/knowledge.css` — styling within the existing Quantix visual system.
- `docs/reports/knowledge-ui.md` — this report.

No shared source, configuration, generated binding or backend file was edited. The primary agent wires `Knowledge()` into Settings after the general settings form, gated by the `knowledge` capability.

## Behavior

- Lists the real `/knowledge` records with category and approved/withdrawn state. Category and include-withdrawn filters are sent to the API; lists are paginated in groups of 50.
- Creation requires the engineer's note, explicit reuse confirmation and approval rationale. Verification and recheck dates remain optional; they are never filled with an assumed verification date.
- A supporting tender is optional and is chosen from real `/tenders` results. Source search is scoped to that tender. Changing it clears previous source IDs; no raw ID entry is exposed.
- Source-selection and inspection buttons are non-submit controls. Source drawers load the original supporting tender's file registry, including historical versions, and use the exact linked evidence ID.
- Detail reads use `/knowledge/{id}` and display the immutable text, approval and withdrawal notes, dates, category, state, recheck reasons, use limitations and source provenance. Provenance includes original tender, file path, source locator, saved version and recorded hashes.
- Price and tax notes always state that commercial use requires current verification, regardless of approval or recorded dates.
- Withdrawal requires explicit confirmation and a rationale. Failed requests retain the form and existing approved state; a successful request shows the returned withdrawn record without rewriting its approved text.

## Verification

Four focused tests pass under the normal `npm run test:ui -- src/features/Knowledge.test.tsx` command. They cover:

1. Explicit approval and retained input after creation failure.
2. Source selection and viewing within the original tender, no source-button form submission even with an otherwise valid approved form, and clearing old source IDs when the supporting tender changes.
3. Category filtering and mandatory current verification for commercial notes, including source-revision recheck reasons and a recorded verification date.
4. No change to approved text/state until the service accepts withdrawal, followed by the returned withdrawn record.

`npm run check:ui` passes against the generated knowledge schemas. No knowledge record, approval, withdrawal or credential was written to the real reference workspace.

Settings integration is present. The final real-empty-state browser check is pending service refresh: a guarded Chrome read of the running `/api/health` returned `estimates`, `outputs`, `meaning_search`, `backups` and `quotations`, without `knowledge`. The feature therefore remains correctly hidden and the Reusable notes heading was unavailable in the live page. No empty-state success is claimed yet.

The attempted browser check used the previously authorized Playwright Chrome fallback because built-in CUA failed with `Debugger unattached`. All API methods except GET and OPTIONS were blocked. After the primary agent restarts the service with `knowledge` enabled, the remaining check is to open Settings, verify the genuine empty list, open the unsaved creation form, verify its unchecked approval and empty verification date, select Price to inspect its current-verification notice, and cancel. No real note should be created for this check.

All four source/report files are released for primary-agent integration and final verification.
