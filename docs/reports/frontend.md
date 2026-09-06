# Frontend implementation and verification

Implemented the accepted Quantix desktop direction against the generated HTTP API schemas. This report covers the frontend increment, not completion of every capability in the product specification.

## Implemented surfaces

- Persistent tender sidebar, real initial empty state, new tender form, folder/ZIP import form, tender tabs and responsive navigation.
- Tender Manager conversation, source-linked findings and engineer decisions, distinct office/document-processing records, proposed plan approval, and a composer that retains drafts through failed requests and navigation to Work or Settings.
- Files with processing-status, area and revision filters; exact source search; explicit processing exceptions; PDF PNG previews; spreadsheet/text source locators and cell/formula metadata.
- Work plans, task results, ready-task execution, run history/events, cancellation and supported resumption. Failed or interrupted manager/import work remains visible after leaving the active-run list.
- Settings with a write-only masked key, read-only supported model, currency/preferences, and truthful configured-versus-tested connection wording.
- Estimate capability gated by `/health.capabilities`. Current source candidates, pagination, source confirmation, supplied and effective quantities, direct rates with dated provenance and tax treatment, separate measured proposals and explicit quantity approval.
- Output capability independently gated. Draft BOQ/analysis creation forms and authenticated XLSX/DOCX downloads. Draft status and pricing blockers remain visible.

All API calls use bearer authentication. PDF and output downloads use authenticated blob requests. Money and quantity values remain decimal strings in requests and displays; the frontend does not recalculate commercial totals. No example tender, artificial findings, fabricated rates or sample production records are seeded by this implementation.

## Verification

`npm run check:ui` passed against `src/bindings/api.ts`. The latest integrated full-suite run passed 53 tests across 15 files. The final attachment-filter change then passed all 13 quotation tests and TypeScript. Coverage includes explicit search methods, filters sent before ranking, matched meaning-search passages, historical file selection, search preparation failures, authenticated backups, inspected-archive binding, restoration recovery, per-tender draft recovery and secret-safe validation errors. Prettier checks pass for the files edited by this task.

Initial browser verification used built-in CUA on the real local service at `http://127.0.0.1:1420/`, title `Quantix`. The later CUA session failed with `Debugger unattached` during creation and reacquisition of its QA tab. Following explicit fallback authorization, fresh verification used Playwright 1.63 with the installed Chrome channel and an isolated temporary browser profile. It blocked every API method except GET and OPTIONS. The reference viewport was 1505 × 1045, with an additional 390 × 844 narrow-window check. No test-only renderer was used.

| Check | Result |
| --- | --- |
| Meaningful initial content, correct page title and URL | Passed |
| Framework error overlay | None in the final pass |
| Console warning/error inspection | Empty in the final pass |
| Blank tender-name validation and cancelled creation | Passed; no synthetic tender created |
| Settings state and masked key form | Passed; no secrets/preferences changed |
| Real document filtering and source search | Passed |
| PDF image preview and exact spreadsheet source range | Passed |
| Nested source drawer Escape handling and return focus | Passed |
| Estimate candidate table and incomplete pricing state | Passed |
| Source/rate review forms remain unconfirmed until engineer action | Passed by inspection; cancelled without decisions |
| Import dialog cancellation | Passed; no additional import started |
| Narrow layout and accessible project-details modal | Passed |

The local service was deliberately restarted during integration. Earlier requests displayed visible connection failures; the final reload used the new connection and had no console errors. The first large PDF preview required additional render time; its loading state stayed visible and the image completed successfully.

## Visual comparison

The accepted concept and settled Manager screenshot were both inspected with `view_image` in the final QA pass.

| Comparison point | Outcome |
| --- | --- |
| Navy rail, white canvas and teal action hierarchy | Preserved |
| Wordmark, strong tender heading and understated tabs | Preserved |
| Open manager paper sections and ruled contextual rail | Preserved |
| Citation chips, outlined controls and lucide stroke treatment | Preserved |
| Bottom composer, attachment and send geometry | Preserved |
| Responsive context and navigation | Rail becomes a project-details modal; narrow navigation is hidden from keyboard/AX until opened |

Material fixes from QA: prevented the right rail growing excessively in wide windows; corrected desktop-only visibility of the project-details control; gave source areas their full rail width; removed the internal tender record counter from contractual-looking revision copy; focused the first form field on dialog open; limited keyboard trapping to the topmost nested dialog; retained composer state while inspecting other surfaces.

Intentional content differences from the fictional concept follow actual API state: local processing records and the provider requirement replace fabricated AI findings; no proposed plan appears until a real plan exists; coverage shows the actual processing categories; scope displays source areas rather than invented work/location/currency facts. The visual system and primary scaffold were faithfully checked against the accepted reference. The populated AI plan state could not receive live visual verification without a configured provider.

## Screenshot evidence

Screenshots are in the ignored `.quantix-dev/qa/` directory because they may display private source material. Do not commit or publish them.

- `manager-desktop.png` — final settled Manager at 1505 × 1045.
- `manager-mobile.png` — Manager at 390 × 844.
- `estimate-desktop.png` — real estimate candidates and review blockers at 1505 × 1045.
- `source-search.png` — real exact source search.
- `spreadsheet-source.png` — exact cited worksheet range in the source drawer.
- `files-desktop.png` — filtered real document register.
- `import-dialog.png` — import form before cancellation.
- `settings-desktop.png` — initial real settings state.
- `source-preview.png` — completed PDF preview; captured during the service restart, so it also records visible connection failures behind the preserved image.

## Limits and integration ownership

Live provider calls, source confirmations, price decisions, quantity approvals, and output generation were not executed against the user's reference tender during browser QA. Their controls call the documented APIs; synthetic frontend tests and the primary agent's backend/integration checks cover decision paths. No claim is made that a live AI analysis, checked takeoff, price verification, or release approval occurred.

The direct-rate editor is implemented. Existing rate-component build-ups are inspectable; component editing was optional in the assigned increment and is not included. Native package chooser invocation is wired, while native-window verification belongs to the primary agent. No release package was built by this frontend task.

Owned changes are `src/**` excluding generated `src/bindings/**`, plus this report. No backend, Rust, root configuration or package files were edited, and no commits were created by this task.

## Search and backup follow-up

Files now offers Words, Meaning and Both when the service advertises `meaning_search`. Words is the initial choice. The requested mode, area and reading status are sent to the search endpoint and included in the query key. A failed Meaning or Both request remains visibly failed; it does not become a Words request. Where available, a meaning result displays the matched passage from `metadata.semantic_match.text`.

The meaning-search preparation control reads real status, starts the documented background run, displays its real progress/failure, and links to Work. Work labels these runs as Prepare search. All revisions now fetches `include_history=true`; opening an earlier version carries that exact artifact into the source viewer. A visible note distinguishes current-version search from the historical register.

Backups remains outside the Settings form. Archive checks lock the path while pending. Restoration uses the inspected path and `expected_sha256`, requires a reason and an explicit prepare action, and locks the selected archive after preparation. Known prepared state survives Settings navigation. Missing originals are disclosed. Closing for restoration preserves the prepared state on errors; a successful service stop is remembered so a failed native close can be retried without repeating a dead HTTP request.

The estimate table now clamps long descriptions to three lines and keeps each full description accessible through its review button. Repeated per-row warning paragraphs were removed; all issues remain in the row review drawer and the estimate-wide blockers remain visible. UI-owned wording now uses Read, source text and reading issues in place of technical processing terms.

Additional files in this increment: `src/features/Backups.tsx`, `Backups.test.tsx`, `DocumentSearch.tsx`, `DocumentSearch.test.tsx`; updated `App.tsx`, `components/ui.tsx`, `Files.tsx`, `Files.test.tsx`, `Manager.tsx`, `Sources.tsx`, `Work.tsx`, `Settings.tsx`, `Estimate.tsx`, `EstimateEditor.tsx`, `styles/features.css` and `styles/estimate.css`.

The compact estimate presentation was checked in the real browser at 1505 × 1045: its first long description retained the full text while displaying three lines, and full source/issue details remained available in the review drawer. Screenshot: `.quantix-dev/qa/estimate-compact.png`. After the service restart, fresh checks passed for Words, Meaning and Both (real HTTP 200 responses), exact meaning-result source navigation, the history endpoint and current-version search notice, backup guards, read-only model, empty API-key field and narrow-window layout. There were no API writes, JavaScript errors or console errors in the guarded pass.

The restoration-reload issue is resolved: Backups reads `/backups/pending` and reconstructs the prepared notice from the service, including after a fresh renderer starts. Actions remain locked until that status is known. A focused test verifies recovery with a fresh QueryClient; no restoration was staged against the real reference workspace during QA.

Unsent manager drafts now use browser-local storage keyed by tender ID. They survive full renderer reloads, remain separate between tenders, and are removed from the unsent store after the API accepts the message. The in-memory copy remains available while work runs. Storage failures are visible. No API-key field is persisted to browser storage. Tests cover separation and accepted-message behavior; the real browser pass also verified draft reload recovery in an isolated temporary profile, then cleared that temporary draft without sending it.

FastAPI 422 responses now show their human-readable `msg` values. The renderer does not stringify validation objects or include their `input`/context fields. The regression test checks a useful validation message and ensures a submitted password value is absent.

## Final quotation attachment adjustment

Temporarily owned `Quotes.tsx`, `Quotes.test.tsx` and `styles/correspondence.css` for one bounded improvement. The attachment picker filters file paths without case sensitivity, keeps selected IDs independent from the filter, reports selected items outside the current filter, and keeps the entire checklist within 240px. No file type is preselected or excluded by this change. Existing attachment limits and supplier actions remain unchanged.

The test selects two files across different filters, clears the filter, verifies both remain checked and verifies both IDs in the saved draft payload. The real-package browser check inspected all 104 available choices, verified the 240px bound, filtered with uppercase path text, verified hidden selection retention and restored visibility, then cancelled the form. No draft, approval or mail request was written to the real service.

Fresh ignored evidence:

- `.quantix-dev/qa/recovery-qa.json` — complete guarded read-only result, including meaning search and isolated-profile draft reload.
- `.quantix-dev/qa/recovery-manager-desktop.png`.
- `.quantix-dev/qa/meaning-search-desktop.png` and `meaning-search-source.png`.
- `.quantix-dev/qa/history-register-desktop.png`.
- `.quantix-dev/qa/backups-recovery-desktop.png` and `search-mobile.png`.
- `.quantix-dev/qa/quotation-request-form.png` — request form opened without submitting; Supplier mail was also verified as present.
- `.quantix-dev/qa/attachment-qa.json` — real 104-file picker verification with zero writes/errors.
- `.quantix-dev/qa/quotation-attachments-filter.png` and `quotation-attachment-controls.png` — final bounded picker and accessible review controls.

The new recovery helper is `src/features/drafts.ts`; additional changes are in `Composer.tsx`/tests, `Backups.tsx`/tests, `api.ts`/tests, `Manager.tsx`, `Settings.test.tsx` and test storage cleanup. The primary agent owns the Node test launcher, all later supplier/rate integration and the final full verification gate. All source ownership is released back to the primary agent.
